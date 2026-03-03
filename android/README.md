# PEAT-BTLE Android Library

Android library providing Bluetooth Low Energy mesh transport for the Peat Protocol.

## Overview

This library is a **transport-only** layer for Peat mesh networking over BLE on Android devices, including Wear OS smartwatches. It provides:

- **BLE Scanning & Advertising**: Discover and advertise Peat nodes
- **GATT Client & Server**: Bidirectional BLE connections
- **Encryption/Decryption**: ChaCha20-Poly1305 mesh-wide encryption
- **Mesh Peer Management**: Automatic peer discovery and connection management
- **Raw Data Callback**: `onDecryptedData()` for app-layer message handling

For app-layer messaging (CannedMessage, tactical status codes), use **hive-lite** as a separate dependency. See [docs/INTEGRATION.md](../docs/INTEGRATION.md) for details.

## Requirements

- **Android API 26+** (Android 8.0 / Wear OS 3)
- **Bluetooth LE** support
- **Kotlin 1.9+** or Java 11+

## Installation

### Gradle (from GitHub Packages)

```kotlin
// settings.gradle.kts
dependencyResolutionManagement {
    repositories {
        maven {
            url = uri("https://maven.pkg.github.com/Ascent-Integrated-Tech/peat-btle")
            credentials {
                username = project.findProperty("gpr.user") as String? ?: System.getenv("GITHUB_ACTOR")
                password = project.findProperty("gpr.key") as String? ?: System.getenv("GITHUB_TOKEN")
            }
        }
    }
}

// build.gradle.kts
dependencies {
    implementation("com.peat:peat-btle-android:0.1.0-SNAPSHOT")
}
```

### Local Build & Publish

```bash
# From peat-btle root directory
cd android

# Build AAR with native libraries (all-in-one)
./gradlew buildAar

# Or publish to local Maven (~/.m2) for testing
./gradlew publishLocal

# The AAR will be at:
# build/outputs/aar/peat-btle-android-release.aar
```

### Use Local AAR in wearos-tak

After running `./gradlew publishLocal`, add to wearos-tak's `settings.gradle.kts`:

```kotlin
dependencyResolutionManagement {
    repositories {
        mavenLocal()  // Uses ~/.m2/repository
        // ... other repos
    }
}
```

Then in `app/build.gradle.kts`:

```kotlin
dependencies {
    implementation("com.peat:peat-btle-android:0.1.0-SNAPSHOT")
}
```

## Quick Start

### Basic Usage

```kotlin
import com.defenseunicorns.peat.*

class MainActivity : AppCompatActivity(), PeatMeshListener {

    private lateinit var peatBtle: PeatBtle

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Initialize Peat BLE with optional node ID and mesh ID
        peatBtle = PeatBtle(
            context = this,
            nodeId = null,  // Auto-generated from Bluetooth MAC
            meshId = "DEMO" // Mesh isolation ID
        )

        // Initialize and check permissions
        if (peatBtle.hasPermissions()) {
            peatBtle.init()
            peatBtle.startMesh(this)
        } else {
            // Request permissions
            requestPermissions(peatBtle.getRequiredPermissions(), REQUEST_CODE)
        }
    }

    // PeatMeshListener callbacks
    override fun onMeshUpdated(peers: List<PeatPeer>) {
        // Update UI with peer list
        Log.d("PEAT", "Peers: ${peers.size}")
    }

    // Transport layer callback - receives raw decrypted bytes
    override fun onDecryptedData(peer: PeatPeer?, data: ByteArray) {
        if (data.isEmpty()) return

        when (data[0]) {
            0xAF.toByte() -> {
                // App-layer message (e.g., CannedMessage)
                // Decode with your app's protocol or hive-lite
                handleAppLayerMessage(peer, data)
            }
        }
    }

    // Legacy callback - still works for backward compatibility
    override fun onPeerEvent(peer: PeatPeer, eventType: PeatEventType) {
        when (eventType) {
            PeatEventType.EMERGENCY -> handleEmergency(peer)
            PeatEventType.ACK -> handleAck(peer)
            else -> {}
        }
    }

    override fun onDestroy() {
        peatBtle.stopMesh()
        peatBtle.shutdown()
        super.onDestroy()
    }
}
```

### Using Native Rust PeatMesh

For direct access to the native Rust mesh implementation:

```kotlin
import com.defenseunicorns.peat.PeatMesh
import com.defenseunicorns.peat.PeripheralType

// Create mesh with configuration
val mesh = PeatMesh(
    nodeId = 0x12345678,
    callsign = "ALPHA-1",
    meshId = "DEMO",
    peripheralType = PeripheralType.SOLDIER_SENSOR
)

// Transport layer: Decrypt received data
val decrypted = mesh.decryptOnly(encryptedBytes)
if (decrypted.isNotEmpty()) {
    when (decrypted[0]) {
        0xAF.toByte() -> {
            // App-layer message - decode with your protocol
        }
    }
}

// Encrypt and send app-layer data
val encrypted = mesh.encryptDocument(myAppLayerData)
// ... broadcast encrypted via BLE

// Periodic sync (for mesh state)
val syncDoc = mesh.tick()
if (syncDoc.isNotEmpty()) {
    // ... broadcast syncDoc to peers
}

// Clean up when done
mesh.destroy()
```

## Permissions

The library declares all required BLE permissions in its manifest. Your app must
request runtime permissions:

### Android 12+ (API 31+)
- `BLUETOOTH_SCAN`
- `BLUETOOTH_CONNECT`
- `BLUETOOTH_ADVERTISE`

### Android 8-11 (API 26-30)
- `BLUETOOTH`
- `BLUETOOTH_ADMIN`
- `ACCESS_FINE_LOCATION`

## Building Native Libraries

To build the native Rust libraries:

### Prerequisites

1. Install Rust and Android targets:
```bash
rustup target add aarch64-linux-android armv7-linux-androideabi
```

2. Install Android NDK and set environment:
```bash
export ANDROID_NDK_HOME=/path/to/android-ndk
export PATH=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin:$PATH
```

### Build

```bash
# From peat-btle root directory
./android/gradlew buildNativeLibs
```

Or manually:

```bash
cargo build --release --target aarch64-linux-android --features android
cargo build --release --target armv7-linux-androideabi --features android

# Copy to jniLibs
cp target/aarch64-linux-android/release/libpeat_btle.so android/src/main/jniLibs/arm64-v8a/
cp target/armv7-linux-androideabi/release/libpeat_btle.so android/src/main/jniLibs/armeabi-v7a/
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Your Application                          │
├─────────────────────────────────────────────────────────────┤
│  PeatBtle (Pure Kotlin)    │    PeatMesh (JNI Wrapper)      │
│  - BLE scanning/advertising│    - Native Rust mesh logic    │
│  - GATT client/server      │    - CRDT document sync        │
│  - Peer management         │    - Cross-platform consistent │
├────────────────────────────┴────────────────────────────────┤
│              Android BLE APIs (BluetoothGatt, etc.)          │
├─────────────────────────────────────────────────────────────┤
│                 Native libpeat_btle.so (Rust)                │
└─────────────────────────────────────────────────────────────┘
```

## BLE MTU Considerations

### Default MTU Limitation

BLE has a default ATT MTU of 23 bytes, allowing only ~20 bytes of payload after
headers. This is insufficient for `PeatDocument` structures which can be:

- **12 bytes minimum**: Header only (no GCounter entries)
- **24+ bytes**: With 1 GCounter entry
- **40-60 bytes**: With location/event data

### Automatic MTU Negotiation

The library automatically requests a larger MTU (185 bytes) when connecting as
a GATT client. This happens in `GattCallbackProxy`:

1. On connection established → Request MTU of 185 bytes
2. On MTU negotiated → Proceed with service discovery
3. If MTU request fails → Fall back to default (documents may be truncated)

### Server-Side MTU

When acting as a GATT server, the library accepts MTU requests from clients.
The actual negotiated MTU depends on both devices' capabilities.

### Troubleshooting Document Truncation

If you see errors like:
```
Document too short for GCounter entries: need 24, have 18
```

This indicates MTU negotiation failed or wasn't performed. Check:
1. Both devices support BLE 4.2+ (required for MTU negotiation)
2. The GATT client is using `GattCallbackProxy` which handles MTU automatically
3. Connection isn't being closed before MTU negotiation completes

## API Reference

### PeatBtle

Main entry point for BLE operations (transport layer).

| Method | Description |
|--------|-------------|
| `init()` | Initialize BLE adapter |
| `hasPermissions()` | Check if BLE permissions are granted |
| `startMesh(listener)` | Start mesh networking |
| `stopMesh()` | Stop mesh networking |
| `broadcastDocument(data)` | Send encrypted data to all peers |
| `getPeers()` | Get list of known peers |
| `shutdown()` | Clean up resources |

### PeatMeshListener

Callback interface for mesh events.

| Method | Description |
|--------|-------------|
| `onDecryptedData(peer, data)` | **Primary callback** - raw decrypted bytes |
| `onMeshUpdated(peers)` | Peer list changed |
| `onPeerEvent(peer, type)` | Legacy callback for events |

### PeatMesh

Native Rust mesh implementation wrapper.

| Method | Description |
|--------|-------------|
| `decryptOnly(data)` | Decrypt without parsing (transport layer) |
| `encryptDocument(data)` | Encrypt data with mesh key |
| `buildDocument()` | Build current state document |
| `onBleDataReceived()` | Process received BLE data |
| `tick()` | Periodic maintenance / sync |
| `getPeerCount()` | Total known peers |
| `getConnectedCount()` | Currently connected peers |

### PeatEventType

| Event | Value | Description |
|-------|-------|-------------|
| `NONE` | 0 | No event |
| `PING` | 1 | Heartbeat |
| `NEED_ASSIST` | 2 | Assistance needed |
| `EMERGENCY` | 3 | Emergency alert |
| `MOVING` | 4 | Moving status |
| `IN_POSITION` | 5 | In position status |
| `ACK` | 6 | Acknowledgment |

## Mesh ID Isolation

Mesh IDs allow multiple independent Peat networks to coexist:

```kotlin
// Only discover/connect to nodes in same mesh
val peatBtle = PeatBtle(context, meshId = "ALPHA")

// Check if a device matches our mesh
if (PeatBtle.matchesMesh("ALPHA", device.meshId)) {
    // Connect
}
```

## License

Apache License 2.0

## Contributing

See the main [peat-btle repository](https://github.com/Ascent-Integrated-Tech/peat-btle) for contribution guidelines.
