# Security Integration Guide

> For Developers: APIs, code examples, and integration patterns

## Quick Start

### Minimal Secure Mesh

```rust
use eche_btle::{EcheMesh, EcheMeshConfig, NodeId};

// 1. Generate or load shared secret (32 bytes)
let secret: [u8; 32] = load_from_secure_storage();

// 2. Create mesh with encryption
let config = EcheMeshConfig::new(
    NodeId::new(0x12345678),  // Unique node ID
    "ALPHA-1",                 // Human-readable callsign
    "DEMO",                    // Mesh identifier
).with_encryption(secret);

let mesh = EcheMesh::new(config);

// 3. All documents are now encrypted automatically
let encrypted_doc = mesh.build_document();  // Ready for transmission
```

## Mesh Configuration

### EcheMeshConfig

```rust
pub struct EcheMeshConfig {
    pub node_id: NodeId,              // 32-bit unique identifier
    pub callsign: String,             // Display name (e.g., "ALPHA-1")
    pub mesh_id: String,              // Network identifier (e.g., "DEMO")
    pub peripheral_type: PeripheralType,
    pub peer_config: PeerManagerConfig,
    pub sync_interval_ms: u64,        // How often to broadcast (default: 5000)
    pub auto_broadcast_events: bool,  // Auto-send on emergency/ack
    pub encryption_secret: Option<[u8; 32]>,  // Mesh-wide encryption key
}
```

### Configuration Builder Pattern

```rust
let config = EcheMeshConfig::new(node_id, "ALPHA-1", "DEMO")
    .with_encryption(secret)                    // Enable encryption
    .with_peripheral_type(PeripheralType::SoldierSensor)
    .with_sync_interval(10_000)                 // 10 second sync
    .with_peer_timeout(60_000)                  // 60 second peer timeout
    .with_max_peers(16);                        // Limit peer tracking
```

## Phase 1: Mesh-Wide Encryption

### Enabling Encryption

**Option A: At Construction**
```rust
let config = EcheMeshConfig::new(node_id, callsign, mesh_id)
    .with_encryption(secret);
let mesh = EcheMesh::new(config);
```

**Option B: After Construction**
```rust
let mut mesh = EcheMesh::new(config);
mesh.enable_encryption(&secret);

// Later, if needed:
mesh.disable_encryption();
```

### Checking Encryption Status

```rust
if mesh.is_encryption_enabled() {
    println!("Documents will be encrypted");
} else {
    println!("Warning: Documents sent in cleartext");
}

// Check strict mode
if mesh.is_strict_encryption_enabled() {
    println!("Strict mode: unencrypted documents will be rejected");
}
```

### Enabling Strict Encryption Mode

Strict mode rejects unencrypted documents when encryption is enabled, preventing downgrade attacks:

```rust
let config = EcheMeshConfig::new(node_id, callsign, mesh_id)
    .with_encryption(secret)
    .with_strict_encryption();  // Reject unencrypted docs

let mesh = EcheMesh::new(config);
```

**When to use strict mode:**
- Production deployments where all nodes are encrypted
- After verifying all mesh participants have encryption enabled
- When downgrade attack prevention is required

**When NOT to use strict mode:**
- During gradual rollout (some nodes not yet encrypted)
- Development/testing with mixed encryption states
- Backward compatibility with legacy unencrypted nodes

### How Documents Are Encrypted

Encryption is transparent—just use the normal APIs:

```rust
// These are automatically encrypted if encryption is enabled:
let doc = mesh.build_document();           // Sync document
let doc = mesh.send_emergency(timestamp);  // Emergency event
let doc = mesh.send_ack(timestamp);        // ACK response

// Receiving side decrypts automatically:
mesh.on_ble_data_received(identifier, &encrypted_data, now_ms);
```

### Wire Format

```
Unencrypted:  [Document bytes...]
Encrypted:    [0xAE][0x00][nonce: 12 bytes][ciphertext + tag]
```

### Overhead Calculation

```rust
const ENCRYPTION_OVERHEAD: usize = 30;  // 2 + 12 + 16 bytes

// Example: 100-byte document
// Unencrypted: 100 bytes
// Encrypted:   130 bytes (well under 244-byte BLE MTU)
```

## Phase 2: Per-Peer E2EE

Per-peer E2EE provides point-to-point encryption where **only sender and recipient can decrypt**, even other mesh members cannot read the message.

### Enabling E2EE

```rust
// Each node must enable E2EE independently
mesh.enable_peer_e2ee();

// Check status
assert!(mesh.is_peer_e2ee_enabled());

// Get our public key (for out-of-band exchange if needed)
let our_pubkey: [u8; 32] = mesh.peer_e2ee_public_key().unwrap();
```

### Session Establishment

```rust
// Node A initiates session to Node B
let peer_node_id = NodeId::new(0x22222222);
let key_exchange_msg = mesh_a.initiate_peer_e2ee(peer_node_id, now_ms)
    .expect("E2EE not enabled");

// Send key_exchange_msg to Node B over BLE...
// (use normal mesh.on_ble_data_received() - handles automatically)

// Node B receives and auto-responds via the key exchange handler
// Session establishes automatically when both sides complete handshake
```

### Complete Handshake Example

```rust
// === Setup ===
let mesh_a = EcheMesh::new(config_a);
let mesh_b = EcheMesh::new(config_b);

mesh_a.enable_peer_e2ee();
mesh_b.enable_peer_e2ee();

// === Node A initiates ===
let ke1 = mesh_a.initiate_peer_e2ee(node_b_id, now_ms).unwrap();

// === Node B receives, responds ===
// In practice, this happens via on_ble_data_received()
// which calls handle_key_exchange() internally
let ke2 = mesh_b.handle_key_exchange(&ke1, now_ms)
    .expect("Invalid key exchange")
    .0;  // Response message

// === Node A completes handshake ===
mesh_a.handle_key_exchange(&ke2, now_ms);

// === Verify session established ===
assert!(mesh_a.has_peer_e2ee_session(node_b_id));
assert!(mesh_b.has_peer_e2ee_session(node_a_id));
```

### Sending Encrypted Messages

```rust
// Check session exists first
if mesh.has_peer_e2ee_session(peer_node_id) {
    let plaintext = b"Sensitive command data";

    let encrypted = mesh.send_peer_e2ee(peer_node_id, plaintext, now_ms)
        .expect("Encryption failed");

    // Send `encrypted` bytes to peer over BLE
    // Peer decrypts via on_ble_data_received() automatically
}
```

### Receiving Encrypted Messages

Decryption is automatic when using standard receive methods:

```rust
// Handles key exchange AND encrypted messages automatically
let result = mesh.on_ble_data_received(identifier, &data, now_ms);

// For E2EE messages, observers are notified:
impl EcheObserver for MyObserver {
    fn on_event(&self, event: EcheEvent) {
        match event {
            EcheEvent::PeerE2eeEstablished { peer_node_id } => {
                println!("E2EE session ready with {:08X}", peer_node_id.as_u32());
            }
            EcheEvent::PeerE2eeMessageReceived { from_node, data } => {
                println!("Got E2EE message from {:08X}: {:?}",
                    from_node.as_u32(), data);
            }
            EcheEvent::PeerE2eeClosed { peer_node_id } => {
                println!("E2EE session closed with {:08X}", peer_node_id.as_u32());
            }
            _ => {}
        }
    }
}
```

### Session Management

```rust
// Check session state
let state = mesh.peer_e2ee_session_state(peer_node_id);
match state {
    Some(SessionState::AwaitingPeerKey) => println!("Handshake in progress"),
    Some(SessionState::Established) => println!("Ready for encrypted comms"),
    Some(SessionState::Closed) => println!("Session ended"),
    None => println!("No session"),
}

// Get session counts
let total = mesh.peer_e2ee_session_count();
let ready = mesh.peer_e2ee_established_count();
println!("{}/{} sessions established", ready, total);

// Close a session
mesh.close_peer_e2ee(peer_node_id);

// Disable E2EE entirely (clears all sessions)
mesh.disable_peer_e2ee();
```

## Peer Management

### Configuration

```rust
pub struct PeerManagerConfig {
    pub peer_timeout_ms: u64,      // Stale peer removal (default: 45000)
    pub cleanup_interval_ms: u64,  // Cleanup frequency (default: 10000)
    pub sync_interval_ms: u64,     // Sync broadcast interval (default: 5000)
    pub sync_cooldown_ms: u64,     // Min time between syncs to same peer (default: 30000)
    pub auto_connect: bool,        // Auto-connect on discovery (default: true)
    pub mesh_id: String,           // Filter peers by mesh
    pub max_peers: usize,          // Max tracked peers (default: 8)
}
```

### Peer Lifecycle

```rust
// Discovery (called by platform BLE)
let peer = mesh.on_ble_discovered(
    "device-uuid",              // Platform identifier
    Some("HIVE_DEMO-22222222"), // Device name
    -65,                        // RSSI
    Some("DEMO"),               // Mesh ID (from name parsing)
    now_ms,
);

// Connection
let node_id = mesh.on_ble_connected("device-uuid", now_ms);

// Data received (auto-decrypts if encrypted)
let result = mesh.on_ble_data_received("device-uuid", &data, now_ms);

// Disconnection
mesh.on_ble_disconnected("device-uuid", DisconnectReason::RemoteRequest);
```

### Querying Peers

```rust
// All known peers
let all_peers: Vec<EchePeer> = mesh.get_peers();

// Connected peers only
let connected: Vec<EchePeer> = mesh.get_connected_peers();

// Specific peer
if let Some(peer) = mesh.get_peer(node_id) {
    println!("Peer {} RSSI: {}", peer.display_name(), peer.rssi);
    println!("Signal: {:?}", peer.signal_strength());
    println!("Connected: {}", peer.is_connected);
}

// Counts
println!("Discovered: {}, Connected: {}",
    mesh.peer_count(),
    mesh.connected_count());
```

### Mesh Filtering

```rust
// Check if a device belongs to our mesh
if mesh.matches_mesh(device_mesh_id) {
    // Same mesh, proceed with connection
} else {
    // Different mesh, ignore
}

// Device name format: "HIVE_<mesh_id>-<node_id>"
// Example: "HIVE_DEMO-12345678"
// Parsed as: mesh_id="DEMO", node_id=0x12345678
```

## Observer Pattern

### Subscribing to Events

```rust
use std::sync::Arc;

struct MyObserver;

impl EcheObserver for MyObserver {
    fn on_event(&self, event: EcheEvent) {
        match event {
            // Peer events
            EcheEvent::PeerDiscovered { peer } => {
                println!("Found: {} (RSSI: {})", peer.display_name(), peer.rssi);
            }
            EcheEvent::PeerConnected { node_id } => {
                println!("Connected: {:08X}", node_id.as_u32());
            }
            EcheEvent::PeerDisconnected { node_id, reason } => {
                println!("Disconnected: {:08X} ({:?})", node_id.as_u32(), reason);
            }
            EcheEvent::PeerLost { node_id } => {
                println!("Lost (stale): {:08X}", node_id.as_u32());
            }

            // Mesh events
            EcheEvent::MeshStateChanged { peer_count, connected_count } => {
                println!("Mesh: {}/{} connected", connected_count, peer_count);
            }

            // Sync events
            EcheEvent::DocumentSynced { from_node, total_count } => {
                println!("Synced from {:08X}, count={}", from_node.as_u32(), total_count);
            }
            EcheEvent::EmergencyReceived { from_node } => {
                println!("EMERGENCY from {:08X}!", from_node.as_u32());
            }
            EcheEvent::AckReceived { from_node } => {
                println!("ACK from {:08X}", from_node.as_u32());
            }

            // E2EE events
            EcheEvent::PeerE2eeEstablished { peer_node_id } => {
                println!("E2EE ready: {:08X}", peer_node_id.as_u32());
            }
            EcheEvent::PeerE2eeMessageReceived { from_node, data } => {
                println!("E2EE msg from {:08X}: {} bytes",
                    from_node.as_u32(), data.len());
            }
            EcheEvent::PeerE2eeClosed { peer_node_id } => {
                println!("E2EE closed: {:08X}", peer_node_id.as_u32());
            }

            // Security events
            EcheEvent::SecurityViolation { kind, source } => {
                log::warn!("Security violation: {:?} from {:?}", kind, source);
                // Handle violations:
                // - UnencryptedInStrictMode: downgrade attack attempt
                // - DecryptionFailed: wrong key or corrupted data
                // - ReplayDetected: duplicate message counter
                // - UnauthorizedNode: unknown node attempted access
            }
        }
    }
}

// Register observer
let observer = Arc::new(MyObserver);
mesh.add_observer(observer.clone());

// Unregister when done
mesh.remove_observer(&observer);
```

## Error Handling

### Encryption Errors

```rust
use eche_btle::security::EncryptionError;

match mesh_encryption_key.decrypt(&encrypted_doc) {
    Ok(plaintext) => { /* process */ }
    Err(EncryptionError::DecryptionFailed) => {
        // Wrong key or corrupted data
        log::warn!("Decryption failed - possible key mismatch");
    }
    Err(EncryptionError::InvalidFormat) => {
        // Not valid encrypted format
        log::warn!("Invalid encrypted document format");
    }
    Err(EncryptionError::EncryptionFailed) => {
        // Should not happen in practice
        log::error!("Encryption operation failed");
    }
}
```

### Handling Mismatched Keys

```rust
// When an encrypted mesh receives data from wrong key:
let result = mesh.on_ble_data_received(id, &data, now_ms);
if result.is_none() && data[0] == ENCRYPTED_MARKER {
    // Document was encrypted but we couldn't decrypt
    // Either: wrong shared secret, or corrupted data
    log::warn!("Could not decrypt document from {}", id);
}
```

## Platform Integration

### iOS (Swift via UniFFI)

```swift
import EcheBTLE

// Create mesh
let config = EcheMeshConfig(
    nodeId: NodeId(value: 0x12345678),
    callsign: "ALPHA-1",
    meshId: "DEMO"
)
config.setEncryption(secret: secretData)

let mesh = EcheMesh(config: config)

// CoreBluetooth callbacks → EcheMesh
func peripheral(_ peripheral: CBPeripheral,
                didUpdateValueFor characteristic: CBCharacteristic,
                error: Error?) {
    guard let data = characteristic.value else { return }

    let nowMs = UInt64(Date().timeIntervalSince1970 * 1000)
    mesh.onBleDataReceived(
        identifier: peripheral.identifier.uuidString,
        data: data,
        nowMs: nowMs
    )
}
```

### Android (Kotlin via JNI)

```kotlin
import com.hive.btle.EcheMesh
import com.hive.btle.EcheMeshConfig

// Create mesh
val config = EcheMeshConfig(
    nodeId = NodeId(0x12345678),
    callsign = "ALPHA-1",
    meshId = "DEMO"
).apply {
    setEncryption(secret)
}

val mesh = EcheMesh(config)

// BluetoothGattCallback → EcheMesh
override fun onCharacteristicChanged(
    gatt: BluetoothGatt,
    characteristic: BluetoothGattCharacteristic
) {
    val data = characteristic.value
    val nowMs = System.currentTimeMillis()

    mesh.onBleDataReceived(
        identifier = gatt.device.address,
        data = data,
        nowMs = nowMs
    )
}
```

### ESP32 (C via FFI)

```c
#include "eche_btle.h"

// Create mesh
hive_mesh_config_t config = {
    .node_id = 0x12345678,
    .callsign = "ALPHA-1",
    .mesh_id = "DEMO",
};
memcpy(config.encryption_secret, secret, 32);
config.encryption_enabled = true;

hive_mesh_t* mesh = hive_mesh_new(&config);

// NimBLE callback → EcheMesh
static int gatt_write_cb(uint16_t conn_handle, ...) {
    uint64_t now_ms = esp_timer_get_time() / 1000;
    hive_mesh_on_ble_data(mesh, identifier, data, len, now_ms);
    return 0;
}
```

## Best Practices

### Secret Management

```rust
// DO: Generate cryptographically secure secret
use rand::RngCore;
let mut secret = [0u8; 32];
rand::rngs::OsRng.fill_bytes(&mut secret);

// DO: Store in secure enclave/keystore
#[cfg(target_os = "ios")]
let secret = keychain::get("mesh_secret")?;

#[cfg(target_os = "android")]
let secret = android_keystore::get("mesh_secret")?;

// DON'T: Hardcode secrets
let secret = [0x42u8; 32];  // INSECURE!

// DON'T: Log secrets
log::info!("Using secret: {:?}", secret);  // NEVER DO THIS
```

### Graceful Degradation

```rust
// Handle mixed encrypted/unencrypted mesh during rollout
let config = if secure_mode_required {
    EcheMeshConfig::new(node_id, callsign, mesh_id)
        .with_encryption(secret)
} else {
    // Development mode - no encryption
    EcheMeshConfig::new(node_id, callsign, mesh_id)
};
```

### Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_mesh(node_id: u32, secret: Option<[u8; 32]>) -> EcheMesh {
        let mut config = EcheMeshConfig::new(
            NodeId::new(node_id),
            &format!("TEST-{}", node_id),
            "TEST"
        );
        if let Some(s) = secret {
            config = config.with_encryption(s);
        }
        EcheMesh::new(config)
    }

    #[test]
    fn test_encrypted_exchange() {
        let secret = [0x42u8; 32];
        let mesh1 = create_test_mesh(0x11111111, Some(secret));
        let mesh2 = create_test_mesh(0x22222222, Some(secret));

        // mesh1 sends
        let doc = mesh1.build_document();
        assert!(doc[0] == ENCRYPTED_MARKER);

        // mesh2 receives
        let result = mesh2.on_ble_data_received_from_node(
            NodeId::new(0x11111111),
            &doc,
            1000
        );
        assert!(result.is_some());
    }

    #[test]
    fn test_wrong_key_rejected() {
        let mesh1 = create_test_mesh(0x11111111, Some([0x42u8; 32]));
        let mesh2 = create_test_mesh(0x22222222, Some([0x43u8; 32])); // Different!

        let doc = mesh1.build_document();
        let result = mesh2.on_ble_data_received_from_node(
            NodeId::new(0x11111111),
            &doc,
            1000
        );
        assert!(result.is_none()); // Decryption failed
    }
}
```

## Troubleshooting

### "Decryption failed" on all messages

1. Verify both nodes have identical 32-byte secret
2. Check mesh_id matches (key is derived from mesh_id + secret)
3. Ensure encryption enabled on both ends

### E2EE session not establishing

1. Verify `enable_peer_e2ee()` called on both nodes
2. Check max_sessions limit not reached
3. Verify key exchange messages being delivered

### Peers not discovered

1. Verify mesh_id matches in device name
2. Check BLE advertising is active
3. Verify `matches_mesh()` returns true

### High latency / dropped messages

1. Check encryption overhead fits in MTU (< 244 bytes typical)
2. Reduce document size if near MTU limit
3. Consider compression for large payloads
