# ADR-003: Extensible Document Registry

**Status**: Draft
**Date**: 2026-02-01
**Context**: Enable external crates (hive-lite, app-layer) to register custom CRDT document types that sync through peat-btle's delta mechanism.

## Problem

Currently, peat-btle has hardcoded document types (`Peripheral`, `EmergencyEvent`, `ChatCRDT`) in `PeatDocument`. External crates like hive-lite define their own CRDT types (e.g., `CannedMessageEvent`) that need to sync across the mesh.

The current workaround uses app-layer relay (0xAF marker) which:
1. Bypasses CRDT delta sync, causing unnecessary re-transmissions
2. Requires separate deduplication logic
3. Creates broadcast storms when relay nodes re-encode after merge

## Goals

1. **Extensibility**: Allow external crates to register document types
2. **Unified sync path**: All registered types use the same delta mechanism
3. **no_std compatibility**: Work on embedded devices (256KB RAM budget)
4. **Type safety**: Compile-time guarantees where possible
5. **Backward compatibility**: Existing marker bytes continue to work

## Design

### Marker Byte Allocation

Reserved marker byte ranges:

| Range | Purpose |
|-------|---------|
| `0xAB-0xAF` | Built-in sections (Peripheral, Emergency, Chat, E2EE) |
| `0xB0-0xB2` | Control messages (KeyExchange, Relay, Delta) |
| `0xC0-0xCF` | **App-layer document types (new)** |
| `0xD0-0xDF` | Reserved for future use |

### DocumentType Trait

```rust
/// A registered document type that can be synced through the mesh.
///
/// Implementations must be deterministic - the same logical state
/// must always encode to the same bytes (or use document identity
/// for deduplication instead of content hash).
pub trait DocumentType: Clone + Send + Sync + 'static {
    /// Unique type identifier (marker byte in 0xC0-0xCF range).
    const TYPE_ID: u8;

    /// Human-readable type name for debugging.
    const TYPE_NAME: &'static str;

    /// Document identity for deduplication.
    ///
    /// Returns (source_node, timestamp) tuple that uniquely identifies
    /// this document instance. Used instead of content hash because
    /// CRDT merge may change byte ordering.
    fn identity(&self) -> (u32, u64);

    /// Encode to wire format.
    ///
    /// Format: [type_id: 1B][length: 2B LE][payload: variable]
    fn encode(&self) -> Vec<u8>;

    /// Decode from wire format (after type_id byte).
    ///
    /// Input is the payload after the 3-byte header.
    fn decode(data: &[u8]) -> Option<Self> where Self: Sized;

    /// Merge with another instance using CRDT semantics.
    ///
    /// Returns true if our state changed.
    fn merge(&mut self, other: &Self) -> bool;

    /// Convert to a delta operation for the sync protocol.
    ///
    /// Returns None if this type doesn't support delta sync
    /// (will use full-state sync instead).
    fn to_delta_op(&self) -> Option<AppOperation> {
        None  // Default: no delta support
    }

    /// Apply a delta operation to this document.
    fn apply_delta_op(&mut self, _op: &AppOperation) -> bool {
        false  // Default: no delta support
    }
}
```

### AppOperation for Delta Sync

```rust
/// App-layer delta operation.
///
/// Extends the built-in Operation enum for registered document types.
pub struct AppOperation {
    /// Document type ID (0xC0-0xCF)
    pub type_id: u8,

    /// Operation code (type-specific)
    pub op_code: u8,

    /// Source node that created this operation
    pub source_node: u32,

    /// Timestamp of the operation
    pub timestamp: u64,

    /// Operation payload
    pub payload: Vec<u8>,
}

impl AppOperation {
    /// Wire format:
    /// [0x10 + (type_id - 0xC0)]: 1B - op type in 0x10-0x1F range
    /// [op_code]: 1B
    /// [source_node]: 4B LE
    /// [timestamp]: 8B LE
    /// [payload_len]: 2B LE
    /// [payload]: variable
    pub fn encode(&self) -> Vec<u8>;
    pub fn decode(data: &[u8]) -> Option<(Self, usize)>;
}
```

### DocumentRegistry

```rust
/// Registry for document type handlers.
///
/// Thread-safe, supports dynamic registration at runtime.
pub struct DocumentRegistry {
    handlers: RwLock<HashMap<u8, Box<dyn DocumentHandler>>>,
}

/// Type-erased handler for document operations.
trait DocumentHandler: Send + Sync {
    fn type_name(&self) -> &'static str;
    fn decode(&self, data: &[u8]) -> Option<Box<dyn Any + Send>>;
    fn merge(&self, doc: &mut dyn Any, other: &dyn Any) -> bool;
    fn encode(&self, doc: &dyn Any) -> Vec<u8>;
    fn identity(&self, doc: &dyn Any) -> (u32, u64);
}

impl DocumentRegistry {
    /// Register a document type handler.
    ///
    /// # Panics
    /// Panics if type_id is outside 0xC0-0xCF range or already registered.
    pub fn register<T: DocumentType>(&self);

    /// Check if a type is registered.
    pub fn is_registered(&self, type_id: u8) -> bool;

    /// Get type name for debugging.
    pub fn type_name(&self, type_id: u8) -> Option<&'static str>;

    /// Decode a document section.
    pub fn decode(&self, type_id: u8, data: &[u8]) -> Option<Box<dyn Any + Send>>;

    /// Merge two documents of the same type.
    pub fn merge(&self, type_id: u8, doc: &mut dyn Any, other: &dyn Any) -> bool;
}
```

### Integration with PeatMesh

```rust
impl PeatMesh {
    /// Get the document registry for registering app-layer types.
    pub fn document_registry(&self) -> &DocumentRegistry;

    /// Store an app-layer document for sync.
    ///
    /// The document will be synced to peers using delta operations
    /// if the type supports it, otherwise full-state sync.
    pub fn store_document<T: DocumentType>(&self, doc: T);

    /// Get all documents of a registered type.
    pub fn get_documents<T: DocumentType>(&self) -> Vec<T>;

    /// Subscribe to document updates for a type.
    pub fn on_document_update<T: DocumentType, F>(&self, callback: F)
    where
        F: Fn(&T, bool /* is_new */) + Send + Sync + 'static;
}
```

### Wire Format for App-Layer Sections

When an app-layer document is included in a PeatDocument:

```text
[marker: 1B]     - 0xC0-0xCF (type ID)
[flags: 1B]      - bit 0: has_delta, bits 1-7: reserved
[length: 2B LE]  - payload length
[payload: var]   - type-specific encoded data
```

When included in a DeltaDocument operation:

```text
[op_type: 1B]    - 0x10 + (type_id - 0xC0), so 0x10-0x1F range
[op_code: 1B]    - type-specific operation code
[source: 4B LE]  - source node
[timestamp: 8B]  - operation timestamp
[len: 2B LE]     - payload length
[payload: var]   - type-specific delta payload
```

## CannedMessage Integration (hive-lite)

### Type Registration

```rust
// In hive-lite Android bindings
impl DocumentType for CannedMessageEvent {
    const TYPE_ID: u8 = 0xC0;  // First app-layer slot
    const TYPE_NAME: &'static str = "CannedMessage";

    fn identity(&self) -> (u32, u64) {
        (self.source_node, self.timestamp)
    }

    fn encode(&self) -> Vec<u8> {
        self.to_bytes()  // Existing wire format
    }

    fn decode(data: &[u8]) -> Option<Self> {
        Self::from_bytes(data)
    }

    fn merge(&mut self, other: &Self) -> bool {
        // Document identity must match
        if self.identity() != other.identity() {
            return false;
        }
        // Merge ACK sets (OR-set semantics)
        let old_ack_count = self.acks.len();
        for ack in &other.acks {
            if !self.acks.contains(ack) {
                self.acks.push(*ack);
            }
        }
        self.acks.len() > old_ack_count
    }

    fn to_delta_op(&self) -> Option<AppOperation> {
        Some(AppOperation {
            type_id: Self::TYPE_ID,
            op_code: 0x01,  // ACK_UPDATE
            source_node: self.source_node,
            timestamp: self.timestamp,
            payload: self.encode_acks_only(),
        })
    }
}
```

### Usage in WearTAK/ATAK

```kotlin
// Register CannedMessage type on mesh init
hiveMesh.documentRegistry.register(CannedMessageEvent::class)

// Send a canned message
val msg = CannedMessageEvent.create(
    code = CannedMessage.ACK,
    sourceNode = myNodeId,
    targetNode = targetNodeId
)
hiveMesh.storeDocument(msg)  // Syncs via delta

// Receive canned messages
hiveMesh.onDocumentUpdate<CannedMessageEvent> { event, isNew ->
    if (isNew) {
        showCannedMessage(event)
    } else {
        updateAckStatus(event)  // ACKs merged
    }
}
```

## Migration Path

### Phase 1: Add Registry Infrastructure
- Add `DocumentType` trait and `DocumentRegistry` to peat-btle
- Add `AppOperation` to delta_document.rs
- Expose registry via `PeatMesh::document_registry()`

### Phase 2: Wire Format Support
- Update document decoder to handle 0xC0-0xCF markers
- Update delta document decoder to handle 0x10-0x1F op types
- Add routing for app-layer operations in mesh dispatch

### Phase 3: hive-lite Integration
- Implement `DocumentType` for `CannedMessageEvent`
- Update Android bindings to use registry
- Remove legacy 0xAF app-layer relay path

### Phase 4: Deprecate Legacy Path
- Mark `relayToOtherPeers` as deprecated for CannedMessage
- Add migration warnings
- Remove in next major version

## Alternatives Considered

### A. Hardcode CannedMessage in peat-btle
**Rejected**: Creates circular dependency, violates peat-btle's standalone design.

### B. Use Protobuf/Cap'n Proto for extensibility
**Rejected**: Adds dependencies, increases code size for embedded targets.

### C. Keep app-layer relay but fix deduplication
**Rejected**: Doesn't solve the fundamental issue of bypassing CRDT sync.

## Security Considerations

- Type IDs are not authenticated; malicious nodes could send garbage
- Registry validates type_id is in 0xC0-0xCF range
- Individual DocumentType implementations must validate payloads
- Consider rate-limiting unknown type IDs to prevent DoS

## Testing Strategy

1. Unit tests for DocumentType encode/decode/merge
2. Integration tests for registry registration
3. Cross-platform tests (Rust, Android, iOS)
4. Fuzz testing for wire format parsers
5. Stress test with many registered types

## References

- [hive-lite CLAUDE.md](../../../hive-lite/CLAUDE.md) - CannedMessage wire format
- [ADR-001: Trust Architecture](./ADR-001-trust-architecture.md)
- [03-hive-mesh-app-architecture-v2.md](./03-hive-mesh-app-architecture-v2.md) - Original app layer plans
