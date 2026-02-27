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

//! PEAT-BTLE: Bluetooth Low Energy mesh transport for Peat Protocol
//!
//! This crate provides BLE-based peer-to-peer mesh networking for Peat,
//! supporting discovery, advertisement, connectivity, and Peat-Lite sync.
//!
//! ## Overview
//!
//! PEAT-BTLE implements the pluggable transport abstraction (ADR-032) for
//! Bluetooth Low Energy, enabling Peat Protocol to operate over BLE in
//! resource-constrained environments like smartwatches.
//!
//! ## Key Features
//!
//! - **Cross-platform**: Linux, Android, macOS, iOS, Windows, ESP32
//! - **Power efficient**: Designed for 18+ hour battery life on watches
//! - **Long range**: Coded PHY support for 300m+ range
//! - **Peat-Lite sync**: Optimized CRDT sync over GATT
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │                  Application                     │
//! ├─────────────────────────────────────────────────┤
//! │           BluetoothLETransport                   │
//! │  (implements MeshTransport from ADR-032)        │
//! ├─────────────────────────────────────────────────┤
//! │              BleAdapter Trait                    │
//! ├──────────┬──────────┬──────────┬────────────────┤
//! │  Linux   │ Android  │  Apple   │    Windows     │
//! │ (BlueZ)  │  (JNI)   │(CoreBT)  │    (WinRT)     │
//! └──────────┴──────────┴──────────┴────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ```ignore
//! use peat_btle::{BleConfig, BluetoothLETransport, NodeId};
//!
//! // Create Peat-Lite optimized config for battery efficiency
//! let config = BleConfig::peat_lite(NodeId::new(0x12345678));
//!
//! // Create transport with platform adapter
//! #[cfg(feature = "linux")]
//! let adapter = peat_btle::platform::linux::BluerAdapter::new()?;
//!
//! let transport = BluetoothLETransport::new(config, adapter);
//!
//! // Start advertising and scanning
//! transport.start().await?;
//!
//! // Connect to a peer
//! let conn = transport.connect(&peer_id).await?;
//! ```
//!
//! ## Feature Flags
//!
//! - `std` (default): Standard library support
//! - `transport-only`: Pure BLE transport, no app-layer CRDTs
//! - `legacy-chat`: Deprecated ChatCRDT support (will be removed in 0.2.0)
//! - `linux`: Linux/BlueZ support via `bluer`
//! - `android`: Android support via JNI
//! - `macos`: macOS support via CoreBluetooth
//! - `ios`: iOS support via CoreBluetooth
//! - `windows`: Windows support via WinRT
//! - `embedded`: Embedded/no_std support
//! - `coded-phy`: Enable Coded PHY for extended range
//! - `extended-adv`: Enable extended advertising
//!
//! ## External Crate Usage (peat-ffi)
//!
//! This crate exports platform adapters for use by external crates like `peat-ffi`.
//! Each platform adapter is conditionally exported based on feature flags:
//!
//! ```toml
//! # In your Cargo.toml
//! [dependencies]
//! peat-btle = { version = "0.2.0", features = ["linux"] }
//! ```
//!
//! Then use the appropriate adapter:
//!
//! ```ignore
//! use peat_btle::{BleConfig, BluerAdapter, PeatMesh, NodeId};
//!
//! // Platform adapter is automatically available via feature flag
//! let adapter = BluerAdapter::new().await?;
//! let config = BleConfig::peat_lite(NodeId::new(0x12345678));
//! ```
//!
//! ### Platform → Adapter Mapping
//!
//! | Feature | Target | Adapter Type |
//! |---------|--------|--------------|
//! | `linux` | Linux | `BluerAdapter` |
//! | `android` | Android | `AndroidAdapter` |
//! | `macos` | macOS | `CoreBluetoothAdapter` |
//! | `ios` | iOS | `CoreBluetoothAdapter` |
//! | `windows` | Windows | `WinRtBleAdapter` |
//!
//! ### Document Encoding for Translation Layer
//!
//! For translating between Automerge (full Peat) and peat-btle documents:
//!
//! ```ignore
//! use peat_btle::PeatDocument;
//!
//! // Decode bytes received from BLE
//! let doc = PeatDocument::from_bytes(&received_bytes)?;
//!
//! // Encode for BLE transmission
//! let bytes = doc.to_bytes();
//! ```
//!
//! ## Power Profiles
//!
//! | Profile | Duty Cycle | Watch Battery |
//! |---------|------------|---------------|
//! | Aggressive | 20% | ~6 hours |
//! | Balanced | 10% | ~12 hours |
//! | **LowPower** | **2%** | **~20+ hours** |
//!
//! ## Related ADRs
//!
//! - ADR-039: PEAT-BTLE Mesh Transport Crate
//! - ADR-032: Pluggable Transport Abstraction
//! - ADR-035: Peat-Lite Embedded Nodes
//! - ADR-037: Resource-Constrained Device Optimization

#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(rustdoc::missing_crate_level_docs)]

#[cfg(not(feature = "std"))]
extern crate alloc;

pub mod address_rotation;
pub mod config;
pub mod discovery;
pub mod document;
pub mod document_sync;
pub mod error;
pub mod gatt;
#[cfg(feature = "std")]
pub mod gossip;
pub mod mesh;
pub mod observer;
pub mod peat_mesh;
pub mod peer;
pub mod peer_lifetime;
pub mod peer_manager;
#[cfg(feature = "std")]
pub mod persistence;
pub mod phy;
pub mod platform;
pub mod power;
pub mod reconnect;
pub mod registry;
pub mod relay;

pub mod security;
pub mod sync;
pub mod transport;

// UniFFI bindings (generates Kotlin + Swift)
#[cfg(feature = "uniffi")]
pub mod uniffi_bindings;

// UniFFI scaffolding - must be at crate root
#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

// Re-exports for convenience
pub use config::{
    BleConfig, BlePhy, DiscoveryConfig, GattConfig, MeshConfig, PowerProfile, DEFAULT_MESH_ID,
};
#[cfg(feature = "std")]
pub use discovery::Scanner;
pub use discovery::{Advertiser, PeatBeacon, ScanFilter};
pub use error::{BleError, Result};
#[cfg(feature = "std")]
pub use gatt::PeatGattService;
pub use gatt::SyncProtocol;
#[cfg(feature = "std")]
pub use mesh::MeshManager;
pub use mesh::{MeshRouter, MeshTopology, TopologyConfig, TopologyEvent};
pub use phy::{PhyCapabilities, PhyController, PhyStrategy};
pub use platform::{BleAdapter, ConnectionEvent, DisconnectReason, DiscoveredDevice, StubAdapter};

// Platform-specific adapter re-exports for external crates (peat-ffi)
// These allow external crates to use platform adapters via feature flags
#[cfg(all(feature = "linux", target_os = "linux"))]
pub use platform::linux::BluerAdapter;

#[cfg(feature = "android")]
pub use platform::android::AndroidAdapter;

#[cfg(any(feature = "macos", feature = "ios"))]
pub use platform::apple::CoreBluetoothAdapter;

#[cfg(feature = "windows")]
pub use platform::windows::WinRtBleAdapter;

#[cfg(feature = "std")]
pub use platform::mock::MockBleAdapter;
pub use power::{BatteryState, RadioScheduler, SyncPriority};
pub use sync::{GattSyncProtocol, SyncConfig, SyncState};
pub use transport::{BleConnection, BluetoothLETransport, MeshTransport, TransportCapabilities};

// New centralized mesh management types
pub use document::{
    MergeResult, PeatDocument, ENCRYPTED_MARKER, EXTENDED_MARKER, KEY_EXCHANGE_MARKER,
    PEER_E2EE_MARKER,
};

// Security (mesh-wide and per-peer encryption)
pub use document_sync::{DocumentCheck, DocumentSync};
#[cfg(feature = "std")]
pub use observer::{CollectingObserver, ObserverManager};
pub use observer::{DisconnectReason as PeatDisconnectReason, PeatEvent, PeatObserver};
#[cfg(feature = "std")]
pub use peat_mesh::{DataReceivedResult, PeatMesh, PeatMeshConfig, RelayDecision};
pub use peer::{
    ConnectionState, ConnectionStateGraph, FullStateCountSummary, IndirectPeer, PeatPeer,
    PeerConnectionState, PeerDegree, PeerManagerConfig, SignalStrength, StateCountSummary,
    MAX_TRACKED_DEGREE,
};
pub use peer_manager::PeerManager;

// Device identity and attestation
pub use security::{
    DeviceIdentity, IdentityAttestation, IdentityError, IdentityRecord, IdentityRegistry,
    RegistryResult,
};
// Mesh genesis and credentials
pub use security::{MembershipPolicy, MeshCredentials, MeshGenesis};

// Phase 1: Mesh-wide encryption
pub use security::{EncryptedDocument, EncryptionError, MeshEncryptionKey};
// Phase 2: Per-peer E2EE
#[cfg(feature = "std")]
pub use security::{
    KeyExchangeMessage, PeerEncryptedMessage, PeerIdentityKey, PeerSession, PeerSessionKey,
    PeerSessionManager, SessionState,
};

// Credential persistence
#[cfg(feature = "std")]
pub use security::{
    MemoryStorage, PersistedState, PersistenceError, SecureStorage, PERSISTED_STATE_VERSION,
};

// Gossip and persistence abstractions
#[cfg(feature = "std")]
pub use gossip::{BroadcastAll, EmergencyAware, GossipStrategy, RandomFanout, SignalBasedFanout};
#[cfg(feature = "std")]
pub use persistence::{DocumentStore, FileStore, MemoryStore, SharedStore};

// Multi-hop relay support
pub use relay::{
    MessageId, RelayEnvelope, RelayFlags, SeenMessageCache, DEFAULT_MAX_HOPS, DEFAULT_SEEN_TTL_MS,
    RELAY_ENVELOPE_MARKER,
};

// Extensible document registry for app-layer types
pub use registry::{
    decode_header, decode_typed, encode_with_header, AppOperation, DocumentRegistry, DocumentType,
    APP_OP_BASE, APP_TYPE_MAX, APP_TYPE_MIN,
};

/// Peat BLE Service UUID (128-bit)
///
/// All Peat nodes advertise this UUID for discovery.
pub const PEAT_SERVICE_UUID: uuid::Uuid = uuid::uuid!("a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d");

/// Peat BLE Service UUID (16-bit short form)
///
/// Derived from the first two bytes of the 128-bit UUID (0xA1B2 from a1b2c3d4).
/// Used for space-constrained advertising to fit within 31-byte limit.
pub const PEAT_SERVICE_UUID_16BIT: u16 = 0xA1B2;

/// PEAT Node Info Characteristic UUID
pub const CHAR_NODE_INFO_UUID: u16 = 0x0001;

/// PEAT Sync State Characteristic UUID
pub const CHAR_SYNC_STATE_UUID: u16 = 0x0002;

/// PEAT Sync Data Characteristic UUID
pub const CHAR_SYNC_DATA_UUID: u16 = 0x0003;

/// PEAT Command Characteristic UUID
pub const CHAR_COMMAND_UUID: u16 = 0x0004;

/// PEAT Status Characteristic UUID
pub const CHAR_STATUS_UUID: u16 = 0x0005;

/// Crate version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Node identifier
///
/// Represents a unique node in the Peat mesh. For BLE, this is typically
/// derived from the Bluetooth MAC address or a configured value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct NodeId {
    /// 32-bit node identifier
    id: u32,
}

impl NodeId {
    /// Create a new node ID from a 32-bit value
    pub fn new(id: u32) -> Self {
        Self { id }
    }

    /// Get the raw 32-bit ID value
    pub fn as_u32(&self) -> u32 {
        self.id
    }

    /// Create from a string representation (hex format)
    pub fn parse(s: &str) -> Option<Self> {
        // Try parsing as hex (with or without 0x prefix)
        let s = s.trim_start_matches("0x").trim_start_matches("0X");
        u32::from_str_radix(s, 16).ok().map(Self::new)
    }

    /// Derive a NodeId from a BLE MAC address.
    ///
    /// Uses the last 4 bytes of the 6-byte MAC address as the 32-bit node ID.
    /// This provides a consistent node ID derived from the device's Bluetooth
    /// hardware address.
    ///
    /// # Arguments
    /// * `mac` - 6-byte MAC address array (e.g., [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF])
    ///
    /// # Example
    /// ```
    /// use peat_btle::NodeId;
    ///
    /// let mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
    /// let node_id = NodeId::from_mac_address(&mac);
    /// assert_eq!(node_id.as_u32(), 0x22334455);
    /// ```
    pub fn from_mac_address(mac: &[u8; 6]) -> Self {
        // Use last 4 bytes: mac[2], mac[3], mac[4], mac[5]
        let id = ((mac[2] as u32) << 24)
            | ((mac[3] as u32) << 16)
            | ((mac[4] as u32) << 8)
            | (mac[5] as u32);
        Self::new(id)
    }

    /// Derive a NodeId from a MAC address string.
    ///
    /// Parses a MAC address in "AA:BB:CC:DD:EE:FF" format and derives
    /// the node ID from the last 4 bytes.
    ///
    /// # Arguments
    /// * `mac_str` - MAC address string in colon-separated hex format
    ///
    /// # Returns
    /// `Some(NodeId)` if parsing succeeds, `None` otherwise
    ///
    /// # Example
    /// ```
    /// use peat_btle::NodeId;
    ///
    /// let node_id = NodeId::from_mac_string("00:11:22:33:44:55").unwrap();
    /// assert_eq!(node_id.as_u32(), 0x22334455);
    /// ```
    pub fn from_mac_string(mac_str: &str) -> Option<Self> {
        let parts: Vec<&str> = mac_str.split(':').collect();
        if parts.len() != 6 {
            return None;
        }

        let mut mac = [0u8; 6];
        for (i, part) in parts.iter().enumerate() {
            mac[i] = u8::from_str_radix(part, 16).ok()?;
        }

        Some(Self::from_mac_address(&mac))
    }
}

impl core::fmt::Display for NodeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:08X}", self.id)
    }
}

impl From<u32> for NodeId {
    fn from(id: u32) -> Self {
        Self::new(id)
    }
}

impl From<NodeId> for u32 {
    fn from(node_id: NodeId) -> Self {
        node_id.id
    }
}

/// Node capability flags
///
/// Advertised in the Peat beacon to indicate what this node can do.
pub mod capabilities {
    /// This is an Peat-Lite node (minimal state, single parent)
    pub const LITE_NODE: u16 = 0x0001;
    /// Has accelerometer sensor
    pub const SENSOR_ACCEL: u16 = 0x0002;
    /// Has temperature sensor
    pub const SENSOR_TEMP: u16 = 0x0004;
    /// Has button input
    pub const SENSOR_BUTTON: u16 = 0x0008;
    /// Has LED output
    pub const ACTUATOR_LED: u16 = 0x0010;
    /// Has vibration motor
    pub const ACTUATOR_VIBRATE: u16 = 0x0020;
    /// Has display
    pub const HAS_DISPLAY: u16 = 0x0040;
    /// Can relay messages (not a leaf)
    pub const CAN_RELAY: u16 = 0x0080;
    /// Supports Coded PHY
    pub const CODED_PHY: u16 = 0x0100;
    /// Has GPS
    pub const HAS_GPS: u16 = 0x0200;
}

/// Hierarchy levels in the Peat mesh
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(u8)]
pub enum HierarchyLevel {
    /// Platform/soldier level (leaf nodes)
    #[default]
    Platform = 0,
    /// Squad level
    Squad = 1,
    /// Platoon level
    Platoon = 2,
    /// Company level
    Company = 3,
}

impl From<u8> for HierarchyLevel {
    fn from(value: u8) -> Self {
        match value {
            0 => HierarchyLevel::Platform,
            1 => HierarchyLevel::Squad,
            2 => HierarchyLevel::Platoon,
            3 => HierarchyLevel::Company,
            _ => HierarchyLevel::Platform,
        }
    }
}

impl From<HierarchyLevel> for u8 {
    fn from(level: HierarchyLevel) -> Self {
        level as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_id() {
        let id = NodeId::new(0x12345678);
        assert_eq!(id.as_u32(), 0x12345678);
        assert_eq!(id.to_string(), "12345678");
    }

    #[test]
    fn test_node_id_parse() {
        assert_eq!(NodeId::parse("12345678").unwrap().as_u32(), 0x12345678);
        assert_eq!(NodeId::parse("0x12345678").unwrap().as_u32(), 0x12345678);
        assert!(NodeId::parse("not_hex").is_none());
    }

    #[test]
    fn test_node_id_from_mac_address() {
        // MAC: AA:BB:CC:DD:EE:FF -> NodeId from last 4 bytes: 0xCCDDEEFF
        let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let node_id = NodeId::from_mac_address(&mac);
        assert_eq!(node_id.as_u32(), 0xCCDDEEFF);
    }

    #[test]
    fn test_node_id_from_mac_string() {
        let node_id = NodeId::from_mac_string("AA:BB:CC:DD:EE:FF").unwrap();
        assert_eq!(node_id.as_u32(), 0xCCDDEEFF);

        // Lowercase should work too
        let node_id = NodeId::from_mac_string("aa:bb:cc:dd:ee:ff").unwrap();
        assert_eq!(node_id.as_u32(), 0xCCDDEEFF);

        // Invalid formats
        assert!(NodeId::from_mac_string("invalid").is_none());
        assert!(NodeId::from_mac_string("AA:BB:CC:DD:EE").is_none()); // Too short
        assert!(NodeId::from_mac_string("AA:BB:CC:DD:EE:FF:GG").is_none()); // Too long
        assert!(NodeId::from_mac_string("ZZ:BB:CC:DD:EE:FF").is_none()); // Invalid hex
    }

    #[test]
    fn test_hierarchy_level() {
        assert_eq!(HierarchyLevel::from(0), HierarchyLevel::Platform);
        assert_eq!(HierarchyLevel::from(3), HierarchyLevel::Company);
        assert_eq!(u8::from(HierarchyLevel::Squad), 1);
    }

    #[test]
    fn test_service_uuid() {
        assert_eq!(
            PEAT_SERVICE_UUID.to_string(),
            "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d"
        );
    }

    #[test]
    fn test_capabilities() {
        let caps = capabilities::LITE_NODE | capabilities::SENSOR_ACCEL | capabilities::HAS_GPS;
        assert_eq!(caps, 0x0203);
    }
}
