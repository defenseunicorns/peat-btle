# peat-btle Integration Guide

**Version**: peat-btle v0.1.0-rc26+
**Date**: 2026-01-26

## Architecture Overview

peat-btle is a **transport-only** library. It handles:
- BLE mesh networking (scanning, advertising, connections)
- Encryption/decryption (ChaCha20-Poly1305)
- Peer management and mesh sync
- Message relay between nodes

peat-btle does **NOT** handle application-layer protocols. For tactical messaging (CannedMessage, etc.), use **peat-lite** as a separate dependency.

```
┌─────────────────────────────────────────────────────────────┐
│                    Your Application                          │
├─────────────────────────────────────────────────────────────┤
│  peat-lite (optional)      │    peat-btle (required)        │
│  - CannedMessage encoding  │    - BLE transport             │
│  - CannedMessage decoding  │    - Encryption/decryption     │
│  - Tactical message types  │    - Mesh peer management      │
│                            │    - onDecryptedData callback  │
└─────────────────────────────────────────────────────────────┘
```

## Quick Start

### Rust

```toml
# Cargo.toml
[dependencies]
peat-btle = "0.1"
peat-lite = "0.0.1"  # Optional: for CannedMessage support
```

```rust
use peat_btle::{PeatMesh, PeatMeshConfig, NodeId};

// Create mesh (transport layer)
let config = PeatMeshConfig::new(
    NodeId::new(0x12345678),
    "ALPHA-1",
    "DEMO"
);
let mesh = PeatMesh::new(config);

// Receive data - get raw decrypted bytes
let decrypted = mesh.decrypt_only(&encrypted_data);
if let Some(bytes) = decrypted {
    // Check marker byte to determine message type
    match bytes.first() {
        Some(0xAF) => {
            // App-layer message - decode with peat-lite
            #[cfg(feature = "peat-lite")]
            if let Some(event) = peat_lite::CannedMessageEvent::decode(&bytes) {
                println!("Received: {:?} from {:08X}", event.message, event.source_node.as_u32());
            }
        }
        Some(0xAA) => { /* PeatDocument */ }
        Some(0xB2) => { /* DeltaDocument */ }
        _ => {}
    }
}
```

### Android/Kotlin

```kotlin
// build.gradle.kts
dependencies {
    implementation("com.defenseunicorns:peat-btle:0.1.0-rc26")
}
```

```kotlin
import com.defenseunicorns.peat.*

class MyActivity : AppCompatActivity(), PeatMeshListener {

    private lateinit var peatBtle: PeatBtle

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        peatBtle = PeatBtle(context = this, meshId = "DEMO")
        peatBtle.init()
        peatBtle.startMesh(this)
    }

    // Transport layer callback - receives raw decrypted bytes
    override fun onDecryptedData(peer: PeatPeer?, data: ByteArray) {
        if (data.isEmpty()) return

        when (data[0]) {
            0xAF.toByte() -> {
                // App-layer message (e.g., CannedMessage from peat-lite)
                // Decode with your app's protocol handler
                handleAppLayerMessage(peer, data)
            }
            0xAA.toByte() -> { /* PeatDocument - handled internally */ }
            0xB2.toByte() -> { /* DeltaDocument - handled internally */ }
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

    private fun handleAppLayerMessage(peer: PeatPeer?, data: ByteArray) {
        // Example: Parse with your own CannedMessage decoder
        // Apps should add peat-lite dependency and use CannedMessageEvent.decode()
        Log.i("APP", "Received ${data.size} byte app-layer message")
    }
}
```

## Wire Format Markers

| Marker | Name | Handler |
|--------|------|---------|
| `0xAE` | Encrypted | peat-btle decrypts, passes inner data |
| `0xAF` | App-layer | Passed to `onDecryptedData`, app decodes |
| `0xAA` | PeatDocument | Processed internally by peat-btle |
| `0xB2` | DeltaDocument | Processed internally by peat-btle |

## Sending App-Layer Messages

To send app-layer messages (like CannedMessage), encrypt them with the mesh key:

```kotlin
// Kotlin - using PeatMesh for encryption
val mesh = peatBtle.getMesh()

// Encode your message (e.g., using peat-lite)
val rawMessage = encodeYourMessage()  // Must start with 0xAF marker

// Encrypt with mesh key
val encrypted = mesh?.encryptDocument(rawMessage)

// Broadcast to peers
if (encrypted != null) {
    peatBtle.broadcastDocument(encrypted)
}
```

```rust
// Rust - using PeatMesh for encryption
let raw_message = encode_your_message();  // Must start with 0xAF marker
let encrypted = mesh.encrypt_document(&raw_message);
// Send encrypted bytes over BLE
```

## Using peat-lite for CannedMessage

If your app needs CannedMessage support, add peat-lite:

### Rust

```toml
[dependencies]
peat-lite = "0.0.1"
```

```rust
use peat_lite::{CannedMessage, CannedMessageEvent, NodeId};

// Create a CannedMessage
let event = CannedMessageEvent::new(
    CannedMessage::Emergency,
    NodeId::new(my_node_id),
    None,  // broadcast
    timestamp_ms,
);
let encoded = event.encode();  // Includes 0xAF marker

// Encrypt and send via peat-btle
let encrypted = mesh.encrypt_document(&encoded);

// Decode received CannedMessage
if let Some(event) = CannedMessageEvent::decode(&decrypted_bytes) {
    match event.message {
        CannedMessage::Ack => println!("ACK from {:08X}", event.source_node.as_u32()),
        CannedMessage::Emergency => println!("EMERGENCY from {:08X}", event.source_node.as_u32()),
        _ => {}
    }
}
```

### Kotlin (using JSON bridge)

Since peat-lite is Rust-only, Android apps can:
1. Include peat-lite in their native code
2. Use a JSON/Protobuf bridge
3. Implement CannedMessage encoding/decoding in Kotlin directly

Example Kotlin CannedMessage decoder:

```kotlin
object CannedMessageDecoder {
    const val MARKER: Byte = 0xAF.toByte()

    data class CannedMessageEvent(
        val messageCode: Int,
        val sourceNode: Long,
        val targetNode: Long,
        val timestamp: Long,
        val sequence: Int
    )

    fun decode(data: ByteArray): CannedMessageEvent? {
        if (data.size < 22 || data[0] != MARKER) return null

        val buffer = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN)
        buffer.get()  // skip marker

        return CannedMessageEvent(
            messageCode = buffer.get().toInt() and 0xFF,
            sourceNode = buffer.int.toLong() and 0xFFFFFFFFL,
            targetNode = buffer.int.toLong() and 0xFFFFFFFFL,
            timestamp = buffer.long,
            sequence = buffer.int
        )
    }

    fun encode(
        messageCode: Int,
        sourceNode: Long,
        targetNode: Long = 0,
        timestamp: Long = System.currentTimeMillis(),
        sequence: Int = 0
    ): ByteArray {
        val buffer = ByteBuffer.allocate(22).order(ByteOrder.LITTLE_ENDIAN)
        buffer.put(MARKER)
        buffer.put(messageCode.toByte())
        buffer.putInt(sourceNode.toInt())
        buffer.putInt(targetNode.toInt())
        buffer.putLong(timestamp)
        buffer.putInt(sequence)
        return buffer.array()
    }
}
```

## CannedMessage Types Reference

| Code | Name | Description |
|------|------|-------------|
| **Acknowledgments** | | |
| 0x00 | ACK | "Message received" |
| 0x01 | ACK_WILCO | "Will comply" |
| 0x02 | ACK_NEGATIVE | "Cannot comply" |
| 0x03 | ACK_SAY_AGAIN | "Say again" |
| **Status** | | |
| 0x10 | CHECK_IN | "I'm here / still alive" |
| 0x11 | MOVING | "En route" |
| 0x12 | HOLDING | "Stationary / waiting" |
| 0x13 | ON_STATION | "Arrived at position" |
| 0x14 | RETURNING | "Heading back" |
| 0x15 | COMPLETE | "Mission complete" |
| **Alerts** | | |
| 0x20 | EMERGENCY | "Need immediate help" |
| 0x21 | ALERT | "Attention needed" |
| 0x22 | ALL_CLEAR | "Situation resolved" |
| 0x23 | CONTACT | "Contact spotted" |
| 0x24 | UNDER_FIRE | "Taking fire" |
| **Requests** | | |
| 0x30 | NEED_EXTRACT | "Request pickup" |
| 0x31 | NEED_SUPPORT | "Request assistance" |
| 0x32 | NEED_MEDIC | "Medical emergency" |
| 0x33 | NEED_RESUPPLY | "Need resupply" |
| 0xFF | CUSTOM | Application-specific |

## Migration from Previous Versions

If you were using `mesh.sendCannedMessage()` or `mesh.decodeCannedMessage()`:

**Before (rc22-rc25):**
```kotlin
// Old API - removed
val wireData = mesh.sendCannedMessage(CannedMessageCode.EMERGENCY, targetNode)
peatBtle.broadcastDocument(wireData)
```

**After (rc26+):**
```kotlin
// New API - use onDecryptedData callback + your own encoding
override fun onDecryptedData(peer: PeatPeer?, data: ByteArray) {
    if (data.isNotEmpty() && data[0] == 0xAF.toByte()) {
        val event = CannedMessageDecoder.decode(data)
        // Handle event
    }
}

// Sending
val encoded = CannedMessageDecoder.encode(0x20, myNodeId)  // EMERGENCY
val encrypted = mesh.encryptDocument(encoded)
peatBtle.broadcastDocument(encrypted)
```

## Summary

| Component | Responsibility |
|-----------|---------------|
| **peat-btle** | BLE transport, encryption, mesh sync, peer management |
| **peat-lite** | CannedMessage primitives, CRDT types (optional) |
| **Your App** | Message encoding/decoding, business logic, UI |

This separation ensures:
- peat-btle remains lightweight and transport-focused
- Apps have flexibility in message protocols
- No forced dependencies between libraries
