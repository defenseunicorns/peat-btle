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

//! Transport trait implementation for ECHE-BTLE
//!
//! Implements the pluggable transport abstraction (ADR-032) for Bluetooth LE,
//! providing the `BluetoothLETransport` struct that can be registered with
//! the `TransportManager`.

#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, vec::Vec};

use async_trait::async_trait;
use core::time::Duration;

use crate::config::{BleConfig, BlePhy};
use crate::error::Result;
use crate::platform::BleAdapter;
use crate::NodeId;

/// Transport capabilities for Bluetooth LE
///
/// Advertises what this transport can do, allowing the TransportManager
/// to select the best transport for each message.
#[derive(Debug, Clone)]
pub struct TransportCapabilities {
    /// Maximum bandwidth in bytes/second
    pub max_bandwidth_bps: u64,
    /// Typical latency in milliseconds
    pub typical_latency_ms: u32,
    /// Maximum practical range in meters
    pub max_range_meters: u32,
    /// Supports bidirectional communication
    pub bidirectional: bool,
    /// Supports reliable delivery
    pub reliable: bool,
    /// Battery impact score (0-100, higher = more power)
    pub battery_impact: u8,
    /// Supports broadcast/advertising
    pub supports_broadcast: bool,
    /// Requires pairing before use
    pub requires_pairing: bool,
    /// Maximum message size in bytes
    pub max_message_size: usize,
}

impl TransportCapabilities {
    /// Create default BLE capabilities
    pub fn bluetooth_le() -> Self {
        Self {
            max_bandwidth_bps: 250_000, // ~250 KB/s practical throughput
            typical_latency_ms: 30,
            max_range_meters: 100,
            bidirectional: true,
            reliable: true,
            battery_impact: 15,
            supports_broadcast: true,
            requires_pairing: false,
            max_message_size: 512,
        }
    }

    /// Create capabilities for Coded PHY (long range)
    pub fn bluetooth_le_coded() -> Self {
        Self {
            max_bandwidth_bps: 125_000, // Coded S=8
            typical_latency_ms: 100,
            max_range_meters: 400,
            bidirectional: true,
            reliable: true,
            battery_impact: 20, // Slightly higher due to longer TX time
            supports_broadcast: true,
            requires_pairing: false,
            max_message_size: 512,
        }
    }

    /// Update capabilities based on PHY
    pub fn for_phy(phy: BlePhy) -> Self {
        match phy {
            BlePhy::Le1M => Self::bluetooth_le(),
            BlePhy::Le2M => Self {
                max_bandwidth_bps: 500_000,
                typical_latency_ms: 20,
                max_range_meters: 50,
                ..Self::bluetooth_le()
            },
            BlePhy::LeCodedS2 => Self {
                max_bandwidth_bps: 250_000,
                typical_latency_ms: 50,
                max_range_meters: 200,
                ..Self::bluetooth_le()
            },
            BlePhy::LeCodedS8 => Self::bluetooth_le_coded(),
        }
    }
}

impl Default for TransportCapabilities {
    fn default() -> Self {
        Self::bluetooth_le()
    }
}

/// Connection to a BLE peer
///
/// Represents an active GATT connection to a remote device.
pub trait BleConnection: Send + Sync {
    /// Get the remote peer's node ID
    fn peer_id(&self) -> &NodeId;

    /// Check if connection is still alive
    fn is_alive(&self) -> bool;

    /// Get the negotiated MTU
    fn mtu(&self) -> u16;

    /// Get the current PHY
    fn phy(&self) -> BlePhy;

    /// Get RSSI (signal strength) in dBm
    fn rssi(&self) -> Option<i8>;

    /// Get connection duration
    fn connected_duration(&self) -> Duration;
}

/// Bluetooth LE mesh transport
///
/// Implements the transport abstraction for BLE, providing:
/// - Peer discovery via advertising/scanning
/// - GATT-based data exchange
/// - Connection management
/// - PHY selection
///
/// # Example
///
/// ```ignore
/// use eche_btle::{BluetoothLETransport, BleConfig, NodeId};
///
/// let config = BleConfig::eche_lite(NodeId::new(0x12345678));
/// let transport = BluetoothLETransport::new(config)?;
///
/// transport.start().await?;
/// let conn = transport.connect(&peer_id).await?;
/// ```
pub struct BluetoothLETransport<A: BleAdapter> {
    /// Configuration
    config: BleConfig,
    /// Platform-specific adapter
    adapter: A,
    /// Current capabilities (may change with PHY)
    capabilities: TransportCapabilities,
}

impl<A: BleAdapter> BluetoothLETransport<A> {
    /// Create a new BLE transport with the given adapter
    pub fn new(config: BleConfig, adapter: A) -> Self {
        let capabilities = TransportCapabilities::for_phy(config.phy.preferred_phy);
        Self {
            config,
            adapter,
            capabilities,
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &BleConfig {
        &self.config
    }

    /// Get the current capabilities
    pub fn capabilities(&self) -> &TransportCapabilities {
        &self.capabilities
    }

    /// Get the node ID
    pub fn node_id(&self) -> &NodeId {
        &self.config.node_id
    }
}

/// Async transport operations
///
/// These are the core transport operations that integrate with
/// the Eche protocol's transport abstraction (ADR-032).
#[async_trait]
pub trait MeshTransport: Send + Sync {
    /// Start the transport layer
    async fn start(&self) -> Result<()>;

    /// Stop the transport layer
    async fn stop(&self) -> Result<()>;

    /// Connect to a peer by node ID
    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn BleConnection>>;

    /// Disconnect from a peer
    async fn disconnect(&self, peer_id: &NodeId) -> Result<()>;

    /// Get an existing connection
    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn BleConnection>>;

    /// Get the number of connected peers
    fn peer_count(&self) -> usize;

    /// Get list of connected peer IDs
    fn connected_peers(&self) -> Vec<NodeId>;

    /// Check if connected to a specific peer
    fn is_connected(&self, peer_id: &NodeId) -> bool {
        self.get_connection(peer_id).is_some()
    }

    /// Send data to a connected peer
    ///
    /// Fragments the payload based on the connection's negotiated MTU
    /// and writes each fragment to the peer's sync data characteristic.
    ///
    /// Returns the number of application bytes sent (original payload size).
    async fn send_to(&self, peer_id: &NodeId, data: &[u8]) -> Result<usize> {
        let _ = (peer_id, data);
        Err(crate::error::BleError::NotSupported(
            "send_to not implemented".into(),
        ))
    }

    /// Get transport capabilities
    fn capabilities(&self) -> &TransportCapabilities;
}

/// Construct a full 128-bit UUID from a BLE 16-bit short UUID
///
/// Uses the Bluetooth Base UUID: `0000xxxx-0000-1000-8000-00805F9B34FB`
fn ble_uuid_from_u16(short: u16) -> uuid::Uuid {
    uuid::Uuid::from_fields(
        short as u32,
        0x0000,
        0x1000,
        &[0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB],
    )
}

#[async_trait]
impl<A: BleAdapter + Send + Sync> MeshTransport for BluetoothLETransport<A> {
    async fn start(&self) -> Result<()> {
        // Start advertising and scanning via adapter
        self.adapter.start().await
    }

    async fn stop(&self) -> Result<()> {
        self.adapter.stop().await
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn BleConnection>> {
        self.adapter.connect(peer_id).await
    }

    async fn disconnect(&self, peer_id: &NodeId) -> Result<()> {
        self.adapter.disconnect(peer_id).await
    }

    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn BleConnection>> {
        self.adapter.get_connection(peer_id)
    }

    fn peer_count(&self) -> usize {
        self.adapter.peer_count()
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        self.adapter.connected_peers()
    }

    async fn send_to(&self, peer_id: &NodeId, data: &[u8]) -> Result<usize> {
        use crate::sync::protocol::chunk_data;

        // Get connection for MTU
        let conn = self.get_connection(peer_id).ok_or_else(|| {
            crate::error::BleError::ConnectionFailed(format!("No connection to {}", peer_id))
        })?;
        let mtu = conn.mtu() as usize;

        // Fragment data into MTU-sized chunks with reassembly headers
        let chunks = chunk_data(data, mtu, 0);

        // Write each chunk to the sync data characteristic
        let char_uuid = ble_uuid_from_u16(crate::CHAR_SYNC_DATA_UUID);
        for chunk in &chunks {
            self.adapter
                .write_to_peer(peer_id, char_uuid, &chunk.encode())
                .await?;
        }

        Ok(data.len())
    }

    fn capabilities(&self) -> &TransportCapabilities {
        &self.capabilities
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities_for_phy() {
        let caps = TransportCapabilities::for_phy(BlePhy::LeCodedS8);
        assert_eq!(caps.max_range_meters, 400);
        assert_eq!(caps.max_bandwidth_bps, 125_000);
    }

    #[test]
    fn test_capabilities_le2m() {
        let caps = TransportCapabilities::for_phy(BlePhy::Le2M);
        assert_eq!(caps.max_range_meters, 50);
        assert_eq!(caps.max_bandwidth_bps, 500_000);
    }

    #[test]
    fn test_ble_uuid_from_u16() {
        let uuid = ble_uuid_from_u16(0x0003);
        assert_eq!(uuid.to_string(), "00000003-0000-1000-8000-00805f9b34fb");
    }

    #[test]
    fn test_send_to_default_returns_error() {
        // Verify the default trait impl returns NotSupported
        use crate::platform::StubAdapter;

        let config = BleConfig::default();
        let adapter = StubAdapter::default();
        let transport = BluetoothLETransport::new(config, adapter);

        // send_to without a connection should fail
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt.block_on(transport.send_to(&NodeId::new(0x222), b"hello"));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_send_to() {
        use crate::platform::mock::{MockBleAdapter, MockNetwork};

        let network = MockNetwork::new();
        let mut adapter1 = MockBleAdapter::new(NodeId::new(0x111), network.clone());
        let mut adapter2 = MockBleAdapter::new(NodeId::new(0x222), network.clone());

        adapter1.init(&BleConfig::default()).await.unwrap();
        adapter2.init(&BleConfig::default()).await.unwrap();
        adapter2
            .start_advertising(&crate::config::DiscoveryConfig::default())
            .await
            .unwrap();

        // Connect
        let _conn = adapter1.connect(&NodeId::new(0x222)).await.unwrap();

        // Create transport and send data
        let transport = BluetoothLETransport::new(BleConfig::default(), adapter1);
        let result = transport.send_to(&NodeId::new(0x222), b"hello mesh").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 10);

        // Verify data was queued in the network
        let packets = network.receive_data(&NodeId::new(0x222));
        assert!(!packets.is_empty());
    }

    #[tokio::test]
    async fn test_send_to_disconnected_peer() {
        use crate::platform::mock::{MockBleAdapter, MockNetwork};

        let network = MockNetwork::new();
        let adapter = MockBleAdapter::new(NodeId::new(0x111), network);
        let transport = BluetoothLETransport::new(BleConfig::default(), adapter);

        // Sending to a peer we're not connected to should fail
        let result = transport.send_to(&NodeId::new(0x999), b"hello").await;
        assert!(result.is_err());
    }
}
