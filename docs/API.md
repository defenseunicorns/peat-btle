# ECHE-BTLE API Reference

This document provides a comprehensive guide to the main types, traits, and APIs in the `eche-btle` crate.

## Table of Contents

- [Overview](#overview)
- [Core Types](#core-types)
  - [NodeId](#nodeid)
  - [HierarchyLevel](#hierarchylevel)
  - [Capabilities](#capabilities)
- [Entry Points](#entry-points)
  - [EcheMesh (High-Level)](#hivemesh-high-level)
  - [BluetoothLETransport (Low-Level)](#bluetoothletransport-low-level)
- [Configuration](#configuration)
  - [BleConfig](#bleconfig)
  - [EcheMeshConfig](#hivemeshconfig)
  - [PowerProfile](#powerprofile)
  - [BlePhy](#blephy)
- [Platform Abstraction](#platform-abstraction)
  - [BleAdapter Trait](#bleadapter-trait)
  - [BleConnection Trait](#bleconnection-trait)
- [Security](#security)
  - [Mesh-Wide Encryption](#mesh-wide-encryption)
  - [Per-Peer E2EE](#per-peer-e2ee)
- [Events and Observers](#events-and-observers)
  - [EcheEvent](#hiveevent)
  - [EcheObserver Trait](#hiveobserver-trait)
- [Error Handling](#error-handling)
- [Feature Flags](#feature-flags)

---

## Overview

`eche-btle` provides two main entry points:

| Entry Point | Use Case | Complexity |
|-------------|----------|------------|
| `EcheMesh` | Full mesh management with peers, sync, events | High-level |
| `BluetoothLETransport` | Raw BLE transport for custom implementations | Low-level |

For most applications, use `EcheMesh`. It handles peer discovery, document synchronization, encryption, and event notification automatically.

---

## Core Types

### NodeId

A 32-bit unique identifier for nodes in the mesh.

```rust
use eche_btle::NodeId;

// From explicit value
let node = NodeId::new(0x12345678);

// From MAC address
let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
let node = NodeId::from_mac_address(&mac);  // Uses last 4 bytes

// From MAC string
let node = NodeId::from_mac_string("AA:BB:CC:DD:EE:FF").unwrap();

// Parse hex string
let node = NodeId::parse("12345678").unwrap();
let node = NodeId::parse("0x12345678").unwrap();

// Display
println!("{}", node);  // "12345678" (uppercase hex)

// Conversions
let raw: u32 = node.as_u32();
let node: NodeId = 0x12345678.into();
```

### HierarchyLevel

Represents the node's position in the tactical hierarchy.

```rust
use eche_btle::HierarchyLevel;

let level = HierarchyLevel::Platform;  // Leaf node (soldier)
let level = HierarchyLevel::Squad;     // Squad leader
let level = HierarchyLevel::Platoon;   // Platoon leader
let level = HierarchyLevel::Company;   // Company command

// Conversions
let byte: u8 = level.into();
let level: HierarchyLevel = 2u8.into();  // Platoon
```

### Capabilities

Bitflags indicating node capabilities, advertised in the Eche beacon.

```rust
use eche_btle::capabilities;

let caps = capabilities::LITE_NODE        // Eche-Lite node
         | capabilities::SENSOR_ACCEL     // Has accelerometer
         | capabilities::HAS_GPS          // Has GPS
         | capabilities::CAN_RELAY;       // Can relay messages

// Check capabilities
if caps & capabilities::CODED_PHY != 0 {
    // Node supports Coded PHY for long range
}
```

Available flags:
- `LITE_NODE` (0x0001): Eche-Lite node (minimal state)
- `SENSOR_ACCEL` (0x0002): Has accelerometer
- `SENSOR_TEMP` (0x0004): Has temperature sensor
- `SENSOR_BUTTON` (0x0008): Has button input
- `ACTUATOR_LED` (0x0010): Has LED output
- `ACTUATOR_VIBRATE` (0x0020): Has vibration motor
- `HAS_DISPLAY` (0x0040): Has display
- `CAN_RELAY` (0x0080): Can relay messages
- `CODED_PHY` (0x0100): Supports Coded PHY
- `HAS_GPS` (0x0200): Has GPS

---

## Entry Points

### EcheMesh (High-Level)

`EcheMesh` is the main facade for Eche BLE mesh operations. It composes peer management, document sync, and observer notifications.

**Creation:**

```rust
use eche_btle::{EcheMesh, EcheMeshConfig, NodeId};

let config = EcheMeshConfig::new(
    NodeId::new(0x12345678),
    "ALPHA-1",   // Callsign
    "DEMO",      // Mesh ID
);

let mesh = EcheMesh::new(config);
```

**Configuration Options:**

```rust
let config = EcheMeshConfig::new(node_id, "ALPHA-1", "DEMO")
    .with_encryption([0x42u8; 32])     // Enable mesh-wide encryption
    .with_strict_encryption()           // Reject unencrypted docs
    .with_sync_interval(5000)          // Sync every 5 seconds
    .with_peer_timeout(30_000)         // 30 second peer timeout
    .with_max_peers(10)                // Maximum 10 peers
    .with_peripheral_type(PeripheralType::SoldierSensor);
```

**BLE Callbacks (Platform Integration):**

Your platform BLE layer calls these methods:

```rust
// When a device is discovered
mesh.on_ble_discovered(
    "device-uuid",               // Platform identifier
    Some("HIVE_DEMO-AABBCCDD"), // Device name
    -65,                         // RSSI in dBm
    Some("DEMO"),                // Mesh ID from name
    now_ms,                      // Current timestamp
);

// When connected
mesh.on_ble_connected("device-uuid", now_ms);

// When disconnected
mesh.on_ble_disconnected("device-uuid", DisconnectReason::RemoteRequest);

// When data received
let result = mesh.on_ble_data_received("device-uuid", &data, now_ms);
if let Some(result) = result {
    if result.is_emergency {
        // Handle emergency!
    }
}

// Periodic maintenance (call every ~1 second)
if let Some(sync_data) = mesh.tick(now_ms) {
    // Broadcast sync_data to connected peers
}
```

**User Actions:**

```rust
// Send emergency alert
let doc = mesh.send_emergency(timestamp);
// Broadcast `doc` to all peers

// Send ACK
let doc = mesh.send_ack(timestamp);

// Clear event
mesh.clear_event();

// Check state
mesh.is_emergency_active();
mesh.is_ack_active();
```

**Emergency with ACK Tracking:**

```rust
// Start emergency with known peers
let doc = mesh.start_emergency_with_known_peers(timestamp);

// Check ACK status
let (source, timestamp, acked, pending) = mesh.get_emergency_status().unwrap();
println!("{} of {} peers ACKed", acked, acked + pending);

// Check if specific peer ACKed
if mesh.has_peer_acked(peer_node_id) { /* ... */ }

// Check if all peers ACKed
if mesh.all_peers_acked() { /* ... */ }
```

**State Queries:**

```rust
mesh.node_id()           // Our NodeId
mesh.callsign()          // Our callsign
mesh.mesh_id()           // Mesh identifier
mesh.device_name()       // BLE device name (e.g., "HIVE_DEMO-12345678")
mesh.peer_count()        // Number of known peers
mesh.connected_count()   // Number of connected peers
mesh.get_peers()         // Vec<EchePeer> of all peers
mesh.get_connected_peers() // Vec<EchePeer> of connected peers
mesh.get_peer(node_id)   // Option<EchePeer>
mesh.total_count()       // CRDT counter total
mesh.version()           // Document version
```

**Health Updates:**

```rust
mesh.update_health(battery_percent);  // 0-100
mesh.update_activity(activity_level); // 0=still, 1=walking, 2=running, 3=fall
mesh.update_health_full(battery, activity);
```

### BluetoothLETransport (Low-Level)

For custom transport implementations that need direct control.

```rust
use eche_btle::{BluetoothLETransport, BleConfig, NodeId, MeshTransport};

let config = BleConfig::hive_lite(NodeId::new(0x12345678));
let adapter = platform::linux::BluerAdapter::new()?;

let transport = BluetoothLETransport::new(config, adapter);

// Start transport
transport.start().await?;

// Connect to peer
let conn = transport.connect(&peer_id).await?;

// Query state
transport.peer_count();
transport.connected_peers();
transport.capabilities();

// Stop
transport.stop().await?;
```

---

## Configuration

### BleConfig

Main configuration structure for the BLE transport.

```rust
use eche_btle::{BleConfig, NodeId, PowerProfile, BlePhy};

// Default configuration
let config = BleConfig::new(NodeId::new(0x12345678));

// Eche-Lite optimized (low power, leaf node)
let config = BleConfig::hive_lite(NodeId::new(0x12345678));
```

**Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `node_id` | `NodeId` | This node's identifier |
| `capabilities` | `u16` | Capability flags |
| `hierarchy_level` | `u8` | Position in hierarchy (0-3) |
| `geohash` | `u32` | 24-bit geohash location |
| `discovery` | `DiscoveryConfig` | Scan/advertise settings |
| `gatt` | `GattConfig` | GATT settings |
| `mesh` | `MeshConfig` | Mesh topology settings |
| `power_profile` | `PowerProfile` | Power management profile |
| `phy` | `PhyConfig` | PHY selection settings |
| `security` | `SecurityConfig` | Security settings |

### EcheMeshConfig

Configuration for the high-level `EcheMesh` facade.

```rust
use eche_btle::{EcheMeshConfig, NodeId};
use eche_btle::sync::crdt::PeripheralType;

let config = EcheMeshConfig::new(
    NodeId::new(0x12345678),
    "ALPHA-1",  // callsign
    "DEMO",     // mesh_id
);
```

**Builder Methods:**

| Method | Description |
|--------|-------------|
| `.with_encryption(secret)` | Enable mesh-wide encryption |
| `.with_strict_encryption()` | Reject unencrypted documents |
| `.with_peripheral_type(type)` | Set device type |
| `.with_sync_interval(ms)` | Set sync broadcast interval |
| `.with_peer_timeout(ms)` | Set peer timeout |
| `.with_max_peers(n)` | Set maximum peer count |

### PowerProfile

Controls radio duty cycle for power management.

```rust
use eche_btle::PowerProfile;

// Preset profiles
let profile = PowerProfile::Aggressive; // ~20% duty, ~6h battery
let profile = PowerProfile::Balanced;   // ~10% duty, ~12h battery
let profile = PowerProfile::LowPower;   // ~2% duty, ~20h battery

// Custom profile
let profile = PowerProfile::Custom {
    scan_interval_ms: 5000,
    scan_window_ms: 100,
    adv_interval_ms: 2000,
    conn_interval_ms: 100,
};

// Query profile
profile.scan_interval_ms();
profile.scan_window_ms();
profile.adv_interval_ms();
profile.conn_interval_ms();
profile.duty_cycle_percent();
```

### BlePhy

BLE Physical Layer options (BLE 5.0+).

```rust
use eche_btle::BlePhy;

let phy = BlePhy::Le1M;      // 1 Mbps, ~100m range (default)
let phy = BlePhy::Le2M;      // 2 Mbps, ~50m range
let phy = BlePhy::LeCodedS2; // 500 kbps, ~200m range
let phy = BlePhy::LeCodedS8; // 125 kbps, ~400m range

// Query PHY properties
phy.bandwidth_bps();        // Theoretical bandwidth
phy.typical_range_meters(); // Typical range
phy.requires_ble5();        // Needs BLE 5.0?
```

---

## Platform Abstraction

### BleAdapter Trait

Each platform implements this trait to provide BLE functionality.

```rust
use eche_btle::platform::BleAdapter;

#[async_trait]
pub trait BleAdapter: Send + Sync {
    // Lifecycle
    async fn init(&mut self, config: &BleConfig) -> Result<()>;
    async fn start(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    fn is_powered(&self) -> bool;
    fn address(&self) -> Option<String>;

    // Discovery
    async fn start_scan(&self, config: &DiscoveryConfig) -> Result<()>;
    async fn stop_scan(&self) -> Result<()>;
    async fn start_advertising(&self, config: &DiscoveryConfig) -> Result<()>;
    async fn stop_advertising(&self) -> Result<()>;
    fn set_discovery_callback(&mut self, callback: Option<DiscoveryCallback>);

    // Connections
    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn BleConnection>>;
    async fn disconnect(&self, peer_id: &NodeId) -> Result<()>;
    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn BleConnection>>;
    fn peer_count(&self) -> usize;
    fn connected_peers(&self) -> Vec<NodeId>;
    fn set_connection_callback(&mut self, callback: Option<ConnectionCallback>);

    // GATT
    async fn register_gatt_service(&self) -> Result<()>;
    async fn unregister_gatt_service(&self) -> Result<()>;

    // Capabilities
    fn supports_coded_phy(&self) -> bool;
    fn supports_extended_advertising(&self) -> bool;
    fn max_mtu(&self) -> u16;
    fn max_connections(&self) -> u8;
}
```

**Platform Implementations:**

| Platform | Module | Adapter Type |
|----------|--------|--------------|
| Linux | `platform::linux` | `BluerAdapter` |
| Android | `platform::android` | `AndroidAdapter` |
| macOS/iOS | `platform::apple` | `CoreBluetoothAdapter` |
| Windows | `platform::windows` | `WinRTAdapter` |
| ESP32 | `platform::esp32` | `NimBLEAdapter` |
| Testing | `platform::mock` | `MockAdapter` |

### BleConnection Trait

Represents an active GATT connection.

```rust
pub trait BleConnection: Send + Sync {
    fn peer_id(&self) -> &NodeId;
    fn is_alive(&self) -> bool;
    fn mtu(&self) -> u16;
    fn phy(&self) -> BlePhy;
    fn rssi(&self) -> Option<i8>;
    fn connected_duration(&self) -> Duration;
}
```

---

## Security

### Mesh-Wide Encryption

All mesh members share a secret. Protects against external eavesdroppers.

**Via EcheMesh:**

```rust
// At creation
let config = EcheMeshConfig::new(node_id, "ALPHA", "DEMO")
    .with_encryption([0x42u8; 32]);
let mesh = EcheMesh::new(config);

// At runtime
mesh.enable_encryption(&[0x42u8; 32]);
mesh.disable_encryption();

// Query state
mesh.is_encryption_enabled();
mesh.is_strict_encryption_enabled();
```

**Direct API (Advanced):**

```rust
use eche_btle::security::MeshEncryptionKey;

let secret = [0x42u8; 32];
let key = MeshEncryptionKey::from_shared_secret("DEMO", &secret);

// Encrypt
let ciphertext = key.encrypt(b"plaintext").unwrap();

// Decrypt
let plaintext = key.decrypt(&ciphertext).unwrap();

// Byte format for transmission
let bytes = key.encrypt_to_bytes(b"plaintext").unwrap();
let plaintext = key.decrypt_from_bytes(&bytes).unwrap();
```

**Overhead:** 30 bytes (2 marker + 12 nonce + 16 auth tag)

### Per-Peer E2EE

Two specific peers establish encrypted sessions via X25519 key exchange. Only sender and recipient can decrypt.

**Via EcheMesh:**

```rust
// Enable E2EE capability
mesh.enable_peer_e2ee();

// Initiate session with specific peer
let key_exchange = mesh.initiate_peer_e2ee(peer_node_id, now_ms);
// Send key_exchange bytes to peer

// After key exchange completes
if mesh.has_peer_e2ee_session(peer_node_id) {
    // Send encrypted message
    let encrypted = mesh.send_peer_e2ee(peer_node_id, b"secret", now_ms);
    // Send encrypted bytes to peer
}

// Query state
mesh.is_peer_e2ee_enabled();
mesh.peer_e2ee_public_key();
mesh.peer_e2ee_session_state(peer_node_id);
mesh.peer_e2ee_session_count();
mesh.peer_e2ee_established_count();

// Close session
mesh.close_peer_e2ee(peer_node_id);
```

**Direct API (Advanced):**

```rust
use eche_btle::security::PeerSessionManager;
use eche_btle::NodeId;

let mut alice = PeerSessionManager::new(NodeId::new(0x11111111));
let mut bob = PeerSessionManager::new(NodeId::new(0x22222222));

// Alice initiates
let alice_msg = alice.initiate_session(NodeId::new(0x22222222), now_ms);

// Bob responds
let (bob_response, established) = bob.handle_key_exchange(&alice_msg, now_ms).unwrap();

// Alice completes
alice.handle_key_exchange(&bob_response, now_ms).unwrap();

// Now both can encrypt/decrypt
let encrypted = alice.encrypt_for_peer(NodeId::new(0x22222222), b"secret", now_ms).unwrap();
let decrypted = bob.decrypt_from_peer(&encrypted, now_ms).unwrap();
```

**Overhead:** 46 bytes (2 marker + 4 recipient + 4 sender + 8 counter + 12 nonce + 16 tag)

---

## Events and Observers

### EcheEvent

Events emitted by the mesh.

```rust
use eche_btle::observer::EcheEvent;

match event {
    // Peer lifecycle
    EcheEvent::PeerDiscovered { peer } => { /* new peer found */ }
    EcheEvent::PeerConnected { node_id } => { /* peer connected */ }
    EcheEvent::PeerDisconnected { node_id, reason } => { /* peer disconnected */ }
    EcheEvent::PeerLost { node_id } => { /* peer timed out */ }

    // Mesh events
    EcheEvent::EmergencyReceived { from_node } => { /* emergency! */ }
    EcheEvent::AckReceived { from_node } => { /* peer ACKed */ }
    EcheEvent::DocumentSynced { from_node, total_count } => { /* sync complete */ }
    EcheEvent::MeshStateChanged { peer_count, connected_count } => { /* state change */ }

    // Per-peer E2EE
    EcheEvent::PeerE2eeEstablished { peer_node_id } => { /* E2EE ready */ }
    EcheEvent::PeerE2eeClosed { peer_node_id } => { /* E2EE ended */ }
    EcheEvent::PeerE2eeMessageReceived { from_node, data } => { /* encrypted msg */ }

    // Security
    EcheEvent::SecurityViolation { kind, source } => { /* security issue */ }
}
```

### EcheObserver Trait

Implement to receive mesh events.

```rust
use eche_btle::observer::{EcheEvent, EcheObserver};
use std::sync::Arc;

struct MyObserver;

impl EcheObserver for MyObserver {
    fn on_event(&self, event: EcheEvent) {
        match event {
            EcheEvent::EmergencyReceived { from_node } => {
                println!("EMERGENCY from {:08X}!", from_node.as_u32());
                // Play alarm, show notification, etc.
            }
            _ => {}
        }
    }
}

// Register observer
let observer = Arc::new(MyObserver);
mesh.add_observer(observer.clone());

// Remove when done
mesh.remove_observer(&observer);
```

**Testing Helper:**

```rust
use eche_btle::observer::CollectingObserver;

let observer = Arc::new(CollectingObserver::new());
mesh.add_observer(observer.clone());

// ... perform operations ...

let events = observer.events();
assert!(events.iter().any(|e| matches!(e, EcheEvent::EmergencyReceived { .. })));
```

---

## Error Handling

All fallible operations return `Result<T, BleError>`.

```rust
use eche_btle::{BleError, Result};

fn example() -> Result<()> {
    // Errors you may encounter:
    Err(BleError::AdapterNotAvailable)?;
    Err(BleError::NotPowered)?;
    Err(BleError::PermissionDenied("location".into()))?;
    Err(BleError::NotSupported("coded_phy".into()))?;
    Err(BleError::ConnectionFailed("timeout".into()))?;
    Err(BleError::ConnectionLost("link_loss".into()))?;
    Err(BleError::GattError("write failed".into()))?;
    Err(BleError::Timeout)?;
    Err(BleError::InvalidConfig("bad interval".into()))?;
    Err(BleError::ResourceExhausted("max connections".into()))?;
    Ok(())
}
```

---

## Feature Flags

| Feature | Description | Default |
|---------|-------------|---------|
| `std` | Standard library support | Yes |
| `linux` | Linux/BlueZ support | No |
| `android` | Android JNI support | No |
| `macos` | macOS CoreBluetooth | No |
| `ios` | iOS CoreBluetooth | No |
| `windows` | Windows WinRT | No |
| `embedded` | Embedded/no_std support | No |
| `esp32` | ESP32 NimBLE support | No |
| `coded-phy` | Enable Coded PHY | No |
| `extended-adv` | Extended advertising | No |

**Usage in Cargo.toml:**

```toml
[dependencies]
eche-btle = { version = "0.1", features = ["linux", "coded-phy"] }
```

**Conditional Compilation:**

```rust
#[cfg(feature = "linux")]
use eche_btle::platform::linux::BluerAdapter;

#[cfg(feature = "android")]
use eche_btle::platform::android::AndroidAdapter;
```

---

## Constants

```rust
use eche_btle::{
    ECHE_SERVICE_UUID,       // 128-bit service UUID
    ECHE_SERVICE_UUID_16BIT, // 16-bit short form (0xF47A)
    CHAR_NODE_INFO_UUID,     // Node info characteristic
    CHAR_SYNC_STATE_UUID,    // Sync state characteristic
    CHAR_SYNC_DATA_UUID,     // Sync data characteristic
    CHAR_COMMAND_UUID,       // Command characteristic
    CHAR_STATUS_UUID,        // Status characteristic
    VERSION,                 // Crate version
};
```

---

## Common Patterns

### Basic Mesh Setup

```rust
use eche_btle::{EcheMesh, EcheMeshConfig, NodeId};
use eche_btle::observer::{EcheEvent, EcheObserver};
use std::sync::Arc;

// 1. Create config
let config = EcheMeshConfig::new(
    NodeId::new(0x12345678),
    "ALPHA-1",
    "DEMO",
);

// 2. Create mesh
let mesh = EcheMesh::new(config);

// 3. Add observer
struct Handler;
impl EcheObserver for Handler {
    fn on_event(&self, event: EcheEvent) {
        // Handle events
    }
}
mesh.add_observer(Arc::new(Handler));

// 4. Platform integration (called from BLE callbacks)
// mesh.on_ble_discovered(...)
// mesh.on_ble_connected(...)
// mesh.on_ble_data_received(...)

// 5. Periodic tick
loop {
    if let Some(data) = mesh.tick(now_ms()) {
        // Broadcast data to peers
    }
    sleep(Duration::from_secs(1));
}
```

### Emergency Flow

```rust
// Sender
let doc = mesh.start_emergency_with_known_peers(now_ms());
broadcast_to_peers(&doc);

// Receivers automatically get EmergencyReceived event

// Receiver sends ACK
let ack_doc = mesh.ack_emergency(now_ms());
broadcast_to_peers(&ack_doc.unwrap());

// Sender checks ACK status
while !mesh.all_peers_acked() {
    let (_, _, acked, pending) = mesh.get_emergency_status().unwrap();
    println!("ACK: {}/{}", acked, acked + pending);
}
```

### Encrypted Mesh

```rust
let secret = derive_key_from_password("formation-password");

let config = EcheMeshConfig::new(node_id, "ALPHA", "DEMO")
    .with_encryption(secret)
    .with_strict_encryption();

let mesh = EcheMesh::new(config);

// All documents automatically encrypted/decrypted
// SecurityViolation events for unauthorized messages
```
