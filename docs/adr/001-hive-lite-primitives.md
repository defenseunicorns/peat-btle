# ADR-001: hive-lite Primitives Crate

**Status**: Accepted
**Date**: 2026-01-25
**Authors**: Kit Plummer, Codex
**Deciders**: HIVE Core Team, hive-btle Team

## Context

hive-btle currently conflates two responsibilities:
1. **BLE Transport** - Moving bytes over Bluetooth Low Energy
2. **Document/CRDT Logic** - Defining and merging application data structures

This has caused bugs (chat not syncing reliably) and architectural confusion. Additionally, the current `ChatCRDT` implementation doesn't fit within the 256KB RAM budget targeted for resource-constrained devices (ESP32, WearTAK on Samsung watches).

### The Problem

The Kotlin delta sync builds operations that don't include chat, causing 90% of syncs to miss chat data. More fundamentally, full chat history is inappropriate for hive-lite's memory constraints.

### Use Cases

1. **WearTAK**: Needs ACK/Emergency signaling, not full chat. Button-based interaction (no keyboard).
2. **ESP32 Sensors**: Need counters, position, health - no text messaging.
3. **hive-lora (future)**: Will need same primitives over different transport.

## Decision

Create **hive-lite** as a **separate repository** containing lightweight CRDT primitives suitable for resource-constrained devices.

### Key Design Decisions

1. **Separate Repository**: Not a workspace member of hive-btle or hive. Enables reuse by multiple transports (hive-btle, hive-lora, ESP-NOW).

2. **Leaf Crate**: Zero dependencies on hive-schema, hive-protocol, or any HIVE crate. Prevents circular dependencies.

3. **`no_std` by Default**: Works on embedded targets without modification.

4. **CannedMessage Primitive**: Predefined message codes replace free-text chat. Fits memory budget, works with button-based UIs.

5. **Bounded Storage**: All data structures have fixed maximum sizes. No unbounded growth.

### Dependency Structure

```
hive-protocol
    │
    ├── hive-btle (transport-only mode)
    │       │
    │       └── hive-lite (standalone feature)
    │
    └── hive-lite (for translation layer)

hive-lora (future)
    │
    └── hive-lite
```

hive-lite is always at the bottom - no upward dependencies.

## hive-lite Contents

### Primitives

| Type | Purpose | Memory |
|------|---------|--------|
| `NodeId` | 32-bit node identifier | 4 bytes |
| `CannedMessage` | Predefined message codes | 1 byte |
| `CannedMessageStore` | Bounded LWW event store | ~10KB for 50 peers |
| `LwwRegister<T>` | Last-writer-wins register | sizeof(T) + 12 bytes |
| `GCounter` | Grow-only counter | 4 bytes per peer |

### CannedMessage Codes

```rust
pub enum CannedMessage {
    // Acknowledgments (0x00-0x0F)
    Ack = 0x00,
    AckWilco = 0x01,
    AckNegative = 0x02,

    // Status (0x10-0x1F)
    CheckIn = 0x10,
    Moving = 0x11,
    Holding = 0x12,
    OnStation = 0x13,

    // Alerts (0x20-0x2F)
    Emergency = 0x20,
    Alert = 0x21,
    AllClear = 0x22,

    // Requests (0x30-0x3F)
    NeedExtract = 0x30,
    NeedSupport = 0x31,
    NeedMedic = 0x32,
}
```

### Wire Format

New marker `0xAF` for CannedMessage events:

```
┌──────┬──────────┬──────────┬──────────┬───────────┬──────┐
│ 0xAF │ msg_code │ src_node │ tgt_node │ timestamp │ seq  │
│ 1B   │ 1B       │ 4B       │ 4B (opt) │ 8B        │ 4B   │
└──────┴──────────┴──────────┴──────────┴───────────┴──────┘
```

## Consequences

### Positive

1. **Clean separation**: Transport (hive-btle) vs primitives (hive-lite) vs protocol (hive-protocol)
2. **Reusable**: hive-lora can use same primitives
3. **Bounded memory**: Fits 256KB budget for ESP32/WearTAK
4. **no_std**: Works on embedded without std library
5. **Simpler debugging**: Fewer moving parts, clearer data flow

### Negative

1. **Migration work**: Existing hive-btle users need to update
2. **New repo**: Additional maintenance overhead
3. **ChatCRDT deprecated**: Full chat moves to HIVE-Full only

### Neutral

1. **Translation layer needed**: hive-protocol must translate between hive-lite and Automerge
2. **Feature flags**: hive-btle needs `standalone` vs `transport-only` modes

## Implementation

### Phase 0: Create hive-lite
- New repository at `hive-lite`
- Implement NodeId, CannedMessage, LwwRegister, GCounter
- Wire format encoding
- `no_std` compatible

### Phase 1: Integrate into hive-btle
- Add hive-lite as optional dependency
- `standalone` feature flag
- Replace EmergencyEvent with CannedMessage::Emergency
- Replace chat-based ACK with CannedMessage::Ack
- Deprecate ChatCRDT

### Phase 2: Update consumers
- ATAK plugin uses CannedMessage for ACK/Emergency
- WearTAK integration testing

### Phase 3: Translation layer
- hive-protocol maps hive-lite ↔ Automerge

## References

- [ARCHITECTURE_REFACTORING.md](../ARCHITECTURE_REFACTORING.md) - Full analysis
- [HIVE ADR-035: HIVE-Lite Embedded Nodes](https://github.com/kitplummer/hive/blob/main/docs/adr/035-hive-lite-embedded-nodes.md)
- [HIVE ADR-039: HIVE-BTLE Mesh Transport](https://github.com/kitplummer/hive/blob/main/docs/adr/039-hive-btle-mesh-transport.md)
