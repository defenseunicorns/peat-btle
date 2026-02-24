// UniFFI bindings for eche-btle
//
// This module provides UniFFI-compatible wrappers around the core eche-btle types.
// It generates Kotlin and Swift bindings automatically.
//
// Note: uniffi::setup_scaffolding!() is called in lib.rs (must be at crate root)

#![allow(missing_docs)]

use std::sync::Arc;

// Initialize Android logger on first use
#[cfg(target_os = "android")]
static ANDROID_LOGGER_INIT: std::sync::Once = std::sync::Once::new();

#[cfg(target_os = "android")]
fn ensure_android_logger() {
    ANDROID_LOGGER_INIT.call_once(|| {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("EcheFFI"),
        );
    });
}

use crate::eche_mesh::{self, DataReceivedResult as InternalDataReceivedResult};
use crate::observer::DisconnectReason as ObserverDisconnectReason;
use crate::peer::{
    ConnectionState as InternalConnectionState, EchePeer as InternalEchePeer,
    FullStateCountSummary as InternalFullStateCountSummary, IndirectPeer as InternalIndirectPeer,
    PeerConnectionState as InternalPeerConnectionState,
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
pub struct EchePeer {
    pub node_id: u32,
    pub identifier: String,
    pub name: Option<String>,
    pub mesh_id: Option<String>,
    pub rssi: i8,
    pub is_connected: bool,
    pub last_seen_ms: u64,
}

impl From<InternalEchePeer> for EchePeer {
    fn from(p: InternalEchePeer) -> Self {
        EchePeer {
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

/// Information about a stored CannedMessage document.
#[cfg(feature = "eche-lite-sync")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct CannedMessageInfo {
    /// Source node that created the message
    pub source_node: u32,
    /// Timestamp when the message was created
    pub timestamp: u64,
    /// Encoded message bytes (includes 0xAF marker)
    pub encoded_bytes: Vec<u8>,
}

// ============================================================================
// EcheMesh Object
// ============================================================================

#[derive(uniffi::Object)]
pub struct EcheMesh {
    inner: eche_mesh::EcheMesh,
}

#[uniffi::export]
impl EcheMesh {
    /// Create a basic EcheMesh
    #[uniffi::constructor]
    pub fn new(node_id: u32, callsign: &str, mesh_id: &str) -> Arc<Self> {
        #[cfg(target_os = "android")]
        ensure_android_logger();

        let config = crate::EcheMeshConfig::new(NodeId::new(node_id), callsign, mesh_id);
        Arc::new(Self {
            inner: eche_mesh::EcheMesh::new(config),
        })
    }

    /// Create a EcheMesh with peripheral type
    #[uniffi::constructor]
    pub fn new_with_peripheral(
        node_id: u32,
        callsign: &str,
        mesh_id: &str,
        peripheral_type: PeripheralType,
    ) -> Arc<Self> {
        #[cfg(target_os = "android")]
        ensure_android_logger();

        let config = crate::EcheMeshConfig::new(NodeId::new(node_id), callsign, mesh_id)
            .with_peripheral_type(peripheral_type.into());
        Arc::new(Self {
            inner: eche_mesh::EcheMesh::new(config),
        })
    }

    /// Create a EcheMesh from genesis (recommended for production)
    #[uniffi::constructor]
    pub fn new_from_genesis(
        callsign: &str,
        identity: &DeviceIdentity,
        genesis: &MeshGenesis,
    ) -> Arc<Self> {
        #[cfg(target_os = "android")]
        ensure_android_logger();

        Arc::new(Self {
            inner: eche_mesh::EcheMesh::from_genesis(
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

    /// Build a delta document for a specific peer.
    ///
    /// This includes only operations that have changed since the last sync
    /// with this peer, including app-layer documents (CannedMessages, etc.).
    /// Returns None if there's nothing new to send.
    pub fn build_delta_document_for_peer(&self, peer_id: u32, now_ms: u64) -> Option<Vec<u8>> {
        self.inner
            .build_delta_document_for_peer(&crate::NodeId::new(peer_id), now_ms)
    }

    /// Build a full delta document containing all current state.
    ///
    /// Unlike `build_delta_document_for_peer`, this includes all state
    /// regardless of what has been sent before. Use for broadcasts or
    /// new peer connections. Includes app-layer documents.
    pub fn build_full_delta_document(&self, now_ms: u64) -> Vec<u8> {
        self.inner.build_full_delta_document(now_ms)
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
    /// This is useful for sending extension data like CannedMessages from eche-lite.
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
    ) -> Option<EchePeer> {
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

    /// Call when a remote device connects to us (incoming peripheral connection)
    ///
    /// Unlike on_ble_connected(), this creates the peer if it doesn't exist yet.
    /// Use this when acting as a GATT server and a central connects to us.
    pub fn on_incoming_connection(&self, identifier: &str, node_id: u32, now_ms: u64) -> bool {
        self.inner
            .on_incoming_connection(identifier, NodeId::new(node_id), now_ms)
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
    pub fn get_connected_peers(&self) -> Vec<EchePeer> {
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

    // ==================== CannedMessage Integration ====================
    // These methods enable proper CRDT sync for eche-lite CannedMessages,
    // using document identity for deduplication instead of raw relay.

    /// Check if a CannedMessage has been seen recently.
    ///
    /// Uses document identity (source_node + timestamp) for deduplication.
    /// This prevents broadcast storms when relaying CannedMessages.
    ///
    /// Returns true if the message should be processed, false if it's a duplicate.
    pub fn check_canned_message(&self, source_node: u32, timestamp: u64, ttl_ms: u64) -> bool {
        self.inner
            .check_canned_message(source_node, timestamp, ttl_ms)
    }

    /// Mark a CannedMessage as seen (for deduplication).
    ///
    /// Call this after receiving and processing a CannedMessage to prevent
    /// reprocessing the same message from other relay paths.
    pub fn mark_canned_message_seen(&self, source_node: u32, timestamp: u64) {
        self.inner.mark_canned_message_seen(source_node, timestamp);
    }

    /// Get the list of connected peer identifiers for relay.
    ///
    /// Used by the Kotlin layer to relay CannedMessages to other peers
    /// after deduplication check.
    pub fn get_connected_peer_identifiers(&self) -> Vec<String> {
        self.inner.get_connected_peer_identifiers()
    }

    /// Get the number of stored app documents.
    pub fn app_document_count(&self) -> u32 {
        self.inner.app_document_count() as u32
    }
}

// ==================== App Document Storage (eche-lite-sync) ====================
// These methods enable proper CRDT sync for app-layer documents
// through the document registry (0xC0-0xCF type range).
// Separated into own impl block because UniFFI proc macros don't
// respect #[cfg] on individual methods within a #[uniffi::export] block.

#[cfg(feature = "eche-lite-sync")]
#[uniffi::export]
impl EcheMesh {
    /// Create and store a new CannedMessage document.
    ///
    /// Creates a CannedMessageAckEvent with the given message code, stores it
    /// for CRDT sync, and returns the encoded bytes (with 0xAF marker) for
    /// broadcasting to peers.
    ///
    /// Returns the encoded bytes if the document was newly created, None if
    /// there was an error or the message code is invalid.
    pub fn send_canned_message(&self, message_code: u8, timestamp_ms: u64) -> Option<Vec<u8>> {
        use crate::eche_lite_sync::CannedMessageDocument;
        use eche_lite::{CannedMessage, CannedMessageAckEvent, NodeId as EcheLiteNodeId};

        let message = CannedMessage::from_u8(message_code)?;
        let source_node = EcheLiteNodeId::new(self.inner.node_id().as_u32());
        let event = CannedMessageAckEvent::new(message, source_node, None, timestamp_ms);
        let doc = CannedMessageDocument::new(event);

        // Encode before storing (store consumes ownership via clone internally)
        let inner = doc.inner().clone();
        let encoded = inner.encode().to_vec();

        if self.inner.store_app_document(doc) {
            Some(encoded)
        } else {
            None
        }
    }

    /// Store a CannedMessage document for CRDT sync.
    ///
    /// Takes raw eche-lite encoded bytes (including 0xAF marker).
    /// The document will be stored and synced to peers via delta sync.
    ///
    /// Returns true if the document was newly added or changed via merge.
    pub fn store_canned_message_document(&self, encoded_bytes: &[u8]) -> bool {
        use crate::eche_lite_sync::CannedMessageDocument;
        use crate::registry::DocumentType;

        // The encoded bytes include 0xAF marker from eche-lite.
        // CannedMessageDocument::decode expects payload WITHOUT 0xAF
        // (since it prepends it), but the incoming bytes already have it.
        // So we need to strip it first.
        if encoded_bytes.is_empty() {
            return false;
        }

        // Decode using CannedMessageDocument (which handles the 0xAF internally)
        // The encode() strips 0xAF, decode() prepends 0xAF.
        // But here we have raw eche-lite bytes WITH 0xAF.
        // So we skip the first byte to get the payload that encode() would produce.
        let payload = &encoded_bytes[1..];
        if let Some(doc) = CannedMessageDocument::decode(payload) {
            self.inner.store_app_document(doc)
        } else {
            false
        }
    }

    /// Record an ACK on a stored CannedMessage document.
    ///
    /// This is the efficient path for adding ACKs - the full document
    /// doesn't need to be re-sent, just the ACK delta.
    ///
    /// Returns true if the ACK was new (document changed).
    pub fn ack_canned_message(
        &self,
        source_node: u32,
        timestamp: u64,
        acker_node: u32,
        ack_timestamp: u64,
    ) -> bool {
        use crate::eche_lite_sync::CannedMessageDocument;

        // Get the document, add ACK, store back
        if let Some(mut doc) = self
            .inner
            .get_app_document::<CannedMessageDocument>(source_node, timestamp)
        {
            if doc.ack(acker_node, ack_timestamp) {
                self.inner.store_app_document(doc)
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Get a stored CannedMessage document as raw eche-lite bytes.
    ///
    /// Returns the document encoded in eche-lite format (with 0xAF marker),
    /// or None if not found.
    pub fn get_canned_message_document(&self, source_node: u32, timestamp: u64) -> Option<Vec<u8>> {
        use crate::eche_lite_sync::CannedMessageDocument;

        self.inner
            .get_app_document::<CannedMessageDocument>(source_node, timestamp)
            .map(|doc| {
                // Return with 0xAF marker for eche-lite compatibility
                let inner = doc.into_inner();
                inner.encode().to_vec()
            })
    }

    /// Get all stored CannedMessage documents as encoded bytes.
    ///
    /// Returns a list of (source_node, timestamp, encoded_bytes) tuples.
    /// The encoded_bytes include the 0xAF marker for eche-lite compatibility.
    pub fn get_all_canned_messages(&self) -> Vec<CannedMessageInfo> {
        use crate::eche_lite_sync::CannedMessageDocument;
        use crate::registry::DocumentType;

        self.inner
            .get_all_app_documents_of_type::<CannedMessageDocument>()
            .into_iter()
            .map(|doc| {
                let (source_node, timestamp) = doc.identity();
                let inner = doc.into_inner();
                CannedMessageInfo {
                    source_node,
                    timestamp,
                    encoded_bytes: inner.encode().to_vec(),
                }
            })
            .collect()
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

/// Create a EcheMesh with encryption enabled (returns null if secret is wrong length)
#[uniffi::export]
pub fn create_eche_mesh_with_encryption(
    node_id: u32,
    callsign: &str,
    mesh_id: &str,
    encryption_secret: &[u8],
) -> Option<std::sync::Arc<EcheMesh>> {
    if encryption_secret.len() != 32 {
        return None;
    }
    let mut secret = [0u8; 32];
    secret.copy_from_slice(encryption_secret);
    let config =
        crate::EcheMeshConfig::new(NodeId::new(node_id), callsign, mesh_id).with_encryption(secret);
    Some(std::sync::Arc::new(EcheMesh {
        inner: eche_mesh::EcheMesh::new(config),
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

// ============================================================================
// Reconnection Manager (auto-reconnect with exponential backoff)
// ============================================================================

use crate::reconnect::{
    PeerReconnectionStats as InternalPeerReconnectionStats,
    ReconnectionConfig as InternalReconnectionConfig,
    ReconnectionManager as InternalReconnectionManager,
    ReconnectionStatus as InternalReconnectionStatus,
};

/// Configuration for reconnection behavior
#[derive(Debug, Clone, uniffi::Record)]
pub struct ReconnectionConfig {
    /// Base delay between reconnection attempts in milliseconds
    pub base_delay_ms: u64,
    /// Maximum delay between attempts in milliseconds
    pub max_delay_ms: u64,
    /// Maximum number of reconnection attempts before giving up
    pub max_attempts: u32,
    /// Interval for checking which peers need reconnection in milliseconds
    pub check_interval_ms: u64,
    /// Use flat delay instead of exponential backoff
    pub use_flat_delay: bool,
    /// Auto-reset attempt counter when max_attempts is exhausted
    pub reset_on_exhaustion: bool,
}

impl Default for ReconnectionConfig {
    fn default() -> Self {
        Self {
            base_delay_ms: 2000,
            max_delay_ms: 60000,
            max_attempts: 10,
            check_interval_ms: 5000,
            use_flat_delay: false,
            reset_on_exhaustion: false,
        }
    }
}

impl From<ReconnectionConfig> for InternalReconnectionConfig {
    fn from(c: ReconnectionConfig) -> Self {
        let mut config = InternalReconnectionConfig::new(
            std::time::Duration::from_millis(c.base_delay_ms),
            std::time::Duration::from_millis(c.max_delay_ms),
            c.max_attempts,
            std::time::Duration::from_millis(c.check_interval_ms),
        );
        config.use_flat_delay = c.use_flat_delay;
        config.reset_on_exhaustion = c.reset_on_exhaustion;
        config
    }
}

/// Result of checking if a peer should be reconnected
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ReconnectionStatus {
    /// Ready to attempt reconnection
    Ready,
    /// Waiting for backoff delay to expire
    Waiting {
        /// Time remaining until next attempt is allowed in milliseconds
        remaining_ms: u64,
    },
    /// Maximum attempts exceeded, peer is abandoned
    Exhausted {
        /// Number of attempts that were made
        attempts: u32,
    },
    /// Peer is not being tracked for reconnection
    NotTracked,
}

impl From<InternalReconnectionStatus> for ReconnectionStatus {
    fn from(s: InternalReconnectionStatus) -> Self {
        match s {
            InternalReconnectionStatus::Ready => ReconnectionStatus::Ready,
            InternalReconnectionStatus::Waiting { remaining } => ReconnectionStatus::Waiting {
                remaining_ms: remaining.as_millis() as u64,
            },
            InternalReconnectionStatus::Exhausted { attempts } => {
                ReconnectionStatus::Exhausted { attempts }
            }
            InternalReconnectionStatus::NotTracked => ReconnectionStatus::NotTracked,
        }
    }
}

/// Statistics for a peer's reconnection state
#[derive(Debug, Clone, uniffi::Record)]
pub struct PeerReconnectionStats {
    /// Number of attempts made
    pub attempts: u32,
    /// Maximum allowed attempts
    pub max_attempts: u32,
    /// How long since the peer disconnected in milliseconds
    pub disconnected_duration_ms: u64,
    /// Time until next reconnection attempt in milliseconds (0 if ready, u64::MAX if exhausted)
    pub next_attempt_delay_ms: u64,
}

impl From<InternalPeerReconnectionStats> for PeerReconnectionStats {
    fn from(s: InternalPeerReconnectionStats) -> Self {
        PeerReconnectionStats {
            attempts: s.attempts,
            max_attempts: s.max_attempts,
            disconnected_duration_ms: s.disconnected_duration.as_millis() as u64,
            next_attempt_delay_ms: s.next_attempt_delay.as_millis() as u64,
        }
    }
}

/// Manager for auto-reconnection with exponential backoff
#[derive(uniffi::Object)]
pub struct ReconnectionManager {
    inner: std::sync::Mutex<InternalReconnectionManager>,
}

#[uniffi::export]
impl ReconnectionManager {
    /// Create a new reconnection manager with the given configuration
    #[uniffi::constructor]
    pub fn new(config: ReconnectionConfig) -> Arc<Self> {
        Arc::new(Self {
            inner: std::sync::Mutex::new(InternalReconnectionManager::new(config.into())),
        })
    }

    /// Create a manager with default configuration
    #[uniffi::constructor]
    pub fn with_defaults() -> Arc<Self> {
        Arc::new(Self {
            inner: std::sync::Mutex::new(InternalReconnectionManager::with_defaults()),
        })
    }

    /// Track a peer for reconnection after disconnection
    pub fn track_disconnection(&self, address: String) {
        self.inner.lock().unwrap().track_disconnection(address);
    }

    /// Check if a peer is being tracked for reconnection
    pub fn is_tracked(&self, address: &str) -> bool {
        self.inner.lock().unwrap().is_tracked(address)
    }

    /// Get the reconnection status for a peer
    pub fn get_status(&self, address: &str) -> ReconnectionStatus {
        self.inner.lock().unwrap().get_status(address).into()
    }

    /// Get all peers that are ready for a reconnection attempt
    pub fn get_peers_to_reconnect(&self) -> Vec<String> {
        self.inner.lock().unwrap().get_peers_to_reconnect()
    }

    /// Record a reconnection attempt for a peer
    pub fn record_attempt(&self, address: &str) {
        self.inner.lock().unwrap().record_attempt(address);
    }

    /// Called when a connection succeeds
    pub fn on_connection_success(&self, address: &str) {
        self.inner.lock().unwrap().on_connection_success(address);
    }

    /// Stop tracking a peer
    pub fn stop_tracking(&self, address: &str) {
        self.inner.lock().unwrap().stop_tracking(address);
    }

    /// Clear all reconnection tracking
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    /// Get the number of peers being tracked
    pub fn tracked_count(&self) -> u32 {
        self.inner.lock().unwrap().tracked_count() as u32
    }

    /// Get statistics for a peer
    pub fn get_peer_stats(&self, address: &str) -> Option<PeerReconnectionStats> {
        self.inner
            .lock()
            .unwrap()
            .get_peer_stats(address)
            .map(|s| s.into())
    }

    /// Get the check interval from configuration in milliseconds
    pub fn check_interval_ms(&self) -> u64 {
        self.inner.lock().unwrap().check_interval().as_millis() as u64
    }
}

// ============================================================================
// Peer Lifetime Manager (stale peer cleanup)
// ============================================================================

use crate::peer_lifetime::{
    PeerInfo as InternalPeerInfo, PeerLifetimeConfig as InternalPeerLifetimeConfig,
    PeerLifetimeManager as InternalPeerLifetimeManager,
    PeerLifetimeStats as InternalPeerLifetimeStats, StalePeerInfo as InternalStalePeerInfo,
    StaleReason as InternalStaleReason,
};

/// Configuration for peer lifetime management
#[derive(Debug, Clone, uniffi::Record)]
pub struct PeerLifetimeConfig {
    /// Timeout for disconnected peers in milliseconds
    pub disconnected_timeout_ms: u64,
    /// Timeout for connected peers in milliseconds
    pub connected_timeout_ms: u64,
    /// Interval for cleanup checks in milliseconds
    pub cleanup_interval_ms: u64,
}

impl Default for PeerLifetimeConfig {
    fn default() -> Self {
        Self {
            disconnected_timeout_ms: 30000,
            connected_timeout_ms: 60000,
            cleanup_interval_ms: 10000,
        }
    }
}

impl From<PeerLifetimeConfig> for InternalPeerLifetimeConfig {
    fn from(c: PeerLifetimeConfig) -> Self {
        InternalPeerLifetimeConfig::new(
            std::time::Duration::from_millis(c.disconnected_timeout_ms),
            std::time::Duration::from_millis(c.connected_timeout_ms),
            std::time::Duration::from_millis(c.cleanup_interval_ms),
        )
    }
}

/// Reason a peer is considered stale
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum StaleReason {
    /// Disconnected peer hasn't been seen in a while
    DisconnectedTimeout,
    /// Connected peer hasn't had activity (possible ghost connection)
    ConnectedTimeout,
}

impl From<InternalStaleReason> for StaleReason {
    fn from(r: InternalStaleReason) -> Self {
        match r {
            InternalStaleReason::DisconnectedTimeout => StaleReason::DisconnectedTimeout,
            InternalStaleReason::ConnectedTimeout => StaleReason::ConnectedTimeout,
        }
    }
}

/// Information about a stale peer
#[derive(Debug, Clone, uniffi::Record)]
pub struct StalePeerInfo {
    /// Peer address
    pub address: String,
    /// Why the peer is considered stale
    pub reason: StaleReason,
    /// How long since the peer was last seen in milliseconds
    pub time_since_last_seen_ms: u64,
    /// Whether the peer was connected when it went stale
    pub was_connected: bool,
}

impl From<InternalStalePeerInfo> for StalePeerInfo {
    fn from(p: InternalStalePeerInfo) -> Self {
        StalePeerInfo {
            address: p.address,
            reason: p.reason.into(),
            time_since_last_seen_ms: p.time_since_last_seen.as_millis() as u64,
            was_connected: p.was_connected,
        }
    }
}

/// Statistics about tracked peers
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct PeerLifetimeStats {
    /// Total number of tracked peers
    pub total_tracked: u32,
    /// Number of connected peers
    pub connected: u32,
    /// Number of disconnected peers
    pub disconnected: u32,
}

impl From<InternalPeerLifetimeStats> for PeerLifetimeStats {
    fn from(s: InternalPeerLifetimeStats) -> Self {
        PeerLifetimeStats {
            total_tracked: s.total_tracked as u32,
            connected: s.connected as u32,
            disconnected: s.disconnected as u32,
        }
    }
}

/// Detailed information about a peer
#[derive(Debug, Clone, uniffi::Record)]
pub struct PeerInfo {
    /// Whether the peer is currently connected
    pub connected: bool,
    /// Time since last activity in milliseconds
    pub time_since_last_seen_ms: u64,
    /// Time since first discovery in milliseconds
    pub time_since_first_seen_ms: u64,
    /// Time since disconnect in milliseconds (if disconnected)
    pub time_since_disconnect_ms: Option<u64>,
}

impl From<InternalPeerInfo> for PeerInfo {
    fn from(p: InternalPeerInfo) -> Self {
        PeerInfo {
            connected: p.connected,
            time_since_last_seen_ms: p.time_since_last_seen.as_millis() as u64,
            time_since_first_seen_ms: p.time_since_first_seen.as_millis() as u64,
            time_since_disconnect_ms: p.time_since_disconnect.map(|d| d.as_millis() as u64),
        }
    }
}

/// Manager for peer lifetime and stale peer cleanup
#[derive(uniffi::Object)]
pub struct PeerLifetimeManager {
    inner: std::sync::Mutex<InternalPeerLifetimeManager>,
}

#[uniffi::export]
impl PeerLifetimeManager {
    /// Create a new peer lifetime manager with the given configuration
    #[uniffi::constructor]
    pub fn new(config: PeerLifetimeConfig) -> Arc<Self> {
        Arc::new(Self {
            inner: std::sync::Mutex::new(InternalPeerLifetimeManager::new(config.into())),
        })
    }

    /// Create a manager with default configuration
    #[uniffi::constructor]
    pub fn with_defaults() -> Arc<Self> {
        Arc::new(Self {
            inner: std::sync::Mutex::new(InternalPeerLifetimeManager::with_defaults()),
        })
    }

    /// Record activity for a peer
    pub fn on_peer_activity(&self, address: &str, connected: bool) {
        self.inner
            .lock()
            .unwrap()
            .on_peer_activity(address, connected);
    }

    /// Record that a peer has disconnected
    pub fn on_peer_disconnected(&self, address: &str) {
        self.inner.lock().unwrap().on_peer_disconnected(address);
    }

    /// Check if a peer is being tracked
    pub fn is_tracked(&self, address: &str) -> bool {
        self.inner.lock().unwrap().is_tracked(address)
    }

    /// Check if a peer is connected
    pub fn is_connected(&self, address: &str) -> bool {
        self.inner.lock().unwrap().is_connected(address)
    }

    /// Get the list of stale peers that should be removed
    pub fn get_stale_peers(&self) -> Vec<StalePeerInfo> {
        self.inner
            .lock()
            .unwrap()
            .get_stale_peers()
            .into_iter()
            .map(|p| p.into())
            .collect()
    }

    /// Get just the addresses of stale peers
    pub fn get_stale_peer_addresses(&self) -> Vec<String> {
        self.inner.lock().unwrap().get_stale_peer_addresses()
    }

    /// Remove a peer from tracking
    pub fn remove_peer(&self, address: &str) -> bool {
        self.inner.lock().unwrap().remove_peer(address)
    }

    /// Remove all stale peers and return their info
    pub fn cleanup_stale_peers(&self) -> Vec<StalePeerInfo> {
        self.inner
            .lock()
            .unwrap()
            .cleanup_stale_peers()
            .into_iter()
            .map(|p| p.into())
            .collect()
    }

    /// Get statistics about tracked peers
    pub fn stats(&self) -> PeerLifetimeStats {
        self.inner.lock().unwrap().stats().into()
    }

    /// Get detailed info about a specific peer
    pub fn get_peer_info(&self, address: &str) -> Option<PeerInfo> {
        self.inner
            .lock()
            .unwrap()
            .get_peer_info(address)
            .map(|p| p.into())
    }

    /// Clear all tracked peers
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    /// Get the number of tracked peers
    pub fn tracked_count(&self) -> u32 {
        self.inner.lock().unwrap().tracked_count() as u32
    }

    /// Get the cleanup interval from configuration in milliseconds
    pub fn cleanup_interval_ms(&self) -> u64 {
        self.inner.lock().unwrap().cleanup_interval().as_millis() as u64
    }
}

// ============================================================================
// Address Rotation Handler (BLE address rotation for WearOS)
// ============================================================================

use crate::address_rotation::{
    AddressRotationHandler as InternalAddressRotationHandler,
    AddressRotationStats as InternalAddressRotationStats,
    DeviceLookupResult as InternalDeviceLookupResult, DevicePattern as InternalDevicePattern,
};

/// Patterns that indicate a device may rotate its BLE address
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DevicePattern {
    /// WearTAK on WearOS (WT-WEAROS-XXXX)
    WearTak,
    /// Generic WearOS device (WEAROS-XXXX)
    WearOs,
    /// Eche mesh device (ECHE_MESH-XXXX or ECHE-XXXX)
    Eche,
    /// Unknown pattern (may still rotate addresses)
    Unknown,
}

impl From<InternalDevicePattern> for DevicePattern {
    fn from(p: InternalDevicePattern) -> Self {
        match p {
            InternalDevicePattern::WearTak => DevicePattern::WearTak,
            InternalDevicePattern::WearOs => DevicePattern::WearOs,
            InternalDevicePattern::Eche => DevicePattern::Eche,
            InternalDevicePattern::Unknown => DevicePattern::Unknown,
        }
    }
}

impl DevicePattern {
    /// Check if this device type is known to rotate addresses
    pub fn rotates_addresses(&self) -> bool {
        matches!(self, DevicePattern::WearTak | DevicePattern::WearOs)
    }
}

/// Result of looking up a device by name
#[derive(Debug, Clone, uniffi::Record)]
pub struct DeviceLookupResult {
    /// The node ID for this device
    pub node_id: u32,
    /// The current known address
    pub current_address: String,
    /// Whether the address has changed
    pub address_changed: bool,
    /// The previous address (if changed)
    pub previous_address: Option<String>,
}

impl From<InternalDeviceLookupResult> for DeviceLookupResult {
    fn from(r: InternalDeviceLookupResult) -> Self {
        DeviceLookupResult {
            node_id: r.node_id.as_u32(),
            current_address: r.current_address,
            address_changed: r.address_changed,
            previous_address: r.previous_address,
        }
    }
}

/// Statistics about address rotation handling
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct AddressRotationStats {
    /// Number of devices tracked by name
    pub devices_with_names: u32,
    /// Total number of devices tracked
    pub total_devices: u32,
    /// Number of address mappings
    pub address_mappings: u32,
}

impl From<InternalAddressRotationStats> for AddressRotationStats {
    fn from(s: InternalAddressRotationStats) -> Self {
        AddressRotationStats {
            devices_with_names: s.devices_with_names as u32,
            total_devices: s.total_devices as u32,
            address_mappings: s.address_mappings as u32,
        }
    }
}

/// Handler for BLE address rotation
#[derive(uniffi::Object)]
pub struct AddressRotationHandler {
    inner: std::sync::Mutex<InternalAddressRotationHandler>,
}

#[uniffi::export]
impl AddressRotationHandler {
    /// Create a new address rotation handler
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: std::sync::Mutex::new(InternalAddressRotationHandler::new()),
        })
    }

    /// Register a new device
    pub fn register_device(&self, name: &str, address: &str, node_id: u32) {
        self.inner
            .lock()
            .unwrap()
            .register_device(name, address, NodeId::new(node_id));
    }

    /// Look up a device by name
    pub fn lookup_by_name(&self, name: &str) -> Option<u32> {
        self.inner
            .lock()
            .unwrap()
            .lookup_by_name(name)
            .map(|n| n.as_u32())
    }

    /// Look up a device by address
    pub fn lookup_by_address(&self, address: &str) -> Option<u32> {
        self.inner
            .lock()
            .unwrap()
            .lookup_by_address(address)
            .map(|n| n.as_u32())
    }

    /// Get the current address for a node
    pub fn get_address(&self, node_id: u32) -> Option<String> {
        self.inner
            .lock()
            .unwrap()
            .get_address(&NodeId::new(node_id))
            .cloned()
    }

    /// Get the name for a node
    pub fn get_name(&self, node_id: u32) -> Option<String> {
        self.inner
            .lock()
            .unwrap()
            .get_name(&NodeId::new(node_id))
            .cloned()
    }

    /// Handle a device discovery, detecting address rotation
    pub fn on_device_discovered(&self, name: &str, address: &str) -> Option<DeviceLookupResult> {
        self.inner
            .lock()
            .unwrap()
            .on_device_discovered(name, address)
            .map(|r| r.into())
    }

    /// Update the address for a device (used when address rotation is detected)
    pub fn update_address(&self, name: &str, new_address: &str) -> bool {
        self.inner.lock().unwrap().update_address(name, new_address)
    }

    /// Update the name for a device
    pub fn update_name(&self, node_id: u32, new_name: &str) {
        self.inner
            .lock()
            .unwrap()
            .update_name(NodeId::new(node_id), new_name);
    }

    /// Remove a device from all mappings
    pub fn remove_device(&self, node_id: u32) {
        self.inner
            .lock()
            .unwrap()
            .remove_device(&NodeId::new(node_id));
    }

    /// Clear all mappings
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    /// Get the number of tracked devices
    pub fn device_count(&self) -> u32 {
        self.inner.lock().unwrap().device_count() as u32
    }

    /// Get statistics about tracked mappings
    pub fn stats(&self) -> AddressRotationStats {
        self.inner.lock().unwrap().stats().into()
    }
}

// ============================================================================
// Address Rotation Helper Functions
// ============================================================================

/// Detect the device pattern from a BLE device name
#[uniffi::export]
pub fn detect_device_pattern(name: &str) -> DevicePattern {
    crate::address_rotation::detect_device_pattern(name).into()
}

/// Check if a device name matches a WearTAK/WearOS pattern
#[uniffi::export]
pub fn is_weartak_device(name: &str) -> bool {
    crate::address_rotation::is_weartak_device(name)
}

/// Normalize a WearTAK device name (removes "WT-" prefix if present)
#[uniffi::export]
pub fn normalize_weartak_name(name: &str) -> String {
    crate::address_rotation::normalize_weartak_name(name).to_string()
}

/// Check if a device pattern is known to rotate addresses
#[uniffi::export]
pub fn device_pattern_rotates_addresses(pattern: DevicePattern) -> bool {
    pattern.rotates_addresses()
}
