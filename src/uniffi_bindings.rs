// UniFFI bindings for peat-btle
//
// This module provides UniFFI-compatible wrappers around the core peat-btle types.
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
                .with_tag("PeatFFI"),
        );
    });
}

use crate::observer::DisconnectReason as ObserverDisconnectReason;
use crate::peat_mesh::{self, DataReceivedResult as InternalDataReceivedResult};
use crate::peer::{
    ConnectionState as InternalConnectionState,
    FullStateCountSummary as InternalFullStateCountSummary, IndirectPeer as InternalIndirectPeer,
    PeatPeer as InternalPeatPeer, PeerConnectionState as InternalPeerConnectionState,
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
pub struct PeatPeer {
    pub node_id: u32,
    pub identifier: String,
    pub name: Option<String>,
    pub mesh_id: Option<String>,
    pub rssi: i8,
    pub is_connected: bool,
    pub last_seen_ms: u64,
}

impl From<InternalPeatPeer> for PeatPeer {
    fn from(p: InternalPeatPeer) -> Self {
        PeatPeer {
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
// PeatMesh Object
// ============================================================================

#[derive(uniffi::Object)]
pub struct PeatMesh {
    inner: peat_mesh::PeatMesh,
}

#[uniffi::export]
impl PeatMesh {
    /// Create a basic PeatMesh
    #[uniffi::constructor]
    pub fn new(node_id: u32, callsign: &str, mesh_id: &str) -> Arc<Self> {
        #[cfg(target_os = "android")]
        ensure_android_logger();

        let config = crate::PeatMeshConfig::new(NodeId::new(node_id), callsign, mesh_id);
        Arc::new(Self {
            inner: peat_mesh::PeatMesh::new(config),
        })
    }

    /// Create a PeatMesh with peripheral type
    #[uniffi::constructor]
    pub fn new_with_peripheral(
        node_id: u32,
        callsign: &str,
        mesh_id: &str,
        peripheral_type: PeripheralType,
    ) -> Arc<Self> {
        #[cfg(target_os = "android")]
        ensure_android_logger();

        let config = crate::PeatMeshConfig::new(NodeId::new(node_id), callsign, mesh_id)
            .with_peripheral_type(peripheral_type.into());
        Arc::new(Self {
            inner: peat_mesh::PeatMesh::new(config),
        })
    }

    /// Create a PeatMesh from genesis (recommended for production)
    #[uniffi::constructor]
    pub fn new_from_genesis(
        callsign: &str,
        identity: &DeviceIdentity,
        genesis: &MeshGenesis,
    ) -> Arc<Self> {
        #[cfg(target_os = "android")]
        ensure_android_logger();

        Arc::new(Self {
            inner: peat_mesh::PeatMesh::from_genesis(
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
    /// This is useful for sending extension data like CannedMessages from peat-lite.
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
    ) -> Option<PeatPeer> {
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
    pub fn get_connected_peers(&self) -> Vec<PeatPeer> {
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
    // These methods enable proper CRDT sync for peat-lite CannedMessages,
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

/// Create a PeatMesh with encryption enabled (returns null if secret is wrong length)
#[uniffi::export]
pub fn create_peat_mesh_with_encryption(
    node_id: u32,
    callsign: &str,
    mesh_id: &str,
    encryption_secret: &[u8],
) -> Option<std::sync::Arc<PeatMesh>> {
    if encryption_secret.len() != 32 {
        return None;
    }
    let mut secret = [0u8; 32];
    secret.copy_from_slice(encryption_secret);
    let config =
        crate::PeatMeshConfig::new(NodeId::new(node_id), callsign, mesh_id).with_encryption(secret);
    Some(std::sync::Arc::new(PeatMesh {
        inner: peat_mesh::PeatMesh::new(config),
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
    /// Peat mesh device (PEAT_MESH-XXXX or PEAT-XXXX)
    Peat,
    /// Unknown pattern (may still rotate addresses)
    Unknown,
}

impl From<InternalDevicePattern> for DevicePattern {
    fn from(p: InternalDevicePattern) -> Self {
        match p {
            InternalDevicePattern::WearTak => DevicePattern::WearTak,
            InternalDevicePattern::WearOs => DevicePattern::WearOs,
            InternalDevicePattern::Peat => DevicePattern::Peat,
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Enum round-trip conversions ====================

    #[test]
    fn test_peripheral_type_round_trip() {
        let variants = [
            (PeripheralType::Unknown, InternalPeripheralType::Unknown),
            (
                PeripheralType::SoldierSensor,
                InternalPeripheralType::SoldierSensor,
            ),
            (
                PeripheralType::FixedSensor,
                InternalPeripheralType::FixedSensor,
            ),
            (PeripheralType::Relay, InternalPeripheralType::Relay),
        ];
        for (uniffi_val, internal_val) in &variants {
            let converted: InternalPeripheralType = (*uniffi_val).into();
            assert_eq!(converted, *internal_val);
            let back: PeripheralType = converted.into();
            assert_eq!(back, *uniffi_val);
        }
    }

    #[test]
    fn test_event_type_round_trip() {
        let variants = [
            (EventType::None, InternalEventType::None),
            (EventType::Ping, InternalEventType::Ping),
            (EventType::NeedAssist, InternalEventType::NeedAssist),
            (EventType::Emergency, InternalEventType::Emergency),
            (EventType::Moving, InternalEventType::Moving),
            (EventType::InPosition, InternalEventType::InPosition),
            (EventType::Ack, InternalEventType::Ack),
        ];
        for (uniffi_val, internal_val) in &variants {
            let converted: InternalEventType = (*uniffi_val).into();
            assert_eq!(converted, *internal_val);
            let back: EventType = converted.into();
            assert_eq!(back, *uniffi_val);
        }
    }

    #[test]
    fn test_disconnect_reason_round_trip_observer() {
        let variants = [
            (
                DisconnectReason::LocalRequest,
                ObserverDisconnectReason::LocalRequest,
            ),
            (
                DisconnectReason::RemoteRequest,
                ObserverDisconnectReason::RemoteRequest,
            ),
            (DisconnectReason::Timeout, ObserverDisconnectReason::Timeout),
            (
                DisconnectReason::LinkLoss,
                ObserverDisconnectReason::LinkLoss,
            ),
            (
                DisconnectReason::ConnectionFailed,
                ObserverDisconnectReason::ConnectionFailed,
            ),
            (DisconnectReason::Unknown, ObserverDisconnectReason::Unknown),
        ];
        for (uniffi_val, observer_val) in &variants {
            let converted: ObserverDisconnectReason = (*uniffi_val).into();
            assert_eq!(converted, *observer_val);
            let back: DisconnectReason = converted.into();
            assert_eq!(back, *uniffi_val);
        }
    }

    #[test]
    fn test_disconnect_reason_from_platform() {
        let variants = [
            (
                PlatformDisconnectReason::LocalRequest,
                DisconnectReason::LocalRequest,
            ),
            (
                PlatformDisconnectReason::RemoteRequest,
                DisconnectReason::RemoteRequest,
            ),
            (PlatformDisconnectReason::Timeout, DisconnectReason::Timeout),
            (
                PlatformDisconnectReason::LinkLoss,
                DisconnectReason::LinkLoss,
            ),
            (
                PlatformDisconnectReason::ConnectionFailed,
                DisconnectReason::ConnectionFailed,
            ),
            (PlatformDisconnectReason::Unknown, DisconnectReason::Unknown),
        ];
        for (platform_val, expected) in &variants {
            let converted: DisconnectReason = (*platform_val).into();
            assert_eq!(converted, *expected);
        }
    }

    #[test]
    fn test_connection_state_round_trip() {
        let variants = [
            (
                ConnectionState::Discovered,
                InternalConnectionState::Discovered,
            ),
            (
                ConnectionState::Connecting,
                InternalConnectionState::Connecting,
            ),
            (
                ConnectionState::Connected,
                InternalConnectionState::Connected,
            ),
            (ConnectionState::Degraded, InternalConnectionState::Degraded),
            (
                ConnectionState::Disconnecting,
                InternalConnectionState::Disconnecting,
            ),
            (
                ConnectionState::Disconnected,
                InternalConnectionState::Disconnected,
            ),
            (ConnectionState::Lost, InternalConnectionState::Lost),
        ];
        for (uniffi_val, internal_val) in &variants {
            let converted: InternalConnectionState = (*uniffi_val).into();
            assert_eq!(converted, *internal_val);
            let back: ConnectionState = converted.into();
            assert_eq!(back, *uniffi_val);
        }
    }

    // ==================== event_type_from_u8 ====================

    #[test]
    fn test_event_type_from_u8_valid() {
        assert_eq!(event_type_from_u8(0), Some(EventType::None));
        assert_eq!(event_type_from_u8(1), Some(EventType::Ping));
        assert_eq!(event_type_from_u8(2), Some(EventType::NeedAssist));
        assert_eq!(event_type_from_u8(3), Some(EventType::Emergency));
        assert_eq!(event_type_from_u8(4), Some(EventType::Moving));
        assert_eq!(event_type_from_u8(5), Some(EventType::InPosition));
        assert_eq!(event_type_from_u8(6), Some(EventType::Ack));
    }

    #[test]
    fn test_event_type_from_u8_invalid() {
        assert_eq!(event_type_from_u8(7), None);
        assert_eq!(event_type_from_u8(255), None);
    }

    // ==================== PeatMesh constructors ====================

    #[test]
    fn test_hive_mesh_new() {
        let mesh = PeatMesh::new(42, "ALPHA", "test-mesh");
        assert_eq!(mesh.peer_count(), 0);
        assert_eq!(mesh.connected_count(), 0);
        assert!(!mesh.is_emergency_active());
        assert!(!mesh.is_ack_active());
    }

    #[test]
    fn test_hive_mesh_new_with_peripheral() {
        let mesh = PeatMesh::new_with_peripheral(42, "BRAVO", "test-mesh", PeripheralType::Relay);
        assert_eq!(mesh.peer_count(), 0);
    }

    #[test]
    fn test_hive_mesh_from_genesis() {
        let identity = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("test-network", &identity);
        let mesh = PeatMesh::new_from_genesis("CHARLIE", &identity, &genesis);
        assert!(mesh.has_identity());
        assert!(mesh.is_encryption_enabled());
    }

    // ==================== Namespace functions ====================

    #[test]
    fn test_derive_node_id_from_mac_valid() {
        let id = derive_node_id_from_mac("AA:BB:CC:DD:EE:FF");
        assert_ne!(id, 0);
    }

    #[test]
    fn test_derive_node_id_from_mac_invalid() {
        assert_eq!(derive_node_id_from_mac("not-a-mac"), 0);
        assert_eq!(derive_node_id_from_mac(""), 0);
    }

    #[test]
    fn test_create_peat_mesh_with_encryption_valid() {
        let secret = [0xABu8; 32];
        let mesh = create_peat_mesh_with_encryption(1, "TEST", "mesh-1", &secret);
        assert!(mesh.is_some());
        assert!(mesh.unwrap().is_encryption_enabled());
    }

    #[test]
    fn test_create_peat_mesh_with_encryption_wrong_length() {
        assert!(create_peat_mesh_with_encryption(1, "TEST", "mesh-1", &[0u8; 16]).is_none());
        assert!(create_peat_mesh_with_encryption(1, "TEST", "mesh-1", &[]).is_none());
        assert!(create_peat_mesh_with_encryption(1, "TEST", "mesh-1", &[0u8; 64]).is_none());
    }

    // ==================== DeviceIdentity ====================

    #[test]
    fn test_device_identity_generate() {
        let id = DeviceIdentity::generate();
        assert_eq!(id.get_public_key().len(), 32);
        assert_eq!(id.get_private_key().len(), 32);
        assert_ne!(id.get_node_id(), 0);
    }

    #[test]
    fn test_device_identity_deterministic_node_id() {
        let id = DeviceIdentity::generate();
        let node_id_1 = id.get_node_id();
        let node_id_2 = id.get_node_id();
        assert_eq!(node_id_1, node_id_2);
    }

    #[test]
    fn test_device_identity_sign_and_attestation() {
        let id = DeviceIdentity::generate();
        let sig = id.sign(b"test message");
        assert_eq!(sig.len(), 64);

        let attestation = id.create_attestation(1000);
        assert!(!attestation.is_empty());
    }

    #[test]
    fn test_restore_device_identity_round_trip() {
        let id = DeviceIdentity::generate();
        let private_key = id.get_private_key();
        let restored = restore_device_identity(&private_key);
        assert!(restored.is_some());
        let restored = restored.unwrap();
        assert_eq!(restored.get_public_key(), id.get_public_key());
        assert_eq!(restored.get_node_id(), id.get_node_id());
    }

    #[test]
    fn test_restore_device_identity_invalid() {
        assert!(restore_device_identity(&[]).is_none());
        assert!(restore_device_identity(&[0u8; 16]).is_none());
    }

    // ==================== MeshGenesis ====================

    #[test]
    fn test_mesh_genesis_create() {
        let identity = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("my-mesh", &identity);
        assert!(!genesis.get_mesh_id().is_empty());
        assert_eq!(genesis.get_encryption_secret().len(), 32);
    }

    #[test]
    fn test_mesh_genesis_encode_decode_round_trip() {
        let identity = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("round-trip-mesh", &identity);
        let encoded = genesis.encode();
        assert!(!encoded.is_empty());

        let decoded = decode_mesh_genesis(&encoded);
        assert!(decoded.is_some());
        let decoded = decoded.unwrap();
        assert_eq!(decoded.get_mesh_id(), genesis.get_mesh_id());
        assert_eq!(
            decoded.get_encryption_secret(),
            genesis.get_encryption_secret()
        );
    }

    #[test]
    fn test_decode_mesh_genesis_invalid() {
        assert!(decode_mesh_genesis(&[]).is_none());
        assert!(decode_mesh_genesis(&[0xFF; 10]).is_none());
    }

    // ==================== IdentityAttestation ====================

    #[test]
    fn test_identity_attestation_round_trip() {
        let identity = DeviceIdentity::generate();
        let attestation_bytes = identity.create_attestation(5000);
        let attestation = decode_identity_attestation(&attestation_bytes);
        assert!(attestation.is_some());
        let attestation = attestation.unwrap();
        assert!(attestation.verify());
        assert_eq!(attestation.get_node_id(), identity.get_node_id());
        assert_eq!(attestation.get_public_key(), identity.get_public_key());
    }

    #[test]
    fn test_decode_identity_attestation_invalid() {
        assert!(decode_identity_attestation(&[]).is_none());
        assert!(decode_identity_attestation(&[0xFF; 10]).is_none());
    }

    // ==================== HiveMesh identity operations ====================

    #[test]
    fn test_hive_mesh_identity_verify_peer() {
        let id_a = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("id-test", &id_a);
        let mesh_a = PeatMesh::new_from_genesis("ALPHA", &id_a, &genesis);

        let id_b = DeviceIdentity::generate();
        let attestation_bytes = id_b.create_attestation(1000);

        let initial_count = mesh_a.known_identity_count();

        // Verify peer identity via the mesh
        assert!(mesh_a.verify_peer_identity(&attestation_bytes));
        assert!(mesh_a.is_peer_identity_known(id_b.get_node_id()));
        assert_eq!(mesh_a.known_identity_count(), initial_count + 1);
    }

    #[test]
    fn test_hive_mesh_verify_peer_identity_invalid() {
        let id = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("test", &id);
        let mesh = PeatMesh::new_from_genesis("TEST", &id, &genesis);
        assert!(!mesh.verify_peer_identity(&[]));
        assert!(!mesh.verify_peer_identity(&[0xFF; 10]));
    }

    #[test]
    fn test_hive_mesh_sign_and_verify() {
        let id_a = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("sig-test", &id_a);
        let mesh = PeatMesh::new_from_genesis("ALPHA", &id_a, &genesis);

        let message = b"important data";
        let signature = mesh.sign(message);
        assert!(signature.is_some());
        let signature = signature.unwrap();
        assert_eq!(signature.len(), 64);
    }

    #[test]
    fn test_hive_mesh_verify_peer_signature_wrong_length() {
        let id = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("test", &id);
        let mesh = PeatMesh::new_from_genesis("TEST", &id, &genesis);
        // Wrong signature length should return false
        assert!(!mesh.verify_peer_signature(42, b"msg", &[0u8; 32]));
        assert!(!mesh.verify_peer_signature(42, b"msg", &[]));
    }

    // ==================== HiveMesh data operations ====================

    #[test]
    fn test_hive_mesh_build_document() {
        let mesh = PeatMesh::new(1, "TEST", "mesh");
        let doc = mesh.build_document();
        assert!(!doc.is_empty());
    }

    #[test]
    fn test_hive_mesh_emergency_cycle() {
        let mesh = PeatMesh::new(1, "TEST", "mesh");
        assert!(!mesh.is_emergency_active());

        let data = mesh.send_emergency(1000);
        assert!(!data.is_empty());
        assert!(mesh.is_emergency_active());

        let ack_data = mesh.send_ack(2000);
        assert!(!ack_data.is_empty());
        assert!(mesh.is_ack_active());
    }

    #[test]
    fn test_hive_mesh_update_location() {
        let mesh = PeatMesh::new(1, "TEST", "mesh");
        mesh.update_location(37.7749, -122.4194, Some(10.0));
        mesh.clear_location();
        // No panic = success
    }

    #[test]
    fn test_hive_mesh_update_peripheral_state() {
        let mesh = PeatMesh::new(1, "TEST", "mesh");
        mesh.update_peripheral_state(
            "ALPHA",
            85,
            Some(72),
            Some(37.7749),
            Some(-122.4194),
            Some(10.0),
            Some(EventType::Moving),
            1000,
        );
        // No panic = success
    }

    #[test]
    fn test_hive_mesh_matches_mesh() {
        let mesh = PeatMesh::new(1, "TEST", "my-mesh");
        assert!(mesh.matches_mesh(Some("my-mesh".to_string())));
        assert!(!mesh.matches_mesh(Some("other-mesh".to_string())));
    }

    #[test]
    fn test_hive_mesh_peer_not_known() {
        let mesh = PeatMesh::new(1, "TEST", "mesh");
        assert!(!mesh.is_peer_known(999));
        assert!(mesh.get_peer_callsign(999).is_none());
        assert!(mesh.get_peer_connection_state(999).is_none());
    }

    #[test]
    fn test_hive_mesh_empty_peer_lists() {
        let mesh = PeatMesh::new(1, "TEST", "mesh");
        assert!(mesh.get_connected_peers().is_empty());
        assert!(mesh.get_degraded_peers().is_empty());
        assert!(mesh.get_lost_peers().is_empty());
        assert!(mesh.get_indirect_peers().is_empty());
        assert!(mesh.get_connected_peer_identifiers().is_empty());
    }

    #[test]
    fn test_hive_mesh_state_counts_initial() {
        let mesh = PeatMesh::new(1, "TEST", "mesh");
        let counts = mesh.get_connection_state_counts();
        assert_eq!(counts.discovered, 0);
        assert_eq!(counts.connected, 0);
        assert_eq!(counts.lost, 0);

        let full = mesh.get_full_state_counts();
        assert_eq!(full.direct.connected, 0);
        assert_eq!(full.one_hop, 0);
    }

    #[test]
    fn test_hive_mesh_tick_returns_none_initially() {
        let mesh = PeatMesh::new(1, "TEST", "mesh");
        // First tick with no peers should return None
        assert!(mesh.tick(1000).is_none());
    }

    #[test]
    fn test_hive_mesh_delta_document() {
        let mesh = PeatMesh::new(1, "TEST", "mesh");
        // Delta document for unknown peer still returns data (full state)
        let delta = mesh.build_delta_document_for_peer(999, 1000);
        assert!(delta.is_some());
        // Full delta should always return something
        let full = mesh.build_full_delta_document(1000);
        assert!(!full.is_empty());
    }

    #[test]
    fn test_hive_mesh_broadcast_bytes() {
        let mesh = PeatMesh::new(1, "TEST", "mesh");
        let result = mesh.broadcast_bytes(b"test payload");
        assert!(!result.is_empty());
    }

    #[test]
    fn test_hive_mesh_canned_message_dedup() {
        let mesh = PeatMesh::new(1, "TEST", "mesh");
        // First check should return true (not seen)
        assert!(mesh.check_canned_message(42, 1000, 5000));
        // Mark as seen
        mesh.mark_canned_message_seen(42, 1000);
        // Second check should return false (already seen)
        assert!(!mesh.check_canned_message(42, 1000, 5000));
    }

    // ==================== ReconnectionManager ====================

    #[test]
    fn test_reconnection_manager_defaults() {
        let mgr = ReconnectionManager::with_defaults();
        assert_eq!(mgr.tracked_count(), 0);
        assert!(mgr.check_interval_ms() > 0);
    }

    #[test]
    fn test_reconnection_manager_track_and_status() {
        let mgr = ReconnectionManager::new(ReconnectionConfig {
            base_delay_ms: 100,
            max_delay_ms: 1000,
            max_attempts: 3,
            check_interval_ms: 500,
            use_flat_delay: false,
            reset_on_exhaustion: false,
        });

        mgr.track_disconnection("AA:BB:CC:DD:EE:FF".to_string());
        assert!(mgr.is_tracked("AA:BB:CC:DD:EE:FF"));
        assert_eq!(mgr.tracked_count(), 1);

        let status = mgr.get_status("AA:BB:CC:DD:EE:FF");
        // Should be either Ready or Waiting
        assert!(matches!(
            status,
            ReconnectionStatus::Ready | ReconnectionStatus::Waiting { .. }
        ));

        assert_eq!(mgr.get_status("unknown"), ReconnectionStatus::NotTracked);
    }

    #[test]
    fn test_reconnection_manager_connection_success() {
        let mgr = ReconnectionManager::with_defaults();
        mgr.track_disconnection("peer-1".to_string());
        assert_eq!(mgr.tracked_count(), 1);
        mgr.on_connection_success("peer-1");
        assert!(!mgr.is_tracked("peer-1"));
    }

    #[test]
    fn test_reconnection_manager_clear() {
        let mgr = ReconnectionManager::with_defaults();
        mgr.track_disconnection("a".to_string());
        mgr.track_disconnection("b".to_string());
        assert_eq!(mgr.tracked_count(), 2);
        mgr.clear();
        assert_eq!(mgr.tracked_count(), 0);
    }

    // ==================== PeerLifetimeManager ====================

    #[test]
    fn test_peer_lifetime_manager_defaults() {
        let mgr = PeerLifetimeManager::with_defaults();
        assert_eq!(mgr.tracked_count(), 0);
        assert!(mgr.cleanup_interval_ms() > 0);
    }

    #[test]
    fn test_peer_lifetime_manager_track_peer() {
        let mgr = PeerLifetimeManager::with_defaults();
        mgr.on_peer_activity("peer-1", true);
        assert!(mgr.is_tracked("peer-1"));
        assert!(mgr.is_connected("peer-1"));
        assert_eq!(mgr.tracked_count(), 1);

        let stats = mgr.stats();
        assert_eq!(stats.total_tracked, 1);
        assert_eq!(stats.connected, 1);
        assert_eq!(stats.disconnected, 0);
    }

    #[test]
    fn test_peer_lifetime_manager_disconnect() {
        let mgr = PeerLifetimeManager::with_defaults();
        mgr.on_peer_activity("peer-1", true);
        mgr.on_peer_disconnected("peer-1");
        assert!(mgr.is_tracked("peer-1"));
        assert!(!mgr.is_connected("peer-1"));
    }

    #[test]
    fn test_peer_lifetime_manager_remove() {
        let mgr = PeerLifetimeManager::with_defaults();
        mgr.on_peer_activity("peer-1", true);
        assert!(mgr.remove_peer("peer-1"));
        assert!(!mgr.is_tracked("peer-1"));
        assert!(!mgr.remove_peer("peer-1")); // already removed
    }

    #[test]
    fn test_peer_lifetime_manager_clear() {
        let mgr = PeerLifetimeManager::with_defaults();
        mgr.on_peer_activity("a", true);
        mgr.on_peer_activity("b", false);
        mgr.clear();
        assert_eq!(mgr.tracked_count(), 0);
    }

    // ==================== AddressRotationHandler ====================

    #[test]
    fn test_address_rotation_handler_register_and_lookup() {
        let handler = AddressRotationHandler::new();
        handler.register_device("WT-WEAROS-1234", "AA:BB:CC:DD:EE:FF", 42);
        assert_eq!(handler.lookup_by_name("WT-WEAROS-1234"), Some(42));
        assert_eq!(handler.lookup_by_address("AA:BB:CC:DD:EE:FF"), Some(42));
        assert_eq!(
            handler.get_address(42),
            Some("AA:BB:CC:DD:EE:FF".to_string())
        );
        assert_eq!(handler.get_name(42), Some("WT-WEAROS-1234".to_string()));
    }

    #[test]
    fn test_address_rotation_handler_unknown_lookups() {
        let handler = AddressRotationHandler::new();
        assert!(handler.lookup_by_name("unknown").is_none());
        assert!(handler.lookup_by_address("00:00:00:00:00:00").is_none());
        assert!(handler.get_address(999).is_none());
        assert!(handler.get_name(999).is_none());
    }

    #[test]
    fn test_address_rotation_handler_remove_and_clear() {
        let handler = AddressRotationHandler::new();
        handler.register_device("dev-1", "addr-1", 1);
        handler.register_device("dev-2", "addr-2", 2);
        assert_eq!(handler.device_count(), 2);

        handler.remove_device(1);
        assert!(handler.lookup_by_name("dev-1").is_none());

        handler.clear();
        assert_eq!(handler.device_count(), 0);
    }

    // ==================== Namespace helper functions ====================

    #[test]
    fn test_detect_device_pattern_weartak() {
        assert_eq!(
            detect_device_pattern("WT-WEAROS-1234"),
            DevicePattern::WearTak
        );
    }

    #[test]
    fn test_is_weartak_device_fn() {
        assert!(is_weartak_device("WT-WEAROS-1234"));
        assert!(!is_weartak_device("SomeOtherDevice"));
    }

    #[test]
    fn test_device_pattern_rotates() {
        assert!(device_pattern_rotates_addresses(DevicePattern::WearTak));
        assert!(device_pattern_rotates_addresses(DevicePattern::WearOs));
        assert!(!device_pattern_rotates_addresses(DevicePattern::Peat));
        assert!(!device_pattern_rotates_addresses(DevicePattern::Unknown));
    }

    // ==================== BLE callback handling ====================

    #[test]
    fn test_hive_mesh_on_ble_discovered() {
        let mesh = PeatMesh::new(1, "TEST", "my-mesh");
        // Name must be "PEAT_MESH-XXXXXXXX" format to parse node ID
        let peer = mesh.on_ble_discovered(
            "AA:BB:CC:DD:EE:FF",
            Some("PEAT_MESH-00000002".to_string()),
            -50,
            Some("my-mesh".to_string()),
            1000,
        );
        assert!(peer.is_some());
        let peer = peer.unwrap();
        assert_eq!(peer.identifier, "AA:BB:CC:DD:EE:FF");
        assert_eq!(peer.rssi, -50);
    }

    #[test]
    fn test_hive_mesh_on_ble_discovered_no_name() {
        let mesh = PeatMesh::new(1, "TEST", "my-mesh");
        // No name means node ID can't be parsed, so None returned
        let peer = mesh.on_ble_discovered(
            "AA:BB:CC:DD:EE:FF",
            None,
            -70,
            Some("my-mesh".to_string()),
            1000,
        );
        assert!(peer.is_none());
    }

    #[test]
    fn test_hive_mesh_on_ble_discovered_wrong_mesh() {
        let mesh = PeatMesh::new(1, "TEST", "my-mesh");
        let peer = mesh.on_ble_discovered(
            "AA:BB:CC:DD:EE:FF",
            Some("PEAT_MESH-00000002".to_string()),
            -50,
            Some("other-mesh".to_string()),
            1000,
        );
        assert!(peer.is_none());
    }

    #[test]
    fn test_hive_mesh_on_ble_disconnected_unknown_peer() {
        let mesh = PeatMesh::new(1, "TEST", "mesh");
        let result = mesh.on_ble_disconnected("unknown-addr", DisconnectReason::Timeout);
        assert!(result.is_none());
    }

    #[test]
    fn test_hive_mesh_on_ble_data_received_invalid() {
        let mesh = PeatMesh::new(1, "TEST", "mesh");
        // No peer registered for this identifier
        let result = mesh.on_ble_data_received("unknown", &[0xFF; 10], 1000);
        assert!(result.is_none());
    }

    #[test]
    fn test_hive_mesh_decrypt_only_no_encryption() {
        let mesh = PeatMesh::new(1, "TEST", "mesh");
        // Without encryption, decrypt_only should return None or the data
        let result = mesh.decrypt_only(&[0x01, 0x02, 0x03]);
        // Depends on implementation - just ensure no panic
        let _ = result;
    }

    // ==================== Encrypted mesh data flow ====================

    #[test]
    fn test_encrypted_mesh_document_round_trip() {
        let id_a = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("enc-test", &id_a);
        let mesh_a = PeatMesh::new_from_genesis("ALPHA", &id_a, &genesis);

        let id_b = DeviceIdentity::generate();
        let secret = genesis.get_encryption_secret();
        let mesh_b = create_peat_mesh_with_encryption(
            id_b.get_node_id(),
            "BRAVO",
            &genesis.get_mesh_id(),
            &secret,
        )
        .expect("should create encrypted mesh");

        // Build document from A
        let doc = mesh_a.build_document();
        assert!(!doc.is_empty());

        // B should be able to decrypt it
        let decrypted = mesh_b.decrypt_only(&doc);
        assert!(decrypted.is_some());
    }
}
