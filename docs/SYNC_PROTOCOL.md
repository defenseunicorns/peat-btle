# HIVE-BTLE Sync Protocol Specification

This document specifies the CRDT synchronization protocol used by `hive-btle` for mesh state replication over BLE.

## Table of Contents

- [Overview](#overview)
- [CRDT Types](#crdt-types)
  - [G-Counter](#g-counter)
  - [LWW-Register](#lww-register)
  - [EmergencyEvent (Custom CRDT)](#emergencyevent-custom-crdt)
- [Wire Format](#wire-format)
  - [Document Structure](#document-structure)
  - [Section Markers](#section-markers)
  - [Encoding Details](#encoding-details)
- [Sync Protocol](#sync-protocol)
  - [Protocol Flow](#protocol-flow)
  - [Chunking](#chunking)
  - [Delta Encoding](#delta-encoding)
- [Data Types](#data-types)
  - [Peripheral](#peripheral)
  - [PeripheralEvent](#peripheralevent)
  - [HealthStatus](#healthstatus)
  - [Position](#position)
- [Encryption](#encryption)
- [Examples](#examples)
- [Size Constraints](#size-constraints)

---

## Overview

HIVE-BTLE uses Conflict-free Replicated Data Types (CRDTs) to enable mesh synchronization without coordination. The protocol is designed for:

- **Low bandwidth**: Optimized for BLE's constrained MTU
- **Eventual consistency**: All nodes converge to the same state
- **Power efficiency**: Batching and delta encoding reduce radio time
- **Partition tolerance**: Nodes can reconnect after disconnection

### Architecture

```
┌────────────────────────────────────────────────────┐
│                  Application                        │
│    (emergency alerts, health status, events)       │
└─────────────────────┬──────────────────────────────┘
                      │
                      ▼
┌────────────────────────────────────────────────────┐
│              HiveDocument                           │
│  ┌──────────────┐  ┌────────────┐  ┌────────────┐  │
│  │  G-Counter   │  │ Peripheral │  │  Emergency │  │
│  │   (CRDT)     │  │   (LWW)    │  │   (CRDT)   │  │
│  └──────────────┘  └────────────┘  └────────────┘  │
└─────────────────────┬──────────────────────────────┘
                      │ encode/decode
                      ▼
┌────────────────────────────────────────────────────┐
│              Wire Format (bytes)                    │
│           [Header][Counter][Peripheral][Emergency] │
└─────────────────────┬──────────────────────────────┘
                      │ chunk/reassemble
                      ▼
┌────────────────────────────────────────────────────┐
│              GATT Characteristics                   │
│    (writes/notifications up to MTU bytes each)     │
└────────────────────────────────────────────────────┘
```

---

## CRDT Types

### G-Counter

A **Grow-only Counter** where each node maintains its own count. The total is the sum of all node counts.

**Properties:**
- Increment only (no decrement)
- Merge: take max of each node's count
- Commutative, associative, idempotent

**Operations:**

| Operation | Description |
|-----------|-------------|
| `increment(node_id, amount)` | Add `amount` to this node's count |
| `value()` | Sum of all node counts |
| `merge(other)` | `max(self[n], other[n])` for each node n |

**Wire Format:**

```
num_entries: 4 bytes (LE u32)
entries[N]:
  node_id:  4 bytes (LE u32)
  count:    8 bytes (LE u64)
```

**Example:**

```
Node A increments 5 times: {A: 5}
Node B increments 3 times: {B: 3}

After merge at A:
{A: 5, B: 3} → value = 8

After merge at B:
{A: 5, B: 3} → value = 8
```

### LWW-Register

A **Last-Writer-Wins Register** stores a single value where concurrent writes are resolved by timestamp.

**Properties:**
- Higher timestamp wins
- Tie-breaker: higher node_id wins
- Merge: take value with higher (timestamp, node_id)

**Semantics:**

```rust
fn should_update(self_ts, self_node, other_ts, other_node) -> bool {
    other_ts > self_ts ||
    (other_ts == self_ts && other_node > self_node)
}
```

**Used For:**
- Peripheral health status
- Position updates
- Event state

### EmergencyEvent (Custom CRDT)

A custom CRDT for distributed emergency acknowledgment tracking.

**Identity:** Events are uniquely identified by `(source_node, timestamp)`

**Merge Rules:**

1. **Same event** (same source_node and timestamp):
   - ACK maps merge with OR (once acked, stays acked)
   - `acks[n] = self.acks[n] OR other.acks[n]`

2. **Different events**:
   - Take the event with higher timestamp
   - Newer emergency replaces older

**Properties:**
- Source node auto-acks their own emergency
- ACK state is monotonic: `false → true` (never back)
- Distributed tracking of who has acknowledged

**Wire Format:**

```
source_node: 4 bytes (LE u32)
timestamp:   8 bytes (LE u64)
num_acks:    4 bytes (LE u32)
acks[N]:
  node_id:   4 bytes (LE u32)
  acked:     1 byte (0 or 1)
```

---

## Wire Format

### Document Structure

The HIVE document has a layered structure:

```
┌────────────────────────────────────────────────────┐
│ Header (8 bytes)                                    │
│   version:  4 bytes (LE u32)                        │
│   node_id:  4 bytes (LE u32)                        │
├────────────────────────────────────────────────────┤
│ G-Counter (4 + N×12 bytes)                          │
│   num_entries: 4 bytes (LE u32)                     │
│   entries[N]:                                       │
│     node_id: 4 bytes (LE u32)                       │
│     count:   8 bytes (LE u64)                       │
├────────────────────────────────────────────────────┤
│ Extended Section (optional) - Peripheral            │
│   marker:         1 byte (0xAB)                     │
│   reserved:       1 byte (0x00)                     │
│   section_len:    2 bytes (LE u16)                  │
│   peripheral:     variable (34-43 bytes)            │
├────────────────────────────────────────────────────┤
│ Emergency Section (optional)                        │
│   marker:         1 byte (0xAC)                     │
│   reserved:       1 byte (0x00)                     │
│   section_len:    2 bytes (LE u16)                  │
│   emergency:      variable (16 + N×5 bytes)         │
└────────────────────────────────────────────────────┘
```

### Section Markers

| Marker | Hex | Description |
|--------|-----|-------------|
| `EXTENDED_MARKER` | `0xAB` | Peripheral data section |
| `EMERGENCY_MARKER` | `0xAC` | Emergency event section |
| `ENCRYPTED_MARKER` | `0xAE` | Mesh-wide encrypted payload |
| `PEER_E2EE_MARKER` | `0xAF` | Per-peer E2EE message |
| `KEY_EXCHANGE_MARKER` | `0xB0` | E2EE key exchange |

### Encoding Details

All multi-byte integers use **little-endian** (LE) encoding.

**Document Version:**
- Incremented on each local change
- Used for detecting updates, not for ordering
- Wraps at `u32::MAX`

**Node ID:**
- 32-bit identifier, typically derived from BLE MAC
- Last 4 bytes of 6-byte MAC address
- Displayed as uppercase hex (e.g., `12345678`)

---

## Sync Protocol

### Protocol Flow

```
Node A                           Node B
  │                                │
  │   [1] Build document           │
  │         │                      │
  │         ▼                      │
  │   [2] Encode to bytes          │
  │         │                      │
  │         ▼                      │
  │   [3] Chunk if needed          │
  │         │                      │
  │         ▼                      │
  ├──────── [4] Write chunks ────────►
  │                                │
  │                           [5] Reassemble
  │                                │
  │                           [6] Decode
  │                                │
  │                           [7] Merge (CRDT)
  │                                │
  │   [8] ACK (optional)          │
  ◄────────────────────────────────┤
```

### Chunking

When documents exceed MTU, they are split into chunks.

**Chunk Header (8 bytes):**

```
message_id:    4 bytes (LE u32) - Unique message identifier
chunk_index:   2 bytes (LE u16) - Index (0-based)
total_chunks:  2 bytes (LE u16) - Total chunk count
```

**Payload Size:**
- `payload_size = MTU - 8 (header)`
- Default MTU: 23 bytes → 15 byte payload
- BLE 5.0 MTU: 247 bytes → 239 byte payload

**Reassembly:**
- Buffer chunks by `message_id`
- Complete when all `total_chunks` received
- Concatenate payloads in `chunk_index` order
- Timeout: 30 seconds for partial messages

### Delta Encoding

To reduce bandwidth, nodes track what each peer has seen.

**Vector Clock:**
- Each node maintains a vector clock
- Tracks the latest timestamp seen from each peer
- Only sends operations newer than peer's clock

**Algorithm:**

```rust
fn filter_for_peer(peer_id, operations) {
    let peer_clock = self.peer_clocks[peer_id];
    operations.filter(|op| op.timestamp > peer_clock[op.node_id])
}
```

---

## Data Types

### Peripheral

Represents a peripheral device attached to a node.

**Wire Format (34-43 bytes):**

```
id:             4 bytes (LE u32)
parent_node:    4 bytes (LE u32)
type:           1 byte
callsign:       12 bytes (null-padded ASCII)
health:         4 bytes (HealthStatus)
has_event:      1 byte (0 or 1)
event:          9 bytes (if has_event=1)
timestamp:      8 bytes (LE u64)
```

**Peripheral Types:**

| Value | Type | Description |
|-------|------|-------------|
| 0 | Unknown | Unspecified |
| 1 | SoldierSensor | Wearable sensor |
| 2 | FixedSensor | Stationary sensor |
| 3 | Relay | Mesh relay only |

### PeripheralEvent

Events emitted by peripherals.

**Wire Format (9 bytes):**

```
event_type: 1 byte
timestamp:  8 bytes (LE u64)
```

**Event Types:**

| Value | Type | Description |
|-------|------|-------------|
| 0 | None | No event (cleared) |
| 1 | Ping | "I'm OK" |
| 2 | NeedAssist | Request assistance |
| 3 | Emergency | SOS/Emergency |
| 4 | Moving | In transit |
| 5 | InPosition | Stationary |
| 6 | Ack | Acknowledged |

### HealthStatus

Health/status information for a peripheral.

**Wire Format (4 bytes):**

```
battery_percent: 1 byte (0-100)
activity:        1 byte (0=still, 1=walk, 2=run, 3=fall)
alerts:          1 byte (bitflags)
heart_rate:      1 byte (BPM, 0=not present)
```

**Alert Flags:**

| Bit | Flag | Description |
|-----|------|-------------|
| 0 | `ALERT_MAN_DOWN` | Man down detected |
| 1 | `ALERT_LOW_BATTERY` | Low battery |
| 2 | `ALERT_OUT_OF_RANGE` | Out of range |
| 3 | `ALERT_CUSTOM_1` | Custom alert |

### Position

Geographic position with optional altitude and accuracy.

**Wire Format (9-17 bytes):**

```
latitude:  4 bytes (LE f32)
longitude: 4 bytes (LE f32)
flags:     1 byte
  bit 0: has_altitude
  bit 1: has_accuracy
altitude:  4 bytes (LE f32, if flag set)
accuracy:  4 bytes (LE f32, if flag set)
```

---

## Encryption

### Mesh-Wide Encryption

All mesh members share a secret. Documents are encrypted with ChaCha20-Poly1305.

**Format:**

```
marker:   1 byte (0xAE)
reserved: 1 byte (0x00)
nonce:    12 bytes
ciphertext + tag: variable (includes 16-byte auth tag)
```

**Key Derivation:**
- HKDF-SHA256 from shared secret
- Salt: mesh_id bytes
- Info: "HIVE-BTLE-MESH-KEY"

**Overhead:** 30 bytes (2 marker + 12 nonce + 16 tag)

### Per-Peer E2EE

Two peers establish encrypted sessions via X25519 key exchange.

**Key Exchange Format:**

```
marker:     1 byte (0xB0)
sender:     4 bytes (LE u32)
flags:      1 byte
public_key: 32 bytes
```

**Encrypted Message Format:**

```
marker:     1 byte (0xAF)
flags:      1 byte
recipient:  4 bytes (LE u32)
sender:     4 bytes (LE u32)
counter:    8 bytes (LE u64)
nonce:      12 bytes
ciphertext: variable (includes 16-byte tag)
```

**Overhead:** 46 bytes per message

---

## Examples

### Minimal Document (12 bytes)

```hex
01 00 00 00        # version = 1
78 56 34 12        # node_id = 0x12345678
00 00 00 00        # num_entries = 0
```

### Document with Counter (24 bytes)

```hex
02 00 00 00        # version = 2
78 56 34 12        # node_id = 0x12345678
01 00 00 00        # num_entries = 1
78 56 34 12        # entry[0].node_id = 0x12345678
05 00 00 00 00 00 00 00  # entry[0].count = 5
```

### Document with Emergency (variable)

```hex
01 00 00 00        # version = 1
11 11 11 11        # node_id = 0x11111111
... (counter data)
AC 00              # EMERGENCY_MARKER, reserved
20 00              # section_len = 32 bytes
11 11 11 11        # source_node = 0x11111111
E8 03 00 00 00 00 00 00  # timestamp = 1000
02 00 00 00        # num_acks = 2
11 11 11 11 01     # node 0x11111111 acked
22 22 22 22 00     # node 0x22222222 not acked
```

### Emergency Flow Example

```
# Node A sends emergency
A: set_emergency(A, timestamp=1000, peers=[B, C])
A: document.emergency = {source: A, ts: 1000, acks: {A: true, B: false, C: false}}
A: broadcast(document)

# Node B receives and ACKs
B: merge(A's document)
B: document.emergency = {source: A, ts: 1000, acks: {A: true, B: false, C: false}}
B: ack_emergency(B)
B: document.emergency = {source: A, ts: 1000, acks: {A: true, B: true, C: false}}
B: broadcast(document)

# Node C receives B's document
C: merge(B's document)
C: document.emergency = {source: A, ts: 1000, acks: {A: true, B: true, C: false}}
C: ack_emergency(C)
# All acked!
```

---

## Size Constraints

### Size Limits

| Constant | Value | Description |
|----------|-------|-------------|
| `MIN_DOCUMENT_SIZE` | 8 bytes | Header only |
| `TARGET_DOCUMENT_SIZE` | 244 bytes | Fits in single BLE packet |
| `MAX_DOCUMENT_SIZE` | 512 bytes | Maximum before fragmentation required |
| `MAX_MESH_SIZE` | 20 nodes | Recommended max for single-packet sync |

### Size Calculations

**Document Size Formula:**

```
size = 8 (header)
     + 4 + (num_nodes × 12) (counter)
     + 4 + peripheral_size (if peripheral present)
     + 4 + emergency_size (if emergency present)
```

**Per-Component Sizes:**

| Component | Size |
|-----------|------|
| Header | 8 bytes |
| Counter entry | 12 bytes/node |
| Peripheral (no event) | 38 bytes |
| Peripheral (with event) | 47 bytes |
| Emergency (base) | 16 bytes |
| Emergency ACK entry | 5 bytes/peer |

**Example: 10-node mesh with emergency**

```
Header:     8 bytes
Counter:    4 + (10 × 12) = 124 bytes
Peripheral: 4 + 47 = 51 bytes
Emergency:  4 + 16 + (10 × 5) = 70 bytes
Total:      253 bytes ✓ (fits in target)
```

### MTU Negotiation

| Platform | Default MTU | Max MTU |
|----------|-------------|---------|
| BLE 4.0/4.1 | 23 | 23 |
| BLE 4.2+ | 23 | 251 |
| BLE 5.0+ | 23 | 517 |

After connection, negotiate higher MTU:

```rust
// Request higher MTU
gatt.request_mtu(247)?;

// Update protocol config
sync.set_mtu(negotiated_mtu);
```

---

## Sync Profiles

### Low Power (Smartwatch)

```rust
SyncConfig::low_power()
```

| Parameter | Value |
|-----------|-------|
| Sync interval | 30 seconds |
| MTU | 23 bytes |
| Max retries | 2 |
| Delta encoding | Enabled |

### Responsive (Tablet)

```rust
SyncConfig::responsive()
```

| Parameter | Value |
|-----------|-------|
| Sync interval | 1 second |
| MTU | 517 bytes |
| Max retries | 3 |
| Delta encoding | Enabled |

---

## Compatibility Notes

1. **Backward Compatibility**: Documents without extended sections (pre-0.1) are valid
2. **Unknown Markers**: Stop parsing on unknown marker (forward compatible)
3. **Encryption Optional**: Unencrypted documents accepted unless strict mode
4. **Empty Counter**: Valid document with zero entries
