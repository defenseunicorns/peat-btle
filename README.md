# peat-btle

Bluetooth Low Energy mesh transport for tactical edge networking.

[![Crate](https://img.shields.io/crates/v/peat-btle.svg)](https://crates.io/crates/peat-btle)
[![Documentation](https://docs.rs/peat-btle/badge.svg)](https://docs.rs/peat-btle)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

## Overview

`peat-btle` provides a cross-platform Bluetooth Low Energy mesh networking stack optimized for resource-constrained tactical devices. It enables peer-to-peer discovery, advertisement, connectivity, and efficient CRDT-based data synchronization over BLE.

### Key Features

- **Cross-Platform**: Linux, Android, iOS, macOS, Windows, ESP32
- **Power Efficient**: Designed for 18+ hour battery life on smartwatches
- **Long Range**: Coded PHY support for 300m+ range (BLE 5.0+)
- **Mesh Topology**: Hierarchical mesh with automatic peer discovery
- **Efficient Sync**: Delta-based CRDT synchronization over GATT
- **Embedded Ready**: `no_std` support for bare-metal targets

### Why peat-btle?

Traditional BLE mesh implementations (like those in commercial sync SDKs) often suffer from:

| Problem | Impact | peat-btle Solution |
|---------|--------|-------------------|
| Continuous scanning | 20%+ radio duty cycle | Batched sync windows (<5%) |
| Gossip-based discovery | All devices advertise constantly | Hierarchical discovery (leaf nodes don't scan) |
| Full mesh participation | Every device relays everything | Lite profile (minimal state, single parent) |
| **Result** | **3-4 hour watch battery** | **18-24 hour battery life** |

## Status

> **Pre-release**: This crate is under active development. APIs may change.

| Platform | Status | Notes |
|----------|--------|-------|
| Linux (BlueZ) | ✅ Complete | BlueZ 5.48+ required |
| macOS | ✅ Complete | CoreBluetooth, tested with ESP32 devices |
| iOS | ✅ Complete | CoreBluetooth (shared with macOS) |
| ESP32 | ✅ Complete | ESP-IDF NimBLE integration |
| Android | 🔄 In Progress | JNI bindings to Android Bluetooth API |
| Windows | 📋 Planned | WinRT Bluetooth APIs |

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
peat-btle = { version = "0.1", features = ["linux"] }
```

### Feature Flags

| Feature | Description |
|---------|-------------|
| `std` (default) | Standard library support |
| `linux` | Linux/BlueZ support via `bluer` |
| `android` | Android support via JNI |
| `ios` | iOS support via CoreBluetooth |
| `macos` | macOS support via CoreBluetooth |
| `windows` | Windows support via WinRT |
| `embedded` | Embedded/no_std support |
| `esp32` | ESP32 support via ESP-IDF |
| `coded-phy` | Enable Coded PHY for extended range |
| `extended-adv` | Enable extended advertising |

## Quick Start

```rust
use peat_btle::{BleConfig, BluetoothLETransport, NodeId, PowerProfile};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create power-efficient configuration
    let config = BleConfig::hive_lite(NodeId::new(0x12345678))
        .with_power_profile(PowerProfile::LowPower);

    // Create platform adapter (Linux example)
    #[cfg(feature = "linux")]
    let adapter = peat_btle::platform::linux::BluerAdapter::new().await?;

    // Create and start transport
    let transport = BluetoothLETransport::new(config, adapter);
    transport.start().await?;

    // Transport is now advertising and ready for connections
    println!("Node {} is running", transport.node_id());

    // Connect to a discovered peer
    // let conn = transport.connect(&peer_id).await?;

    Ok(())
}
```

## Architecture

### Standalone vs HIVE Integration

peat-btle is designed for **dual use**:

1. **Standalone**: Pure embedded mesh (ESP32/Pico devices) without any full HIVE nodes
2. **HIVE Integration**: BLE transport for full HIVE nodes, with gateway translation to Automerge

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         HIVE Integration Mode                            │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                    Full HIVE Node (Phone)                          │  │
│  │  ┌─────────────────────────────────────────────────────────────┐  │  │
│  │  │                   AutomergeIroh                              │  │  │
│  │  │             (Full CRDT documents)                            │  │  │
│  │  └─────────────────────────────────────────────────────────────┘  │  │
│  │                            ↕                                       │  │
│  │  ┌─────────────────────────────────────────────────────────────┐  │  │
│  │  │              Translation Layer (HIVE repo)                   │  │  │
│  │  │        Maps: Automerge ↔ peat-btle lightweight               │  │  │
│  │  └─────────────────────────────────────────────────────────────┘  │  │
│  │                            ↕                                       │  │
│  │  ┌─────────────────────────────────────────────────────────────┐  │  │
│  │  │                      peat-btle                               │  │  │
│  │  │            (BLE transport + lightweight CRDTs)               │  │  │
│  │  └─────────────────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                ↕ BLE                                     │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │               Embedded Node (ESP32/Pico)                           │  │
│  │                        peat-btle                                   │  │
│  │              (standalone, lightweight CRDTs)                       │  │
│  └───────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

**Why two modes?** Automerge is too resource-intensive for embedded targets (requires ~10MB+ RAM, `std` library). peat-btle's lightweight CRDTs (GCounter, Peripheral) provide the same semantics in <256KB RAM. Full HIVE nodes translate between formats.

### Component Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Application                             │
├─────────────────────────────────────────────────────────────┤
│                  BluetoothLETransport                        │
│         (MeshTransport trait implementation)                 │
├──────────────┬──────────────┬──────────────┬────────────────┤
│   Discovery  │     GATT     │     Mesh     │     Power      │
│  (Beacon,    │  (Service,   │  (Topology,  │  (Scheduler,   │
│   Scanner)   │   Protocol)  │   Routing)   │   Profiles)    │
├──────────────┴──────────────┴──────────────┴────────────────┤
│                     BleAdapter Trait                         │
├──────────┬──────────┬──────────┬──────────┬─────────────────┤
│  Linux   │ Android  │   iOS    │ Windows  │     ESP32       │
│ (BlueZ)  │  (JNI)   │(CoreBT)  │ (WinRT)  │   (NimBLE)      │
└──────────┴──────────┴──────────┴──────────┴─────────────────┘
```

### Core Components

| Component | Description |
|-----------|-------------|
| **Discovery** | Peat beacon format, scanning, and advertising |
| **GATT** | BLE service definition and sync protocol |
| **Mesh** | Topology management and message routing |
| **PHY** | Physical layer configuration (1M, 2M, Coded) |
| **Power** | Radio scheduling and battery optimization |
| **Sync** | Delta-based CRDT synchronization |

## Power Profiles

| Profile | Radio Duty | Sync Interval | Watch Battery* |
|---------|------------|---------------|----------------|
| Aggressive | 20% | 1 second | ~6 hours |
| Balanced | 10% | 5 seconds | ~12 hours |
| **LowPower** | **2%** | **30 seconds** | **~20 hours** |
| UltraLow | 0.5% | 2 minutes | ~36 hours |

*Estimated for typical smartwatch (300mAh battery)

## GATT Service

peat-btle defines a custom GATT service for mesh communication:

| UUID | Characteristic | Description |
|------|----------------|-------------|
| `0xF47A` | Service | Peat BLE Service (16-bit short form) |
| `0x0001` | Node Info | Node ID, capabilities, hierarchy level |
| `0x0002` | Sync State | Vector clock and sync metadata |
| `0x0003` | Sync Data | CRDT delta payloads (chunked) |
| `0x0004` | Command | Control commands (connect, disconnect, etc.) |
| `0x0005` | Status | Connection status and errors |

## Mesh-Wide Encryption

peat-btle supports optional mesh-wide encryption using ChaCha20-Poly1305 AEAD. When enabled, all documents are encrypted before transmission, providing confidentiality across multi-hop BLE relay.

### Enabling Encryption

```rust
use peat_btle::{PeatMesh, PeatMeshConfig, NodeId};

// Create a 32-byte shared secret (distribute securely to all mesh nodes)
let secret: [u8; 32] = [0x42; 32]; // Use a real secret in production!

// Configure mesh with encryption
let config = PeatMeshConfig::new(NodeId::new(0x12345678), "ALPHA-1", "DEMO")
    .with_encryption(secret);

let mesh = PeatMesh::new(config);

// All documents sent through this mesh are now encrypted
let encrypted_doc = mesh.build_document();
```

### How It Works

- **Algorithm**: ChaCha20-Poly1305 authenticated encryption
- **Key Derivation**: HKDF-SHA256 from shared secret + mesh ID
- **Overhead**: 30 bytes per document (2 marker + 12 nonce + 16 auth tag)
- **Wire Format**: `ENCRYPTED_MARKER (0xAE) | reserved | nonce | ciphertext`

### Multi-Hop Security

```
Node A ──encrypted──> Node B (relay) ──encrypted──> Node C
                           │
                    Can forward but
                    only decrypts if
                    has formation key
```

Nodes without the shared secret can relay encrypted documents but cannot read their contents. This provides end-to-end confidentiality even across untrusted relay nodes.

### Backward Compatibility

- Encrypted nodes can receive unencrypted documents (for gradual rollout)
- Unencrypted nodes will reject encrypted documents they can't decrypt

## Per-Peer E2EE (End-to-End Encryption)

For sensitive communications, peat-btle supports per-peer end-to-end encryption using X25519 key exchange and ChaCha20-Poly1305. Unlike mesh-wide encryption where all formation members share a key, per-peer E2EE ensures only the sender and recipient can decrypt messages—even other mesh members cannot read them.

### Enabling Per-Peer E2EE

```rust
use peat_btle::{PeatMesh, PeatMeshConfig, NodeId};

// Create mesh instances
let config1 = PeatMeshConfig::new(NodeId::new(0x11111111), "ALPHA-1", "DEMO");
let mesh1 = PeatMesh::new(config1);

let config2 = PeatMeshConfig::new(NodeId::new(0x22222222), "BRAVO-1", "DEMO");
let mesh2 = PeatMesh::new(config2);

// Enable E2EE on both nodes (generates identity keys)
mesh1.enable_peer_e2ee();
mesh2.enable_peer_e2ee();

// Initiate E2EE session from mesh1 to mesh2
let key_exchange1 = mesh1.initiate_peer_e2ee(NodeId::new(0x22222222), now_ms).unwrap();
// Send key_exchange1 to mesh2 over BLE...

// mesh2 handles incoming key exchange and responds
let key_exchange2 = mesh2.handle_key_exchange(&key_exchange1, now_ms).unwrap();
// Send key_exchange2 back to mesh1...

// mesh1 completes the handshake
mesh1.handle_key_exchange(&key_exchange2, now_ms);

// Session established! Now send encrypted messages
let encrypted = mesh1.send_peer_e2ee(NodeId::new(0x22222222), b"Secret message", now_ms).unwrap();
// Send encrypted to mesh2...

// mesh2 decrypts
let plaintext = mesh2.handle_peer_e2ee_message(&encrypted, now_ms).unwrap();
assert_eq!(plaintext, b"Secret message");
```

### How It Works

1. **Key Exchange**: X25519 Diffie-Hellman to establish a shared secret
2. **Key Derivation**: HKDF-SHA256 binds the session key to both node IDs
3. **Encryption**: ChaCha20-Poly1305 AEAD with replay protection
4. **Wire Format**: `PEER_E2EE_MARKER (0xAF) | reserved | recipient | sender | counter | nonce | ciphertext`

### Security Properties

| Property | Description |
|----------|-------------|
| **Forward Secrecy** | Ephemeral keys can be used for session establishment |
| **Replay Protection** | Monotonic counters prevent message replay attacks |
| **Authentication** | Poly1305 MAC ensures message integrity |
| **Confidentiality** | Only sender and recipient can decrypt |

### Overhead

Per-peer E2EE adds 46 bytes overhead per message:
- 2 bytes: Marker header
- 4 bytes: Recipient node ID
- 4 bytes: Sender node ID
- 8 bytes: Counter (replay protection)
- 12 bytes: Nonce
- 16 bytes: Authentication tag

### Use Cases

- **Sensitive Commands**: Squad leader → specific soldier
- **Private Messages**: Point-to-point communication within formation
- **Credential Exchange**: Secure key material distribution

### Encryption Layers

```
┌─────────────────────────────────────────────────────────────────┐
│  Phase 1: Mesh-Wide (Formation Key)                             │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  All formation members can decrypt                       │    │
│  │  Protects: External eavesdroppers                        │    │
│  │  Overhead: 30 bytes                                      │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│  Phase 2: Per-Peer E2EE (Session Key)                           │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  Only sender + recipient can decrypt                     │    │
│  │  Protects: Other mesh members, compromised relays        │    │
│  │  Overhead: 46 bytes                                      │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

## Device Identity

peat-btle provides cryptographic device identity using Ed25519 keypairs. Each device generates a persistent identity that binds its node ID to a public key, preventing impersonation attacks.

### Creating a Device Identity

```rust
use peat_btle::security::{DeviceIdentity, IdentityAttestation};

// Generate a new device identity
let identity = DeviceIdentity::generate();

// The node_id is derived from the public key (collision-resistant)
let node_id = identity.node_id();
let public_key = identity.public_key();

// Create a signed attestation proving ownership of this identity
let attestation = identity.create_attestation();

// Verify an attestation from another device
if attestation.verify() {
    println!("Identity verified for node {}", attestation.node_id);
}
```

### Identity Attestation

When nodes communicate, they exchange signed attestations proving they control their claimed node ID:

| Field | Size | Description |
|-------|------|-------------|
| `node_id` | 4 bytes | Claimed node identifier |
| `public_key` | 32 bytes | Ed25519 public key |
| `timestamp_ms` | 8 bytes | Attestation creation time |
| `signature` | 64 bytes | Ed25519 signature over all fields |

## Mesh Genesis

New meshes are created through a genesis protocol that establishes cryptographic roots:

```rust
use peat_btle::security::{DeviceIdentity, MeshGenesis, MembershipPolicy, MeshCredentials};

// Create founder's identity
let founder = DeviceIdentity::generate();

// Create a new mesh
let genesis = MeshGenesis::create("ALPHA-TEAM", &founder, MembershipPolicy::Controlled);

// Get derived values
let mesh_id = genesis.mesh_id();           // e.g., "A1B2C3D4"
let secret = genesis.encryption_secret();   // 32-byte derived key
let beacon_key = genesis.beacon_key_base(); // For encrypted beacons

// Create shareable credentials for other nodes
let credentials = MeshCredentials::from_genesis(&genesis);
```

### Membership Policies

| Policy | Description |
|--------|-------------|
| `Open` | Anyone with mesh_id can attempt to join (demos, open networks) |
| `Controlled` | Explicit enrollment by authority required (default) |
| `Strict` | Only pre-provisioned devices can join (high security) |

### Genesis Data

The genesis contains all cryptographic material to bootstrap a mesh:

- **mesh_seed**: 256-bit CSPRNG seed (root secret)
- **mesh_id**: 8 hex chars derived from name + seed
- **encryption_secret**: HKDF-derived key for mesh-wide encryption
- **beacon_key_base**: HKDF-derived key for encrypted advertisements
- **creator_identity**: Founder's DeviceIdentity (initial authority)

### BLE Pairing Attack Resilience

**Threat**: Attacks like WhisperPair (CVE-2024-XXXXX) can downgrade BLE pairing
security by manipulating the key exchange timing, resulting in weaker session keys.

**PEAT-BTLE Mitigation**: BLE link security is **not** the trust boundary.

1. **Discovery-only dependency**: Peat uses BLE for proximity detection and
   initial rendezvous. Security-critical operations require application-layer
   authentication per ADR-006.

2. **PKI verification**: Device identity is established via Ed25519 keypairs,
   verified at connection establishment before any CRDT sync occurs.

3. **Mesh-wide encryption**: ChaCha20-Poly1305 encrypts all sync payloads
   regardless of BLE security level.

4. **Defense in depth**: Even a fully compromised BLE link exposes only
   encrypted, authenticated traffic that cannot be injected into the Peat mesh.

**Recommendation**: For maximum security, enable mesh-wide encryption (default in
`MeshGenesis`) and consider per-peer E2EE for sensitive operations.

## Platform Requirements

### Linux

- BlueZ 5.48 or later
- D-Bus system bus access
- Bluetooth adapter with BLE support

```bash
# Check BlueZ version
bluetoothctl --version

# Ensure bluetooth service is running
sudo systemctl start bluetooth
```

### Android

- Android 6.0 (API 23) or later
- `BLUETOOTH`, `BLUETOOTH_ADMIN`, `ACCESS_FINE_LOCATION` permissions
- For BLE 5.0 features: Android 8.0 (API 26) or later

### iOS

- iOS 13.0 or later
- `NSBluetoothAlwaysUsageDescription` in Info.plist
- CoreBluetooth framework

## Examples

See the [`examples/`](examples/) directory:

- `linux_scanner.rs` - Scan for Peat nodes on Linux
- `linux_advertiser.rs` - Advertise as an Peat node
- `mesh_demo.rs` - Two-node mesh demonstration

Run examples with:

```bash
cargo run --example linux_scanner --features linux
```

## Testing

```bash
# Run unit tests (no hardware required)
cargo test

# Run with Linux platform tests (requires Bluetooth adapter)
cargo test --features linux

# Run specific test module
cargo test sync::
```

## Contributing

Contributions are welcome! Priority areas:

1. **Android Implementation** - JNI bindings to Android Bluetooth API
2. **Windows Implementation** - WinRT Bluetooth APIs
3. **Hardware Testing** - Real-world validation on various devices

Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.

## Acknowledgments

Developed by [(r)evolve - Revolve Team LLC](https://revolveteam.com) as part of the HIVE Protocol project.


