# UniFFI Migration Guide (v0.1.0-rc.28)

This document describes the migration from manual JNI bindings to UniFFI for Android (and iOS) integration.

## Overview

Starting with rc.28, peat-btle uses [UniFFI](https://mozilla.github.io/uniffi-rs/) for cross-language bindings instead of manual JNI. This provides:

- **Type safety**: Automatic type conversions between Rust and Kotlin/Swift
- **Reduced boilerplate**: No more manual JNI bridge code
- **Consistent API**: Same interface across Android and iOS
- **Easier maintenance**: Changes in Rust automatically reflected in bindings

## Architecture

```
┌─────────────────────────────────────────┐
│     Kotlin PeatBtle (Android BLE)       │
│   - BLE scanning and advertising        │
│   - GATT client/server operations       │
│   - Android permission management       │
├─────────────────────────────────────────┤
│   UniFFI Bindings (uniffi.peat_btle)    │
│   - Auto-generated Kotlin/Swift code    │
│   - Type-safe FFI layer                 │
├─────────────────────────────────────────┤
│          Rust PeatMesh Core             │
│   - Mesh state management               │
│   - CRDT document sync                  │
│   - Encryption/decryption               │
│   - Peer management                     │
└─────────────────────────────────────────┘
```

## Breaking Changes

### 1. Import Path Changed

**Before:**
```kotlin
import com.defenseunicorns.peat.PeatMesh
import com.defenseunicorns.peat.DeviceIdentity
import com.defenseunicorns.peat.MeshGenesis
```

**After:**
```kotlin
import uniffi.peat_btle.PeatMesh
import uniffi.peat_btle.DeviceIdentity
import uniffi.peat_btle.MeshGenesis
import uniffi.peat_btle.PeripheralType
import uniffi.peat_btle.EventType
import uniffi.peat_btle.DisconnectReason
```

### 2. PeatMesh Construction

**Before:**
```kotlin
// Direct constructor
val mesh = PeatMesh(
    nodeId = nodeId,
    callsign = "ANDROID",
    meshId = meshId,
    peripheralType = PeripheralType.SOLDIER_SENSOR
)

// From genesis
val mesh = PeatMesh.createFromGenesis(genesis, identity, "ANDROID")
```

**After:**
```kotlin
// Factory method with peripheral type
val mesh = PeatMesh.newWithPeripheral(
    nodeId.toUInt(),
    "ANDROID",
    meshId,
    PeripheralType.SOLDIER_SENSOR
)

// From genesis (note: argument order changed)
val mesh = PeatMesh.newFromGenesis("ANDROID", identity, genesis)
```

### 3. Unsigned Type Conversions

UniFFI uses Kotlin's unsigned types. You'll need conversions:

```kotlin
// Timestamps (Long → ULong)
mesh.onBleConnected(address, System.currentTimeMillis().toULong())

// Node IDs (Long → UInt)
mesh.sendChatReply(sender, text, replyToNode.toUInt(), ...)

// RSSI (Int → Byte)
mesh.onBleDiscovered(address, name, rssi.toByte(), meshId, now.toULong())

// Battery/HeartRate (Int → UByte)
mesh.updatePeripheralState(
    callsign,
    battery.toUByte(),
    heartRate?.toUByte(),
    ...
)
```

### 4. BLE Callback Signatures

All BLE callbacks now require explicit `nowMs` timestamp:

**Before:**
```kotlin
mesh.onBleDataReceived(address, data)
mesh.onBleDataReceivedAnonymous(address, data)
```

**After:**
```kotlin
mesh.onBleDataReceived(address, data, System.currentTimeMillis().toULong())
mesh.onBleDataReceivedAnonymous(address, data, System.currentTimeMillis().toULong())
```

### 5. Parameter Names

Some parameter names changed to match Rust conventions:

| Old | New |
|-----|-----|
| `deviceMeshId` | `meshId` |
| `timestamp` | `timestampMs` |

### 6. Return Types

`chatCount()` now returns `UInt`:

```kotlin
// Before
val count: Int = mesh.chatCount()

// After
val count: UInt = mesh.chatCount()
// Or convert: val count: Int = mesh.chatCount().toInt()
```

## New API Methods

### updatePeripheralState

Efficiently update all peripheral state before building encrypted documents:

```kotlin
mesh.updatePeripheralState(
    callsign = "ALPHA-1",
    batteryPercent = 85u,
    heartRate = 72u,
    latitude = 37.7749f,
    longitude = -122.4194f,
    altitude = 10.0f,
    eventType = EventType.MOVING,
    timestampMs = System.currentTimeMillis().toULong()
)
```

### Chat Methods

```kotlin
// Send chat message
val docBytes: ByteArray? = mesh.sendChat(sender, text, timestamp.toULong())

// Send reply
val docBytes: ByteArray? = mesh.sendChatReply(
    sender,
    text,
    replyToNode.toUInt(),
    replyToTimestamp.toULong(),
    timestamp.toULong()
)

// Get message count
val count: UInt = mesh.chatCount()

// Get all messages as JSON
val json: String = mesh.getAllChatMessages()

// Get messages since timestamp
val json: String = mesh.getChatMessagesSince(sinceTimestamp.toULong())
```

### deriveNodeIdFromMac

Standalone function to derive node ID from MAC address:

```kotlin
import uniffi.peat_btle.deriveNodeIdFromMac

val nodeId: UInt = deriveNodeIdFromMac("AA:BB:CC:DD:EE:FF")
```

## DeviceIdentity API

```kotlin
// Generate new identity
val identity = DeviceIdentity.generate()

// Restore from stored private key
val identity: DeviceIdentity? = restoreDeviceIdentity(privateKeyBytes)

// Get node ID
val nodeId: UInt = identity.getNodeId()

// Get keys
val publicKey: ByteArray = identity.getPublicKey()
val privateKey: ByteArray = identity.getPrivateKey()

// Sign data
val signature: ByteArray = identity.sign(message)

// Create attestation
val attestation: ByteArray = identity.createAttestation(timestampMs.toULong())
```

## MeshGenesis API

```kotlin
// Create new mesh
val genesis = MeshGenesis.create("ALPHA-TEAM", founderIdentity)

// Restore from stored bytes
val genesis: MeshGenesis? = decodeMeshGenesis(encodedBytes)

// Get mesh ID
val meshId: String = genesis.getMeshId()

// Get encryption secret
val secret: ByteArray = genesis.getEncryptionSecret()

// Encode for storage
val encoded: ByteArray = genesis.encode()
```

## Peer State API

New types for connection state tracking:

```kotlin
import uniffi.peat_btle.ConnectionState
import uniffi.peat_btle.PeerConnectionState
import uniffi.peat_btle.StateCountSummary
import uniffi.peat_btle.FullStateCountSummary
import uniffi.peat_btle.IndirectPeer
import uniffi.peat_btle.ViaPeerRoute

// Get connection state for a specific peer
val peerState: PeerConnectionState? = mesh.getPeerConnectionState(nodeId.toUInt())
peerState?.let {
    println("State: ${it.state}")  // ConnectionState enum
    println("Connected at: ${it.connectedAt}")
    println("Documents synced: ${it.documentsSynced}")
}

// Get degraded peers (connected but poor signal)
val degraded: List<PeerConnectionState> = mesh.getDegradedPeers()

// Get lost peers (disconnected and timed out)
val lost: List<PeerConnectionState> = mesh.getLostPeers()

// Get connection state counts (direct peers)
val counts: StateCountSummary = mesh.getConnectionStateCounts()
println("Connected: ${counts.connected}, Degraded: ${counts.degraded}")

// Get indirect (multi-hop) peers
val indirect: List<IndirectPeer> = mesh.getIndirectPeers()
indirect.forEach { peer ->
    println("Node ${peer.nodeId} reachable in ${peer.minHops} hops")
    peer.viaPeers.forEach { route ->
        println("  via ${route.viaNodeId} (${route.hopCount} hops)")
    }
}

// Get full state counts including indirect peers
val fullCounts: FullStateCountSummary = mesh.getFullStateCounts()
println("Direct: ${fullCounts.direct.total()}, 1-hop: ${fullCounts.oneHop}")
```

### ConnectionState Enum

```kotlin
enum class ConnectionState {
    DISCOVERED,    // Seen in BLE advertisement
    CONNECTING,    // Connection in progress
    CONNECTED,     // Active healthy connection
    DEGRADED,      // Connected but poor signal
    DISCONNECTING, // Graceful disconnect in progress
    DISCONNECTED,  // Previously connected, now disconnected
    LOST           // Disconnected and not seen in advertisements
}
```

## PeatBtle Integration

The `PeatBtle` class in `com.defenseunicorns.peat` has been updated to use UniFFI internally. If you're using `PeatBtle` directly, the public API remains largely the same - the changes are internal.

Key internal changes in PeatBtle:
- Removed `nativeInit`/`nativeShutdown` calls
- PeatMesh created via UniFFI factory methods
- All BLE callbacks updated with proper type conversions

## Library Loading

UniFFI uses JNA instead of manual `System.loadLibrary`. The native library is loaded automatically when you first use any UniFFI type.

## Troubleshooting

### "Unresolved reference" errors

Ensure you've added the UniFFI imports:
```kotlin
import uniffi.peat_btle.*
```

### Type mismatch errors

Add appropriate conversions:
- `Long` → `ULong`: `.toULong()`
- `Int` → `UInt`: `.toUInt()`
- `Int` → `UByte`: `.toUByte()`
- `Int` → `Byte`: `.toByte()`

### UnsatisfiedLinkError

Ensure the native library (`libpeat_btle.so`) is included in your APK's `jniLibs` folder for the correct ABI (arm64-v8a, armeabi-v7a, x86_64).

## Building the Library

```bash
# Build Rust library for Android
cargo build --target aarch64-linux-android --features android --release

# Regenerate Kotlin bindings (if needed)
uniffi-bindgen generate \
    --library target/aarch64-linux-android/release/libpeat_btle.so \
    --language kotlin \
    --out-dir android/src/main/kotlin/
```

## Questions?

Contact the peat-btle team or open an issue on Radicle.
