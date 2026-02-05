# Range Test Node - Development Notes

## Current State (2026-02-05)

### What Works
- **Linux BLE Advertising**: Successfully advertising as `HIVE-BA5E0001` with GATT service
- **GATT Server**: 5 characteristics registered (node_info, sync_state, sync_data, command, status)
- **Encrypted Documents**: Initial sync_state populated with 81-byte encrypted document
- **Discovery**: Successfully discovering WearTAK watches (e.g., `HIVE-C8E32F88`)
- **Mesh Integration**: Using same WEARTAK genesis as watches (mesh ID: 29C916FA)

### What Doesn't Work (Linux/BlueZ)
- **Outbound Connections**: BlueZ consistently fails with `le-connection-abort-by-local`
  - Tried: Stopping scan before connect, retries, address type hints
  - Root cause unclear - may be BlueZ 5.64 limitation or adapter-specific issue
- **Inbound Connections**: Watches discover us but don't connect
  - Passive mode implemented but watches not initiating GATT connections
  - May need mesh ID in advertisement (currently omitted to fit 31-byte limit)

### Files Changed
- `src/platform/linux/adapter.rs`: Added helper methods for device access, discovery control, adapter alias
- `examples/range_test_node.rs`: Range test orchestrator with active/passive connection modes

## Next Steps

### macOS Range Test Orchestrator (Priority)
Move the range test orchestrator to macOS where CoreBluetooth has better BLE support:
1. Create `examples/range_test_node_macos.rs` or adapt existing for cross-platform
2. Use the Apple adapter implementation (`src/platform/apple/`)
3. Test both active (connect to watches) and passive (accept connections) modes

### Linux BLE Investigation (Background)
Continue debugging BlueZ issues:
1. Try on Raspberry Pi (different BlueZ version/config)
2. Investigate btleplug as alternative to bluer crate
3. Check if mesh ID in advertisement is required for watch auto-connect

## Technical Details

### Advertisement Format
Current Linux advertisement:
- Service UUID: 0xF47A (16-bit alias)
- Service Data: [nodeId: 4 bytes BE]
- Local Name: HIVE-BA5E0001
- Total: ~30 bytes (fits 31-byte legacy limit)

Note: Mesh ID omitted from service data. Android code shows `matchesMesh()` returns true for null meshId (legacy compatibility), so this should work.

### WearTAK Genesis
```rust
const WEARTAK_GENESIS_BYTES: &[u8] = &[
    0x07, 0x00, 0x57, 0x45, 0x41, 0x52, 0x54, 0x41, 0x4B, ...
];
// Mesh ID: 29C916FA
// Mesh Name: WEARTAK
```

### Connection Flow (Expected)
1. Linux advertises as HIVE-BA5E0001 with GATT service
2. Watch scans, sees HIVE-BA5E0001, recognizes as HIVE device
3. Watch connects as GATT client, reads sync_state
4. Watch writes to sync_data with its document
5. Linux receives via sync_data_callback, processes with mesh

### Error Reference
- `le-connection-abort-by-local`: BlueZ internally aborting connection attempt
- `ConnectDevice method doesn't exist`: BlueZ 5.64 doesn't support adapter-level connect
