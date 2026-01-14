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

//! Peer management types for HIVE BLE mesh
//!
//! This module provides the core peer representation and configuration
//! for centralized peer management across all platforms (iOS, Android, ESP32).

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

use crate::NodeId;

/// Unified peer representation across all platforms
///
/// Represents a discovered or connected HIVE mesh peer with all
/// relevant metadata for mesh operations.
#[derive(Debug, Clone)]
pub struct HivePeer {
    /// HIVE node identifier (32-bit)
    pub node_id: NodeId,

    /// Platform-specific BLE identifier
    /// - iOS: CBPeripheral UUID string
    /// - Android: MAC address string
    /// - ESP32: MAC address or NimBLE handle
    pub identifier: String,

    /// Mesh ID this peer belongs to (e.g., "DEMO")
    pub mesh_id: Option<String>,

    /// Advertised device name (e.g., "HIVE_DEMO-12345678")
    pub name: Option<String>,

    /// Last known signal strength (RSSI in dBm)
    pub rssi: i8,

    /// Whether we have an active BLE connection to this peer
    pub is_connected: bool,

    /// Timestamp when this peer was last seen (milliseconds since epoch/boot)
    pub last_seen_ms: u64,
}

impl HivePeer {
    /// Create a new peer from discovery data
    pub fn new(
        node_id: NodeId,
        identifier: String,
        mesh_id: Option<String>,
        name: Option<String>,
        rssi: i8,
    ) -> Self {
        Self {
            node_id,
            identifier,
            mesh_id,
            name,
            rssi,
            is_connected: false,
            last_seen_ms: 0,
        }
    }

    /// Update the peer's last seen timestamp
    pub fn touch(&mut self, now_ms: u64) {
        self.last_seen_ms = now_ms;
    }

    /// Check if this peer is stale (not seen within timeout)
    pub fn is_stale(&self, now_ms: u64, timeout_ms: u64) -> bool {
        if self.last_seen_ms == 0 {
            return false; // Never seen, don't consider stale
        }
        now_ms.saturating_sub(self.last_seen_ms) > timeout_ms
    }

    /// Get display name for this peer
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(self.identifier.as_str())
    }

    /// Get signal strength category
    pub fn signal_strength(&self) -> SignalStrength {
        match self.rssi {
            r if r >= -50 => SignalStrength::Excellent,
            r if r >= -70 => SignalStrength::Good,
            r if r >= -85 => SignalStrength::Fair,
            _ => SignalStrength::Weak,
        }
    }
}

impl Default for HivePeer {
    fn default() -> Self {
        Self {
            node_id: NodeId::default(),
            identifier: String::new(),
            mesh_id: None,
            name: None,
            rssi: -100,
            is_connected: false,
            last_seen_ms: 0,
        }
    }
}

/// Signal strength categories for display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalStrength {
    /// RSSI >= -50 dBm
    Excellent,
    /// RSSI >= -70 dBm
    Good,
    /// RSSI >= -85 dBm
    Fair,
    /// RSSI < -85 dBm
    Weak,
}

/// Connection state aligned with hive-protocol abstractions
///
/// Represents the lifecycle states of a peer connection, from initial
/// discovery through connection, degradation, and disconnection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionState {
    /// Peer has been seen via BLE advertisement but never connected
    #[default]
    Discovered,
    /// BLE connection is being established
    Connecting,
    /// Active BLE connection with healthy signal
    Connected,
    /// Connected but with degraded quality (low RSSI or packet loss)
    Degraded,
    /// Graceful disconnect in progress
    Disconnecting,
    /// Was previously connected, now disconnected
    Disconnected,
    /// Disconnected and no longer seen in advertisements
    Lost,
}

impl ConnectionState {
    /// Returns true if this state represents an active connection
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected | Self::Degraded)
    }

    /// Returns true if this state indicates the peer was previously known
    pub fn was_connected(&self) -> bool {
        matches!(
            self,
            Self::Connected
                | Self::Degraded
                | Self::Disconnecting
                | Self::Disconnected
                | Self::Lost
        )
    }

    /// Returns true if this state indicates potential connectivity issues
    pub fn is_degraded_or_worse(&self) -> bool {
        matches!(
            self,
            Self::Degraded | Self::Disconnecting | Self::Disconnected | Self::Lost
        )
    }
}

// Re-export DisconnectReason from platform module
pub use crate::platform::DisconnectReason;

/// Per-peer connection state with history
///
/// Provides a comprehensive view of a peer's connection lifecycle,
/// including timestamps, statistics, and associated data metrics.
/// This enables apps to display appropriate UI indicators and track
/// data provenance.
#[derive(Debug, Clone)]
pub struct PeerConnectionState {
    /// HIVE node identifier
    pub node_id: NodeId,

    /// Platform-specific BLE identifier
    pub identifier: String,

    /// Current connection state
    pub state: ConnectionState,

    /// Timestamp when peer was first discovered (ms since epoch)
    pub discovered_at: u64,

    /// Timestamp of most recent connection (ms since epoch)
    pub connected_at: Option<u64>,

    /// Timestamp of most recent disconnection (ms since epoch)
    pub disconnected_at: Option<u64>,

    /// Reason for most recent disconnection
    pub disconnect_reason: Option<DisconnectReason>,

    /// Most recent RSSI reading (dBm)
    pub last_rssi: Option<i8>,

    /// Total number of successful connections to this peer
    pub connection_count: u32,

    /// Number of documents synced with this peer
    pub documents_synced: u32,

    /// Bytes received from this peer
    pub bytes_received: u64,

    /// Bytes sent to this peer
    pub bytes_sent: u64,

    /// Last time peer was seen (advertisement or data, ms since epoch)
    pub last_seen_ms: u64,

    /// Optional device name
    pub name: Option<String>,

    /// Mesh ID this peer belongs to
    pub mesh_id: Option<String>,
}

impl PeerConnectionState {
    /// Create a new connection state for a discovered peer
    pub fn new_discovered(node_id: NodeId, identifier: String, now_ms: u64) -> Self {
        Self {
            node_id,
            identifier,
            state: ConnectionState::Discovered,
            discovered_at: now_ms,
            connected_at: None,
            disconnected_at: None,
            disconnect_reason: None,
            last_rssi: None,
            connection_count: 0,
            documents_synced: 0,
            bytes_received: 0,
            bytes_sent: 0,
            last_seen_ms: now_ms,
            name: None,
            mesh_id: None,
        }
    }

    /// Create from an existing HivePeer
    pub fn from_peer(peer: &HivePeer, now_ms: u64) -> Self {
        let state = if peer.is_connected {
            ConnectionState::Connected
        } else {
            ConnectionState::Discovered
        };

        Self {
            node_id: peer.node_id,
            identifier: peer.identifier.clone(),
            state,
            discovered_at: now_ms,
            connected_at: if peer.is_connected {
                Some(now_ms)
            } else {
                None
            },
            disconnected_at: None,
            disconnect_reason: None,
            last_rssi: Some(peer.rssi),
            connection_count: if peer.is_connected { 1 } else { 0 },
            documents_synced: 0,
            bytes_received: 0,
            bytes_sent: 0,
            last_seen_ms: peer.last_seen_ms,
            name: peer.name.clone(),
            mesh_id: peer.mesh_id.clone(),
        }
    }

    /// Transition to connecting state
    pub fn set_connecting(&mut self, now_ms: u64) {
        self.state = ConnectionState::Connecting;
        self.last_seen_ms = now_ms;
    }

    /// Transition to connected state
    pub fn set_connected(&mut self, now_ms: u64) {
        self.state = ConnectionState::Connected;
        self.connected_at = Some(now_ms);
        self.connection_count += 1;
        self.last_seen_ms = now_ms;
        self.disconnect_reason = None;
    }

    /// Transition to degraded state (still connected but poor quality)
    pub fn set_degraded(&mut self, now_ms: u64) {
        if self.state == ConnectionState::Connected {
            self.state = ConnectionState::Degraded;
            self.last_seen_ms = now_ms;
        }
    }

    /// Transition to disconnected state
    pub fn set_disconnected(&mut self, now_ms: u64, reason: DisconnectReason) {
        self.state = ConnectionState::Disconnected;
        self.disconnected_at = Some(now_ms);
        self.disconnect_reason = Some(reason);
        self.last_seen_ms = now_ms;
    }

    /// Transition to lost state (not seen in advertisements)
    pub fn set_lost(&mut self, now_ms: u64) {
        if self.state == ConnectionState::Disconnected {
            self.state = ConnectionState::Lost;
            self.last_seen_ms = now_ms;
        }
    }

    /// Update RSSI and check for degradation
    ///
    /// Returns true if state changed to Degraded
    pub fn update_rssi(&mut self, rssi: i8, now_ms: u64, degraded_threshold: i8) -> bool {
        self.last_rssi = Some(rssi);
        self.last_seen_ms = now_ms;

        if self.state == ConnectionState::Connected && rssi < degraded_threshold {
            self.state = ConnectionState::Degraded;
            return true;
        } else if self.state == ConnectionState::Degraded && rssi >= degraded_threshold {
            self.state = ConnectionState::Connected;
        }
        false
    }

    /// Record data transfer statistics
    pub fn record_transfer(&mut self, bytes_received: u64, bytes_sent: u64) {
        self.bytes_received += bytes_received;
        self.bytes_sent += bytes_sent;
    }

    /// Record a document sync
    pub fn record_sync(&mut self) {
        self.documents_synced += 1;
    }

    /// Get time since last connection (if ever connected)
    pub fn time_since_connected(&self, now_ms: u64) -> Option<u64> {
        self.connected_at.map(|t| now_ms.saturating_sub(t))
    }

    /// Get time since disconnection (if disconnected)
    pub fn time_since_disconnected(&self, now_ms: u64) -> Option<u64> {
        self.disconnected_at.map(|t| now_ms.saturating_sub(t))
    }

    /// Get connection duration if currently connected
    pub fn connection_duration(&self, now_ms: u64) -> Option<u64> {
        if self.state.is_connected() {
            self.connected_at.map(|t| now_ms.saturating_sub(t))
        } else {
            None
        }
    }

    /// Get signal strength category
    pub fn signal_strength(&self) -> Option<SignalStrength> {
        self.last_rssi.map(|rssi| match rssi {
            r if r >= -50 => SignalStrength::Excellent,
            r if r >= -70 => SignalStrength::Good,
            r if r >= -85 => SignalStrength::Fair,
            _ => SignalStrength::Weak,
        })
    }
}

#[cfg(feature = "std")]
use std::collections::BTreeMap;

#[cfg(not(feature = "std"))]
use alloc::collections::BTreeMap;

/// Connection state graph for tracking all peer connection states
///
/// Provides a queryable view of all known peers and their connection
/// lifecycle state. Apps can use this to display appropriate UI indicators
/// and associate data with connection state at time of receipt.
///
/// # Example
///
/// ```ignore
/// let graph = mesh.get_connection_graph();
///
/// // Show connected peers with green indicator
/// for peer in graph.get_connected() {
///     ui.show_peer_connected(&peer);
/// }
///
/// // Show recently disconnected peers with yellow indicator
/// for peer in graph.get_recently_disconnected(30_000) {
///     ui.show_peer_stale(&peer, peer.time_since_disconnected(now));
/// }
///
/// // Show lost peers with gray indicator
/// for peer in graph.get_lost() {
///     ui.show_peer_lost(&peer);
/// }
/// ```
#[derive(Debug, Clone, Default)]
pub struct ConnectionStateGraph {
    /// Direct peers (degree 0) indexed by node ID
    peers: BTreeMap<NodeId, PeerConnectionState>,

    /// Indirect peers (degree 1-3) indexed by node ID
    indirect_peers: BTreeMap<NodeId, IndirectPeer>,

    /// RSSI threshold for degraded state
    rssi_degraded_threshold: i8,

    /// Time after disconnect before Lost state
    lost_timeout_ms: u64,

    /// Time after which indirect peers are considered stale
    indirect_peer_timeout_ms: u64,
}

impl ConnectionStateGraph {
    /// Create a new empty connection state graph
    pub fn new() -> Self {
        Self {
            peers: BTreeMap::new(),
            indirect_peers: BTreeMap::new(),
            rssi_degraded_threshold: -80,
            lost_timeout_ms: 30_000,
            indirect_peer_timeout_ms: 120_000, // 2 minutes for indirect peers
        }
    }

    /// Create with custom thresholds
    pub fn with_config(rssi_degraded_threshold: i8, lost_timeout_ms: u64) -> Self {
        Self {
            peers: BTreeMap::new(),
            indirect_peers: BTreeMap::new(),
            rssi_degraded_threshold,
            lost_timeout_ms,
            indirect_peer_timeout_ms: 120_000,
        }
    }

    /// Get all tracked peers
    pub fn get_all(&self) -> Vec<&PeerConnectionState> {
        self.peers.values().collect()
    }

    /// Get all peers as owned values
    pub fn get_all_owned(&self) -> Vec<PeerConnectionState> {
        self.peers.values().cloned().collect()
    }

    /// Get a specific peer's state
    pub fn get_peer(&self, node_id: NodeId) -> Option<&PeerConnectionState> {
        self.peers.get(&node_id)
    }

    /// Get a mutable reference to a peer's state
    pub fn get_peer_mut(&mut self, node_id: NodeId) -> Option<&mut PeerConnectionState> {
        self.peers.get_mut(&node_id)
    }

    /// Get all currently connected peers (Connected or Degraded state)
    pub fn get_connected(&self) -> Vec<&PeerConnectionState> {
        self.peers
            .values()
            .filter(|p| p.state.is_connected())
            .collect()
    }

    /// Get all peers in Degraded state
    pub fn get_degraded(&self) -> Vec<&PeerConnectionState> {
        self.peers
            .values()
            .filter(|p| p.state == ConnectionState::Degraded)
            .collect()
    }

    /// Get peers disconnected within the specified time window
    pub fn get_recently_disconnected(
        &self,
        within_ms: u64,
        now_ms: u64,
    ) -> Vec<&PeerConnectionState> {
        self.peers
            .values()
            .filter(|p| {
                p.state == ConnectionState::Disconnected
                    && p.disconnected_at
                        .map(|t| now_ms.saturating_sub(t) <= within_ms)
                        .unwrap_or(false)
            })
            .collect()
    }

    /// Get all peers in Lost state
    pub fn get_lost(&self) -> Vec<&PeerConnectionState> {
        self.peers
            .values()
            .filter(|p| p.state == ConnectionState::Lost)
            .collect()
    }

    /// Get peers that were previously connected (have connection history)
    pub fn get_with_history(&self) -> Vec<&PeerConnectionState> {
        self.peers
            .values()
            .filter(|p| p.state.was_connected())
            .collect()
    }

    /// Count of peers in each state
    pub fn state_counts(&self) -> StateCountSummary {
        let mut summary = StateCountSummary::default();
        for peer in self.peers.values() {
            match peer.state {
                ConnectionState::Discovered => summary.discovered += 1,
                ConnectionState::Connecting => summary.connecting += 1,
                ConnectionState::Connected => summary.connected += 1,
                ConnectionState::Degraded => summary.degraded += 1,
                ConnectionState::Disconnecting => summary.disconnecting += 1,
                ConnectionState::Disconnected => summary.disconnected += 1,
                ConnectionState::Lost => summary.lost += 1,
            }
        }
        summary
    }

    /// Total number of tracked peers
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// Check if graph is empty
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    /// Register a newly discovered peer
    pub fn on_discovered(
        &mut self,
        node_id: NodeId,
        identifier: String,
        name: Option<String>,
        mesh_id: Option<String>,
        rssi: i8,
        now_ms: u64,
    ) -> &PeerConnectionState {
        let entry = self.peers.entry(node_id).or_insert_with(|| {
            PeerConnectionState::new_discovered(node_id, identifier.clone(), now_ms)
        });

        // Update metadata
        entry.last_rssi = Some(rssi);
        entry.last_seen_ms = now_ms;
        if name.is_some() {
            entry.name = name;
        }
        if mesh_id.is_some() {
            entry.mesh_id = mesh_id;
        }

        // If was disconnected/lost and now seen again, update state
        if entry.state == ConnectionState::Lost {
            entry.state = ConnectionState::Disconnected;
        }

        entry
    }

    /// Handle connection start
    pub fn on_connecting(&mut self, node_id: NodeId, now_ms: u64) {
        if let Some(peer) = self.peers.get_mut(&node_id) {
            peer.set_connecting(now_ms);
        }
    }

    /// Handle successful connection
    pub fn on_connected(&mut self, node_id: NodeId, now_ms: u64) {
        if let Some(peer) = self.peers.get_mut(&node_id) {
            peer.set_connected(now_ms);
        }
    }

    /// Handle disconnection
    pub fn on_disconnected(&mut self, node_id: NodeId, reason: DisconnectReason, now_ms: u64) {
        if let Some(peer) = self.peers.get_mut(&node_id) {
            peer.set_disconnected(now_ms, reason);
        }
    }

    /// Update RSSI for a peer, checking for degradation
    ///
    /// Returns true if peer transitioned to Degraded state
    pub fn update_rssi(&mut self, node_id: NodeId, rssi: i8, now_ms: u64) -> bool {
        if let Some(peer) = self.peers.get_mut(&node_id) {
            return peer.update_rssi(rssi, now_ms, self.rssi_degraded_threshold);
        }
        false
    }

    /// Record data transfer for a peer
    pub fn record_transfer(&mut self, node_id: NodeId, bytes_received: u64, bytes_sent: u64) {
        if let Some(peer) = self.peers.get_mut(&node_id) {
            peer.record_transfer(bytes_received, bytes_sent);
        }
    }

    /// Record a document sync for a peer
    pub fn record_sync(&mut self, node_id: NodeId) {
        if let Some(peer) = self.peers.get_mut(&node_id) {
            peer.record_sync();
        }
    }

    /// Run periodic maintenance (transition Disconnected → Lost)
    ///
    /// Returns list of peers that transitioned to Lost state
    pub fn tick(&mut self, now_ms: u64) -> Vec<NodeId> {
        let mut newly_lost = Vec::new();

        for (node_id, peer) in self.peers.iter_mut() {
            if peer.state == ConnectionState::Disconnected {
                if let Some(disconnected_at) = peer.disconnected_at {
                    if now_ms.saturating_sub(disconnected_at) > self.lost_timeout_ms {
                        peer.set_lost(now_ms);
                        newly_lost.push(*node_id);
                    }
                }
            }
        }

        newly_lost
    }

    /// Remove peers that have been lost for longer than the specified duration
    pub fn cleanup_lost(&mut self, older_than_ms: u64, now_ms: u64) -> Vec<NodeId> {
        let to_remove: Vec<NodeId> = self
            .peers
            .iter()
            .filter(|(_, p)| {
                p.state == ConnectionState::Lost
                    && now_ms.saturating_sub(p.last_seen_ms) > older_than_ms
            })
            .map(|(id, _)| *id)
            .collect();

        for id in &to_remove {
            self.peers.remove(id);
        }

        to_remove
    }

    /// Import state from a HivePeer
    pub fn import_peer(&mut self, peer: &HivePeer, now_ms: u64) {
        let state = PeerConnectionState::from_peer(peer, now_ms);
        self.peers.insert(peer.node_id, state);
    }

    // ========== Indirect Peer Methods ==========

    /// Record that we received a relay message with given origin
    ///
    /// This updates the indirect peer graph when we receive a relay message
    /// where the origin differs from the immediate sender.
    ///
    /// # Arguments
    /// * `source_peer` - The direct peer we received the relay from
    /// * `origin_node` - The original sender (from relay envelope)
    /// * `hop_count` - Current hop count from the relay envelope
    /// * `now_ms` - Current timestamp
    ///
    /// # Returns
    /// `true` if this is a newly discovered indirect peer
    pub fn on_relay_received(
        &mut self,
        source_peer: NodeId,
        origin_node: NodeId,
        hop_count: u8,
        now_ms: u64,
    ) -> bool {
        // Don't track peers beyond our max degree
        if hop_count > MAX_TRACKED_DEGREE {
            return false;
        }

        // Don't track ourselves
        if self.peers.contains_key(&origin_node) {
            // Origin is a direct peer, not indirect
            return false;
        }

        // Update or create indirect peer entry
        if let Some(existing) = self.indirect_peers.get_mut(&origin_node) {
            existing.update_path(source_peer, hop_count, now_ms);
            false
        } else {
            self.indirect_peers.insert(
                origin_node,
                IndirectPeer::new(origin_node, source_peer, hop_count, now_ms),
            );
            true
        }
    }

    /// Get all indirect peers
    pub fn get_indirect_peers(&self) -> Vec<&IndirectPeer> {
        self.indirect_peers.values().collect()
    }

    /// Get all indirect peers as owned values
    pub fn get_indirect_peers_owned(&self) -> Vec<IndirectPeer> {
        self.indirect_peers.values().cloned().collect()
    }

    /// Get a specific indirect peer
    pub fn get_indirect_peer(&self, node_id: NodeId) -> Option<&IndirectPeer> {
        self.indirect_peers.get(&node_id)
    }

    /// Get peers by degree
    pub fn get_peers_by_degree(&self, degree: PeerDegree) -> Vec<NodeId> {
        match degree {
            PeerDegree::Direct => self.peers.keys().copied().collect(),
            _ => self
                .indirect_peers
                .iter()
                .filter(|(_, p)| p.degree() == Some(degree))
                .map(|(id, _)| *id)
                .collect(),
        }
    }

    /// Get the degree of a specific peer (direct or indirect)
    pub fn peer_degree(&self, node_id: NodeId) -> Option<PeerDegree> {
        if self.peers.contains_key(&node_id) {
            Some(PeerDegree::Direct)
        } else {
            self.indirect_peers.get(&node_id).and_then(|p| p.degree())
        }
    }

    /// Get all paths to reach an indirect peer
    ///
    /// Returns Vec of (via_peer_id, hop_count) pairs
    pub fn get_paths_to(&self, node_id: NodeId) -> Vec<(NodeId, u8)> {
        self.indirect_peers
            .get(&node_id)
            .map(|p| p.paths())
            .unwrap_or_default()
    }

    /// Check if a node is known (either direct or indirect)
    pub fn is_known(&self, node_id: NodeId) -> bool {
        self.peers.contains_key(&node_id) || self.indirect_peers.contains_key(&node_id)
    }

    /// Cleanup stale indirect peers
    ///
    /// Returns list of removed peer IDs
    pub fn cleanup_indirect(&mut self, now_ms: u64) -> Vec<NodeId> {
        let to_remove: Vec<NodeId> = self
            .indirect_peers
            .iter()
            .filter(|(_, p)| p.is_stale(now_ms, self.indirect_peer_timeout_ms))
            .map(|(id, _)| *id)
            .collect();

        for id in &to_remove {
            self.indirect_peers.remove(id);
        }

        to_remove
    }

    /// Remove a via_peer path from all indirect peers
    ///
    /// Called when a direct peer disconnects - the indirect paths through
    /// that peer may no longer be valid.
    pub fn remove_via_peer(&mut self, via_peer: NodeId) {
        let mut to_remove = Vec::new();

        for (node_id, indirect) in self.indirect_peers.iter_mut() {
            indirect.via_peers.remove(&via_peer);

            // Recalculate min_hops
            if indirect.via_peers.is_empty() {
                to_remove.push(*node_id);
            } else {
                indirect.min_hops = indirect.via_peers.values().copied().min().unwrap_or(255);
            }
        }

        // Remove peers with no remaining paths
        for id in to_remove {
            self.indirect_peers.remove(&id);
        }
    }

    /// Combined count summary including indirect peers
    pub fn full_state_counts(&self) -> FullStateCountSummary {
        let direct = self.state_counts();

        let mut one_hop = 0;
        let mut two_hop = 0;
        let mut three_hop = 0;

        for peer in self.indirect_peers.values() {
            match peer.min_hops {
                1 => one_hop += 1,
                2 => two_hop += 1,
                3 => three_hop += 1,
                _ => {}
            }
        }

        FullStateCountSummary {
            direct,
            one_hop,
            two_hop,
            three_hop,
        }
    }

    /// Number of indirect peers
    pub fn indirect_peer_count(&self) -> usize {
        self.indirect_peers.len()
    }

    /// Set callsign for an indirect peer (learned from document)
    pub fn set_indirect_callsign(&mut self, node_id: NodeId, callsign: String) {
        if let Some(peer) = self.indirect_peers.get_mut(&node_id) {
            peer.callsign = Some(callsign);
        }
    }
}

/// Summary of peer counts by state
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StateCountSummary {
    /// Peers discovered but never connected
    pub discovered: usize,
    /// Peers currently connecting
    pub connecting: usize,
    /// Peers with healthy connection
    pub connected: usize,
    /// Peers connected but with degraded signal
    pub degraded: usize,
    /// Peers currently disconnecting
    pub disconnecting: usize,
    /// Peers recently disconnected
    pub disconnected: usize,
    /// Peers disconnected and not seen in advertisements
    pub lost: usize,
}

impl StateCountSummary {
    /// Total number of peers actively connected
    pub fn active_connections(&self) -> usize {
        self.connected + self.degraded
    }

    /// Total number of tracked peers
    pub fn total(&self) -> usize {
        self.discovered
            + self.connecting
            + self.connected
            + self.degraded
            + self.disconnecting
            + self.disconnected
            + self.lost
    }
}

/// Maximum number of hops to track for indirect peers
pub const MAX_TRACKED_DEGREE: u8 = 3;

/// Peer degree classification for multi-hop mesh topology
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerDegree {
    /// Directly connected via BLE (degree 0)
    Direct,
    /// Reachable via 1 hop through a direct peer (degree 1)
    OneHop,
    /// Reachable via 2 hops (degree 2)
    TwoHop,
    /// Reachable via 3 hops (degree 3)
    ThreeHop,
}

impl PeerDegree {
    /// Create from hop count
    pub fn from_hops(hops: u8) -> Option<Self> {
        match hops {
            0 => Some(Self::Direct),
            1 => Some(Self::OneHop),
            2 => Some(Self::TwoHop),
            3 => Some(Self::ThreeHop),
            _ => None, // Beyond tracking range
        }
    }

    /// Get the hop count for this degree
    pub fn hops(&self) -> u8 {
        match self {
            Self::Direct => 0,
            Self::OneHop => 1,
            Self::TwoHop => 2,
            Self::ThreeHop => 3,
        }
    }
}

/// Reachability information for an indirect (multi-hop) peer
///
/// Tracks peers that are not directly connected via BLE but are
/// reachable through relay messages via intermediate nodes.
#[derive(Debug, Clone)]
pub struct IndirectPeer {
    /// The indirect peer's node ID
    pub node_id: NodeId,

    /// Minimum hop count to reach this peer (1-3)
    pub min_hops: u8,

    /// Direct peers through which we can reach this peer
    /// Maps via_peer NodeId → hop count through that peer
    pub via_peers: BTreeMap<NodeId, u8>,

    /// When we first learned about this peer (ms since epoch)
    pub discovered_at: u64,

    /// Last time we received data from/about this peer (ms since epoch)
    pub last_seen_ms: u64,

    /// Number of messages relayed from this peer
    pub messages_received: u32,

    /// Optional callsign if learned from documents
    pub callsign: Option<String>,
}

impl IndirectPeer {
    /// Create a new indirect peer entry
    pub fn new(node_id: NodeId, via_peer: NodeId, hop_count: u8, now_ms: u64) -> Self {
        let mut via_peers = BTreeMap::new();
        via_peers.insert(via_peer, hop_count);

        Self {
            node_id,
            min_hops: hop_count,
            via_peers,
            discovered_at: now_ms,
            last_seen_ms: now_ms,
            messages_received: 1,
            callsign: None,
        }
    }

    /// Update with a new path to this peer
    ///
    /// Returns true if this is a better (shorter) path
    pub fn update_path(&mut self, via_peer: NodeId, hop_count: u8, now_ms: u64) -> bool {
        self.last_seen_ms = now_ms;
        self.messages_received += 1;

        let is_better = hop_count < self.min_hops;

        // Update or add this path
        self.via_peers.insert(via_peer, hop_count);

        // Recalculate min_hops
        if is_better {
            self.min_hops = hop_count;
        } else {
            // May need to recalculate if we updated an existing path
            self.min_hops = self.via_peers.values().copied().min().unwrap_or(hop_count);
        }

        is_better
    }

    /// Get the degree classification for this peer
    pub fn degree(&self) -> Option<PeerDegree> {
        PeerDegree::from_hops(self.min_hops)
    }

    /// Check if this peer is stale (not seen within timeout)
    pub fn is_stale(&self, now_ms: u64, timeout_ms: u64) -> bool {
        now_ms.saturating_sub(self.last_seen_ms) > timeout_ms
    }

    /// Get all paths to this peer as (via_peer, hop_count) pairs
    pub fn paths(&self) -> Vec<(NodeId, u8)> {
        self.via_peers.iter().map(|(&k, &v)| (k, v)).collect()
    }
}

/// Extended state summary including indirect peers
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FullStateCountSummary {
    /// Direct peer counts by connection state
    pub direct: StateCountSummary,
    /// Number of 1-hop indirect peers
    pub one_hop: usize,
    /// Number of 2-hop indirect peers
    pub two_hop: usize,
    /// Number of 3-hop indirect peers
    pub three_hop: usize,
}

impl FullStateCountSummary {
    /// Total number of all known peers (direct + indirect)
    pub fn total(&self) -> usize {
        self.direct.total() + self.one_hop + self.two_hop + self.three_hop
    }

    /// Total indirect peers
    pub fn total_indirect(&self) -> usize {
        self.one_hop + self.two_hop + self.three_hop
    }
}

/// Configuration for the PeerManager
///
/// Provides configurable timeouts and behaviors for peer management.
/// All time values are in milliseconds.
#[derive(Debug, Clone)]
pub struct PeerManagerConfig {
    /// Time after which a peer is considered stale and removed (default: 45000ms)
    pub peer_timeout_ms: u64,

    /// How often to run cleanup of stale peers (default: 10000ms)
    pub cleanup_interval_ms: u64,

    /// How often to sync documents with peers (default: 5000ms)
    pub sync_interval_ms: u64,

    /// Minimum time between syncs to the same peer (default: 30000ms)
    /// Prevents "thrashing" when peers keep reconnecting
    pub sync_cooldown_ms: u64,

    /// Whether to automatically connect to discovered peers (default: true)
    pub auto_connect: bool,

    /// Local mesh ID for filtering peers (e.g., "DEMO")
    pub mesh_id: String,

    /// Maximum number of tracked peers (for no_std/embedded, default: 8)
    pub max_peers: usize,

    /// RSSI threshold below which a connection is considered degraded (default: -80 dBm)
    pub rssi_degraded_threshold: i8,

    /// Time after disconnect before peer transitions to Lost state (default: 30000ms)
    pub lost_timeout_ms: u64,
}

impl Default for PeerManagerConfig {
    fn default() -> Self {
        Self {
            peer_timeout_ms: 45_000,     // 45 seconds
            cleanup_interval_ms: 10_000, // 10 seconds
            sync_interval_ms: 5_000,     // 5 seconds
            sync_cooldown_ms: 30_000,    // 30 seconds
            auto_connect: true,
            mesh_id: String::from("DEMO"),
            max_peers: 8,
            rssi_degraded_threshold: -80, // -80 dBm (Fair/Weak boundary)
            lost_timeout_ms: 30_000,      // 30 seconds after disconnect
        }
    }
}

impl PeerManagerConfig {
    /// Create a new config with the specified mesh ID
    pub fn with_mesh_id(mesh_id: impl Into<String>) -> Self {
        Self {
            mesh_id: mesh_id.into(),
            ..Default::default()
        }
    }

    /// Set peer timeout
    pub fn peer_timeout(mut self, timeout_ms: u64) -> Self {
        self.peer_timeout_ms = timeout_ms;
        self
    }

    /// Set sync interval
    pub fn sync_interval(mut self, interval_ms: u64) -> Self {
        self.sync_interval_ms = interval_ms;
        self
    }

    /// Set auto-connect behavior
    pub fn auto_connect(mut self, enabled: bool) -> Self {
        self.auto_connect = enabled;
        self
    }

    /// Set max peers (for embedded systems)
    pub fn max_peers(mut self, max: usize) -> Self {
        self.max_peers = max;
        self
    }

    /// Check if a device mesh ID matches our mesh
    ///
    /// Returns true if:
    /// - Device mesh ID matches our mesh ID exactly, OR
    /// - Device mesh ID is None (legacy device, matches any mesh)
    pub fn matches_mesh(&self, device_mesh_id: Option<&str>) -> bool {
        match device_mesh_id {
            Some(id) => id == self.mesh_id,
            None => true, // Legacy devices match any mesh
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_stale_detection() {
        let mut peer = HivePeer::new(
            NodeId::new(0x12345678),
            "test-id".into(),
            Some("DEMO".into()),
            Some("HIVE_DEMO-12345678".into()),
            -70,
        );

        // Fresh peer is not stale
        peer.touch(1000);
        assert!(!peer.is_stale(2000, 45_000));

        // Peer becomes stale after timeout
        assert!(peer.is_stale(50_000, 45_000));
    }

    #[test]
    fn test_signal_strength() {
        let peer_excellent = HivePeer {
            rssi: -45,
            ..Default::default()
        };
        assert_eq!(peer_excellent.signal_strength(), SignalStrength::Excellent);

        let peer_good = HivePeer {
            rssi: -65,
            ..Default::default()
        };
        assert_eq!(peer_good.signal_strength(), SignalStrength::Good);

        let peer_fair = HivePeer {
            rssi: -80,
            ..Default::default()
        };
        assert_eq!(peer_fair.signal_strength(), SignalStrength::Fair);

        let peer_weak = HivePeer {
            rssi: -95,
            ..Default::default()
        };
        assert_eq!(peer_weak.signal_strength(), SignalStrength::Weak);
    }

    #[test]
    fn test_mesh_matching() {
        let config = PeerManagerConfig::with_mesh_id("ALPHA");

        // Exact match
        assert!(config.matches_mesh(Some("ALPHA")));

        // No match
        assert!(!config.matches_mesh(Some("BETA")));

        // Legacy device (no mesh ID) matches any
        assert!(config.matches_mesh(None));
    }
}
