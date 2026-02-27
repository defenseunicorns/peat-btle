# PEAT-BTLE Examples

This directory contains runnable examples demonstrating various features of the PEAT-BTLE library.

## Running Examples

```bash
# Basic examples (no platform features required)
cargo run --example basic_mesh
cargo run --example encryption_demo
cargo run --example peer_e2ee

# Linux examples (requires BlueZ)
cargo run --example linux_scanner --features linux
```

## Examples Overview

### basic_mesh.rs

Demonstrates the core `PeatMesh` API for CRDT-based mesh synchronization:

- Creating mesh nodes with `PeatMeshConfig`
- Adding observers for mesh events
- Simulating BLE discovery and connection
- Document synchronization between nodes
- Emergency and ACK flow

**Key concepts:**
- `PeatMesh` - Main entry point for mesh operations
- `PeatMeshConfig` - Configuration including node ID, callsign, mesh ID
- `PeatObserver` - Trait for receiving mesh events
- `PeatEvent` - Events like `PeerDiscovered`, `EmergencyReceived`, etc.

### encryption_demo.rs

Demonstrates mesh-wide encryption using ChaCha20-Poly1305:

- Enabling encryption with a shared secret
- Encrypted document exchange between nodes
- Wrong key rejection
- Backward compatibility with unencrypted nodes
- Strict encryption mode

**Key concepts:**
- `PeatMeshConfig::with_encryption()` - Enable mesh-wide encryption
- `with_strict_encryption()` - Reject unencrypted documents
- `SecurityViolation` events for security issues

### peer_e2ee.rs

Demonstrates per-peer end-to-end encryption (E2EE):

- Enabling E2EE with `enable_peer_e2ee()`
- X25519 key exchange handshake
- Encrypted message sending and receiving
- Session management

**Key concepts:**
- `PeerSessionManager` - Low-level E2EE session management
- `KeyExchangeMessage` - X25519 key exchange
- `PeerEncryptedMessage` - Encrypted message format

### linux_scanner.rs

Demonstrates BLE scanning on Linux using BlueZ:

- Initializing the BlueZ adapter
- Scanning for Peat devices
- Integrating with `PeatMesh` for state management

**Requirements:**
- Linux OS
- BlueZ bluetooth stack
- `linux` feature enabled
- May require root or bluetooth group membership

## Common Patterns

### Creating a Mesh Node

```rust
use peat_btle::{PeatMesh, PeatMeshConfig, NodeId};

let config = PeatMeshConfig::new(
    NodeId::new(0x12345678),  // Unique node ID
    "ALPHA-1",                 // Callsign
    "DEMO",                    // Mesh ID
);
let mesh = PeatMesh::new(config);
```

### Adding an Observer

```rust
use peat_btle::observer::{PeatEvent, PeatObserver};
use std::sync::Arc;

struct MyObserver;

impl PeatObserver for MyObserver {
    fn on_event(&self, event: PeatEvent) {
        match event {
            PeatEvent::EmergencyReceived { from_node } => {
                println!("EMERGENCY from {:08X}", from_node.as_u32());
            }
            _ => {}
        }
    }
}

mesh.add_observer(Arc::new(MyObserver));
```

### Handling BLE Callbacks

```rust
// When a device is discovered
mesh.on_ble_discovered(
    "device-uuid",           // Platform identifier
    Some("PEAT_DEMO-AABB"),  // Device name
    -65,                     // RSSI in dBm
    Some("DEMO"),            // Mesh ID from advertisement
    timestamp_ms,
);

// When connected
mesh.on_ble_connected("device-uuid", timestamp_ms);

// When data received
if let Some(result) = mesh.on_ble_data("device-uuid", &data, timestamp_ms) {
    if result.is_emergency {
        // Handle emergency
    }
}
```

### Periodic Maintenance

```rust
// Call tick() regularly (e.g., every second)
if let Some(sync_doc) = mesh.tick(timestamp_ms) {
    // Broadcast sync_doc to connected peers via BLE
}
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Your Application                        │
├─────────────────────────────────────────────────────────────┤
│                        PeatMesh                             │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐ │
│  │  PeerManager    │  │  DocumentSync   │  │  Security   │ │
│  │  (connections)  │  │  (CRDT state)   │  │  (E2EE)     │ │
│  └─────────────────┘  └─────────────────┘  └─────────────┘ │
├─────────────────────────────────────────────────────────────┤
│                    Platform Adapter                         │
│        (BluerAdapter, WinRtAdapter, etc.)                   │
├─────────────────────────────────────────────────────────────┤
│                    OS Bluetooth Stack                       │
│           (BlueZ, WinRT, CoreBluetooth)                     │
└─────────────────────────────────────────────────────────────┘
```

## Feature Flags

- `std` (default) - Standard library support
- `linux` - Linux BlueZ support
- `android` - Android JNI support
- `macos` / `ios` - Apple CoreBluetooth support
- `windows` - Windows WinRT support
- `esp32` - ESP32 NimBLE support
