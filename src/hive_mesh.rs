// Copyright (c) 2025-2026 (r)evolve - Revolve Team LLC
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! HiveMesh - Unified mesh management facade
//!
//! This module provides the main entry point for HIVE BLE mesh operations.
//! It composes peer management, document sync, and observer notifications
//! into a single interface that platform implementations can use.
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::hive_mesh::{HiveMesh, HiveMeshConfig};
//! use hive_btle::observer::{HiveEvent, HiveObserver};
//! use hive_btle::NodeId;
//! use std::sync::Arc;
//!
//! // Create mesh configuration
//! let config = HiveMeshConfig::new(NodeId::new(0x12345678), "ALPHA-1", "DEMO");
//!
//! // Create mesh instance
//! let mesh = HiveMesh::new(config);
//!
//! // Add observer for events
//! struct MyObserver;
//! impl HiveObserver for MyObserver {
//!     fn on_event(&self, event: HiveEvent) {
//!         println!("Event: {:?}", event);
//!     }
//! }
//! mesh.add_observer(Arc::new(MyObserver));
//!
//! // Platform BLE callbacks
//! mesh.on_ble_discovered("device-uuid", Some("HIVE_DEMO-AABBCCDD"), -65, Some("DEMO"), now_ms);
//! mesh.on_ble_connected("device-uuid", now_ms);
//! mesh.on_ble_data_received("device-uuid", &data, now_ms);
//!
//! // Periodic maintenance
//! if let Some(sync_data) = mesh.tick(now_ms) {
//!     // Broadcast sync_data to connected peers
//! }
//! ```

#[cfg(not(feature = "std"))]
use alloc::{string::String, sync::Arc, vec::Vec};
#[cfg(feature = "std")]
use std::collections::HashMap;
#[cfg(feature = "std")]
use std::sync::Arc;

use crate::document::{ENCRYPTED_MARKER, KEY_EXCHANGE_MARKER, PEER_E2EE_MARKER};
use crate::document_sync::DocumentSync;
use crate::gossip::{GossipStrategy, RandomFanout};
use crate::observer::{DisconnectReason, HiveEvent, HiveObserver, SecurityViolationKind};
use crate::peer::{
    ConnectionStateGraph, FullStateCountSummary, HivePeer, IndirectPeer, PeerConnectionState,
    PeerDegree, PeerManagerConfig, StateCountSummary,
};
use crate::peer_manager::PeerManager;
use crate::relay::{
    MessageId, RelayEnvelope, SeenMessageCache, DEFAULT_MAX_HOPS, DEFAULT_SEEN_TTL_MS,
    RELAY_ENVELOPE_MARKER,
};
use crate::security::{
    DeviceIdentity, IdentityAttestation, IdentityRegistry, KeyExchangeMessage, MeshEncryptionKey,
    PeerEncryptedMessage, PeerSessionManager, RegistryResult, SessionState,
};
use crate::sync::crdt::{EventType, Peripheral, PeripheralType};
use crate::sync::delta::{DeltaEncoder, DeltaStats};
use crate::sync::delta_document::{DeltaDocument, Operation};
use crate::NodeId;

#[cfg(feature = "std")]
use crate::observer::ObserverManager;

/// Configuration for HiveMesh
#[derive(Debug, Clone)]
pub struct HiveMeshConfig {
    /// Our node ID
    pub node_id: NodeId,

    /// Our callsign (e.g., "ALPHA-1")
    pub callsign: String,

    /// Mesh ID to filter peers (e.g., "DEMO")
    pub mesh_id: String,

    /// Peripheral type for this device
    pub peripheral_type: PeripheralType,

    /// Peer management configuration
    pub peer_config: PeerManagerConfig,

    /// Sync interval in milliseconds (how often to broadcast state)
    pub sync_interval_ms: u64,

    /// Whether to auto-broadcast on emergency/ack
    pub auto_broadcast_events: bool,

    /// Optional shared secret for mesh-wide encryption (32 bytes)
    ///
    /// When set, all documents are encrypted using ChaCha20-Poly1305 before
    /// transmission and decrypted upon receipt. All nodes in the mesh must
    /// share the same secret to communicate.
    pub encryption_secret: Option<[u8; 32]>,

    /// Strict encryption mode - reject unencrypted documents when encryption is enabled
    ///
    /// When true and encryption is enabled, any unencrypted documents received
    /// will be rejected and trigger a SecurityViolation event. This prevents
    /// downgrade attacks where an adversary sends unencrypted malicious documents.
    ///
    /// Default: false (backward compatible - accepts unencrypted for gradual rollout)
    pub strict_encryption: bool,

    /// Enable multi-hop relay
    ///
    /// When enabled, received messages will be forwarded to other peers based
    /// on the gossip strategy. Requires message deduplication to prevent loops.
    ///
    /// Default: false
    pub enable_relay: bool,

    /// Maximum hops for relay messages (TTL)
    ///
    /// Messages will not be relayed beyond this many hops from the origin.
    /// Default: 7
    pub max_relay_hops: u8,

    /// Gossip fanout for relay
    ///
    /// Number of peers to forward each message to. Higher values increase
    /// convergence speed but also bandwidth usage.
    /// Default: 2
    pub relay_fanout: usize,

    /// TTL for seen message cache (milliseconds)
    ///
    /// How long to remember message IDs for deduplication.
    /// Default: 300_000 (5 minutes)
    pub seen_cache_ttl_ms: u64,
}

impl HiveMeshConfig {
    /// Create a new configuration with required fields
    pub fn new(node_id: NodeId, callsign: &str, mesh_id: &str) -> Self {
        Self {
            node_id,
            callsign: callsign.into(),
            mesh_id: mesh_id.into(),
            peripheral_type: PeripheralType::SoldierSensor,
            peer_config: PeerManagerConfig::with_mesh_id(mesh_id),
            sync_interval_ms: 5000,
            auto_broadcast_events: true,
            encryption_secret: None,
            strict_encryption: false,
            enable_relay: false,
            max_relay_hops: DEFAULT_MAX_HOPS,
            relay_fanout: 2,
            seen_cache_ttl_ms: DEFAULT_SEEN_TTL_MS,
        }
    }

    /// Enable mesh-wide encryption with a shared secret
    ///
    /// All documents will be encrypted using ChaCha20-Poly1305 before
    /// transmission. All mesh participants must use the same secret.
    pub fn with_encryption(mut self, secret: [u8; 32]) -> Self {
        self.encryption_secret = Some(secret);
        self
    }

    /// Set peripheral type
    pub fn with_peripheral_type(mut self, ptype: PeripheralType) -> Self {
        self.peripheral_type = ptype;
        self
    }

    /// Set sync interval
    pub fn with_sync_interval(mut self, interval_ms: u64) -> Self {
        self.sync_interval_ms = interval_ms;
        self
    }

    /// Set peer timeout
    pub fn with_peer_timeout(mut self, timeout_ms: u64) -> Self {
        self.peer_config.peer_timeout_ms = timeout_ms;
        self
    }

    /// Set max peers (for embedded systems)
    pub fn with_max_peers(mut self, max: usize) -> Self {
        self.peer_config.max_peers = max;
        self
    }

    /// Enable strict encryption mode
    ///
    /// When enabled (and encryption is also enabled), any unencrypted documents
    /// received will be rejected and trigger a `SecurityViolation` event.
    /// This prevents downgrade attacks.
    ///
    /// Note: This only has effect when encryption is enabled via `with_encryption()`.
    pub fn with_strict_encryption(mut self) -> Self {
        self.strict_encryption = true;
        self
    }

    /// Enable multi-hop relay
    ///
    /// When enabled, received messages will be forwarded to other connected peers
    /// based on the gossip strategy. This enables mesh-wide message propagation.
    pub fn with_relay(mut self) -> Self {
        self.enable_relay = true;
        self
    }

    /// Set maximum relay hops (TTL)
    ///
    /// Messages will not be relayed beyond this many hops from the origin.
    pub fn with_max_relay_hops(mut self, max_hops: u8) -> Self {
        self.max_relay_hops = max_hops;
        self
    }

    /// Set gossip fanout for relay
    ///
    /// Number of peers to forward each message to.
    pub fn with_relay_fanout(mut self, fanout: usize) -> Self {
        self.relay_fanout = fanout.max(1);
        self
    }

    /// Set TTL for seen message cache
    ///
    /// How long to remember message IDs for deduplication (milliseconds).
    pub fn with_seen_cache_ttl(mut self, ttl_ms: u64) -> Self {
        self.seen_cache_ttl_ms = ttl_ms;
        self
    }
}

/// Main facade for HIVE BLE mesh operations
///
/// Composes peer management, document sync, and observer notifications.
/// Platform implementations call into this from their BLE callbacks.
#[cfg(feature = "std")]
pub struct HiveMesh {
    /// Configuration
    config: HiveMeshConfig,

    /// Peer manager
    peer_manager: PeerManager,

    /// Document sync
    document_sync: DocumentSync,

    /// Observer manager
    observers: ObserverManager,

    /// Last sync broadcast time (u32 wraps every ~49 days, sufficient for intervals)
    last_sync_ms: std::sync::atomic::AtomicU32,

    /// Last cleanup time
    last_cleanup_ms: std::sync::atomic::AtomicU32,

    /// Optional mesh-wide encryption key (derived from shared secret)
    encryption_key: Option<MeshEncryptionKey>,

    /// Optional per-peer E2EE session manager
    peer_sessions: std::sync::Mutex<Option<PeerSessionManager>>,

    /// Connection state graph for tracking peer connection lifecycle
    connection_graph: std::sync::Mutex<ConnectionStateGraph>,

    /// Seen message cache for relay deduplication
    seen_cache: std::sync::Mutex<SeenMessageCache>,

    /// Gossip strategy for relay peer selection
    gossip_strategy: Box<dyn GossipStrategy>,

    /// Delta encoder for per-peer sync state tracking
    ///
    /// Tracks what data has been sent to each peer to enable delta sync
    /// (sending only changes instead of full documents).
    delta_encoder: std::sync::Mutex<DeltaEncoder>,

    /// This node's cryptographic identity (Ed25519 keypair)
    ///
    /// When set, the node_id is derived from the public key and documents
    /// can be signed for authenticity verification.
    identity: Option<DeviceIdentity>,

    /// TOFU identity registry for tracking peer identities
    ///
    /// Maps node_id to public key on first contact, rejects mismatches.
    identity_registry: std::sync::Mutex<IdentityRegistry>,

    /// Peripheral state received from peers
    ///
    /// Stores the most recent peripheral data (callsign, location, etc.)
    /// received from each peer via document sync.
    peer_peripherals: std::sync::RwLock<HashMap<NodeId, Peripheral>>,
}

#[cfg(feature = "std")]
impl HiveMesh {
    /// Create a new HiveMesh instance
    pub fn new(config: HiveMeshConfig) -> Self {
        let peer_manager = PeerManager::new(config.node_id, config.peer_config.clone());
        let document_sync = DocumentSync::with_peripheral_type(
            config.node_id,
            &config.callsign,
            config.peripheral_type,
        );

        // Derive encryption key from shared secret if configured
        let encryption_key = config
            .encryption_secret
            .map(|secret| MeshEncryptionKey::from_shared_secret(&config.mesh_id, &secret));

        // Create connection state graph with config thresholds
        let connection_graph = ConnectionStateGraph::with_config(
            config.peer_config.rssi_degraded_threshold,
            config.peer_config.lost_timeout_ms,
        );

        // Create seen message cache for relay deduplication
        let seen_cache = SeenMessageCache::with_ttl(config.seen_cache_ttl_ms);

        // Create gossip strategy for relay
        let gossip_strategy: Box<dyn GossipStrategy> =
            Box::new(RandomFanout::new(config.relay_fanout));

        // Create delta encoder for per-peer sync state tracking
        let delta_encoder = DeltaEncoder::new(config.node_id);

        Self {
            config,
            peer_manager,
            document_sync,
            observers: ObserverManager::new(),
            last_sync_ms: std::sync::atomic::AtomicU32::new(0),
            last_cleanup_ms: std::sync::atomic::AtomicU32::new(0),
            encryption_key,
            peer_sessions: std::sync::Mutex::new(None),
            connection_graph: std::sync::Mutex::new(connection_graph),
            seen_cache: std::sync::Mutex::new(seen_cache),
            gossip_strategy,
            delta_encoder: std::sync::Mutex::new(delta_encoder),
            identity: None,
            identity_registry: std::sync::Mutex::new(IdentityRegistry::new()),
            peer_peripherals: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Create a new HiveMesh with a cryptographic identity
    ///
    /// The node_id will be derived from the identity's public key, overriding
    /// any node_id specified in the config. This ensures cryptographic binding
    /// between node_id and identity.
    pub fn with_identity(config: HiveMeshConfig, identity: DeviceIdentity) -> Self {
        // Override node_id with identity-derived value
        let mut config = config;
        config.node_id = identity.node_id();

        let peer_manager = PeerManager::new(config.node_id, config.peer_config.clone());
        let document_sync = DocumentSync::with_peripheral_type(
            config.node_id,
            &config.callsign,
            config.peripheral_type,
        );

        let encryption_key = config
            .encryption_secret
            .map(|secret| MeshEncryptionKey::from_shared_secret(&config.mesh_id, &secret));

        let connection_graph = ConnectionStateGraph::with_config(
            config.peer_config.rssi_degraded_threshold,
            config.peer_config.lost_timeout_ms,
        );

        let seen_cache = SeenMessageCache::with_ttl(config.seen_cache_ttl_ms);
        let gossip_strategy: Box<dyn GossipStrategy> =
            Box::new(RandomFanout::new(config.relay_fanout));
        let delta_encoder = DeltaEncoder::new(config.node_id);

        Self {
            config,
            peer_manager,
            document_sync,
            observers: ObserverManager::new(),
            last_sync_ms: std::sync::atomic::AtomicU32::new(0),
            last_cleanup_ms: std::sync::atomic::AtomicU32::new(0),
            encryption_key,
            peer_sessions: std::sync::Mutex::new(None),
            connection_graph: std::sync::Mutex::new(connection_graph),
            seen_cache: std::sync::Mutex::new(seen_cache),
            gossip_strategy,
            delta_encoder: std::sync::Mutex::new(delta_encoder),
            identity: Some(identity),
            identity_registry: std::sync::Mutex::new(IdentityRegistry::new()),
            peer_peripherals: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// Create a new HiveMesh from genesis data
    ///
    /// This is the recommended way to create a mesh for production use.
    /// The mesh will be configured with:
    /// - node_id derived from identity
    /// - mesh_id from genesis
    /// - encryption enabled using genesis-derived secret
    pub fn from_genesis(
        genesis: &crate::security::MeshGenesis,
        identity: DeviceIdentity,
        callsign: &str,
    ) -> Self {
        let config = HiveMeshConfig::new(identity.node_id(), callsign, &genesis.mesh_id())
            .with_encryption(genesis.encryption_secret());

        Self::with_identity(config, identity)
    }

    /// Create a HiveMesh from persisted state.
    ///
    /// Restores mesh configuration from previously saved state, including:
    /// - Device identity (Ed25519 keypair)
    /// - Mesh genesis (if present)
    /// - Identity registry (TOFU cache)
    ///
    /// Use this on device boot to restore mesh membership without re-provisioning.
    ///
    /// # Arguments
    ///
    /// * `state` - Previously persisted state
    /// * `callsign` - Human-readable identifier (may differ from original)
    ///
    /// # Errors
    ///
    /// Returns `PersistenceError` if the identity cannot be restored.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // On boot, restore from secure storage
    /// let state = PersistedState::load(&storage)?;
    /// let mesh = HiveMesh::from_persisted(state, "SENSOR-01")?;
    /// ```
    #[cfg(feature = "std")]
    pub fn from_persisted(
        state: crate::security::PersistedState,
        callsign: &str,
    ) -> Result<Self, crate::security::PersistenceError> {
        // Restore identity
        let identity = state.restore_identity()?;

        // Restore genesis (if present)
        let genesis = state.restore_genesis();

        // Create mesh with or without genesis
        let mesh = if let Some(ref gen) = genesis {
            Self::from_genesis(gen, identity, callsign)
        } else {
            let config = HiveMeshConfig::new(identity.node_id(), callsign, "RESTORED");
            Self::with_identity(config, identity)
        };

        // Restore identity registry
        let restored_registry = state.restore_registry();
        if let Ok(mut registry) = mesh.identity_registry.lock() {
            *registry = restored_registry;
        }

        log::info!(
            "HiveMesh restored from persisted state: node_id={:08X}, known_peers={}",
            mesh.config.node_id.as_u32(),
            mesh.known_identity_count()
        );

        Ok(mesh)
    }

    /// Create persisted state from current mesh state.
    ///
    /// Captures the current identity, genesis, and registry for persistence.
    /// Call this periodically or before shutdown to save state.
    ///
    /// # Arguments
    ///
    /// * `genesis` - Optional genesis to include (if mesh was created from genesis)
    ///
    /// # Returns
    ///
    /// `None` if the mesh has no identity bound.
    #[cfg(feature = "std")]
    pub fn to_persisted_state(
        &self,
        genesis: Option<&crate::security::MeshGenesis>,
    ) -> Option<crate::security::PersistedState> {
        let identity = self.identity.as_ref()?;
        let registry = self.identity_registry.lock().ok()?;

        Some(crate::security::PersistedState::with_registry(
            identity, genesis, &registry,
        ))
    }

    // ==================== Encryption ====================

    /// Check if mesh-wide encryption is enabled
    pub fn is_encryption_enabled(&self) -> bool {
        self.encryption_key.is_some()
    }

    /// Check if strict encryption mode is enabled
    ///
    /// Returns true only if both encryption and strict_encryption are enabled.
    pub fn is_strict_encryption_enabled(&self) -> bool {
        self.config.strict_encryption && self.encryption_key.is_some()
    }

    /// Enable mesh-wide encryption with a shared secret
    ///
    /// Derives a ChaCha20-Poly1305 key from the secret using HKDF-SHA256.
    /// All mesh participants must use the same secret to communicate.
    pub fn enable_encryption(&mut self, secret: &[u8; 32]) {
        self.encryption_key = Some(MeshEncryptionKey::from_shared_secret(
            &self.config.mesh_id,
            secret,
        ));
    }

    /// Disable mesh-wide encryption
    pub fn disable_encryption(&mut self) {
        self.encryption_key = None;
    }

    /// Encrypt document bytes for transmission
    ///
    /// Returns the encrypted bytes with ENCRYPTED_MARKER prefix, or the
    /// original bytes if encryption is disabled.
    fn encrypt_document(&self, plaintext: &[u8]) -> Vec<u8> {
        match &self.encryption_key {
            Some(key) => {
                // Encrypt and prepend marker
                match key.encrypt_to_bytes(plaintext) {
                    Ok(ciphertext) => {
                        let mut buf = Vec::with_capacity(2 + ciphertext.len());
                        buf.push(ENCRYPTED_MARKER);
                        buf.push(0x00); // reserved
                        buf.extend_from_slice(&ciphertext);
                        buf
                    }
                    Err(e) => {
                        log::error!("Encryption failed: {}", e);
                        // Fall back to unencrypted on error (shouldn't happen)
                        plaintext.to_vec()
                    }
                }
            }
            None => plaintext.to_vec(),
        }
    }

    /// Decrypt document bytes received from peer
    ///
    /// Returns the decrypted bytes if encrypted and valid, or the original
    /// bytes if not encrypted. Returns None if decryption fails.
    ///
    /// In strict encryption mode (when both encryption and strict_encryption are enabled),
    /// unencrypted documents are rejected and trigger a SecurityViolation event.
    fn decrypt_document<'a>(
        &self,
        data: &'a [u8],
        source_hint: Option<&str>,
    ) -> Option<std::borrow::Cow<'a, [u8]>> {
        // Check for encrypted marker
        if data.len() >= 2 && data[0] == ENCRYPTED_MARKER {
            // Encrypted document
            let _reserved = data[1];
            let encrypted_payload = &data[2..];

            match &self.encryption_key {
                Some(key) => match key.decrypt_from_bytes(encrypted_payload) {
                    Ok(plaintext) => Some(std::borrow::Cow::Owned(plaintext)),
                    Err(e) => {
                        log::warn!("Decryption failed (wrong key or corrupted): {}", e);
                        self.notify(HiveEvent::SecurityViolation {
                            kind: SecurityViolationKind::DecryptionFailed,
                            source: source_hint.map(String::from),
                        });
                        None
                    }
                },
                None => {
                    log::warn!("Received encrypted document but encryption not enabled");
                    None
                }
            }
        } else {
            // Unencrypted document
            // Check strict encryption mode
            if self.config.strict_encryption && self.encryption_key.is_some() {
                log::warn!(
                    "Rejected unencrypted document in strict encryption mode (source: {:?})",
                    source_hint
                );
                self.notify(HiveEvent::SecurityViolation {
                    kind: SecurityViolationKind::UnencryptedInStrictMode,
                    source: source_hint.map(String::from),
                });
                None
            } else {
                // Permissive mode: accept unencrypted for backward compatibility
                Some(std::borrow::Cow::Borrowed(data))
            }
        }
    }

    // ==================== Identity ====================

    /// Check if this mesh has a cryptographic identity
    pub fn has_identity(&self) -> bool {
        self.identity.is_some()
    }

    /// Get this node's public key (if identity is configured)
    pub fn public_key(&self) -> Option<[u8; 32]> {
        self.identity.as_ref().map(|id| id.public_key())
    }

    /// Create an identity attestation for this node
    ///
    /// Returns None if no identity is configured.
    pub fn create_attestation(&self, now_ms: u64) -> Option<IdentityAttestation> {
        self.identity
            .as_ref()
            .map(|id| id.create_attestation(now_ms))
    }

    /// Verify and register a peer's identity attestation
    ///
    /// Implements TOFU (Trust On First Use):
    /// - On first contact, registers the node_id → public_key binding
    /// - On subsequent contacts, verifies the public key matches
    ///
    /// Returns the verification result. Security violations should be handled
    /// by the caller (e.g., disconnect, alert).
    pub fn verify_peer_identity(&self, attestation: &IdentityAttestation) -> RegistryResult {
        self.identity_registry
            .lock()
            .unwrap()
            .verify_or_register(attestation)
    }

    /// Check if a peer's identity is known (has been registered)
    pub fn is_peer_identity_known(&self, node_id: NodeId) -> bool {
        self.identity_registry.lock().unwrap().is_known(node_id)
    }

    /// Get a peer's public key if known
    pub fn peer_public_key(&self, node_id: NodeId) -> Option<[u8; 32]> {
        self.identity_registry
            .lock()
            .unwrap()
            .get_public_key(node_id)
            .copied()
    }

    /// Get the number of known peer identities
    pub fn known_identity_count(&self) -> usize {
        self.identity_registry.lock().unwrap().len()
    }

    /// Pre-register a peer's identity (for out-of-band key exchange)
    ///
    /// Use this when keys are exchanged through a secure side channel
    /// (e.g., QR code, NFC tap, or provisioning server).
    pub fn pre_register_peer_identity(&self, node_id: NodeId, public_key: [u8; 32], now_ms: u64) {
        self.identity_registry
            .lock()
            .unwrap()
            .pre_register(node_id, public_key, now_ms);
    }

    /// Remove a peer's identity from the registry
    ///
    /// Use with caution - this allows re-registration with a different key.
    pub fn forget_peer_identity(&self, node_id: NodeId) {
        self.identity_registry.lock().unwrap().remove(node_id);
    }

    /// Sign arbitrary data with this node's identity
    ///
    /// Returns None if no identity is configured.
    pub fn sign(&self, data: &[u8]) -> Option<[u8; 64]> {
        self.identity.as_ref().map(|id| id.sign(data))
    }

    /// Verify a signature from a peer
    ///
    /// Uses the peer's public key from the identity registry.
    /// Returns false if peer is unknown or signature is invalid.
    pub fn verify_peer_signature(
        &self,
        node_id: NodeId,
        data: &[u8],
        signature: &[u8; 64],
    ) -> bool {
        if let Some(public_key) = self.peer_public_key(node_id) {
            crate::security::verify_signature(&public_key, data, signature)
        } else {
            false
        }
    }

    // ==================== Multi-Hop Relay ====================

    /// Check if multi-hop relay is enabled
    pub fn is_relay_enabled(&self) -> bool {
        self.config.enable_relay
    }

    /// Enable multi-hop relay
    pub fn enable_relay(&mut self) {
        self.config.enable_relay = true;
    }

    /// Disable multi-hop relay
    pub fn disable_relay(&mut self) {
        self.config.enable_relay = false;
    }

    /// Check if a message has been seen before (for deduplication)
    ///
    /// Returns true if the message was already seen (duplicate).
    pub fn has_seen_message(&self, message_id: &MessageId) -> bool {
        self.seen_cache.lock().unwrap().has_seen(message_id)
    }

    /// Mark a message as seen
    ///
    /// Returns true if this is a new message (first time seen).
    pub fn mark_message_seen(&self, message_id: MessageId, origin: NodeId, now_ms: u64) -> bool {
        self.seen_cache
            .lock()
            .unwrap()
            .check_and_mark(message_id, origin, now_ms)
    }

    /// Get the number of entries in the seen message cache
    pub fn seen_cache_size(&self) -> usize {
        self.seen_cache.lock().unwrap().len()
    }

    /// Clear the seen message cache
    pub fn clear_seen_cache(&self) {
        self.seen_cache.lock().unwrap().clear();
    }

    /// Wrap a document in a relay envelope for multi-hop transmission
    ///
    /// The returned bytes can be sent to peers and will be automatically
    /// relayed through the mesh if relay is enabled on receiving nodes.
    pub fn wrap_for_relay(&self, payload: Vec<u8>) -> Vec<u8> {
        let envelope = RelayEnvelope::broadcast(self.config.node_id, payload)
            .with_max_hops(self.config.max_relay_hops);
        envelope.encode()
    }

    /// Get peers to relay a message to
    ///
    /// Uses the configured gossip strategy to select relay targets.
    /// Excludes the source peer (if provided) to avoid sending back to sender.
    pub fn get_relay_targets(&self, exclude_peer: Option<NodeId>) -> Vec<HivePeer> {
        let connected = self.peer_manager.get_connected_peers();
        let filtered: Vec<_> = if let Some(exclude) = exclude_peer {
            connected
                .into_iter()
                .filter(|p| p.node_id != exclude)
                .collect()
        } else {
            connected
        };

        self.gossip_strategy
            .select_peers(&filtered)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Process an incoming relay envelope
    ///
    /// Handles deduplication, TTL checking, and determines if the message
    /// should be processed and/or relayed.
    ///
    /// Returns:
    /// - `Ok(Some(RelayDecision))` if message should be processed/relayed
    /// - `Ok(None)` if message was a duplicate or TTL expired
    /// - `Err` if parsing failed
    pub fn process_relay_envelope(
        &self,
        data: &[u8],
        source_peer: NodeId,
        now_ms: u64,
    ) -> Option<RelayDecision> {
        // Parse envelope
        let envelope = RelayEnvelope::decode(data)?;

        // Update indirect peer graph if origin differs from source
        // This means the message was relayed through source_peer from origin_node
        if envelope.origin_node != source_peer && envelope.origin_node != self.node_id() {
            let is_new = self.connection_graph.lock().unwrap().on_relay_received(
                source_peer,
                envelope.origin_node,
                envelope.hop_count,
                now_ms,
            );

            if is_new {
                log::debug!(
                    "Discovered indirect peer {:08X} via {:08X} ({} hops)",
                    envelope.origin_node.as_u32(),
                    source_peer.as_u32(),
                    envelope.hop_count
                );
            }
        }

        // Check deduplication
        if !self.mark_message_seen(envelope.message_id, envelope.origin_node, now_ms) {
            // Duplicate message
            let stats = self
                .seen_cache
                .lock()
                .unwrap()
                .get_stats(&envelope.message_id);
            let seen_count = stats.map(|(_, count, _)| count).unwrap_or(1);

            self.notify(HiveEvent::DuplicateMessageDropped {
                origin_node: envelope.origin_node,
                seen_count,
            });

            log::debug!(
                "Dropped duplicate message {} from {:08X} (seen {} times)",
                envelope.message_id,
                envelope.origin_node.as_u32(),
                seen_count
            );
            return None;
        }

        // Check TTL
        if !envelope.can_relay() {
            self.notify(HiveEvent::MessageTtlExpired {
                origin_node: envelope.origin_node,
                hop_count: envelope.hop_count,
            });

            log::debug!(
                "Message {} from {:08X} TTL expired at hop {}",
                envelope.message_id,
                envelope.origin_node.as_u32(),
                envelope.hop_count
            );

            // Still process locally even if TTL expired
            return Some(RelayDecision {
                payload: envelope.payload,
                origin_node: envelope.origin_node,
                hop_count: envelope.hop_count,
                should_relay: false,
                relay_envelope: None,
            });
        }

        // Determine if we should relay
        let should_relay = self.config.enable_relay;
        let relay_envelope = if should_relay {
            envelope.relay() // Increments hop count
        } else {
            None
        };

        Some(RelayDecision {
            payload: envelope.payload,
            origin_node: envelope.origin_node,
            hop_count: envelope.hop_count,
            should_relay,
            relay_envelope,
        })
    }

    /// Build a document wrapped in a relay envelope
    ///
    /// Convenience method that builds the document, encrypts it (if enabled),
    /// and wraps it in a relay envelope for multi-hop transmission.
    pub fn build_relay_document(&self) -> Vec<u8> {
        let doc = self.build_document(); // Already encrypted if encryption enabled
        self.wrap_for_relay(doc)
    }

    // ==================== Delta Sync ====================

    /// Register a peer for delta sync tracking
    ///
    /// Call this when a peer connects to start tracking what data has been
    /// sent to them. This enables future delta sync (sending only changes).
    pub fn register_peer_for_delta(&self, peer_id: &NodeId) {
        let mut encoder = self.delta_encoder.lock().unwrap();
        encoder.add_peer(peer_id);
        log::debug!(
            "Registered peer {:08X} for delta sync tracking",
            peer_id.as_u32()
        );
    }

    /// Unregister a peer from delta sync tracking
    ///
    /// Call this when a peer disconnects to clean up tracking state.
    pub fn unregister_peer_for_delta(&self, peer_id: &NodeId) {
        let mut encoder = self.delta_encoder.lock().unwrap();
        encoder.remove_peer(peer_id);
        log::debug!(
            "Unregistered peer {:08X} from delta sync tracking",
            peer_id.as_u32()
        );
    }

    /// Reset delta sync state for a peer
    ///
    /// Call this when a peer reconnects to force a full sync on next
    /// communication. This clears the record of what was previously sent.
    pub fn reset_peer_delta_state(&self, peer_id: &NodeId) {
        let mut encoder = self.delta_encoder.lock().unwrap();
        encoder.reset_peer(peer_id);
        log::debug!("Reset delta sync state for peer {:08X}", peer_id.as_u32());
    }

    /// Record bytes sent to a peer (for delta statistics)
    pub fn record_delta_sent(&self, peer_id: &NodeId, bytes: usize) {
        let mut encoder = self.delta_encoder.lock().unwrap();
        encoder.record_sent(peer_id, bytes);
    }

    /// Record bytes received from a peer (for delta statistics)
    pub fn record_delta_received(&self, peer_id: &NodeId, bytes: usize, timestamp: u64) {
        let mut encoder = self.delta_encoder.lock().unwrap();
        encoder.record_received(peer_id, bytes, timestamp);
    }

    /// Get delta sync statistics
    ///
    /// Returns aggregate statistics about delta sync across all peers,
    /// including bytes sent/received and sync counts.
    pub fn delta_stats(&self) -> DeltaStats {
        self.delta_encoder.lock().unwrap().stats()
    }

    /// Get delta sync statistics for a specific peer
    ///
    /// Returns the bytes sent/received and sync count for a single peer.
    pub fn peer_delta_stats(&self, peer_id: &NodeId) -> Option<(u64, u64, u32)> {
        let encoder = self.delta_encoder.lock().unwrap();
        encoder
            .get_peer_state(peer_id)
            .map(|state| (state.bytes_sent, state.bytes_received, state.sync_count))
    }

    /// Build a delta document for a specific peer
    ///
    /// This only includes operations that have changed since the last sync
    /// with this peer. Uses the delta encoder to track per-peer state.
    ///
    /// Returns the encoded delta document bytes, or None if there's nothing
    /// new to send to this peer.
    pub fn build_delta_document_for_peer(&self, peer_id: &NodeId, now_ms: u64) -> Option<Vec<u8>> {
        // Collect all current operations
        let mut all_operations: Vec<Operation> = Vec::new();

        // Add counter operations (one per node that has contributed)
        // Use the count value as the "timestamp" for tracking - only send if count increased
        for (node_id_u32, count) in self.document_sync.counter_entries() {
            all_operations.push(Operation::IncrementCounter {
                counter_id: 0, // Default mesh counter
                node_id: NodeId::new(node_id_u32),
                amount: count,
                timestamp: count, // Use count as timestamp for delta tracking
            });
        }

        // Add peripheral update
        // Use event timestamp if available, otherwise use 1 for initial send
        let peripheral = self.document_sync.peripheral_snapshot();
        let peripheral_timestamp = peripheral
            .last_event
            .as_ref()
            .map(|e| e.timestamp)
            .unwrap_or(1); // Use 1 (not 0) so it's sent initially
        all_operations.push(Operation::UpdatePeripheral {
            peripheral,
            timestamp: peripheral_timestamp,
        });

        // Add emergency operations if active
        if let Some(emergency) = self.document_sync.emergency_snapshot() {
            let source_node = NodeId::new(emergency.source_node());
            let timestamp = emergency.timestamp();

            // Add SetEmergency operation
            all_operations.push(Operation::SetEmergency {
                source_node,
                timestamp,
                known_peers: emergency.all_nodes(),
            });

            // Add AckEmergency for each node that has acked
            for acked_node in emergency.acked_nodes() {
                all_operations.push(Operation::AckEmergency {
                    node_id: NodeId::new(acked_node),
                    emergency_timestamp: timestamp,
                });
            }
        }

        // Filter operations for this peer (only send what's new)
        let filtered_operations: Vec<Operation> = {
            let encoder = self.delta_encoder.lock().unwrap();
            if let Some(peer_state) = encoder.get_peer_state(peer_id) {
                all_operations
                    .into_iter()
                    .filter(|op| peer_state.needs_send(&op.key(), op.timestamp()))
                    .collect()
            } else {
                // Unknown peer, send all operations
                all_operations
            }
        };

        // If nothing new to send, return None
        if filtered_operations.is_empty() {
            return None;
        }

        // Mark operations as sent
        {
            let mut encoder = self.delta_encoder.lock().unwrap();
            if let Some(peer_state) = encoder.get_peer_state_mut(peer_id) {
                for op in &filtered_operations {
                    peer_state.mark_sent(&op.key(), op.timestamp());
                }
            }
        }

        // Build the delta document
        let mut delta = DeltaDocument::new(self.config.node_id, now_ms);
        for op in filtered_operations {
            delta.add_operation(op);
        }

        // Encode and optionally encrypt
        let encoded = delta.encode();
        let result = self.encrypt_document(&encoded);

        // Record stats
        {
            let mut encoder = self.delta_encoder.lock().unwrap();
            encoder.record_sent(peer_id, result.len());
        }

        Some(result)
    }

    /// Build a full delta document (for broadcast or new peers)
    ///
    /// Unlike `build_delta_document_for_peer`, this includes all state
    /// regardless of what has been sent before. Use this for broadcasts.
    pub fn build_full_delta_document(&self, now_ms: u64) -> Vec<u8> {
        let mut delta = DeltaDocument::new(self.config.node_id, now_ms);

        // Add all counter operations
        for (node_id_u32, count) in self.document_sync.counter_entries() {
            delta.add_operation(Operation::IncrementCounter {
                counter_id: 0,
                node_id: NodeId::new(node_id_u32),
                amount: count,
                timestamp: now_ms,
            });
        }

        // Add peripheral
        let peripheral = self.document_sync.peripheral_snapshot();
        let peripheral_timestamp = peripheral
            .last_event
            .as_ref()
            .map(|e| e.timestamp)
            .unwrap_or(now_ms);
        delta.add_operation(Operation::UpdatePeripheral {
            peripheral,
            timestamp: peripheral_timestamp,
        });

        // Add emergency if active
        if let Some(emergency) = self.document_sync.emergency_snapshot() {
            let source_node = NodeId::new(emergency.source_node());
            let timestamp = emergency.timestamp();

            delta.add_operation(Operation::SetEmergency {
                source_node,
                timestamp,
                known_peers: emergency.all_nodes(),
            });

            for acked_node in emergency.acked_nodes() {
                delta.add_operation(Operation::AckEmergency {
                    node_id: NodeId::new(acked_node),
                    emergency_timestamp: timestamp,
                });
            }
        }

        let encoded = delta.encode();
        self.encrypt_document(&encoded)
    }

    /// Internal: Process a received delta document
    ///
    /// Applies operations from a delta document to local state.
    fn process_delta_document_internal(
        &self,
        source_node: NodeId,
        data: &[u8],
        now_ms: u64,
        relay_data: Option<Vec<u8>>,
        origin_node: Option<NodeId>,
        hop_count: u8,
    ) -> Option<DataReceivedResult> {
        // Decode the delta document
        let delta = DeltaDocument::decode(data)?;

        // Don't process our own documents
        if delta.origin_node == self.config.node_id {
            return None;
        }

        // Apply operations to local state
        let mut counter_changed = false;
        let mut emergency_changed = false;
        let mut is_emergency = false;
        let mut is_ack = false;
        let mut event_timestamp = 0u64;
        let mut peer_peripheral: Option<crate::sync::crdt::Peripheral> = None;

        log::debug!(
            "Delta document from {:08X}: {} operations",
            delta.origin_node.as_u32(),
            delta.operations.len()
        );
        for op in &delta.operations {
            log::debug!("  Operation: {}", op.key());
            match op {
                Operation::IncrementCounter {
                    node_id, amount, ..
                } => {
                    // Merge counter value (take max)
                    let current = self.document_sync.counter_entries();
                    let current_value = current
                        .iter()
                        .find(|(id, _)| *id == node_id.as_u32())
                        .map(|(_, v)| *v)
                        .unwrap_or(0);

                    if *amount > current_value {
                        // Need to merge - this is handled by the counter merge logic
                        // For now, we record that counter changed
                        counter_changed = true;
                    }
                }
                Operation::UpdatePeripheral {
                    peripheral,
                    timestamp,
                } => {
                    // Store peer peripheral for callsign lookup
                    if let Ok(mut peripherals) = self.peer_peripherals.write() {
                        peripherals.insert(delta.origin_node, peripheral.clone());
                    }
                    // Track the peripheral for the result
                    peer_peripheral = Some(peripheral.clone());
                    // Track the timestamp for the result
                    if *timestamp > event_timestamp {
                        event_timestamp = *timestamp;
                    }
                }
                Operation::SetEmergency { timestamp, .. } => {
                    is_emergency = true;
                    emergency_changed = true;
                    event_timestamp = *timestamp;
                }
                Operation::AckEmergency {
                    emergency_timestamp,
                    ..
                } => {
                    is_ack = true;
                    emergency_changed = true;
                    if *emergency_timestamp > event_timestamp {
                        event_timestamp = *emergency_timestamp;
                    }
                }
                Operation::ClearEmergency {
                    emergency_timestamp,
                } => {
                    emergency_changed = true;
                    if *emergency_timestamp > event_timestamp {
                        event_timestamp = *emergency_timestamp;
                    }
                }
            }
        }

        // Record sync
        self.peer_manager.record_sync(source_node, now_ms);

        // Record delta received
        {
            let mut encoder = self.delta_encoder.lock().unwrap();
            encoder.record_received(&source_node, data.len(), now_ms);
        }

        // Generate events based on what was received
        if is_emergency {
            self.notify(HiveEvent::EmergencyReceived {
                from_node: delta.origin_node,
            });
        } else if is_ack {
            self.notify(HiveEvent::AckReceived {
                from_node: delta.origin_node,
            });
        }

        if counter_changed {
            let total_count = self.document_sync.total_count();
            self.notify(HiveEvent::DocumentSynced {
                from_node: delta.origin_node,
                total_count,
            });
        }

        // Emit relay event if we're relaying
        if relay_data.is_some() {
            let relay_targets = self.get_relay_targets(Some(source_node));
            self.notify(HiveEvent::MessageRelayed {
                origin_node: origin_node.unwrap_or(delta.origin_node),
                relay_count: relay_targets.len(),
                hop_count,
            });
        }

        let (callsign, battery_percent, heart_rate, event_type, latitude, longitude, altitude) =
            DataReceivedResult::peripheral_fields(&peer_peripheral);

        Some(DataReceivedResult {
            source_node: delta.origin_node,
            is_emergency,
            is_ack,
            counter_changed,
            emergency_changed,
            total_count: self.document_sync.total_count(),
            event_timestamp,
            relay_data,
            origin_node,
            hop_count,
            callsign,
            battery_percent,
            heart_rate,
            event_type,
            latitude,
            longitude,
            altitude,
        })
    }

    // ==================== Per-Peer E2EE ====================

    /// Enable per-peer E2EE capability
    ///
    /// Creates a new identity key for this node. This allows establishing
    /// encrypted sessions with specific peers where only the sender and
    /// recipient can read messages (other mesh members cannot).
    pub fn enable_peer_e2ee(&self) {
        let mut sessions = self.peer_sessions.lock().unwrap();
        if sessions.is_none() {
            *sessions = Some(PeerSessionManager::new(self.config.node_id));
            log::info!(
                "Per-peer E2EE enabled for node {:08X}",
                self.config.node_id.as_u32()
            );
        }
    }

    /// Disable per-peer E2EE capability
    ///
    /// Clears all peer sessions and disables E2EE.
    pub fn disable_peer_e2ee(&self) {
        let mut sessions = self.peer_sessions.lock().unwrap();
        *sessions = None;
        log::info!("Per-peer E2EE disabled");
    }

    /// Check if per-peer E2EE is enabled
    pub fn is_peer_e2ee_enabled(&self) -> bool {
        self.peer_sessions.lock().unwrap().is_some()
    }

    /// Get our E2EE public key (for sharing with peers)
    ///
    /// Returns None if per-peer E2EE is not enabled.
    pub fn peer_e2ee_public_key(&self) -> Option<[u8; 32]> {
        self.peer_sessions
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.our_public_key())
    }

    /// Initiate E2EE session with a specific peer
    ///
    /// Returns the key exchange message bytes to send to the peer.
    /// The message should be broadcast/sent to the peer.
    /// Returns None if per-peer E2EE is not enabled.
    pub fn initiate_peer_e2ee(&self, peer_node_id: NodeId, now_ms: u64) -> Option<Vec<u8>> {
        let mut sessions = self.peer_sessions.lock().unwrap();
        let session_mgr = sessions.as_mut()?;

        let key_exchange = session_mgr.initiate_session(peer_node_id, now_ms);
        let mut buf = Vec::with_capacity(2 + 37);
        buf.push(KEY_EXCHANGE_MARKER);
        buf.push(0x00); // reserved
        buf.extend_from_slice(&key_exchange.encode());

        log::info!(
            "Initiated E2EE session with peer {:08X}",
            peer_node_id.as_u32()
        );
        Some(buf)
    }

    /// Check if we have an established E2EE session with a peer
    pub fn has_peer_e2ee_session(&self, peer_node_id: NodeId) -> bool {
        self.peer_sessions
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|s| s.has_session(peer_node_id))
    }

    /// Get E2EE session state with a peer
    pub fn peer_e2ee_session_state(&self, peer_node_id: NodeId) -> Option<SessionState> {
        self.peer_sessions
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|s| s.session_state(peer_node_id))
    }

    /// Send an E2EE encrypted message to a specific peer
    ///
    /// Returns the encrypted message bytes to send, or None if no session exists.
    /// The message should be sent directly to the peer (not broadcast).
    pub fn send_peer_e2ee(
        &self,
        peer_node_id: NodeId,
        plaintext: &[u8],
        now_ms: u64,
    ) -> Option<Vec<u8>> {
        let mut sessions = self.peer_sessions.lock().unwrap();
        let session_mgr = sessions.as_mut()?;

        match session_mgr.encrypt_for_peer(peer_node_id, plaintext, now_ms) {
            Ok(encrypted) => {
                let mut buf = Vec::with_capacity(2 + encrypted.encode().len());
                buf.push(PEER_E2EE_MARKER);
                buf.push(0x00); // reserved
                buf.extend_from_slice(&encrypted.encode());
                Some(buf)
            }
            Err(e) => {
                log::warn!(
                    "Failed to encrypt for peer {:08X}: {:?}",
                    peer_node_id.as_u32(),
                    e
                );
                None
            }
        }
    }

    /// Close E2EE session with a peer
    pub fn close_peer_e2ee(&self, peer_node_id: NodeId) {
        let mut sessions = self.peer_sessions.lock().unwrap();
        if let Some(session_mgr) = sessions.as_mut() {
            session_mgr.close_session(peer_node_id);
            self.notify(HiveEvent::PeerE2eeClosed { peer_node_id });
            log::info!(
                "Closed E2EE session with peer {:08X}",
                peer_node_id.as_u32()
            );
        }
    }

    /// Get count of active E2EE sessions
    pub fn peer_e2ee_session_count(&self) -> usize {
        self.peer_sessions
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.session_count())
            .unwrap_or(0)
    }

    /// Get count of established E2EE sessions
    pub fn peer_e2ee_established_count(&self) -> usize {
        self.peer_sessions
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.established_count())
            .unwrap_or(0)
    }

    /// Handle incoming key exchange message
    ///
    /// Called internally when we receive a KEY_EXCHANGE_MARKER message.
    /// Returns the response key exchange bytes to send back, or None if invalid.
    fn handle_key_exchange(&self, data: &[u8], now_ms: u64) -> Option<Vec<u8>> {
        if data.len() < 2 || data[0] != KEY_EXCHANGE_MARKER {
            return None;
        }

        let payload = &data[2..];
        let msg = KeyExchangeMessage::decode(payload)?;

        let mut sessions = self.peer_sessions.lock().unwrap();
        let session_mgr = sessions.as_mut()?;

        let (response, established) = session_mgr.handle_key_exchange(&msg, now_ms)?;

        if established {
            self.notify(HiveEvent::PeerE2eeEstablished {
                peer_node_id: msg.sender_node_id,
            });
            log::info!(
                "E2EE session established with peer {:08X}",
                msg.sender_node_id.as_u32()
            );
        }

        // Return response key exchange
        let mut buf = Vec::with_capacity(2 + 37);
        buf.push(KEY_EXCHANGE_MARKER);
        buf.push(0x00);
        buf.extend_from_slice(&response.encode());
        Some(buf)
    }

    /// Handle incoming E2EE encrypted message
    ///
    /// Called internally when we receive a PEER_E2EE_MARKER message.
    /// Decrypts and notifies observers of the received message.
    fn handle_peer_e2ee_message(&self, data: &[u8], now_ms: u64) -> Option<Vec<u8>> {
        if data.len() < 2 || data[0] != PEER_E2EE_MARKER {
            return None;
        }

        let payload = &data[2..];
        let msg = PeerEncryptedMessage::decode(payload)?;

        let mut sessions = self.peer_sessions.lock().unwrap();
        let session_mgr = sessions.as_mut()?;

        match session_mgr.decrypt_from_peer(&msg, now_ms) {
            Ok(plaintext) => {
                // Notify observers of the decrypted message
                self.notify(HiveEvent::PeerE2eeMessageReceived {
                    from_node: msg.sender_node_id,
                    data: plaintext.clone(),
                });
                Some(plaintext)
            }
            Err(e) => {
                log::warn!(
                    "Failed to decrypt E2EE message from {:08X}: {:?}",
                    msg.sender_node_id.as_u32(),
                    e
                );
                None
            }
        }
    }

    // ==================== Configuration ====================

    /// Get our node ID
    pub fn node_id(&self) -> NodeId {
        self.config.node_id
    }

    /// Get our callsign
    pub fn callsign(&self) -> &str {
        &self.config.callsign
    }

    /// Get the mesh ID
    pub fn mesh_id(&self) -> &str {
        &self.config.mesh_id
    }

    /// Get the device name for BLE advertising
    pub fn device_name(&self) -> String {
        format!(
            "HIVE_{}-{:08X}",
            self.config.mesh_id,
            self.config.node_id.as_u32()
        )
    }

    /// Get a peer's callsign by node ID
    ///
    /// Returns the callsign from the peer's most recently received peripheral data,
    /// or None if no peripheral data has been received from this peer.
    pub fn get_peer_callsign(&self, node_id: NodeId) -> Option<String> {
        self.peer_peripherals.read().ok().and_then(|peripherals| {
            peripherals
                .get(&node_id)
                .map(|p| p.callsign_str().to_string())
        })
    }

    /// Get a peer's full peripheral data by node ID
    ///
    /// Returns a clone of the peripheral data from the peer's most recently received
    /// document, or None if no peripheral data has been received from this peer.
    pub fn get_peer_peripheral(&self, node_id: NodeId) -> Option<Peripheral> {
        self.peer_peripherals
            .read()
            .ok()
            .and_then(|peripherals| peripherals.get(&node_id).cloned())
    }

    // ==================== Observer Management ====================

    /// Add an observer for mesh events
    pub fn add_observer(&self, observer: Arc<dyn HiveObserver>) {
        self.observers.add(observer);
    }

    /// Remove an observer
    pub fn remove_observer(&self, observer: &Arc<dyn HiveObserver>) {
        self.observers.remove(observer);
    }

    // ==================== User Actions ====================

    /// Send an emergency alert
    ///
    /// Returns the document bytes to broadcast to all peers.
    /// If encryption is enabled, the document is encrypted.
    pub fn send_emergency(&self, timestamp: u64) -> Vec<u8> {
        let data = self.document_sync.send_emergency(timestamp);
        self.notify(HiveEvent::MeshStateChanged {
            peer_count: self.peer_manager.peer_count(),
            connected_count: self.peer_manager.connected_count(),
        });
        self.encrypt_document(&data)
    }

    /// Send an ACK response
    ///
    /// Returns the document bytes to broadcast to all peers.
    /// If encryption is enabled, the document is encrypted.
    pub fn send_ack(&self, timestamp: u64) -> Vec<u8> {
        let data = self.document_sync.send_ack(timestamp);
        self.notify(HiveEvent::MeshStateChanged {
            peer_count: self.peer_manager.peer_count(),
            connected_count: self.peer_manager.connected_count(),
        });
        self.encrypt_document(&data)
    }

    /// Clear the current event (emergency or ack)
    pub fn clear_event(&self) {
        self.document_sync.clear_event();
    }

    /// Check if emergency is active
    pub fn is_emergency_active(&self) -> bool {
        self.document_sync.is_emergency_active()
    }

    /// Check if ACK is active
    pub fn is_ack_active(&self) -> bool {
        self.document_sync.is_ack_active()
    }

    /// Get current event type
    pub fn current_event(&self) -> Option<EventType> {
        self.document_sync.current_event()
    }

    // ==================== Emergency Management (Document-Based) ====================

    /// Start a new emergency event with ACK tracking
    ///
    /// Creates an emergency event that tracks ACKs from all known peers.
    /// Pass the list of known peer node IDs to track.
    /// Returns the document bytes to broadcast.
    /// If encryption is enabled, the document is encrypted.
    pub fn start_emergency(&self, timestamp: u64, known_peers: &[u32]) -> Vec<u8> {
        let data = self.document_sync.start_emergency(timestamp, known_peers);
        self.notify(HiveEvent::MeshStateChanged {
            peer_count: self.peer_manager.peer_count(),
            connected_count: self.peer_manager.connected_count(),
        });
        self.encrypt_document(&data)
    }

    /// Start a new emergency using all currently known peers
    ///
    /// Convenience method that automatically includes all discovered peers.
    pub fn start_emergency_with_known_peers(&self, timestamp: u64) -> Vec<u8> {
        let peers: Vec<u32> = self
            .peer_manager
            .get_peers()
            .iter()
            .map(|p| p.node_id.as_u32())
            .collect();
        self.start_emergency(timestamp, &peers)
    }

    /// Record our ACK for the current emergency
    ///
    /// Returns the document bytes to broadcast, or None if no emergency is active.
    /// If encryption is enabled, the document is encrypted.
    pub fn ack_emergency(&self, timestamp: u64) -> Option<Vec<u8>> {
        let result = self.document_sync.ack_emergency(timestamp);
        if result.is_some() {
            self.notify(HiveEvent::MeshStateChanged {
                peer_count: self.peer_manager.peer_count(),
                connected_count: self.peer_manager.connected_count(),
            });
        }
        result.map(|data| self.encrypt_document(&data))
    }

    /// Clear the current emergency event
    pub fn clear_emergency(&self) {
        self.document_sync.clear_emergency();
    }

    /// Check if there's an active emergency
    pub fn has_active_emergency(&self) -> bool {
        self.document_sync.has_active_emergency()
    }

    /// Get emergency status info
    ///
    /// Returns (source_node, timestamp, acked_count, pending_count) if emergency is active.
    pub fn get_emergency_status(&self) -> Option<(u32, u64, usize, usize)> {
        self.document_sync.get_emergency_status()
    }

    /// Check if a specific peer has ACKed the current emergency
    pub fn has_peer_acked(&self, peer_id: u32) -> bool {
        self.document_sync.has_peer_acked(peer_id)
    }

    /// Check if all peers have ACKed the current emergency
    pub fn all_peers_acked(&self) -> bool {
        self.document_sync.all_peers_acked()
    }

    // ==================== Chat Methods ====================

    /// Send a chat message
    ///
    /// Adds the message to the local CRDT and returns the document bytes
    /// to broadcast to all peers. If encryption is enabled, the document is encrypted.
    ///
    /// Returns the encrypted document bytes if the message was new,
    /// or None if it was a duplicate.
    pub fn send_chat(&self, sender: &str, text: &str, timestamp: u64) -> Option<Vec<u8>> {
        if self.document_sync.add_chat_message(sender, text, timestamp) {
            Some(self.encrypt_document(&self.build_document()))
        } else {
            None
        }
    }

    /// Send a chat reply
    ///
    /// Adds the reply to the local CRDT with reply-to information and returns
    /// the document bytes to broadcast. If encryption is enabled, the document is encrypted.
    ///
    /// Returns the encrypted document bytes if the message was new,
    /// or None if it was a duplicate.
    pub fn send_chat_reply(
        &self,
        sender: &str,
        text: &str,
        reply_to_node: u32,
        reply_to_timestamp: u64,
        timestamp: u64,
    ) -> Option<Vec<u8>> {
        if self.document_sync.add_chat_reply(
            sender,
            text,
            reply_to_node,
            reply_to_timestamp,
            timestamp,
        ) {
            Some(self.encrypt_document(&self.build_document()))
        } else {
            None
        }
    }

    /// Get the number of chat messages in the local CRDT
    pub fn chat_count(&self) -> usize {
        self.document_sync.chat_count()
    }

    /// Get chat messages newer than a timestamp
    ///
    /// Returns a vector of (origin_node, timestamp, sender, text, reply_to_node, reply_to_timestamp) tuples.
    pub fn chat_messages_since(
        &self,
        since_timestamp: u64,
    ) -> Vec<(u32, u64, String, String, u32, u64)> {
        self.document_sync.chat_messages_since(since_timestamp)
    }

    /// Get all chat messages
    ///
    /// Returns a vector of (origin_node, timestamp, sender, text, reply_to_node, reply_to_timestamp) tuples.
    pub fn all_chat_messages(&self) -> Vec<(u32, u64, String, String, u32, u64)> {
        self.document_sync.all_chat_messages()
    }

    // ==================== BLE Callbacks (Platform -> Mesh) ====================

    /// Called when a BLE device is discovered
    ///
    /// Returns `Some(HivePeer)` if this is a new HIVE peer on our mesh.
    pub fn on_ble_discovered(
        &self,
        identifier: &str,
        name: Option<&str>,
        rssi: i8,
        mesh_id: Option<&str>,
        now_ms: u64,
    ) -> Option<HivePeer> {
        let (node_id, is_new) = self
            .peer_manager
            .on_discovered(identifier, name, rssi, mesh_id, now_ms)?;

        let peer = self.peer_manager.get_peer(node_id)?;

        // Update connection graph
        {
            let mut graph = self.connection_graph.lock().unwrap();
            graph.on_discovered(
                node_id,
                identifier.to_string(),
                name.map(|s| s.to_string()),
                mesh_id.map(|s| s.to_string()),
                rssi,
                now_ms,
            );
        }

        if is_new {
            self.notify(HiveEvent::PeerDiscovered { peer: peer.clone() });
            self.notify_mesh_state_changed();
        }

        Some(peer)
    }

    /// Called when a BLE connection is established (outgoing)
    ///
    /// Returns the NodeId if this identifier is known.
    pub fn on_ble_connected(&self, identifier: &str, now_ms: u64) -> Option<NodeId> {
        let node_id = self.peer_manager.on_connected(identifier, now_ms)?;

        // Update connection graph
        {
            let mut graph = self.connection_graph.lock().unwrap();
            graph.on_connected(node_id, now_ms);
        }

        self.notify(HiveEvent::PeerConnected { node_id });
        self.notify_mesh_state_changed();
        Some(node_id)
    }

    /// Called when a BLE connection is lost
    pub fn on_ble_disconnected(
        &self,
        identifier: &str,
        reason: DisconnectReason,
    ) -> Option<NodeId> {
        let (node_id, observer_reason) = self.peer_manager.on_disconnected(identifier, reason)?;

        // Update connection graph (convert observer reason to platform reason)
        {
            let mut graph = self.connection_graph.lock().unwrap();
            let platform_reason = match observer_reason {
                DisconnectReason::LocalRequest => crate::platform::DisconnectReason::LocalRequest,
                DisconnectReason::RemoteRequest => crate::platform::DisconnectReason::RemoteRequest,
                DisconnectReason::Timeout => crate::platform::DisconnectReason::Timeout,
                DisconnectReason::LinkLoss => crate::platform::DisconnectReason::LinkLoss,
                DisconnectReason::ConnectionFailed => {
                    crate::platform::DisconnectReason::ConnectionFailed
                }
                DisconnectReason::Unknown => crate::platform::DisconnectReason::Unknown,
            };
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            graph.on_disconnected(node_id, platform_reason, now_ms);
        }

        self.notify(HiveEvent::PeerDisconnected {
            node_id,
            reason: observer_reason,
        });
        self.notify_mesh_state_changed();
        Some(node_id)
    }

    /// Called when a BLE connection is lost, using NodeId directly
    ///
    /// Alternative to on_ble_disconnected() when only NodeId is known (e.g., ESP32).
    pub fn on_peer_disconnected(&self, node_id: NodeId, reason: DisconnectReason) {
        if self
            .peer_manager
            .on_disconnected_by_node_id(node_id, reason)
        {
            // Update connection graph
            {
                let mut graph = self.connection_graph.lock().unwrap();
                let platform_reason = match reason {
                    DisconnectReason::LocalRequest => {
                        crate::platform::DisconnectReason::LocalRequest
                    }
                    DisconnectReason::RemoteRequest => {
                        crate::platform::DisconnectReason::RemoteRequest
                    }
                    DisconnectReason::Timeout => crate::platform::DisconnectReason::Timeout,
                    DisconnectReason::LinkLoss => crate::platform::DisconnectReason::LinkLoss,
                    DisconnectReason::ConnectionFailed => {
                        crate::platform::DisconnectReason::ConnectionFailed
                    }
                    DisconnectReason::Unknown => crate::platform::DisconnectReason::Unknown,
                };
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                graph.on_disconnected(node_id, platform_reason, now_ms);
            }

            self.notify(HiveEvent::PeerDisconnected { node_id, reason });
            self.notify_mesh_state_changed();
        }
    }

    /// Called when a remote device connects to us (incoming connection)
    ///
    /// Use this when we're acting as a peripheral and a central connects to us.
    pub fn on_incoming_connection(&self, identifier: &str, node_id: NodeId, now_ms: u64) -> bool {
        let is_new = self
            .peer_manager
            .on_incoming_connection(identifier, node_id, now_ms);

        // Update connection graph
        {
            let mut graph = self.connection_graph.lock().unwrap();
            if is_new {
                graph.on_discovered(
                    node_id,
                    identifier.to_string(),
                    None,
                    Some(self.config.mesh_id.clone()),
                    -50, // Default good RSSI for incoming connections
                    now_ms,
                );
            }
            graph.on_connected(node_id, now_ms);
        }

        if is_new {
            if let Some(peer) = self.peer_manager.get_peer(node_id) {
                self.notify(HiveEvent::PeerDiscovered { peer });
            }
        }

        self.notify(HiveEvent::PeerConnected { node_id });
        self.notify_mesh_state_changed();

        is_new
    }

    /// Called when data is received from a peer
    ///
    /// Parses the document, merges it, and generates appropriate events.
    /// If encryption is enabled, decrypts the document first.
    /// Handles per-peer E2EE messages (KEY_EXCHANGE and PEER_E2EE markers).
    /// Returns the source NodeId and whether the document contained an event.
    pub fn on_ble_data_received(
        &self,
        identifier: &str,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        // Get node ID from identifier
        let node_id = self.peer_manager.get_node_id(identifier)?;

        // Check for special message types first
        if data.len() >= 2 {
            match data[0] {
                KEY_EXCHANGE_MARKER => {
                    // Handle key exchange - returns response to send back
                    let _response = self.handle_key_exchange(data, now_ms);
                    // Return None as this isn't a document sync
                    return None;
                }
                PEER_E2EE_MARKER => {
                    // Handle encrypted peer message
                    let _plaintext = self.handle_peer_e2ee_message(data, now_ms);
                    // Return None as this isn't a document sync
                    return None;
                }
                RELAY_ENVELOPE_MARKER => {
                    // Handle relay envelope for multi-hop
                    return self
                        .handle_relay_envelope_with_identifier(node_id, identifier, data, now_ms);
                }
                _ => {}
            }
        }

        // Direct document (not relay envelope)
        self.process_document_data_with_identifier(node_id, identifier, data, now_ms, None, None, 0)
    }

    /// Internal: Process document data with identifier as source hint
    #[allow(clippy::too_many_arguments)]
    fn process_document_data_with_identifier(
        &self,
        source_node: NodeId,
        identifier: &str,
        data: &[u8],
        now_ms: u64,
        relay_data: Option<Vec<u8>>,
        origin_node: Option<NodeId>,
        hop_count: u8,
    ) -> Option<DataReceivedResult> {
        // Decrypt if encrypted (mesh-wide encryption) - use identifier as source hint
        let decrypted = self.decrypt_document(data, Some(identifier))?;

        // Check if this is a delta document (wire format v2)
        if DeltaDocument::is_delta_document(&decrypted) {
            return self.process_delta_document_internal(
                source_node,
                &decrypted,
                now_ms,
                relay_data,
                origin_node,
                hop_count,
            );
        }

        // Merge the document (legacy wire format v1)
        let result = self.document_sync.merge_document(&decrypted)?;

        // Store peer peripheral if present (for callsign lookup)
        if let Some(ref peripheral) = result.peer_peripheral {
            if let Ok(mut peripherals) = self.peer_peripherals.write() {
                peripherals.insert(result.source_node, peripheral.clone());
            }
        }

        // Record sync
        self.peer_manager.record_sync(source_node, now_ms);

        // Generate events based on what was received
        if result.is_emergency() {
            self.notify(HiveEvent::EmergencyReceived {
                from_node: result.source_node,
            });
        } else if result.is_ack() {
            self.notify(HiveEvent::AckReceived {
                from_node: result.source_node,
            });
        }

        if result.counter_changed {
            self.notify(HiveEvent::DocumentSynced {
                from_node: result.source_node,
                total_count: result.total_count,
            });
        }

        // Emit relay event if we're relaying
        if relay_data.is_some() {
            let relay_targets = self.get_relay_targets(Some(source_node));
            self.notify(HiveEvent::MessageRelayed {
                origin_node: origin_node.unwrap_or(result.source_node),
                relay_count: relay_targets.len(),
                hop_count,
            });
        }

        let (callsign, battery_percent, heart_rate, event_type, latitude, longitude, altitude) =
            DataReceivedResult::peripheral_fields(&result.peer_peripheral);

        Some(DataReceivedResult {
            source_node: result.source_node,
            is_emergency: result.is_emergency(),
            is_ack: result.is_ack(),
            counter_changed: result.counter_changed,
            emergency_changed: result.emergency_changed,
            total_count: result.total_count,
            event_timestamp: result.event.as_ref().map(|e| e.timestamp).unwrap_or(0),
            relay_data,
            origin_node,
            hop_count,
            callsign,
            battery_percent,
            heart_rate,
            event_type,
            latitude,
            longitude,
            altitude,
        })
    }

    /// Internal: Handle relay envelope with identifier as source hint
    fn handle_relay_envelope_with_identifier(
        &self,
        source_node: NodeId,
        identifier: &str,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        // Process the relay envelope
        let envelope = RelayEnvelope::decode(data)?;

        // Check deduplication
        if !self.mark_message_seen(envelope.message_id, envelope.origin_node, now_ms) {
            let stats = self
                .seen_cache
                .lock()
                .unwrap()
                .get_stats(&envelope.message_id);
            let seen_count = stats.map(|(_, count, _)| count).unwrap_or(1);

            self.notify(HiveEvent::DuplicateMessageDropped {
                origin_node: envelope.origin_node,
                seen_count,
            });
            return None;
        }

        // Check TTL and get relay data
        let relay_data = if envelope.can_relay() && self.config.enable_relay {
            envelope.relay().map(|e| e.encode())
        } else {
            if !envelope.can_relay() {
                self.notify(HiveEvent::MessageTtlExpired {
                    origin_node: envelope.origin_node,
                    hop_count: envelope.hop_count,
                });
            }
            None
        };

        // Process the inner payload
        self.process_document_data_with_identifier(
            source_node,
            identifier,
            &envelope.payload,
            now_ms,
            relay_data,
            Some(envelope.origin_node),
            envelope.hop_count,
        )
    }

    /// Called when data is received but we don't have the identifier mapped
    ///
    /// Use this when receiving data from a peripheral we discovered.
    /// If encryption is enabled, decrypts the document first.
    /// Handles per-peer E2EE messages (KEY_EXCHANGE and PEER_E2EE markers).
    /// Handles relay envelopes for multi-hop mesh operation.
    pub fn on_ble_data_received_from_node(
        &self,
        node_id: NodeId,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        // Check for special message types first
        if data.len() >= 2 {
            match data[0] {
                KEY_EXCHANGE_MARKER => {
                    let _response = self.handle_key_exchange(data, now_ms);
                    return None;
                }
                PEER_E2EE_MARKER => {
                    let _plaintext = self.handle_peer_e2ee_message(data, now_ms);
                    return None;
                }
                RELAY_ENVELOPE_MARKER => {
                    // Handle relay envelope for multi-hop
                    return self.handle_relay_envelope(node_id, data, now_ms);
                }
                _ => {}
            }
        }

        // Direct document (not relay envelope)
        self.process_document_data(node_id, data, now_ms, None, None, 0)
    }

    /// Called when encrypted data is received from an unknown peer
    ///
    /// This handles the case where we receive an encrypted document from a BLE address
    /// that isn't registered in our peer manager (e.g., due to BLE address rotation).
    /// The function decrypts first using the mesh key, then extracts the source_node
    /// from the decrypted document header and registers the peer.
    ///
    /// Returns `Some(DataReceivedResult)` if decryption and processing succeed.
    /// Returns `None` if decryption fails or the document is invalid.
    pub fn on_ble_data_received_anonymous(
        &self,
        identifier: &str,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        // Only handle encrypted documents with this path
        if data.len() < 10 || data[0] != ENCRYPTED_MARKER {
            log::debug!("on_ble_data_received_anonymous: not an encrypted document");
            return None;
        }

        // Try to decrypt using mesh key
        let decrypted = self.decrypt_document(data, Some(identifier))?;

        // Extract source_node from decrypted document header
        // Header format: [version: 4 bytes (LE)][node_id: 4 bytes (LE)]
        if decrypted.len() < 8 {
            log::warn!("Decrypted document too short to extract source_node");
            return None;
        }

        let source_node_u32 =
            u32::from_le_bytes([decrypted[4], decrypted[5], decrypted[6], decrypted[7]]);
        let source_node = NodeId::new(source_node_u32);

        log::info!(
            "Anonymous document from {}: decrypted, source_node={:08X}",
            identifier,
            source_node_u32
        );

        // Register the peer with this identifier so future lookups work
        // This handles BLE address rotation
        self.peer_manager
            .register_identifier(identifier, source_node);

        // Check if this is a delta document
        let is_delta = DeltaDocument::is_delta_document(&decrypted);
        log::info!(
            "Document format: delta={}, first_byte=0x{:02X}, len={}",
            is_delta,
            decrypted.first().copied().unwrap_or(0),
            decrypted.len()
        );

        if is_delta {
            return self.process_delta_document_internal(
                source_node,
                &decrypted,
                now_ms,
                None,
                None,
                0,
            );
        }

        // Merge the document (legacy wire format v1)
        log::info!(
            "Processing legacy document from {:08X}",
            source_node.as_u32()
        );
        let result = self.document_sync.merge_document(&decrypted)?;

        // Log what we got from the merge
        log::info!(
            "Merge result: peer_peripheral={}, counter_changed={}",
            result.peer_peripheral.is_some(),
            result.counter_changed
        );
        if let Some(ref p) = result.peer_peripheral {
            log::info!("Peripheral callsign: '{}'", p.callsign_str());
        }

        // Record sync
        self.peer_manager.record_sync(source_node, now_ms);

        // Generate events
        if result.is_emergency() {
            self.notify(HiveEvent::EmergencyReceived {
                from_node: result.source_node,
            });
        } else if result.is_ack() {
            self.notify(HiveEvent::AckReceived {
                from_node: result.source_node,
            });
        }

        if result.counter_changed {
            self.notify(HiveEvent::DocumentSynced {
                from_node: result.source_node,
                total_count: result.total_count,
            });
        }

        let (callsign, battery_percent, heart_rate, event_type, latitude, longitude, altitude) =
            DataReceivedResult::peripheral_fields(&result.peer_peripheral);

        Some(DataReceivedResult {
            source_node: result.source_node,
            is_emergency: result.is_emergency(),
            is_ack: result.is_ack(),
            counter_changed: result.counter_changed,
            emergency_changed: result.emergency_changed,
            total_count: result.total_count,
            event_timestamp: result.event.as_ref().map(|e| e.timestamp).unwrap_or(0),
            relay_data: None,
            origin_node: None,
            hop_count: 0,
            callsign,
            battery_percent,
            heart_rate,
            event_type,
            latitude,
            longitude,
            altitude,
        })
    }

    /// Internal: Process document data (shared by direct and relay paths)
    fn process_document_data(
        &self,
        source_node: NodeId,
        data: &[u8],
        now_ms: u64,
        relay_data: Option<Vec<u8>>,
        origin_node: Option<NodeId>,
        hop_count: u8,
    ) -> Option<DataReceivedResult> {
        // Decrypt if encrypted (mesh-wide encryption)
        let source_hint = format!("node:{:08X}", source_node.as_u32());
        let decrypted = self.decrypt_document(data, Some(&source_hint))?;

        // Check if this is a delta document (wire format v2)
        if DeltaDocument::is_delta_document(&decrypted) {
            return self.process_delta_document_internal(
                source_node,
                &decrypted,
                now_ms,
                relay_data,
                origin_node,
                hop_count,
            );
        }

        // Merge the document (legacy wire format v1)
        let result = self.document_sync.merge_document(&decrypted)?;

        // Store peer peripheral if present (for callsign lookup)
        if let Some(ref peripheral) = result.peer_peripheral {
            if let Ok(mut peripherals) = self.peer_peripherals.write() {
                peripherals.insert(result.source_node, peripheral.clone());
            }
        }

        // Record sync
        self.peer_manager.record_sync(source_node, now_ms);

        // Generate events based on what was received
        if result.is_emergency() {
            self.notify(HiveEvent::EmergencyReceived {
                from_node: result.source_node,
            });
        } else if result.is_ack() {
            self.notify(HiveEvent::AckReceived {
                from_node: result.source_node,
            });
        }

        if result.counter_changed {
            self.notify(HiveEvent::DocumentSynced {
                from_node: result.source_node,
                total_count: result.total_count,
            });
        }

        // Emit relay event if we're relaying
        if relay_data.is_some() {
            let relay_targets = self.get_relay_targets(Some(source_node));
            self.notify(HiveEvent::MessageRelayed {
                origin_node: origin_node.unwrap_or(result.source_node),
                relay_count: relay_targets.len(),
                hop_count,
            });
        }

        let (callsign, battery_percent, heart_rate, event_type, latitude, longitude, altitude) =
            DataReceivedResult::peripheral_fields(&result.peer_peripheral);

        Some(DataReceivedResult {
            source_node: result.source_node,
            is_emergency: result.is_emergency(),
            is_ack: result.is_ack(),
            counter_changed: result.counter_changed,
            emergency_changed: result.emergency_changed,
            total_count: result.total_count,
            event_timestamp: result.event.as_ref().map(|e| e.timestamp).unwrap_or(0),
            relay_data,
            origin_node,
            hop_count,
            callsign,
            battery_percent,
            heart_rate,
            event_type,
            latitude,
            longitude,
            altitude,
        })
    }

    /// Internal: Handle relay envelope
    fn handle_relay_envelope(
        &self,
        source_node: NodeId,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        // Process the relay envelope
        let decision = self.process_relay_envelope(data, source_node, now_ms)?;

        // Get relay data if we should relay
        let relay_data = if decision.should_relay {
            decision.relay_data()
        } else {
            None
        };

        // Process the inner payload
        self.process_document_data(
            source_node,
            &decision.payload,
            now_ms,
            relay_data,
            Some(decision.origin_node),
            decision.hop_count,
        )
    }

    /// Called when data is received without a known identifier
    ///
    /// This is the simplest data receive method - it extracts the source node_id
    /// from the document itself. Use this when you don't track identifiers
    /// (e.g., ESP32 NimBLE).
    /// If encryption is enabled, decrypts the document first.
    /// Handles per-peer E2EE messages (KEY_EXCHANGE and PEER_E2EE markers).
    /// Handles relay envelopes for multi-hop mesh operation.
    pub fn on_ble_data(
        &self,
        identifier: &str,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        // Check for special message types first
        if data.len() >= 2 {
            match data[0] {
                KEY_EXCHANGE_MARKER => {
                    let _response = self.handle_key_exchange(data, now_ms);
                    return None;
                }
                PEER_E2EE_MARKER => {
                    let _plaintext = self.handle_peer_e2ee_message(data, now_ms);
                    return None;
                }
                RELAY_ENVELOPE_MARKER => {
                    // Handle relay envelope - extract origin from envelope
                    return self.handle_relay_envelope_with_incoming(identifier, data, now_ms);
                }
                _ => {}
            }
        }

        // Direct document - process normally
        self.process_incoming_document(identifier, data, now_ms, None, None, 0)
    }

    /// Internal: Process incoming document (handles peer registration)
    fn process_incoming_document(
        &self,
        identifier: &str,
        data: &[u8],
        now_ms: u64,
        relay_data: Option<Vec<u8>>,
        origin_node: Option<NodeId>,
        hop_count: u8,
    ) -> Option<DataReceivedResult> {
        // Decrypt if encrypted (mesh-wide encryption)
        let decrypted = self.decrypt_document(data, Some(identifier))?;

        // Merge the document (extracts node_id internally)
        let result = self.document_sync.merge_document(&decrypted)?;

        // Record sync using the source_node from the merged document
        self.peer_manager.record_sync(result.source_node, now_ms);

        // Add the peer if not already known (creates peer entry from document data)
        self.peer_manager
            .on_incoming_connection(identifier, result.source_node, now_ms);

        // Generate events based on what was received
        if result.is_emergency() {
            self.notify(HiveEvent::EmergencyReceived {
                from_node: result.source_node,
            });
        } else if result.is_ack() {
            self.notify(HiveEvent::AckReceived {
                from_node: result.source_node,
            });
        }

        if result.counter_changed {
            self.notify(HiveEvent::DocumentSynced {
                from_node: result.source_node,
                total_count: result.total_count,
            });
        }

        // Emit relay event if we're relaying
        if relay_data.is_some() {
            let relay_targets = self.get_relay_targets(Some(result.source_node));
            self.notify(HiveEvent::MessageRelayed {
                origin_node: origin_node.unwrap_or(result.source_node),
                relay_count: relay_targets.len(),
                hop_count,
            });
        }

        let (callsign, battery_percent, heart_rate, event_type, latitude, longitude, altitude) =
            DataReceivedResult::peripheral_fields(&result.peer_peripheral);

        Some(DataReceivedResult {
            source_node: result.source_node,
            is_emergency: result.is_emergency(),
            is_ack: result.is_ack(),
            counter_changed: result.counter_changed,
            emergency_changed: result.emergency_changed,
            total_count: result.total_count,
            event_timestamp: result.event.as_ref().map(|e| e.timestamp).unwrap_or(0),
            relay_data,
            origin_node,
            hop_count,
            callsign,
            battery_percent,
            heart_rate,
            event_type,
            latitude,
            longitude,
            altitude,
        })
    }

    /// Internal: Handle relay envelope with incoming connection registration
    fn handle_relay_envelope_with_incoming(
        &self,
        identifier: &str,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        // Parse envelope to get origin
        let envelope = RelayEnvelope::decode(data)?;

        // Check deduplication
        if !self.mark_message_seen(envelope.message_id, envelope.origin_node, now_ms) {
            // Duplicate - get stats for event
            let stats = self
                .seen_cache
                .lock()
                .unwrap()
                .get_stats(&envelope.message_id);
            let seen_count = stats.map(|(_, count, _)| count).unwrap_or(1);

            self.notify(HiveEvent::DuplicateMessageDropped {
                origin_node: envelope.origin_node,
                seen_count,
            });
            return None;
        }

        // Check TTL
        let (should_relay, relay_data) = if envelope.can_relay() && self.config.enable_relay {
            let relay_env = envelope.relay();
            (true, relay_env.map(|e| e.encode()))
        } else {
            if !envelope.can_relay() {
                self.notify(HiveEvent::MessageTtlExpired {
                    origin_node: envelope.origin_node,
                    hop_count: envelope.hop_count,
                });
            }
            (false, None)
        };

        // Process the inner payload
        self.process_incoming_document(
            identifier,
            &envelope.payload,
            now_ms,
            if should_relay { relay_data } else { None },
            Some(envelope.origin_node),
            envelope.hop_count,
        )
    }

    // ==================== Periodic Maintenance ====================

    /// Periodic tick - call this regularly (e.g., every second)
    ///
    /// Performs:
    /// - Stale peer cleanup
    /// - Periodic sync broadcast (if interval elapsed)
    ///
    /// Returns `Some(data)` if a sync broadcast is needed.
    pub fn tick(&self, now_ms: u64) -> Option<Vec<u8>> {
        use std::sync::atomic::Ordering;

        // Use u32 for atomic storage (wraps every ~49 days, intervals still work)
        let now_ms_32 = now_ms as u32;

        // Cleanup stale peers
        let last_cleanup = self.last_cleanup_ms.load(Ordering::Relaxed);
        let cleanup_elapsed = now_ms_32.wrapping_sub(last_cleanup);
        if cleanup_elapsed >= self.config.peer_config.cleanup_interval_ms as u32 {
            self.last_cleanup_ms.store(now_ms_32, Ordering::Relaxed);
            let removed = self.peer_manager.cleanup_stale(now_ms);
            for node_id in &removed {
                self.notify(HiveEvent::PeerLost { node_id: *node_id });
            }
            if !removed.is_empty() {
                self.notify_mesh_state_changed();
            }

            // Run connection graph maintenance (transition Disconnected -> Lost)
            {
                let mut graph = self.connection_graph.lock().unwrap();
                let newly_lost = graph.tick(now_ms);
                // Also cleanup peers lost for more than peer_timeout
                graph.cleanup_lost(self.config.peer_config.peer_timeout_ms, now_ms);
                drop(graph);

                // Emit PeerLost events for newly lost peers from graph
                // (these may differ from peer_manager removals)
                for node_id in newly_lost {
                    // Only notify if not already notified by peer_manager
                    if !removed.contains(&node_id) {
                        self.notify(HiveEvent::PeerLost { node_id });
                    }
                }
            }
        }

        // Check if sync broadcast is needed
        let last_sync = self.last_sync_ms.load(Ordering::Relaxed);
        let sync_elapsed = now_ms_32.wrapping_sub(last_sync);
        if sync_elapsed >= self.config.sync_interval_ms as u32 {
            self.last_sync_ms.store(now_ms_32, Ordering::Relaxed);
            // Only broadcast if we have connected peers
            if self.peer_manager.connected_count() > 0 {
                let doc = self.document_sync.build_document();
                return Some(self.encrypt_document(&doc));
            }
        }

        None
    }

    // ==================== State Queries ====================

    /// Get all known peers
    pub fn get_peers(&self) -> Vec<HivePeer> {
        self.peer_manager.get_peers()
    }

    /// Get connected peers only
    pub fn get_connected_peers(&self) -> Vec<HivePeer> {
        self.peer_manager.get_connected_peers()
    }

    /// Get a specific peer by NodeId
    pub fn get_peer(&self, node_id: NodeId) -> Option<HivePeer> {
        self.peer_manager.get_peer(node_id)
    }

    /// Get peer count
    pub fn peer_count(&self) -> usize {
        self.peer_manager.peer_count()
    }

    /// Get connected peer count
    pub fn connected_count(&self) -> usize {
        self.peer_manager.connected_count()
    }

    /// Check if a device mesh ID matches our mesh
    pub fn matches_mesh(&self, device_mesh_id: Option<&str>) -> bool {
        self.peer_manager.matches_mesh(device_mesh_id)
    }

    // ==================== Connection State Graph ====================

    /// Get the connection state graph with all peer states
    ///
    /// Returns a snapshot of all tracked peers and their connection lifecycle state.
    /// Apps can use this to display appropriate UI indicators:
    /// - Green for Connected peers
    /// - Yellow for Degraded or RecentlyDisconnected peers
    /// - Gray for Lost peers
    ///
    /// # Example
    /// ```ignore
    /// let states = mesh.get_connection_graph();
    /// for peer in states {
    ///     match peer.state {
    ///         ConnectionState::Connected => show_green_indicator(&peer),
    ///         ConnectionState::Degraded => show_yellow_indicator(&peer),
    ///         ConnectionState::Disconnected => show_stale_indicator(&peer),
    ///         ConnectionState::Lost => show_gray_indicator(&peer),
    ///         _ => {}
    ///     }
    /// }
    /// ```
    pub fn get_connection_graph(&self) -> Vec<PeerConnectionState> {
        self.connection_graph.lock().unwrap().get_all_owned()
    }

    /// Get a specific peer's connection state
    pub fn get_peer_connection_state(&self, node_id: NodeId) -> Option<PeerConnectionState> {
        self.connection_graph
            .lock()
            .unwrap()
            .get_peer(node_id)
            .cloned()
    }

    /// Get all currently connected peers from the connection graph
    pub fn get_connected_states(&self) -> Vec<PeerConnectionState> {
        self.connection_graph
            .lock()
            .unwrap()
            .get_connected()
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get peers in degraded state (connected but poor signal quality)
    pub fn get_degraded_peers(&self) -> Vec<PeerConnectionState> {
        self.connection_graph
            .lock()
            .unwrap()
            .get_degraded()
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get peers that disconnected within the specified time window
    ///
    /// Useful for showing "stale" peers that were recently connected.
    pub fn get_recently_disconnected(
        &self,
        within_ms: u64,
        now_ms: u64,
    ) -> Vec<PeerConnectionState> {
        self.connection_graph
            .lock()
            .unwrap()
            .get_recently_disconnected(within_ms, now_ms)
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get peers in Lost state (disconnected and no longer advertising)
    pub fn get_lost_peers(&self) -> Vec<PeerConnectionState> {
        self.connection_graph
            .lock()
            .unwrap()
            .get_lost()
            .into_iter()
            .cloned()
            .collect()
    }

    /// Get summary counts of peers in each connection state
    pub fn get_connection_state_counts(&self) -> StateCountSummary {
        self.connection_graph.lock().unwrap().state_counts()
    }

    // ==================== Indirect Peer Methods ====================

    /// Get all indirect (multi-hop) peers
    ///
    /// Returns peers discovered via relay messages that are not directly
    /// connected via BLE. Each indirect peer includes the minimum hop count
    /// and the direct peers through which they can be reached.
    pub fn get_indirect_peers(&self) -> Vec<IndirectPeer> {
        self.connection_graph
            .lock()
            .unwrap()
            .get_indirect_peers_owned()
    }

    /// Get the degree (hop count) for a specific peer
    ///
    /// Returns:
    /// - `Some(PeerDegree::Direct)` for directly connected BLE peers
    /// - `Some(PeerDegree::OneHop/TwoHop/ThreeHop)` for indirect peers
    /// - `None` if peer is not known
    pub fn get_peer_degree(&self, node_id: NodeId) -> Option<PeerDegree> {
        self.connection_graph.lock().unwrap().peer_degree(node_id)
    }

    /// Get full state counts including indirect peers
    ///
    /// Returns counts of direct peers by connection state plus counts
    /// of indirect peers by hop count (1-hop, 2-hop, 3-hop).
    pub fn get_full_state_counts(&self) -> FullStateCountSummary {
        self.connection_graph.lock().unwrap().full_state_counts()
    }

    /// Get all paths to reach an indirect peer
    ///
    /// Returns a list of (via_peer_id, hop_count) pairs showing all
    /// known routes to the specified peer.
    pub fn get_paths_to_peer(&self, node_id: NodeId) -> Vec<(NodeId, u8)> {
        self.connection_graph.lock().unwrap().get_paths_to(node_id)
    }

    /// Check if a node is known (either direct or indirect)
    pub fn is_peer_known(&self, node_id: NodeId) -> bool {
        self.connection_graph.lock().unwrap().is_known(node_id)
    }

    /// Get number of indirect peers
    pub fn indirect_peer_count(&self) -> usize {
        self.connection_graph.lock().unwrap().indirect_peer_count()
    }

    /// Cleanup stale indirect peers
    ///
    /// Removes indirect peers that haven't been seen within the timeout.
    /// Returns the list of removed peer IDs.
    pub fn cleanup_indirect_peers(&self, now_ms: u64) -> Vec<NodeId> {
        self.connection_graph
            .lock()
            .unwrap()
            .cleanup_indirect(now_ms)
    }

    /// Get total counter value
    pub fn total_count(&self) -> u64 {
        self.document_sync.total_count()
    }

    /// Get document version
    pub fn document_version(&self) -> u32 {
        self.document_sync.version()
    }

    /// Get document version (alias)
    pub fn version(&self) -> u32 {
        self.document_sync.version()
    }

    /// Update health status (battery percentage)
    pub fn update_health(&self, battery_percent: u8) {
        self.document_sync.update_health(battery_percent);
    }

    /// Update activity level (0=still, 1=walking, 2=running, 3=fall)
    pub fn update_activity(&self, activity: u8) {
        self.document_sync.update_activity(activity);
    }

    /// Update full health status (battery and activity)
    pub fn update_health_full(&self, battery_percent: u8, activity: u8) {
        self.document_sync
            .update_health_full(battery_percent, activity);
    }

    /// Update heart rate
    pub fn update_heart_rate(&self, heart_rate: u8) {
        self.document_sync.update_heart_rate(heart_rate);
    }

    /// Update location
    pub fn update_location(&self, latitude: f32, longitude: f32, altitude: Option<f32>) {
        self.document_sync
            .update_location(latitude, longitude, altitude);
    }

    /// Clear location
    pub fn clear_location(&self) {
        self.document_sync.clear_location();
    }

    /// Update callsign
    pub fn update_callsign(&self, callsign: &str) {
        self.document_sync.update_callsign(callsign);
    }

    /// Set peripheral event type
    pub fn set_peripheral_event(&self, event_type: EventType, timestamp: u64) {
        self.document_sync
            .set_peripheral_event(event_type, timestamp);
    }

    /// Clear peripheral event
    pub fn clear_peripheral_event(&self) {
        self.document_sync.clear_peripheral_event();
    }

    /// Update full peripheral state in one call
    ///
    /// This is the most efficient way to update all peripheral data before
    /// calling `build_document()` for encrypted transmission.
    #[allow(clippy::too_many_arguments)]
    pub fn update_peripheral_state(
        &self,
        callsign: &str,
        battery_percent: u8,
        heart_rate: Option<u8>,
        latitude: Option<f32>,
        longitude: Option<f32>,
        altitude: Option<f32>,
        event_type: Option<EventType>,
        timestamp: u64,
    ) {
        self.document_sync.update_peripheral_state(
            callsign,
            battery_percent,
            heart_rate,
            latitude,
            longitude,
            altitude,
            event_type,
            timestamp,
        );
    }

    /// Build current document for transmission
    ///
    /// If encryption is enabled, the document is encrypted.
    pub fn build_document(&self) -> Vec<u8> {
        let doc = self.document_sync.build_document();
        self.encrypt_document(&doc)
    }

    /// Get peers that should be synced with
    pub fn peers_needing_sync(&self, now_ms: u64) -> Vec<HivePeer> {
        self.peer_manager.peers_needing_sync(now_ms)
    }

    // ==================== Internal Helpers ====================

    fn notify(&self, event: HiveEvent) {
        self.observers.notify(event);
    }

    fn notify_mesh_state_changed(&self) {
        self.notify(HiveEvent::MeshStateChanged {
            peer_count: self.peer_manager.peer_count(),
            connected_count: self.peer_manager.connected_count(),
        });
    }
}

/// Result from receiving BLE data
#[derive(Debug, Clone)]
pub struct DataReceivedResult {
    /// Node that sent this data
    pub source_node: NodeId,

    /// Whether this contained an emergency event
    pub is_emergency: bool,

    /// Whether this contained an ACK event
    pub is_ack: bool,

    /// Whether the counter changed (new data)
    pub counter_changed: bool,

    /// Whether emergency state changed (new emergency or ACK updates)
    pub emergency_changed: bool,

    /// Updated total count
    pub total_count: u64,

    /// Event timestamp (if event present) - use to detect duplicate events
    pub event_timestamp: u64,

    /// Data to relay to other peers (if multi-hop relay is enabled)
    ///
    /// When present, the platform adapter should send this data to peers
    /// returned by `get_relay_targets(Some(source_node))`.
    pub relay_data: Option<Vec<u8>>,

    /// Origin node for relay (may differ from source_node for relayed messages)
    pub origin_node: Option<NodeId>,

    /// Current hop count (for relayed messages)
    pub hop_count: u8,

    // ========== Peripheral data from sender ==========
    /// Sender's callsign (up to 12 chars)
    pub callsign: Option<String>,

    /// Sender's battery percentage (0-100)
    pub battery_percent: Option<u8>,

    /// Sender's heart rate (BPM)
    pub heart_rate: Option<u8>,

    /// Sender's event type (from PeripheralEvent)
    pub event_type: Option<u8>,

    /// Sender's latitude
    pub latitude: Option<f32>,

    /// Sender's longitude
    pub longitude: Option<f32>,

    /// Sender's altitude (meters)
    pub altitude: Option<f32>,
}

impl DataReceivedResult {
    /// Extract peripheral fields from an Option<Peripheral>
    #[allow(clippy::type_complexity)]
    fn peripheral_fields(
        peripheral: &Option<crate::sync::crdt::Peripheral>,
    ) -> (
        Option<String>,
        Option<u8>,
        Option<u8>,
        Option<u8>,
        Option<f32>,
        Option<f32>,
        Option<f32>,
    ) {
        match peripheral {
            Some(p) => {
                let callsign = {
                    let s = p.callsign_str();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s.to_string())
                    }
                };
                let battery = if p.health.battery_percent > 0 {
                    Some(p.health.battery_percent)
                } else {
                    None
                };
                let heart_rate = p.health.heart_rate;
                let event_type = p.last_event.as_ref().map(|e| e.event_type as u8);
                let (lat, lon, alt) = match &p.location {
                    Some(loc) => (Some(loc.latitude), Some(loc.longitude), loc.altitude),
                    None => (None, None, None),
                };
                (callsign, battery, heart_rate, event_type, lat, lon, alt)
            }
            None => (None, None, None, None, None, None, None),
        }
    }
}

/// Decision from processing a relay envelope
#[derive(Debug, Clone)]
pub struct RelayDecision {
    /// The payload (document) to process locally
    pub payload: Vec<u8>,

    /// Original sender of the message
    pub origin_node: NodeId,

    /// Current hop count
    pub hop_count: u8,

    /// Whether this message should be relayed to other peers
    pub should_relay: bool,

    /// The relay envelope to forward (with incremented hop count)
    ///
    /// Only present if `should_relay` is true and TTL not expired.
    pub relay_envelope: Option<RelayEnvelope>,
}

impl RelayDecision {
    /// Get the relay data to send to peers
    ///
    /// Returns None if relay is not needed.
    pub fn relay_data(&self) -> Option<Vec<u8>> {
        self.relay_envelope.as_ref().map(|e| e.encode())
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;
    use crate::observer::CollectingObserver;

    // Valid timestamp for testing (2024-01-15 00:00:00 UTC)
    const TEST_TIMESTAMP: u64 = 1705276800000;

    fn create_mesh(node_id: u32, callsign: &str) -> HiveMesh {
        let config = HiveMeshConfig::new(NodeId::new(node_id), callsign, "TEST");
        HiveMesh::new(config)
    }

    #[test]
    fn test_mesh_creation() {
        let mesh = create_mesh(0x12345678, "ALPHA-1");

        assert_eq!(mesh.node_id().as_u32(), 0x12345678);
        assert_eq!(mesh.callsign(), "ALPHA-1");
        assert_eq!(mesh.mesh_id(), "TEST");
        assert_eq!(mesh.device_name(), "HIVE_TEST-12345678");
    }

    #[test]
    fn test_peer_discovery() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");
        let observer = Arc::new(CollectingObserver::new());
        mesh.add_observer(observer.clone());

        // Discover a peer
        let peer = mesh.on_ble_discovered(
            "device-uuid",
            Some("HIVE_TEST-22222222"),
            -65,
            Some("TEST"),
            1000,
        );

        assert!(peer.is_some());
        let peer = peer.unwrap();
        assert_eq!(peer.node_id.as_u32(), 0x22222222);

        // Check events were generated
        let events = observer.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerDiscovered { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::MeshStateChanged { .. })));
    }

    #[test]
    fn test_connection_lifecycle() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");
        let observer = Arc::new(CollectingObserver::new());
        mesh.add_observer(observer.clone());

        // Discover and connect
        mesh.on_ble_discovered(
            "device-uuid",
            Some("HIVE_TEST-22222222"),
            -65,
            Some("TEST"),
            1000,
        );

        let node_id = mesh.on_ble_connected("device-uuid", 2000);
        assert_eq!(node_id, Some(NodeId::new(0x22222222)));
        assert_eq!(mesh.connected_count(), 1);

        // Disconnect
        let node_id = mesh.on_ble_disconnected("device-uuid", DisconnectReason::RemoteRequest);
        assert_eq!(node_id, Some(NodeId::new(0x22222222)));
        assert_eq!(mesh.connected_count(), 0);

        // Check events
        let events = observer.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerConnected { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerDisconnected { .. })));
    }

    #[test]
    fn test_emergency_flow() {
        let mesh1 = create_mesh(0x11111111, "ALPHA-1");
        let mesh2 = create_mesh(0x22222222, "BRAVO-1");

        let observer2 = Arc::new(CollectingObserver::new());
        mesh2.add_observer(observer2.clone());

        // mesh1 sends emergency
        let doc = mesh1.send_emergency(TEST_TIMESTAMP);
        assert!(mesh1.is_emergency_active());

        // mesh2 receives it
        let result =
            mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, TEST_TIMESTAMP);

        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.is_emergency);
        assert_eq!(result.source_node.as_u32(), 0x11111111);

        // Check events on mesh2
        let events = observer2.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::EmergencyReceived { .. })));
    }

    #[test]
    fn test_ack_flow() {
        let mesh1 = create_mesh(0x11111111, "ALPHA-1");
        let mesh2 = create_mesh(0x22222222, "BRAVO-1");

        let observer2 = Arc::new(CollectingObserver::new());
        mesh2.add_observer(observer2.clone());

        // mesh1 sends ACK
        let doc = mesh1.send_ack(TEST_TIMESTAMP);
        assert!(mesh1.is_ack_active());

        // mesh2 receives it
        let result =
            mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, TEST_TIMESTAMP);

        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.is_ack);

        // Check events on mesh2
        let events = observer2.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::AckReceived { .. })));
    }

    #[test]
    fn test_tick_cleanup() {
        let config = HiveMeshConfig::new(NodeId::new(0x11111111), "ALPHA-1", "TEST")
            .with_peer_timeout(10_000);
        let mesh = HiveMesh::new(config);

        let observer = Arc::new(CollectingObserver::new());
        mesh.add_observer(observer.clone());

        // Discover a peer
        mesh.on_ble_discovered(
            "device-uuid",
            Some("HIVE_TEST-22222222"),
            -65,
            Some("TEST"),
            1000,
        );
        assert_eq!(mesh.peer_count(), 1);

        // Tick at t=5000 - not stale yet
        mesh.tick(5000);
        assert_eq!(mesh.peer_count(), 1);

        // Tick at t=20000 - peer is stale (10s timeout exceeded)
        mesh.tick(20000);
        assert_eq!(mesh.peer_count(), 0);

        // Check PeerLost event
        let events = observer.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerLost { .. })));
    }

    #[test]
    fn test_tick_sync_broadcast() {
        let config = HiveMeshConfig::new(NodeId::new(0x11111111), "ALPHA-1", "TEST")
            .with_sync_interval(5000);
        let mesh = HiveMesh::new(config);

        // Discover and connect a peer first
        mesh.on_ble_discovered(
            "device-uuid",
            Some("HIVE_TEST-22222222"),
            -65,
            Some("TEST"),
            1000,
        );
        mesh.on_ble_connected("device-uuid", 1000);

        // First tick at t=0 sets last_sync
        let _result = mesh.tick(0);
        // May or may not broadcast depending on initial state

        // Tick before interval - no broadcast
        let result = mesh.tick(3000);
        assert!(result.is_none());

        // After interval - should broadcast
        let result = mesh.tick(6000);
        assert!(result.is_some());

        // Immediate second tick - no broadcast (interval not elapsed)
        let result = mesh.tick(6100);
        assert!(result.is_none());

        // After another interval - should broadcast again
        let result = mesh.tick(12000);
        assert!(result.is_some());
    }

    #[test]
    fn test_incoming_connection() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");
        let observer = Arc::new(CollectingObserver::new());
        mesh.add_observer(observer.clone());

        // Incoming connection from unknown peer
        let is_new = mesh.on_incoming_connection("central-uuid", NodeId::new(0x22222222), 1000);

        assert!(is_new);
        assert_eq!(mesh.peer_count(), 1);
        assert_eq!(mesh.connected_count(), 1);

        // Check events
        let events = observer.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerDiscovered { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerConnected { .. })));
    }

    #[test]
    fn test_mesh_filtering() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");

        // Wrong mesh - ignored
        let peer = mesh.on_ble_discovered(
            "device-uuid-1",
            Some("HIVE_OTHER-22222222"),
            -65,
            Some("OTHER"),
            1000,
        );
        assert!(peer.is_none());
        assert_eq!(mesh.peer_count(), 0);

        // Correct mesh - accepted
        let peer = mesh.on_ble_discovered(
            "device-uuid-2",
            Some("HIVE_TEST-33333333"),
            -65,
            Some("TEST"),
            1000,
        );
        assert!(peer.is_some());
        assert_eq!(mesh.peer_count(), 1);
    }

    // ==================== Encryption Tests ====================

    fn create_encrypted_mesh(node_id: u32, callsign: &str, secret: [u8; 32]) -> HiveMesh {
        let config =
            HiveMeshConfig::new(NodeId::new(node_id), callsign, "TEST").with_encryption(secret);
        HiveMesh::new(config)
    }

    #[test]
    fn test_encryption_enabled() {
        let secret = [0x42u8; 32];
        let mesh = create_encrypted_mesh(0x11111111, "ALPHA-1", secret);

        assert!(mesh.is_encryption_enabled());
    }

    #[test]
    fn test_encryption_disabled_by_default() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");

        assert!(!mesh.is_encryption_enabled());
    }

    #[test]
    fn test_encrypted_document_exchange() {
        let secret = [0x42u8; 32];
        let mesh1 = create_encrypted_mesh(0x11111111, "ALPHA-1", secret);
        let mesh2 = create_encrypted_mesh(0x22222222, "BRAVO-1", secret);

        // mesh1 sends document
        let doc = mesh1.build_document();

        // Document should be encrypted (starts with ENCRYPTED_MARKER)
        assert!(doc.len() >= 2);
        assert_eq!(doc[0], crate::document::ENCRYPTED_MARKER);

        // mesh2 receives and decrypts
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.source_node.as_u32(), 0x11111111);
    }

    #[test]
    fn test_encrypted_emergency_exchange() {
        let secret = [0x42u8; 32];
        let mesh1 = create_encrypted_mesh(0x11111111, "ALPHA-1", secret);
        let mesh2 = create_encrypted_mesh(0x22222222, "BRAVO-1", secret);

        let observer = Arc::new(CollectingObserver::new());
        mesh2.add_observer(observer.clone());

        // mesh1 sends emergency
        let doc = mesh1.send_emergency(TEST_TIMESTAMP);

        // mesh2 receives and decrypts
        let result =
            mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, TEST_TIMESTAMP);

        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.is_emergency);

        // Check EmergencyReceived event was fired
        let events = observer.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::EmergencyReceived { .. })));
    }

    #[test]
    fn test_wrong_key_fails_decrypt() {
        let secret1 = [0x42u8; 32];
        let secret2 = [0x43u8; 32]; // Different key
        let mesh1 = create_encrypted_mesh(0x11111111, "ALPHA-1", secret1);
        let mesh2 = create_encrypted_mesh(0x22222222, "BRAVO-1", secret2);

        // mesh1 sends document
        let doc = mesh1.build_document();

        // mesh2 cannot decrypt (wrong key)
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);

        assert!(result.is_none());
    }

    #[test]
    fn test_unencrypted_mesh_can_read_unencrypted() {
        let mesh1 = create_mesh(0x11111111, "ALPHA-1");
        let mesh2 = create_mesh(0x22222222, "BRAVO-1");

        // mesh1 sends document (unencrypted)
        let doc = mesh1.build_document();

        // mesh2 receives (also unencrypted)
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);

        assert!(result.is_some());
    }

    #[test]
    fn test_encrypted_mesh_can_receive_unencrypted() {
        // Backward compatibility: encrypted mesh can receive unencrypted docs
        let secret = [0x42u8; 32];
        let mesh1 = create_mesh(0x11111111, "ALPHA-1"); // unencrypted
        let mesh2 = create_encrypted_mesh(0x22222222, "BRAVO-1", secret); // encrypted

        // mesh1 sends unencrypted document
        let doc = mesh1.build_document();

        // mesh2 can receive unencrypted (backward compat)
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);

        assert!(result.is_some());
    }

    #[test]
    fn test_unencrypted_mesh_cannot_receive_encrypted() {
        let secret = [0x42u8; 32];
        let mesh1 = create_encrypted_mesh(0x11111111, "ALPHA-1", secret); // encrypted
        let mesh2 = create_mesh(0x22222222, "BRAVO-1"); // unencrypted

        // mesh1 sends encrypted document
        let doc = mesh1.build_document();

        // mesh2 cannot decrypt (no key)
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);

        assert!(result.is_none());
    }

    #[test]
    fn test_enable_disable_encryption() {
        let mut mesh = create_mesh(0x11111111, "ALPHA-1");

        assert!(!mesh.is_encryption_enabled());

        // Enable encryption
        let secret = [0x42u8; 32];
        mesh.enable_encryption(&secret);
        assert!(mesh.is_encryption_enabled());

        // Build document should now be encrypted
        let doc = mesh.build_document();
        assert_eq!(doc[0], crate::document::ENCRYPTED_MARKER);

        // Disable encryption
        mesh.disable_encryption();
        assert!(!mesh.is_encryption_enabled());

        // Build document should now be unencrypted
        let doc = mesh.build_document();
        assert_ne!(doc[0], crate::document::ENCRYPTED_MARKER);
    }

    #[test]
    fn test_encryption_overhead() {
        let secret = [0x42u8; 32];
        let mesh_encrypted = create_encrypted_mesh(0x11111111, "ALPHA-1", secret);
        let mesh_unencrypted = create_mesh(0x22222222, "BRAVO-1");

        let doc_encrypted = mesh_encrypted.build_document();
        let doc_unencrypted = mesh_unencrypted.build_document();

        // Encrypted doc should be larger by:
        // - 2 bytes marker header (0xAE + reserved)
        // - 12 bytes nonce
        // - 16 bytes auth tag
        // Total: 30 bytes overhead
        let overhead = doc_encrypted.len() - doc_unencrypted.len();
        assert_eq!(overhead, 30); // 2 (marker) + 12 (nonce) + 16 (tag)
    }

    // ==================== Per-Peer E2EE Tests ====================

    #[test]
    fn test_peer_e2ee_enable_disable() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");

        assert!(!mesh.is_peer_e2ee_enabled());
        assert!(mesh.peer_e2ee_public_key().is_none());

        mesh.enable_peer_e2ee();
        assert!(mesh.is_peer_e2ee_enabled());
        assert!(mesh.peer_e2ee_public_key().is_some());

        mesh.disable_peer_e2ee();
        assert!(!mesh.is_peer_e2ee_enabled());
    }

    #[test]
    fn test_peer_e2ee_initiate_session() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");
        mesh.enable_peer_e2ee();

        let key_exchange = mesh.initiate_peer_e2ee(NodeId::new(0x22222222), 1000);
        assert!(key_exchange.is_some());

        let key_exchange = key_exchange.unwrap();
        // Should start with KEY_EXCHANGE_MARKER
        assert_eq!(key_exchange[0], crate::document::KEY_EXCHANGE_MARKER);

        // Should have a pending session
        assert_eq!(mesh.peer_e2ee_session_count(), 1);
        assert_eq!(mesh.peer_e2ee_established_count(), 0);
    }

    #[test]
    fn test_peer_e2ee_full_handshake() {
        let mesh1 = create_mesh(0x11111111, "ALPHA-1");
        let mesh2 = create_mesh(0x22222222, "BRAVO-1");

        mesh1.enable_peer_e2ee();
        mesh2.enable_peer_e2ee();

        let observer1 = Arc::new(CollectingObserver::new());
        let observer2 = Arc::new(CollectingObserver::new());
        mesh1.add_observer(observer1.clone());
        mesh2.add_observer(observer2.clone());

        // mesh1 initiates to mesh2
        let key_exchange1 = mesh1
            .initiate_peer_e2ee(NodeId::new(0x22222222), 1000)
            .unwrap();

        // mesh2 receives and responds
        let response = mesh2.handle_key_exchange(&key_exchange1, 1000);
        assert!(response.is_some());

        // Check mesh2 has established session
        assert!(mesh2.has_peer_e2ee_session(NodeId::new(0x11111111)));

        // mesh1 receives mesh2's response
        let key_exchange2 = response.unwrap();
        let _ = mesh1.handle_key_exchange(&key_exchange2, 1000);

        // Check mesh1 has established session
        assert!(mesh1.has_peer_e2ee_session(NodeId::new(0x22222222)));

        // Both should have E2EE established events
        let events1 = observer1.events();
        assert!(events1
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerE2eeEstablished { .. })));

        let events2 = observer2.events();
        assert!(events2
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerE2eeEstablished { .. })));
    }

    #[test]
    fn test_peer_e2ee_encrypt_decrypt() {
        let mesh1 = create_mesh(0x11111111, "ALPHA-1");
        let mesh2 = create_mesh(0x22222222, "BRAVO-1");

        mesh1.enable_peer_e2ee();
        mesh2.enable_peer_e2ee();

        // Establish session via key exchange
        let key_exchange1 = mesh1
            .initiate_peer_e2ee(NodeId::new(0x22222222), 1000)
            .unwrap();
        let key_exchange2 = mesh2.handle_key_exchange(&key_exchange1, 1000).unwrap();
        mesh1.handle_key_exchange(&key_exchange2, 1000);

        // mesh1 sends encrypted message to mesh2
        let plaintext = b"Secret message from mesh1";
        let encrypted = mesh1.send_peer_e2ee(NodeId::new(0x22222222), plaintext, 2000);
        assert!(encrypted.is_some());

        let encrypted = encrypted.unwrap();
        // Should start with PEER_E2EE_MARKER
        assert_eq!(encrypted[0], crate::document::PEER_E2EE_MARKER);

        // mesh2 receives and decrypts
        let observer2 = Arc::new(CollectingObserver::new());
        mesh2.add_observer(observer2.clone());

        let decrypted = mesh2.handle_peer_e2ee_message(&encrypted, 2000);
        assert!(decrypted.is_some());
        assert_eq!(decrypted.unwrap(), plaintext);

        // Should have received message event
        let events = observer2.events();
        assert!(events.iter().any(|e| matches!(
            e,
            HiveEvent::PeerE2eeMessageReceived { from_node, data }
            if from_node.as_u32() == 0x11111111 && data == plaintext
        )));
    }

    #[test]
    fn test_peer_e2ee_bidirectional() {
        let mesh1 = create_mesh(0x11111111, "ALPHA-1");
        let mesh2 = create_mesh(0x22222222, "BRAVO-1");

        mesh1.enable_peer_e2ee();
        mesh2.enable_peer_e2ee();

        // Establish session
        let key_exchange1 = mesh1
            .initiate_peer_e2ee(NodeId::new(0x22222222), 1000)
            .unwrap();
        let key_exchange2 = mesh2.handle_key_exchange(&key_exchange1, 1000).unwrap();
        mesh1.handle_key_exchange(&key_exchange2, 1000);

        // mesh1 -> mesh2
        let msg1 = mesh1
            .send_peer_e2ee(NodeId::new(0x22222222), b"Hello from mesh1", 2000)
            .unwrap();
        let dec1 = mesh2.handle_peer_e2ee_message(&msg1, 2000).unwrap();
        assert_eq!(dec1, b"Hello from mesh1");

        // mesh2 -> mesh1
        let msg2 = mesh2
            .send_peer_e2ee(NodeId::new(0x11111111), b"Hello from mesh2", 2000)
            .unwrap();
        let dec2 = mesh1.handle_peer_e2ee_message(&msg2, 2000).unwrap();
        assert_eq!(dec2, b"Hello from mesh2");
    }

    #[test]
    fn test_peer_e2ee_close_session() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");
        mesh.enable_peer_e2ee();

        let observer = Arc::new(CollectingObserver::new());
        mesh.add_observer(observer.clone());

        // Initiate a session
        mesh.initiate_peer_e2ee(NodeId::new(0x22222222), 1000);
        assert_eq!(mesh.peer_e2ee_session_count(), 1);

        // Close session
        mesh.close_peer_e2ee(NodeId::new(0x22222222));

        // Check close event
        let events = observer.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::PeerE2eeClosed { .. })));
    }

    #[test]
    fn test_peer_e2ee_without_enabling() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");

        // E2EE not enabled - should return None
        let result = mesh.initiate_peer_e2ee(NodeId::new(0x22222222), 1000);
        assert!(result.is_none());

        let result = mesh.send_peer_e2ee(NodeId::new(0x22222222), b"test", 1000);
        assert!(result.is_none());

        assert!(!mesh.has_peer_e2ee_session(NodeId::new(0x22222222)));
    }

    #[test]
    fn test_peer_e2ee_overhead() {
        let mesh1 = create_mesh(0x11111111, "ALPHA-1");
        let mesh2 = create_mesh(0x22222222, "BRAVO-1");

        mesh1.enable_peer_e2ee();
        mesh2.enable_peer_e2ee();

        // Establish session
        let key_exchange1 = mesh1
            .initiate_peer_e2ee(NodeId::new(0x22222222), 1000)
            .unwrap();
        let key_exchange2 = mesh2.handle_key_exchange(&key_exchange1, 1000).unwrap();
        mesh1.handle_key_exchange(&key_exchange2, 1000);

        // Encrypt a message
        let plaintext = b"Test message";
        let encrypted = mesh1
            .send_peer_e2ee(NodeId::new(0x22222222), plaintext, 2000)
            .unwrap();

        // Overhead should be:
        // - 2 bytes marker header
        // - 4 bytes recipient node ID
        // - 4 bytes sender node ID
        // - 8 bytes counter
        // - 12 bytes nonce
        // - 16 bytes auth tag
        // Total: 46 bytes overhead
        let overhead = encrypted.len() - plaintext.len();
        assert_eq!(overhead, 46);
    }

    // ==================== Strict Encryption Mode Tests ====================

    fn create_strict_encrypted_mesh(node_id: u32, callsign: &str, secret: [u8; 32]) -> HiveMesh {
        let config = HiveMeshConfig::new(NodeId::new(node_id), callsign, "TEST")
            .with_encryption(secret)
            .with_strict_encryption();
        HiveMesh::new(config)
    }

    #[test]
    fn test_strict_encryption_enabled() {
        let secret = [0x42u8; 32];
        let mesh = create_strict_encrypted_mesh(0x11111111, "ALPHA-1", secret);

        assert!(mesh.is_encryption_enabled());
        assert!(mesh.is_strict_encryption_enabled());
    }

    #[test]
    fn test_strict_encryption_disabled_by_default() {
        let secret = [0x42u8; 32];
        let mesh = create_encrypted_mesh(0x11111111, "ALPHA-1", secret);

        assert!(mesh.is_encryption_enabled());
        assert!(!mesh.is_strict_encryption_enabled());
    }

    #[test]
    fn test_strict_encryption_requires_encryption_enabled() {
        // strict_encryption without encryption should have no effect
        let config = HiveMeshConfig::new(NodeId::new(0x11111111), "ALPHA-1", "TEST")
            .with_strict_encryption(); // No encryption!
        let mesh = HiveMesh::new(config);

        assert!(!mesh.is_encryption_enabled());
        assert!(!mesh.is_strict_encryption_enabled());
    }

    #[test]
    fn test_strict_mode_accepts_encrypted_documents() {
        let secret = [0x42u8; 32];
        let mesh1 = create_encrypted_mesh(0x11111111, "ALPHA-1", secret);
        let mesh2 = create_strict_encrypted_mesh(0x22222222, "BRAVO-1", secret);

        // mesh1 sends encrypted document
        let doc = mesh1.build_document();
        assert_eq!(doc[0], crate::document::ENCRYPTED_MARKER);

        // mesh2 (strict mode) should accept encrypted documents
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);
        assert!(result.is_some());
    }

    #[test]
    fn test_strict_mode_rejects_unencrypted_documents() {
        let secret = [0x42u8; 32];
        let mesh1 = create_mesh(0x11111111, "ALPHA-1"); // Unencrypted sender
        let mesh2 = create_strict_encrypted_mesh(0x22222222, "BRAVO-1", secret); // Strict receiver

        let observer = Arc::new(CollectingObserver::new());
        mesh2.add_observer(observer.clone());

        // mesh1 sends unencrypted document
        let doc = mesh1.build_document();
        assert_ne!(doc[0], crate::document::ENCRYPTED_MARKER);

        // mesh2 (strict mode) should reject unencrypted documents
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);
        assert!(result.is_none());

        // Should have SecurityViolation event
        let events = observer.events();
        assert!(events.iter().any(|e| matches!(
            e,
            HiveEvent::SecurityViolation {
                kind: crate::observer::SecurityViolationKind::UnencryptedInStrictMode,
                ..
            }
        )));
    }

    #[test]
    fn test_non_strict_mode_accepts_unencrypted_documents() {
        let secret = [0x42u8; 32];
        let mesh1 = create_mesh(0x11111111, "ALPHA-1"); // Unencrypted sender
        let mesh2 = create_encrypted_mesh(0x22222222, "BRAVO-1", secret); // Non-strict receiver

        // mesh1 sends unencrypted document
        let doc = mesh1.build_document();

        // mesh2 (non-strict) should accept unencrypted documents (backward compat)
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);
        assert!(result.is_some());
    }

    #[test]
    fn test_strict_mode_security_violation_event_includes_source() {
        let secret = [0x42u8; 32];
        let mesh1 = create_mesh(0x11111111, "ALPHA-1");
        let mesh2 = create_strict_encrypted_mesh(0x22222222, "BRAVO-1", secret);

        let observer = Arc::new(CollectingObserver::new());
        mesh2.add_observer(observer.clone());

        let doc = mesh1.build_document();

        // Use on_ble_data_received with identifier to test source is captured
        mesh2.on_ble_discovered(
            "test-device-uuid",
            Some("HIVE_TEST-11111111"),
            -65,
            Some("TEST"),
            500,
        );
        mesh2.on_ble_connected("test-device-uuid", 600);

        let _result = mesh2.on_ble_data_received("test-device-uuid", &doc, 1000);

        // Check SecurityViolation event has source
        let events = observer.events();
        let violation = events.iter().find(|e| {
            matches!(
                e,
                HiveEvent::SecurityViolation {
                    kind: crate::observer::SecurityViolationKind::UnencryptedInStrictMode,
                    ..
                }
            )
        });
        assert!(violation.is_some());

        if let Some(HiveEvent::SecurityViolation { source, .. }) = violation {
            assert!(source.is_some());
            assert_eq!(source.as_ref().unwrap(), "test-device-uuid");
        }
    }

    #[test]
    fn test_decryption_failure_emits_security_violation() {
        let secret1 = [0x42u8; 32];
        let secret2 = [0x43u8; 32]; // Different key
        let mesh1 = create_encrypted_mesh(0x11111111, "ALPHA-1", secret1);
        let mesh2 = create_encrypted_mesh(0x22222222, "BRAVO-1", secret2);

        let observer = Arc::new(CollectingObserver::new());
        mesh2.add_observer(observer.clone());

        // mesh1 sends encrypted document
        let doc = mesh1.build_document();

        // mesh2 cannot decrypt (wrong key)
        let result = mesh2.on_ble_data_received_from_node(NodeId::new(0x11111111), &doc, 1000);
        assert!(result.is_none());

        // Should have SecurityViolation event for decryption failure
        let events = observer.events();
        assert!(events.iter().any(|e| matches!(
            e,
            HiveEvent::SecurityViolation {
                kind: crate::observer::SecurityViolationKind::DecryptionFailed,
                ..
            }
        )));
    }

    #[test]
    fn test_strict_mode_builder_chain() {
        let secret = [0x42u8; 32];
        let config = HiveMeshConfig::new(NodeId::new(0x11111111), "ALPHA-1", "TEST")
            .with_encryption(secret)
            .with_strict_encryption()
            .with_sync_interval(10_000)
            .with_peer_timeout(60_000);

        let mesh = HiveMesh::new(config);

        assert!(mesh.is_encryption_enabled());
        assert!(mesh.is_strict_encryption_enabled());
    }

    // ==================== Multi-Hop Relay Tests ====================

    fn create_relay_mesh(node_id: u32, callsign: &str) -> HiveMesh {
        let config = HiveMeshConfig::new(NodeId::new(node_id), callsign, "TEST").with_relay();
        HiveMesh::new(config)
    }

    #[test]
    fn test_relay_disabled_by_default() {
        let mesh = create_mesh(0x11111111, "ALPHA-1");
        assert!(!mesh.is_relay_enabled());
    }

    #[test]
    fn test_relay_enabled() {
        let mesh = create_relay_mesh(0x11111111, "ALPHA-1");
        assert!(mesh.is_relay_enabled());
    }

    #[test]
    fn test_relay_config_builder() {
        let config = HiveMeshConfig::new(NodeId::new(0x11111111), "ALPHA-1", "TEST")
            .with_relay()
            .with_max_relay_hops(5)
            .with_relay_fanout(3)
            .with_seen_cache_ttl(60_000);

        assert!(config.enable_relay);
        assert_eq!(config.max_relay_hops, 5);
        assert_eq!(config.relay_fanout, 3);
        assert_eq!(config.seen_cache_ttl_ms, 60_000);
    }

    #[test]
    fn test_seen_message_deduplication() {
        let mesh = create_relay_mesh(0x11111111, "ALPHA-1");
        let origin = NodeId::new(0x22222222);
        let msg_id = crate::relay::MessageId::from_content(origin, 1000, 0xDEADBEEF);

        // First time - should be new
        assert!(mesh.mark_message_seen(msg_id, origin, 1000));

        // Second time - should be duplicate
        assert!(!mesh.mark_message_seen(msg_id, origin, 2000));

        assert_eq!(mesh.seen_cache_size(), 1);
    }

    #[test]
    fn test_wrap_for_relay() {
        let mesh = create_relay_mesh(0x11111111, "ALPHA-1");

        let payload = vec![1, 2, 3, 4, 5];
        let wrapped = mesh.wrap_for_relay(payload.clone());

        // Should start with relay envelope marker
        assert_eq!(wrapped[0], crate::relay::RELAY_ENVELOPE_MARKER);

        // Decode and verify
        let envelope = crate::relay::RelayEnvelope::decode(&wrapped).unwrap();
        assert_eq!(envelope.payload, payload);
        assert_eq!(envelope.origin_node, NodeId::new(0x11111111));
        assert_eq!(envelope.hop_count, 0);
    }

    #[test]
    fn test_process_relay_envelope_new_message() {
        let mesh = create_relay_mesh(0x11111111, "ALPHA-1");
        let observer = Arc::new(CollectingObserver::new());
        mesh.add_observer(observer.clone());

        // Create an envelope from another node
        let payload = vec![1, 2, 3, 4, 5];
        let envelope =
            crate::relay::RelayEnvelope::broadcast(NodeId::new(0x22222222), payload.clone())
                .with_max_hops(7);
        let data = envelope.encode();

        // Process it
        let decision = mesh.process_relay_envelope(&data, NodeId::new(0x33333333), 1000);

        assert!(decision.is_some());
        let decision = decision.unwrap();
        assert_eq!(decision.payload, payload);
        assert_eq!(decision.origin_node.as_u32(), 0x22222222);
        assert_eq!(decision.hop_count, 0);
        assert!(decision.should_relay);
        assert!(decision.relay_envelope.is_some());

        // Relay envelope should have incremented hop count
        let relay_env = decision.relay_envelope.unwrap();
        assert_eq!(relay_env.hop_count, 1);
    }

    #[test]
    fn test_process_relay_envelope_duplicate() {
        let mesh = create_relay_mesh(0x11111111, "ALPHA-1");
        let observer = Arc::new(CollectingObserver::new());
        mesh.add_observer(observer.clone());

        let payload = vec![1, 2, 3, 4, 5];
        let envelope = crate::relay::RelayEnvelope::broadcast(NodeId::new(0x22222222), payload);
        let data = envelope.encode();

        // First time - should succeed
        let decision = mesh.process_relay_envelope(&data, NodeId::new(0x33333333), 1000);
        assert!(decision.is_some());

        // Second time - should be duplicate
        let decision = mesh.process_relay_envelope(&data, NodeId::new(0x33333333), 2000);
        assert!(decision.is_none());

        // Should have DuplicateMessageDropped event
        let events = observer.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::DuplicateMessageDropped { .. })));
    }

    #[test]
    fn test_process_relay_envelope_ttl_expired() {
        let mesh = create_relay_mesh(0x11111111, "ALPHA-1");
        let observer = Arc::new(CollectingObserver::new());
        mesh.add_observer(observer.clone());

        // Create envelope at max hops (TTL expired)
        let payload = vec![1, 2, 3, 4, 5];
        let mut envelope =
            crate::relay::RelayEnvelope::broadcast(NodeId::new(0x22222222), payload.clone())
                .with_max_hops(3);

        // Simulate having been relayed 3 times already
        envelope = envelope.relay().unwrap(); // hop 1
        envelope = envelope.relay().unwrap(); // hop 2
        envelope = envelope.relay().unwrap(); // hop 3 - at max now

        let data = envelope.encode();

        // Process - should still process locally but not relay further
        let decision = mesh.process_relay_envelope(&data, NodeId::new(0x33333333), 1000);

        assert!(decision.is_some());
        let decision = decision.unwrap();
        assert_eq!(decision.payload, payload);
        assert!(!decision.should_relay); // Cannot relay further
        assert!(decision.relay_envelope.is_none());

        // Should have MessageTtlExpired event
        let events = observer.events();
        assert!(events
            .iter()
            .any(|e| matches!(e, HiveEvent::MessageTtlExpired { .. })));
    }

    #[test]
    fn test_build_relay_document() {
        let mesh = create_relay_mesh(0x11111111, "ALPHA-1");

        let relay_doc = mesh.build_relay_document();

        // Should be a valid relay envelope
        assert_eq!(relay_doc[0], crate::relay::RELAY_ENVELOPE_MARKER);

        // Decode and verify it contains a valid document
        let envelope = crate::relay::RelayEnvelope::decode(&relay_doc).unwrap();
        assert_eq!(envelope.origin_node.as_u32(), 0x11111111);

        // The payload should be a valid HiveDocument
        let doc = crate::document::HiveDocument::decode(&envelope.payload);
        assert!(doc.is_some());
    }

    #[test]
    fn test_relay_targets_excludes_source() {
        let mesh = create_relay_mesh(0x11111111, "ALPHA-1");

        // Add some peers
        mesh.on_ble_discovered(
            "peer-1",
            Some("HIVE_TEST-22222222"),
            -60,
            Some("TEST"),
            1000,
        );
        mesh.on_ble_connected("peer-1", 1000);

        mesh.on_ble_discovered(
            "peer-2",
            Some("HIVE_TEST-33333333"),
            -65,
            Some("TEST"),
            1000,
        );
        mesh.on_ble_connected("peer-2", 1000);

        mesh.on_ble_discovered(
            "peer-3",
            Some("HIVE_TEST-44444444"),
            -70,
            Some("TEST"),
            1000,
        );
        mesh.on_ble_connected("peer-3", 1000);

        // Get relay targets excluding peer-2
        let targets = mesh.get_relay_targets(Some(NodeId::new(0x33333333)));

        // Should not include peer-2 in targets
        assert!(targets.iter().all(|p| p.node_id.as_u32() != 0x33333333));
    }

    #[test]
    fn test_clear_seen_cache() {
        let mesh = create_relay_mesh(0x11111111, "ALPHA-1");
        let origin = NodeId::new(0x22222222);

        // Add some messages
        mesh.mark_message_seen(
            crate::relay::MessageId::from_content(origin, 1000, 0x11111111),
            origin,
            1000,
        );
        mesh.mark_message_seen(
            crate::relay::MessageId::from_content(origin, 2000, 0x22222222),
            origin,
            2000,
        );

        assert_eq!(mesh.seen_cache_size(), 2);

        // Clear
        mesh.clear_seen_cache();
        assert_eq!(mesh.seen_cache_size(), 0);
    }
}
