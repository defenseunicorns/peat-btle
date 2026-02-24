# HIVE / eche-btle Architecture Refactoring Plan

**Status**: **Decisions Made** - Ready for Implementation
**Date**: 2026-01-25 (updated)
**Authors**: Kit, Claude
**Stakeholders**: HIVE Team, ATAK Plugin Team, WearTAK Team

> **Summary of Key Decisions** (2026-01-25):
> 1. **hive-lite** → Separate repo (leaf crate, no HIVE deps, `no_std`)
> 2. **CannedMessage** → Predefined message codes replace ChatCRDT
> 3. **Emergency** → Becomes `CannedMessage::Emergency`
> 4. **eche-btle** → Transport only; `standalone` feature adds hive-lite dep
> 5. **Translation layer** → Lives in hive-protocol, not transports

---

## Executive Summary

The current eche-btle library conflates two different responsibilities:
1. **BLE Transport** - Moving bytes over Bluetooth Low Energy
2. **Document/CRDT Logic** - Defining and merging application data structures

This has caused bugs (chat not syncing) and architectural confusion. This document proposes a clear separation of concerns between HIVE (protocol layer) and eche-btle (transport layer).

---

## Table of Contents

1. [Problem Statement](#problem-statement)
2. [Current Architecture (Problematic)](#current-architecture-problematic)
3. [Root Cause Analysis](#root-cause-analysis)
4. [Proposed Architecture](#proposed-architecture)
5. [Migration Tasks](#migration-tasks)
6. [Open Questions](#open-questions)
7. [Decision Log](#decision-log)

---

## Problem Statement

### Symptoms

1. **Chat messages don't sync reliably** between WearTAK devices and ATAK plugin
2. **Duplicate delta sync implementations** - Kotlin builds delta operations separately from Rust
3. **Architectural confusion** - Unclear what eche-btle should know about vs HIVE

### The Specific Bug

From device testing:
```
[ENCRYPTED-MERGE] sourceNode=B1063BF6, isAck=false, counterChanged=false, total=0
```

- Watch 2 has `chatCount=1` locally
- Plugin receives `total=0` - chat state missing
- Location/callsign sync works, but chat doesn't propagate

### Root Cause

In `EcheBtle.kt`, the `buildSyncDocumentForPeer()` function has two paths:

1. **Full sync** (every 10th sync): Uses native `buildDocument()` which includes chat ✓
2. **Delta sync** (9/10 syncs): Builds Kotlin-side delta operations ✗

The Kotlin delta operations only include:
- `IncrementCounter`
- `UpdateLocation`
- `UpdateHealth`
- `UpdateCallsign`
- `UpdateEvent`

**Chat is NOT included in delta operations** - so 90% of syncs don't send chat.

---

## Current Architecture (Problematic)

```
┌─────────────────────────────────────────────────────────────────┐
│                        eche-btle                                 │
│  (Currently owns too much)                                       │
├─────────────────────────────────────────────────────────────────┤
│  ✓ Platform adapters (BlueZ, CoreBluetooth, Android, ESP32)     │
│  ✓ GATT service & fragmentation                                 │
│  ✓ Mesh relay                                                   │
│  ✓ Encryption primitives                                        │
│  ✓ Discovery & connection management                            │
├─────────────────────────────────────────────────────────────────┤
│  ✗ EcheDocument format (counter, peripheral, emergency, chat)   │
│  ✗ CRDT implementations (GCounter, ChatCRDT, EmergencyEvent)    │
│  ✗ Delta sync with CRDT-specific operations                     │
│  ✗ Document merge semantics                                     │
│  ✗ Identity/Genesis/Security policy                             │
│  ✗ Hierarchy levels                                             │
└─────────────────────────────────────────────────────────────────┘
```

### What eche-btle Currently Defines

| Component | Location | Problem |
|-----------|----------|---------|
| `EcheDocument` | `src/document.rs` | Application-level structure in transport |
| `ChatCRDT` | `src/sync/crdt.rs` | Chat is a HIVE concept, not transport |
| `GCounter` | `src/sync/crdt.rs` | Counter semantics belong in HIVE |
| `EmergencyEvent` | `src/sync/crdt.rs` | Emergency protocol is HIVE-level |
| `DeltaDocument` | `src/sync/delta_document.rs` | CRDT-specific operations |
| `MeshGenesis` | `src/security/genesis.rs` | Trust policy is HIVE-level |
| `HierarchyLevel` | `src/eche_mesh.rs` | Hierarchy is HIVE concept |

---

## Root Cause Analysis

### Two Different Use Cases Conflated

**Use Case 1: Standalone Embedded Mesh** (ESP32 sensors without HIVE)
- Needs lightweight CRDTs in eche-btle
- Self-contained document format
- Valid for ESP32-to-ESP32 mesh

**Use Case 2: HIVE Transport Layer** (WearTAK, phones with full HIVE)
- Should be opaque byte transport
- HIVE manages documents via Automerge/Ditto
- eche-btle should just move bytes

**The problem**: These are not clearly separated. The Android/Kotlin code tries to use both, causing bugs.

### HIVE Ontology Review

The HIVE project has 8 core protobuf schemas:

| Schema | Purpose |
|--------|---------|
| `cap.common.v1` | Uuid, Timestamp, Position |
| `cap.node.v1` | Platform definitions |
| `cap.capability.v1` | Capability discovery |
| `cap.cell.v1` | Squad formation |
| `cap.zone.v1` | Hierarchy coordination |
| `cap.role.v1` | Role assignments |
| `cap.beacon.v1` | Discovery phase |
| `cap.composition.v1` | Capability composition |

**Key Finding**: No "chat" or "messaging" in the core HIVE ontology.

### Where Should Chat Live?

| Layer | Responsibility | Chat? |
|-------|----------------|-------|
| hive-schema | Domain models (protobuf) | Add `cap.message.v1` if needed |
| hive-protocol | Sync abstraction | Document type definition |
| hive-persistence | Automerge/Ditto backends | CRDT storage & merge |
| hive-lite | Constrained primitives | **NO** - too complex for 256KB |
| eche-btle | BLE transport | **NO** - just moves bytes |

---

## Proposed Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        HIVE (Protocol Layer)                     │
├─────────────────────────────────────────────────────────────────┤
│  hive-schema     │ Protobuf definitions (node, cell, message?)  │
│  hive-protocol   │ DataStore, SyncCapable traits                │
│  hive-persistence│ Automerge/Ditto document storage             │
│  hive-lite       │ Constrained primitives (LWW, counters only)  │
└─────────────────────────────────────────────────────────────────┘
                              │
                    ┌─────────┴─────────┐
                    │  Translation API   │
                    │  (thin, in HIVE)   │
                    └─────────┬─────────┘
                              │
┌─────────────────────────────────────────────────────────────────┐
│                   eche-btle (Transport Layer)                    │
├─────────────────────────────────────────────────────────────────┤
│  Platform adapters (BlueZ, CoreBluetooth, Android, ESP32)       │
│  GATT service & fragmentation                                   │
│  Mesh relay (forward opaque bytes)                              │
│  Encryption primitives (encrypt/decrypt with provided key)      │
│  Discovery & connection management                              │
│                                                                 │
│  [STANDALONE MODE - feature-gated]:                             │
│  Lightweight CRDTs for ESP32-only mesh (no HIVE integration)    │
└─────────────────────────────────────────────────────────────────┘
```

### Target eche-btle API

**Current API** (wrong - knows about documents):
```rust
pub fn build_document(&self) -> Vec<u8>;
pub fn merge_document(&self, data: &[u8]) -> MergeResult;
pub fn send_chat(&self, sender: &str, text: &str);
pub fn send_emergency(&self, known_peers: &[u32]);
```

**Target API** (correct - opaque transport):
```rust
pub trait BleTransport {
    /// Send data to a specific peer
    fn send(&self, peer: NodeId, data: &[u8]) -> Result<()>;

    /// Broadcast data to all connected peers
    fn broadcast(&self, data: &[u8]) -> Result<()>;

    /// Set callback for received data
    fn on_receive(&self, callback: fn(NodeId, &[u8]));

    /// Encryption (key provided by HIVE layer)
    fn set_encryption_key(&self, key: &[u8; 32]);
    fn encrypt(&self, plaintext: &[u8]) -> Vec<u8>;
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>>;
}
```

**Feature-gated standalone mode** (for ESP32 without HIVE):
```rust
#[cfg(feature = "standalone")]
pub mod standalone {
    // Lightweight CRDTs for ESP32-only mesh
    // NOT for use with full HIVE integration
    pub struct StandaloneDocument { ... }
    pub fn build_standalone_document(&self) -> Vec<u8>;
    pub fn merge_standalone_document(&self, data: &[u8]);
}
```

---

## Migration Tasks

### Phase 1: HIVE Team - Define Messaging (if needed)

#### Task 1.1: Decide on Tactical Messaging Scope

**Question**: Is tactical text messaging in scope for HIVE protocol?

Options:
- **A**: Yes - Add `cap.message.v1` to hive-schema
- **B**: No - Messaging handled externally (TAK, etc.)
- **C**: Defer - Focus on core protocol first

**Assigned to**: _____________
**Decision date**: _____________

#### Task 1.2: Add Message Schema (if Option A)

**File**: `hive-schema/proto/cap/message/v1/message.proto`

```protobuf
syntax = "proto3";
package cap.message.v1;

import "cap/common/v1/common.proto";

// Tactical text message (HIVE-Full only, not for hive-lite)
message TextMessage {
  cap.common.v1.Uuid id = 1;
  cap.common.v1.Uuid sender_id = 2;
  cap.common.v1.Timestamp timestamp = 3;
  string sender_callsign = 4;
  string text = 5;
  optional cap.common.v1.Uuid reply_to = 6;
  bool broadcast = 7;
}

message MessageCollection {
  repeated TextMessage messages = 1;
  // CRDT semantics: Append-only or RGA for ordering
}
```

**Assigned to**: _____________
**Target date**: _____________

#### Task 1.3: Implement Message Sync in hive-persistence

- Store messages in Automerge document
- Use appropriate CRDT (append-only log or RGA)
- Implement `DataStore` trait for messages
- Handle merge conflicts

**Assigned to**: _____________
**Target date**: _____________

---

### Phase 2: Translation Layer

#### Task 2.1: Define Translation API

**Location**: `hive-protocol/src/transport/ble_bridge.rs` (or new crate)

```rust
/// Bridge between HIVE documents and eche-btle transport
pub trait BleBridge {
    /// Serialize current HIVE state for BLE transmission
    fn serialize_for_ble(&self) -> Vec<u8>;

    /// Deserialize received BLE data into HIVE updates
    fn apply_from_ble(&self, data: &[u8]) -> Result<()>;

    /// Get encryption key for mesh
    fn mesh_encryption_key(&self) -> &[u8; 32];
}
```

**Key decisions**:
- [ ] Where does this live? (hive-protocol, new crate, or apps)
- [ ] What serialization format? (Protobuf, custom binary, JSON)
- [ ] How to handle version compatibility?

**Assigned to**: _____________
**Target date**: _____________

---

### Phase 3: eche-btle Refactoring

#### Task 3.1: Feature-Gate Standalone Mode

Add feature flag to separate standalone embedded use from HIVE integration:

```toml
# Cargo.toml
[features]
default = ["transport"]
transport = []  # Pure transport layer
standalone = ["transport"]  # Includes lightweight CRDTs for ESP32-only mesh
```

**Assigned to**: _____________
**Target date**: _____________

#### Task 3.2: Refactor Public API

Move document-aware methods behind `standalone` feature:

```rust
// Always available (transport layer)
impl EcheMesh {
    pub fn send(&self, peer: NodeId, data: &[u8]) -> Result<()>;
    pub fn broadcast(&self, data: &[u8]) -> Result<()>;
    pub fn on_data_received(&self, peer: NodeId, data: &[u8]);
}

// Only with standalone feature
#[cfg(feature = "standalone")]
impl EcheMesh {
    pub fn build_document(&self) -> Vec<u8>;
    pub fn merge_document(&self, data: &[u8]) -> MergeResult;
    pub fn send_chat(&self, sender: &str, text: &str) -> Option<Vec<u8>>;
}
```

**Assigned to**: _____________
**Target date**: _____________

#### Task 3.3: Update Android/Kotlin Integration

Remove Kotlin-side delta building. HIVE layer provides bytes, Kotlin just transports.

**Current** (wrong):
```kotlin
private fun buildSyncDocumentForPeer(...): ByteArray? {
    val operations = mutableListOf<DeltaOperation>()
    // Kotlin manually builds deltas - chat missing!
}
```

**Target** (correct):
```kotlin
// HIVE layer provides serialized state
val documentBytes = hiveProtocol.serializeForBle()

// eche-btle just transports
hiveBtle.broadcast(documentBytes)
```

**Assigned to**: _____________
**Target date**: _____________

---

### Phase 4: Documentation & Testing

#### Task 4.1: Update Architecture Documentation

- [ ] Update eche-btle README to clarify scope
- [ ] Add ADR for transport/protocol separation
- [ ] Document standalone vs HIVE integration modes
- [ ] Update CLAUDE.md with new architecture

#### Task 4.2: Integration Tests

- [ ] Test HIVE → eche-btle → HIVE round-trip
- [ ] Test message sync across mesh
- [ ] Test standalone ESP32 mesh (no HIVE)
- [ ] Performance benchmarks

---

## Open Questions

### Q1: Is tactical messaging in scope for HIVE?

**Context**: The current eche-btle ChatCRDT was added for WearTAK use. HIVE core ontology doesn't include messaging.

**Options**:
- A: Add `cap.message.v1` to HIVE schema
- B: Keep messaging external (TAK integration)
- C: Messaging is app-level concern, not protocol

**Decision**: **D: CannedMessage primitive in hive-lite** (2026-01-25)
**Rationale**: WearTAK needs ACK/Emergency, not full chat. Predefined message codes fit 256KB budget. Full text messaging (if needed) would be HIVE-Full only via `cap.message.v1`.

---

### Q2: Where should the translation layer live?

**Options**:
- A: In `hive-protocol` crate
- B: New `eche-btle-bridge` crate
- C: In each app (Android, iOS, etc.)

**Decision**: _____________
**Rationale**: _____________

---

### Q3: What happens to existing eche-btle users?

**Concern**: Breaking changes for standalone ESP32 users.

**Options**:
- A: Major version bump (0.x → 1.0) with migration guide
- B: Feature flags to maintain backward compatibility
- C: Deprecation period with warnings

**Decision**: _____________
**Rationale**: _____________

---

### Q4: Should standalone mode support chat?

**Current**: eche-btle has ChatCRDT in standalone mode

**Concern**: Chat is complex for 256KB RAM budget (hive-lite target)

**Options**:
- A: Remove ChatCRDT from standalone (simplify)
- B: Keep but document limitations
- C: Make chat optional within standalone

**Decision**: **A: Remove ChatCRDT, replace with CannedMessage** (2026-01-25)
**Rationale**: CannedMessage covers WearTAK's ACK/Emergency needs. Full ChatCRDT will be deprecated. hive-lite provides bounded, predictable memory usage.

---

### Q5: Timeline for Automerge backend?

**Context**: hive-persistence Phase 7 (Automerge+Iroh) is in progress.

**Questions**:
- When will it support custom document types (messages)?
- Can we test message sync before full completion?

**Status**: _____________

---

## Decision Log

| Date | Decision | Rationale | Participants |
|------|----------|-----------|--------------|
| 2026-01-25 | Document created | Initial analysis of chat sync bug | Kit, Claude |
| 2026-01-25 | **hive-lite as separate repo** | Leaf crate with no HIVE deps; reusable by eche-btle, hive-lora, future transports | Kit, Claude |
| 2026-01-25 | **CannedMessage primitive** | Predefined message codes (ACK, EMERGENCY, etc.) fit 256KB budget; no free-text chat in hive-lite | Kit, Claude |
| 2026-01-25 | **Emergency → CannedMessage** | Unify event types; Emergency becomes `CannedMessage::Emergency` | Kit, Claude |
| 2026-01-25 | **ChatCRDT deprecated** | Full chat doesn't fit hive-lite; will be removed or feature-gated in eche-btle | Kit, Claude |

---

## References

- [HIVE ADR-010: Transport Abstraction](../hive/docs/adr/010-transport-abstraction.md)
- [HIVE ADR-011: Automerge+Iroh Backend](../hive/docs/adr/011-automerge-iroh.md)
- [HIVE ADR-032: Transport Trait](../hive/docs/adr/032-transport-trait.md)
- [HIVE ADR-035: HIVE-Lite Embedded Nodes](../hive/docs/adr/035-hive-lite.md)
- [HIVE ADR-037: Differential Sync](../hive/docs/adr/037-differential-sync.md)
- [HIVE ADR-039: ECHE-BTLE Mesh Transport](../hive/docs/adr/039-eche-btle.md)
- [eche-btle Issue cc953d6: Delta Sync Integration](https://app.radicle.xyz/nodes/seed.radicle.garden/rad:z458mp9Um3AYNQQFMdHaNEUtmiohq/issues/cc953d6)

---

## Appendix A: Current eche-btle Document Format

For reference, the current `EcheDocument` structure that would be deprecated for HIVE integration:

```rust
pub struct EcheDocument {
    pub version: u32,
    pub node_id: NodeId,
    pub counter: GCounter,
    pub peripheral: Option<Peripheral>,
    pub emergency: Option<EmergencyEvent>,
    pub chat: Option<ChatCRDT>,
}
```

Wire format markers:
- `0xAA` - Unencrypted EcheDocument
- `0xAE` - Encrypted EcheDocument
- `0xAC` - Emergency section
- `0xAD` - Chat section
- `0xB2` - Delta document

---

## Appendix B: hive-lite Memory Budget

From ADR-035, the 256KB target allocation:

| Component | Budget | Notes |
|-----------|--------|-------|
| Network stack | 64KB | lwIP/smoltcp |
| CRDT state | 64KB | ~100 LWW registers |
| Gossip buffers | 32KB | 64 × 512-byte packets |
| Protocol state | 16KB | Peer table, routing |
| Application | 80KB | Sensor logic, display |

**Implication**: Full chat history doesn't fit in hive-lite budget.

---

## Appendix C: hive-lite Crate Architecture

### Decision: Separate Repository

hive-lite will be a **separate repository** (`hive-lite`), not a workspace member of eche-btle or hive. This enables:

1. **Reuse by multiple transports**: eche-btle, hive-lora (future), ESP-NOW
2. **Independent versioning**: Primitives may stabilize faster than transport implementations
3. **Clean dependency tree**: No circular dependencies with HIVE ecosystem

### Dependency Graph

```
                    ┌─────────────┐
                    │  hive-ffi   │
                    └──────┬──────┘
                           │
                    ┌──────▼──────┐
                    │hive-protocol│
                    └──────┬──────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
              ▼            ▼            ▼
        ┌──────────┐ ┌──────────┐ ┌──────────┐
        │hive-schema│ │eche-btle │ │hive-lora │
        └──────────┘ │(transport)│ │ (future) │
                     └─────┬─────┘ └────┬─────┘
                           │            │
                           │ optional   │ optional
                           │ dep        │ dep
                           ▼            ▼
                     ┌─────────────────────┐
                     │      hive-lite      │  ← LEAF (no upward deps)
                     │   (primitives)      │
                     └─────────────────────┘
```

**Key constraint**: hive-lite has **zero dependencies** on hive-schema, hive-protocol, or any HIVE crate. It is a leaf node in the dependency tree.

### hive-lite Contents

```rust
// hive-lite/src/lib.rs
#![no_std]  // no_std by default

pub mod node_id;       // NodeId (u32)
pub mod canned;        // CannedMessage enum and store
pub mod lww;           // LwwRegister<T>
pub mod counter;       // GCounter, PnCounter
pub mod wire;          // Binary encoding/decoding
pub mod event;         // LiteEvent (bounded event store)
```

### CannedMessage Definition

```rust
/// Predefined message codes for resource-constrained devices
/// Designed for button-based interaction (no keyboard input)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CannedMessage {
    // Acknowledgments (0x00-0x0F)
    Ack = 0x00,           // "Message received"
    AckWilco = 0x01,      // "Will comply"
    AckNegative = 0x02,   // "Cannot comply"

    // Status (0x10-0x1F)
    CheckIn = 0x10,       // "I'm here / still alive"
    Moving = 0x11,        // "En route"
    Holding = 0x12,       // "Stationary / waiting"
    OnStation = 0x13,     // "Arrived at position"

    // Alerts (0x20-0x2F)
    Emergency = 0x20,     // "Need immediate help" (replaces EmergencyEvent)
    Alert = 0x21,         // "Attention needed"
    AllClear = 0x22,      // "Situation resolved"

    // Requests (0x30-0x3F)
    NeedExtract = 0x30,   // "Request pickup"
    NeedSupport = 0x31,   // "Request assistance"
    NeedMedic = 0x32,     // "Medical emergency"

    // Reserved (0xF0-0xFF)
    Custom = 0xFF,        // For future/app-specific use
}

/// Bounded event store - LWW per (source_node, message_type)
/// Memory usage: max_peers × 8 event_types × 24 bytes ≈ 9.6KB for 50 peers
pub struct CannedMessageStore {
    messages: heapless::FnvIndexMap<(NodeId, CannedMessage), CannedMessageEvent, 400>,
}

pub struct CannedMessageEvent {
    pub message: CannedMessage,   // 1 byte
    pub source_node: NodeId,      // 4 bytes
    pub target_node: Option<NodeId>, // 5 bytes (1 + 4)
    pub timestamp: u64,           // 8 bytes
    pub sequence: u32,            // 4 bytes
}                                 // Total: ~24 bytes
```

### Wire Format

New marker byte `0xAF` for CannedMessage:

```
┌──────┬──────────┬──────────┬──────────┬───────────┬──────┐
│ 0xAF │ msg_code │ src_node │ tgt_node │ timestamp │ seq  │
│ 1B   │ 1B       │ 4B       │ 4B (opt) │ 8B        │ 4B   │
└──────┴──────────┴──────────┴──────────┴───────────┴──────┘
```

- Minimum: 14 bytes (no target)
- Maximum: 18 bytes (with target)

### eche-btle Integration

```toml
# eche-btle/Cargo.toml
[features]
default = ["std", "standalone"]
standalone = ["dep:hive-lite"]  # Include primitives
transport-only = []              # Pure byte transport

[dependencies]
hive-lite = { version = "0.1", optional = true }
```

### Translation Layer

hive-protocol (or apps) translates between hive-lite and HIVE-Full:

| hive-lite | HIVE-Full (Automerge) |
|-----------|----------------------|
| `CannedMessage::Ack` | Event in document: `{ type: "ack", from: ..., ts: ... }` |
| `CannedMessage::Emergency` | Alert document with severity=critical |
| `LwwRegister<Position>` | `cap.common.v1.Position` in Automerge doc |
| `GCounter` | Automerge counter |

The translation layer lives in **hive-protocol**, not hive-lite or eche-btle.

---

## Appendix D: Implementation Roadmap

### Phase 0: Create hive-lite (Immediate)

**Owner**: Core Team
**Target**: This week

- [ ] Create `hive-lite` repository
- [ ] Implement `NodeId` (u32 wrapper)
- [ ] Implement `CannedMessage` enum
- [ ] Implement `CannedMessageStore` (bounded LWW store)
- [ ] Implement `LwwRegister<T>`
- [ ] Implement `GCounter`
- [ ] Wire format encoding/decoding
- [ ] `no_std` compatible, optional `std` feature
- [ ] Basic tests

### Phase 1: Integrate hive-lite into eche-btle

**Owner**: eche-btle Team
**Target**: Following week
**Depends on**: Phase 0

- [ ] Add `hive-lite` as optional dependency
- [ ] Add `standalone` feature flag
- [ ] Migrate `GCounter` to use hive-lite's
- [ ] Replace `EmergencyEvent` with `CannedMessage::Emergency`
- [ ] Replace ACK handling with `CannedMessage::Ack`
- [ ] Deprecate `ChatCRDT` (feature-gate or remove)
- [ ] Update Android/Kotlin bindings for CannedMessage
- [ ] Wire format: add `0xAF` marker for CannedMessage

### Phase 2: Update ATAK Plugin

**Owner**: ATAK Plugin Team
**Target**: Following Phase 1
**Depends on**: Phase 1

- [ ] Update eche-btle dependency
- [ ] Replace chat-based ACK with CannedMessage ACK
- [ ] Update UI to show CannedMessage events
- [ ] Test with WearTAK devices

### Phase 3: Translation Layer (Future)

**Owner**: HIVE Protocol Team
**Target**: TBD
**Depends on**: Phase 1

- [ ] Define translation API in hive-protocol
- [ ] Map CannedMessage ↔ Automerge events
- [ ] Map LwwRegister ↔ Automerge fields
- [ ] Integration tests

### Phase 4: hive-lora Integration (Future)

**Owner**: TBD
**Target**: TBD
**Depends on**: Phase 0

- [ ] Create hive-lora transport crate
- [ ] Add hive-lite dependency
- [ ] Implement LoRa-specific wire format optimizations
