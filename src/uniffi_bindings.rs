// UniFFI bindings for hive-btle
//
// This module provides UniFFI-compatible wrappers around the core hive-btle types.
// It generates Kotlin and Swift bindings automatically.
//
// Note: uniffi::setup_scaffolding!() is called in lib.rs (must be at crate root)

#![allow(missing_docs)]

use std::sync::Arc;

use crate::hive_mesh::{self, DataReceivedResult as InternalDataReceivedResult};
use crate::observer::DisconnectReason as ObserverDisconnectReason;
use crate::peer::{
    ConnectionState as InternalConnectionState,
    FullStateCountSummary as InternalFullStateCountSummary, HivePeer as InternalHivePeer,
    IndirectPeer as InternalIndirectPeer, PeerConnectionState as InternalPeerConnectionState,
    StateCountSummary as InternalStateCountSummary,
};
use crate::platform::DisconnectReason as PlatformDisconnectReason;
use crate::security::{
    DeviceIdentity as InternalDeviceIdentity, IdentityAttestation as InternalIdentityAttestation,
    MembershipPolicy, MeshGenesis as InternalMeshGenesis, RegistryResult,
};
use crate::sync::crdt::{EventType as InternalEventType, PeripheralType as InternalPeripheralType};
use crate::NodeId;

// ============================================================================
// Enums
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PeripheralType {
    Unknown,
    SoldierSensor,
    FixedSensor,
    Relay,
}

impl From<InternalPeripheralType> for PeripheralType {
    fn from(pt: InternalPeripheralType) -> Self {
        match pt {
            InternalPeripheralType::Unknown => PeripheralType::Unknown,
            InternalPeripheralType::SoldierSensor => PeripheralType::SoldierSensor,
            InternalPeripheralType::FixedSensor => PeripheralType::FixedSensor,
            InternalPeripheralType::Relay => PeripheralType::Relay,
        }
    }
}

impl From<PeripheralType> for InternalPeripheralType {
    fn from(pt: PeripheralType) -> Self {
        match pt {
            PeripheralType::Unknown => InternalPeripheralType::Unknown,
            PeripheralType::SoldierSensor => InternalPeripheralType::SoldierSensor,
            PeripheralType::FixedSensor => InternalPeripheralType::FixedSensor,
            PeripheralType::Relay => InternalPeripheralType::Relay,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum EventType {
    None,
    Ping,
    NeedAssist,
    Emergency,
    Moving,
    InPosition,
    Ack,
}

impl From<InternalEventType> for EventType {
    fn from(et: InternalEventType) -> Self {
        match et {
            InternalEventType::None => EventType::None,
            InternalEventType::Ping => EventType::Ping,
            InternalEventType::NeedAssist => EventType::NeedAssist,
            InternalEventType::Emergency => EventType::Emergency,
            InternalEventType::Moving => EventType::Moving,
            InternalEventType::InPosition => EventType::InPosition,
            InternalEventType::Ack => EventType::Ack,
        }
    }
}

impl From<EventType> for InternalEventType {
    fn from(et: EventType) -> Self {
        match et {
            EventType::None => InternalEventType::None,
            EventType::Ping => InternalEventType::Ping,
            EventType::NeedAssist => InternalEventType::NeedAssist,
            EventType::Emergency => InternalEventType::Emergency,
            EventType::Moving => InternalEventType::Moving,
            EventType::InPosition => InternalEventType::InPosition,
            EventType::Ack => InternalEventType::Ack,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DisconnectReason {
    LocalRequest,
    RemoteRequest,
    Timeout,
    LinkLoss,
    ConnectionFailed,
    Unknown,
}

impl From<DisconnectReason> for ObserverDisconnectReason {
    fn from(r: DisconnectReason) -> Self {
        match r {
            DisconnectReason::LocalRequest => ObserverDisconnectReason::LocalRequest,
            DisconnectReason::RemoteRequest => ObserverDisconnectReason::RemoteRequest,
            DisconnectReason::Timeout => ObserverDisconnectReason::Timeout,
            DisconnectReason::LinkLoss => ObserverDisconnectReason::LinkLoss,
            DisconnectReason::ConnectionFailed => ObserverDisconnectReason::ConnectionFailed,
            DisconnectReason::Unknown => ObserverDisconnectReason::Unknown,
        }
    }
}

impl From<ObserverDisconnectReason> for DisconnectReason {
    fn from(r: ObserverDisconnectReason) -> Self {
        match r {
            ObserverDisconnectReason::LocalRequest => DisconnectReason::LocalRequest,
            ObserverDisconnectReason::RemoteRequest => DisconnectReason::RemoteRequest,
            ObserverDisconnectReason::Timeout => DisconnectReason::Timeout,
            ObserverDisconnectReason::LinkLoss => DisconnectReason::LinkLoss,
            ObserverDisconnectReason::ConnectionFailed => DisconnectReason::ConnectionFailed,
            ObserverDisconnectReason::Unknown => DisconnectReason::Unknown,
        }
    }
}

impl From<PlatformDisconnectReason> for DisconnectReason {
    fn from(r: PlatformDisconnectReason) -> Self {
        match r {
            PlatformDisconnectReason::LocalRequest => DisconnectReason::LocalRequest,
            PlatformDisconnectReason::RemoteRequest => DisconnectReason::RemoteRequest,
            PlatformDisconnectReason::Timeout => DisconnectReason::Timeout,
            PlatformDisconnectReason::LinkLoss => DisconnectReason::LinkLoss,
            PlatformDisconnectReason::ConnectionFailed => DisconnectReason::ConnectionFailed,
            PlatformDisconnectReason::Unknown => DisconnectReason::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionState {
    Discovered,
    Connecting,
    Connected,
    Degraded,
    Disconnecting,
    Disconnected,
    Lost,
}

impl From<InternalConnectionState> for ConnectionState {
    fn from(s: InternalConnectionState) -> Self {
        match s {
            InternalConnectionState::Discovered => ConnectionState::Discovered,
            InternalConnectionState::Connecting => ConnectionState::Connecting,
            InternalConnectionState::Connected => ConnectionState::Connected,
            InternalConnectionState::Degraded => ConnectionState::Degraded,
            InternalConnectionState::Disconnecting => ConnectionState::Disconnecting,
            InternalConnectionState::Disconnected => ConnectionState::Disconnected,
            InternalConnectionState::Lost => ConnectionState::Lost,
        }
    }
}

impl From<ConnectionState> for InternalConnectionState {
    fn from(s: ConnectionState) -> Self {
        match s {
            ConnectionState::Discovered => InternalConnectionState::Discovered,
            ConnectionState::Connecting => InternalConnectionState::Connecting,
            ConnectionState::Connected => InternalConnectionState::Connected,
            ConnectionState::Degraded => InternalConnectionState::Degraded,
            ConnectionState::Disconnecting => InternalConnectionState::Disconnecting,
            ConnectionState::Disconnected => InternalConnectionState::Disconnected,
            ConnectionState::Lost => InternalConnectionState::Lost,
        }
    }
}

/// Convert raw u8 event_type to EventType enum
fn event_type_from_u8(value: u8) -> Option<EventType> {
    match value {
        0 => Some(EventType::None),
        1 => Some(EventType::Ping),
        2 => Some(EventType::NeedAssist),
        3 => Some(EventType::Emergency),
        4 => Some(EventType::Moving),
        5 => Some(EventType::InPosition),
        6 => Some(EventType::Ack),
        _ => None,
    }
}

// ============================================================================
// Data Structures (Records)
// ============================================================================

#[derive(Debug, Clone, uniffi::Record)]
pub struct HivePeer {
    pub node_id: u32,
    pub identifier: String,
    pub name: Option<String>,
    pub mesh_id: Option<String>,
    pub rssi: i8,
    pub is_connected: bool,
    pub last_seen_ms: u64,
}

impl From<InternalHivePeer> for HivePeer {
    fn from(p: InternalHivePeer) -> Self {
        HivePeer {
            node_id: p.node_id.as_u32(),
            identifier: p.identifier.clone(),
            name: p.name.clone(),
            mesh_id: p.mesh_id.clone(),
            rssi: p.rssi,
            is_connected: p.is_connected,
            last_seen_ms: p.last_seen_ms,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DataReceivedResult {
    pub source_node: u32,
    pub is_emergency: bool,
    pub is_ack: bool,
    pub counter_changed: bool,
    pub emergency_changed: bool,
    pub total_count: u64,
    pub event_timestamp: u64,
    pub relay_data: Option<Vec<u8>>,
    pub origin_node: Option<u32>,
    pub hop_count: u8,
    pub callsign: Option<String>,
    pub battery_percent: Option<u8>,
    pub heart_rate: Option<u8>,
    pub event_type: Option<EventType>,
    pub latitude: Option<f32>,
    pub longitude: Option<f32>,
    pub altitude: Option<f32>,
}

impl From<InternalDataReceivedResult> for DataReceivedResult {
    fn from(r: InternalDataReceivedResult) -> Self {
        DataReceivedResult {
            source_node: r.source_node.as_u32(),
            is_emergency: r.is_emergency,
            is_ack: r.is_ack,
            counter_changed: r.counter_changed,
            emergency_changed: r.emergency_changed,
            total_count: r.total_count,
            event_timestamp: r.event_timestamp,
            relay_data: r.relay_data,
            origin_node: r.origin_node.map(|n| n.as_u32()),
            hop_count: r.hop_count,
            callsign: r.callsign,
            battery_percent: r.battery_percent,
            heart_rate: r.heart_rate,
            event_type: r.event_type.and_then(event_type_from_u8),
            latitude: r.latitude,
            longitude: r.longitude,
            altitude: r.altitude,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct PeerConnectionState {
    pub node_id: u32,
    pub identifier: String,
    pub state: ConnectionState,
    pub discovered_at: u64,
    pub connected_at: Option<u64>,
    pub disconnected_at: Option<u64>,
    pub disconnect_reason: Option<DisconnectReason>,
    pub last_rssi: Option<i8>,
    pub connection_count: u32,
    pub documents_synced: u32,
    pub bytes_received: u64,
    pub bytes_sent: u64,
    pub last_seen_ms: u64,
    pub name: Option<String>,
    pub mesh_id: Option<String>,
}

impl From<InternalPeerConnectionState> for PeerConnectionState {
    fn from(p: InternalPeerConnectionState) -> Self {
        PeerConnectionState {
            node_id: p.node_id.as_u32(),
            identifier: p.identifier,
            state: p.state.into(),
            discovered_at: p.discovered_at,
            connected_at: p.connected_at,
            disconnected_at: p.disconnected_at,
            disconnect_reason: p.disconnect_reason.map(|r| r.into()),
            last_rssi: p.last_rssi,
            connection_count: p.connection_count,
            documents_synced: p.documents_synced,
            bytes_received: p.bytes_received,
            bytes_sent: p.bytes_sent,
            last_seen_ms: p.last_seen_ms,
            name: p.name,
            mesh_id: p.mesh_id,
        }
    }
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct StateCountSummary {
    pub discovered: u32,
    pub connecting: u32,
    pub connected: u32,
    pub degraded: u32,
    pub disconnecting: u32,
    pub disconnected: u32,
    pub lost: u32,
}

impl From<InternalStateCountSummary> for StateCountSummary {
    fn from(s: InternalStateCountSummary) -> Self {
        StateCountSummary {
            discovered: s.discovered as u32,
            connecting: s.connecting as u32,
            connected: s.connected as u32,
            degraded: s.degraded as u32,
            disconnecting: s.disconnecting as u32,
            disconnected: s.disconnected as u32,
            lost: s.lost as u32,
        }
    }
}

/// Via-peer routing entry for indirect peer
#[derive(Debug, Clone, uniffi::Record)]
pub struct ViaPeerRoute {
    pub via_node_id: u32,
    pub hop_count: u8,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct IndirectPeer {
    pub node_id: u32,
    pub min_hops: u8,
    pub via_peers: Vec<ViaPeerRoute>,
    pub discovered_at: u64,
    pub last_seen_ms: u64,
    pub messages_received: u32,
    pub callsign: Option<String>,
}

impl From<InternalIndirectPeer> for IndirectPeer {
    fn from(p: InternalIndirectPeer) -> Self {
        IndirectPeer {
            node_id: p.node_id.as_u32(),
            min_hops: p.min_hops,
            via_peers: p
                .via_peers
                .iter()
                .map(|(&node_id, &hop_count)| ViaPeerRoute {
                    via_node_id: node_id.as_u32(),
                    hop_count,
                })
                .collect(),
            discovered_at: p.discovered_at,
            last_seen_ms: p.last_seen_ms,
            messages_received: p.messages_received,
            callsign: p.callsign,
        }
    }
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct FullStateCountSummary {
    pub direct: StateCountSummary,
    pub one_hop: u32,
    pub two_hop: u32,
    pub three_hop: u32,
}

impl From<InternalFullStateCountSummary> for FullStateCountSummary {
    fn from(s: InternalFullStateCountSummary) -> Self {
        FullStateCountSummary {
            direct: s.direct.into(),
            one_hop: s.one_hop as u32,
            two_hop: s.two_hop as u32,
            three_hop: s.three_hop as u32,
        }
    }
}

// ============================================================================
// HiveMesh Object
// ============================================================================

#[derive(uniffi::Object)]
pub struct HiveMesh {
    inner: hive_mesh::HiveMesh,
}

#[uniffi::export]
impl HiveMesh {
    /// Create a basic HiveMesh
    #[uniffi::constructor]
    pub fn new(node_id: u32, callsign: &str, mesh_id: &str) -> Arc<Self> {
        let config = crate::HiveMeshConfig::new(NodeId::new(node_id), callsign, mesh_id);
        Arc::new(Self {
            inner: hive_mesh::HiveMesh::new(config),
        })
    }

    /// Create a HiveMesh with peripheral type
    #[uniffi::constructor]
    pub fn new_with_peripheral(
        node_id: u32,
        callsign: &str,
        mesh_id: &str,
        peripheral_type: PeripheralType,
    ) -> Arc<Self> {
        let config = crate::HiveMeshConfig::new(NodeId::new(node_id), callsign, mesh_id)
            .with_peripheral_type(peripheral_type.into());
        Arc::new(Self {
            inner: hive_mesh::HiveMesh::new(config),
        })
    }

    /// Create a HiveMesh from genesis (recommended for production)
    #[uniffi::constructor]
    pub fn new_from_genesis(
        callsign: &str,
        identity: &DeviceIdentity,
        genesis: &MeshGenesis,
    ) -> Arc<Self> {
        Arc::new(Self {
            inner: hive_mesh::HiveMesh::from_genesis(
                &genesis.inner,
                identity.inner.clone(),
                callsign,
            ),
        })
    }

    // ==================== Transport Layer ====================

    /// Decrypt data received over BLE (transport layer only)
    pub fn decrypt_only(&self, data: &[u8]) -> Option<Vec<u8>> {
        self.inner.decrypt_only(data)
    }

    /// Build the current document for transmission
    pub fn build_document(&self) -> Vec<u8> {
        self.inner.build_document()
    }

    /// Periodic tick - call every sync interval
    pub fn tick(&self, now_ms: u64) -> Option<Vec<u8>> {
        self.inner.tick(now_ms)
    }

    // ==================== Emergency/Ack ====================

    /// Send emergency alert
    pub fn send_emergency(&self, timestamp_ms: u64) -> Vec<u8> {
        self.inner.send_emergency(timestamp_ms)
    }

    /// Send ACK
    pub fn send_ack(&self, timestamp_ms: u64) -> Vec<u8> {
        self.inner.send_ack(timestamp_ms)
    }

    /// Check if emergency is currently active
    pub fn is_emergency_active(&self) -> bool {
        self.inner.is_emergency_active()
    }

    /// Check if ACK is currently active
    pub fn is_ack_active(&self) -> bool {
        self.inner.is_ack_active()
    }

    /// Broadcast arbitrary bytes over the mesh.
    ///
    /// Takes raw payload bytes, encrypts them (if encryption is enabled),
    /// and returns bytes ready to send to all connected peers.
    ///
    /// This is useful for sending extension data like CannedMessages from hive-lite.
    pub fn broadcast_bytes(&self, payload: &[u8]) -> Vec<u8> {
        self.inner.broadcast_bytes(payload)
    }

    // ==================== BLE Callbacks ====================

    /// Call when a BLE device is discovered
    pub fn on_ble_discovered(
        &self,
        identifier: &str,
        name: Option<String>,
        rssi: i8,
        mesh_id: Option<String>,
        now_ms: u64,
    ) -> Option<HivePeer> {
        self.inner
            .on_ble_discovered(
                identifier,
                name.as_deref(),
                rssi,
                mesh_id.as_deref(),
                now_ms,
            )
            .map(|p| p.into())
    }

    /// Call when a BLE connection is established
    pub fn on_ble_connected(&self, identifier: &str, now_ms: u64) {
        self.inner.on_ble_connected(identifier, now_ms);
    }

    /// Call when a BLE connection is lost
    pub fn on_ble_disconnected(&self, identifier: &str, reason: DisconnectReason) -> Option<u32> {
        self.inner
            .on_ble_disconnected(identifier, reason.into())
            .map(|n| n.as_u32())
    }

    /// Call when BLE data is received from a known peer
    pub fn on_ble_data_received(
        &self,
        identifier: &str,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        self.inner
            .on_ble_data_received(identifier, data, now_ms)
            .map(|r| r.into())
    }

    /// Call when BLE data is received from an unknown source (e.g., broadcast)
    pub fn on_ble_data_received_anonymous(
        &self,
        identifier: &str,
        data: &[u8],
        now_ms: u64,
    ) -> Option<DataReceivedResult> {
        self.inner
            .on_ble_data_received_anonymous(identifier, data, now_ms)
            .map(|r| r.into())
    }

    // ==================== Peer Management ====================

    /// Get count of discovered peers
    pub fn peer_count(&self) -> u32 {
        self.inner.peer_count() as u32
    }

    /// Get count of connected peers
    pub fn connected_count(&self) -> u32 {
        self.inner.connected_count() as u32
    }

    /// Get total mesh count
    pub fn total_count(&self) -> u32 {
        self.inner.total_count() as u32
    }

    /// Get callsign for a peer
    pub fn get_peer_callsign(&self, node_id: u32) -> Option<String> {
        self.inner.get_peer_callsign(NodeId::new(node_id))
    }

    /// Get list of connected peers
    pub fn get_connected_peers(&self) -> Vec<HivePeer> {
        self.inner
            .get_connected_peers()
            .into_iter()
            .map(|p| p.into())
            .collect()
    }

    /// Get count of degraded peers (connected but poor signal)
    pub fn degraded_peer_count(&self) -> u32 {
        self.inner.get_degraded_peers().len() as u32
    }

    /// Get count of recently disconnected peers
    pub fn recently_disconnected_count(&self, within_ms: u64, now_ms: u64) -> u32 {
        self.inner
            .get_recently_disconnected(within_ms, now_ms)
            .len() as u32
    }

    /// Get count of lost peers (disconnected and timed out)
    pub fn lost_peer_count(&self) -> u32 {
        self.inner.get_lost_peers().len() as u32
    }

    /// Check if a peer is known
    pub fn is_peer_known(&self, node_id: u32) -> bool {
        self.inner.is_peer_known(NodeId::new(node_id))
    }

    /// Check if mesh matches (for filtering BLE discovery)
    pub fn matches_mesh(&self, device_mesh_id: Option<String>) -> bool {
        self.inner.matches_mesh(device_mesh_id.as_deref())
    }

    /// Get connection state for a specific peer
    pub fn get_peer_connection_state(&self, node_id: u32) -> Option<PeerConnectionState> {
        self.inner
            .get_peer_connection_state(NodeId::new(node_id))
            .map(|p| p.into())
    }

    /// Get all degraded peers (connected but poor signal quality)
    pub fn get_degraded_peers(&self) -> Vec<PeerConnectionState> {
        self.inner
            .get_degraded_peers()
            .into_iter()
            .map(|p| p.into())
            .collect()
    }

    /// Get all lost peers (disconnected and timed out)
    pub fn get_lost_peers(&self) -> Vec<PeerConnectionState> {
        self.inner
            .get_lost_peers()
            .into_iter()
            .map(|p| p.into())
            .collect()
    }

    /// Get connection state counts (direct peers)
    pub fn get_connection_state_counts(&self) -> StateCountSummary {
        self.inner.get_connection_state_counts().into()
    }

    /// Get indirect (multi-hop) peers
    pub fn get_indirect_peers(&self) -> Vec<IndirectPeer> {
        self.inner
            .get_indirect_peers()
            .into_iter()
            .map(|p| p.into())
            .collect()
    }

    /// Get full state counts including indirect peers
    pub fn get_full_state_counts(&self) -> FullStateCountSummary {
        self.inner.get_full_state_counts().into()
    }

    // ==================== Location & State ====================

    /// Update own location
    pub fn update_location(&self, latitude: f32, longitude: f32, altitude: Option<f32>) {
        self.inner.update_location(latitude, longitude, altitude);
    }

    /// Clear own location
    pub fn clear_location(&self) {
        self.inner.clear_location();
    }

    /// Update own callsign
    pub fn update_callsign(&self, callsign: &str) {
        self.inner.update_callsign(callsign);
    }

    /// Update own heart rate
    pub fn update_heart_rate(&self, bpm: u8) {
        self.inner.update_heart_rate(bpm);
    }

    /// Set peripheral event
    pub fn set_peripheral_event(&self, event_type: EventType, timestamp_ms: u64) {
        self.inner
            .set_peripheral_event(event_type.into(), timestamp_ms);
    }

    /// Clear peripheral event
    pub fn clear_peripheral_event(&self) {
        self.inner.clear_peripheral_event();
    }

    /// Update all peripheral state at once (efficient for encrypted transmission)
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
        timestamp_ms: u64,
    ) {
        self.inner.update_peripheral_state(
            callsign,
            battery_percent,
            heart_rate,
            latitude,
            longitude,
            altitude,
            event_type.map(|e| e.into()),
            timestamp_ms,
        );
    }

    // ==================== Chat (Legacy) ====================

    /// Send a chat message
    /// Returns encrypted document bytes if message was new, None if duplicate
    pub fn send_chat(&self, sender: &str, text: &str, timestamp_ms: u64) -> Option<Vec<u8>> {
        self.inner.send_chat(sender, text, timestamp_ms)
    }

    /// Send a chat reply
    /// Returns encrypted document bytes if message was new, None if duplicate
    pub fn send_chat_reply(
        &self,
        sender: &str,
        text: &str,
        reply_to_node: u32,
        reply_to_timestamp: u64,
        timestamp_ms: u64,
    ) -> Option<Vec<u8>> {
        self.inner.send_chat_reply(
            sender,
            text,
            reply_to_node,
            reply_to_timestamp,
            timestamp_ms,
        )
    }

    /// Get count of chat messages
    pub fn chat_count(&self) -> u32 {
        self.inner.chat_count() as u32
    }

    /// Get all chat messages as JSON array string
    /// Format: [{"origin_node":123,"timestamp":456,"sender":"name","text":"msg","reply_to_node":0,"reply_to_timestamp":0},...]
    pub fn get_all_chat_messages(&self) -> String {
        let messages = self.inner.all_chat_messages();
        chat_messages_to_json(&messages)
    }

    /// Get chat messages since timestamp as JSON array string
    pub fn get_chat_messages_since(&self, since_timestamp: u64) -> String {
        let messages = self.inner.chat_messages_since(since_timestamp);
        chat_messages_to_json(&messages)
    }

    // ==================== Identity ====================

    /// Check if mesh has a cryptographic identity
    pub fn has_identity(&self) -> bool {
        self.inner.has_identity()
    }

    /// Get own public key
    pub fn get_public_key(&self) -> Option<Vec<u8>> {
        self.inner.public_key().map(|k| k.to_vec())
    }

    /// Create an identity attestation
    pub fn create_attestation(&self, timestamp_ms: u64) -> Option<Vec<u8>> {
        self.inner
            .create_attestation(timestamp_ms)
            .map(|a| a.encode())
    }

    /// Verify a peer's identity from attestation bytes
    /// Returns true if identity was registered or verified, false otherwise
    pub fn verify_peer_identity(&self, attestation_bytes: &[u8]) -> bool {
        if let Some(attestation) = InternalIdentityAttestation::decode(attestation_bytes) {
            matches!(
                self.inner.verify_peer_identity(&attestation),
                RegistryResult::Registered | RegistryResult::Verified
            )
        } else {
            false
        }
    }

    /// Check if a peer's identity is known
    pub fn is_peer_identity_known(&self, node_id: u32) -> bool {
        self.inner.is_peer_identity_known(NodeId::new(node_id))
    }

    /// Get count of known identities
    pub fn known_identity_count(&self) -> u32 {
        self.inner.known_identity_count() as u32
    }

    /// Sign data with own identity
    pub fn sign(&self, message: &[u8]) -> Option<Vec<u8>> {
        self.inner.sign(message).map(|s| s.to_vec())
    }

    /// Verify a peer's signature
    pub fn verify_peer_signature(&self, node_id: u32, message: &[u8], signature: &[u8]) -> bool {
        if signature.len() != 64 {
            return false;
        }
        let mut sig_array = [0u8; 64];
        sig_array.copy_from_slice(signature);
        self.inner
            .verify_peer_signature(NodeId::new(node_id), message, &sig_array)
    }

    // ==================== Encryption ====================

    /// Check if encryption is enabled
    pub fn is_encryption_enabled(&self) -> bool {
        self.inner.is_encryption_enabled()
    }

    // ==================== Relay ====================

    /// Check if relay is enabled
    pub fn is_relay_enabled(&self) -> bool {
        self.inner.is_relay_enabled()
    }
}

// ============================================================================
// DeviceIdentity Object
// ============================================================================

#[derive(uniffi::Object)]
pub struct DeviceIdentity {
    inner: InternalDeviceIdentity,
}

#[uniffi::export]
impl DeviceIdentity {
    /// Generate a new random identity
    #[uniffi::constructor]
    pub fn generate() -> Arc<Self> {
        Arc::new(Self {
            inner: InternalDeviceIdentity::generate(),
        })
    }

    /// Get public key bytes
    pub fn get_public_key(&self) -> Vec<u8> {
        self.inner.public_key().to_vec()
    }

    /// Get private key bytes (for secure storage)
    pub fn get_private_key(&self) -> Vec<u8> {
        self.inner.private_key_bytes().to_vec()
    }

    /// Get derived node ID
    pub fn get_node_id(&self) -> u32 {
        self.inner.node_id().as_u32()
    }

    /// Create an attestation proving identity ownership
    pub fn create_attestation(&self, timestamp_ms: u64) -> Vec<u8> {
        self.inner.create_attestation(timestamp_ms).encode()
    }

    /// Sign data
    pub fn sign(&self, message: &[u8]) -> Vec<u8> {
        self.inner.sign(message).to_vec()
    }
}

// ============================================================================
// MeshGenesis Object
// ============================================================================

#[derive(uniffi::Object)]
pub struct MeshGenesis {
    inner: InternalMeshGenesis,
}

#[uniffi::export]
impl MeshGenesis {
    /// Create a new mesh as founder
    #[uniffi::constructor]
    pub fn create(mesh_name: &str, founder_identity: &DeviceIdentity) -> Arc<Self> {
        Arc::new(Self {
            inner: InternalMeshGenesis::create(
                mesh_name,
                &founder_identity.inner,
                MembershipPolicy::Open,
            ),
        })
    }

    /// Get mesh ID
    pub fn get_mesh_id(&self) -> String {
        self.inner.mesh_id()
    }

    /// Get encryption secret for mesh
    pub fn get_encryption_secret(&self) -> Vec<u8> {
        self.inner.encryption_secret().to_vec()
    }

    /// Encode genesis to bytes for storage/transmission
    pub fn encode(&self) -> Vec<u8> {
        self.inner.encode()
    }
}

// ============================================================================
// IdentityAttestation Object
// ============================================================================

#[derive(uniffi::Object)]
pub struct IdentityAttestation {
    inner: InternalIdentityAttestation,
}

#[uniffi::export]
impl IdentityAttestation {
    /// Verify attestation is valid
    pub fn verify(&self) -> bool {
        self.inner.verify()
    }

    /// Get node ID from attestation
    pub fn get_node_id(&self) -> u32 {
        self.inner.node_id.as_u32()
    }

    /// Get public key from attestation
    pub fn get_public_key(&self) -> Vec<u8> {
        self.inner.public_key.to_vec()
    }
}

// ============================================================================
// Namespace Functions
// ============================================================================

/// Derive a node ID from a MAC address string (format: "AA:BB:CC:DD:EE:FF")
#[uniffi::export]
pub fn derive_node_id_from_mac(mac_address: &str) -> u32 {
    NodeId::from_mac_string(mac_address)
        .map(|n| n.as_u32())
        .unwrap_or(0)
}

/// Create a HiveMesh with encryption enabled (returns null if secret is wrong length)
#[uniffi::export]
pub fn create_hive_mesh_with_encryption(
    node_id: u32,
    callsign: &str,
    mesh_id: &str,
    encryption_secret: &[u8],
) -> Option<std::sync::Arc<HiveMesh>> {
    if encryption_secret.len() != 32 {
        return None;
    }
    let mut secret = [0u8; 32];
    secret.copy_from_slice(encryption_secret);
    let config =
        crate::HiveMeshConfig::new(NodeId::new(node_id), callsign, mesh_id).with_encryption(secret);
    Some(std::sync::Arc::new(HiveMesh {
        inner: hive_mesh::HiveMesh::new(config),
    }))
}

/// Restore a DeviceIdentity from private key bytes (returns null if invalid)
#[uniffi::export]
pub fn restore_device_identity(private_key: &[u8]) -> Option<std::sync::Arc<DeviceIdentity>> {
    if private_key.len() < 32 {
        return None;
    }
    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&private_key[..32]);
    InternalDeviceIdentity::from_private_key(&key_bytes)
        .ok()
        .map(|inner| std::sync::Arc::new(DeviceIdentity { inner }))
}

/// Decode a MeshGenesis from bytes (returns null if invalid)
#[uniffi::export]
pub fn decode_mesh_genesis(encoded: &[u8]) -> Option<std::sync::Arc<MeshGenesis>> {
    InternalMeshGenesis::decode(encoded).map(|inner| std::sync::Arc::new(MeshGenesis { inner }))
}

/// Decode an IdentityAttestation from bytes (returns null if invalid)
#[uniffi::export]
pub fn decode_identity_attestation(encoded: &[u8]) -> Option<std::sync::Arc<IdentityAttestation>> {
    InternalIdentityAttestation::decode(encoded)
        .map(|inner| std::sync::Arc::new(IdentityAttestation { inner }))
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert chat messages tuple to JSON string
fn chat_messages_to_json(messages: &[(u32, u64, String, String, u32, u64)]) -> String {
    let json_array: Vec<String> = messages
        .iter()
        .map(|(origin_node, timestamp, sender, text, reply_to_node, reply_to_timestamp)| {
            // Escape special characters in strings
            let escaped_sender = escape_json_string(sender);
            let escaped_text = escape_json_string(text);
            format!(
                r#"{{"origin_node":{},"timestamp":{},"sender":"{}","text":"{}","reply_to_node":{},"reply_to_timestamp":{}}}"#,
                origin_node, timestamp, escaped_sender, escaped_text, reply_to_node, reply_to_timestamp
            )
        })
        .collect();
    format!("[{}]", json_array.join(","))
}

/// Escape special characters for JSON string
fn escape_json_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
