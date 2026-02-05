# Range Test Node - Development Notes

## Current State (2026-02-05)

### What Works (macOS - PRIMARY)
- **CoreBluetooth Adapter**: Successfully initialized (waits for PoweredOn state)
- **BLE Advertising**: Advertising as `HIVE-BA5E0001` with HIVE service UUID
- **GATT Server**: 5 characteristics registered (node_info, sync_state, sync_data, command, status)
- **BLE Discovery**: Working! Discovers all nearby BLE devices
- **NSRunLoop Integration**: Pumps run loop to deliver CoreBluetooth callbacks
- **Service UUID Filtering**: Supports both 128-bit and 16-bit expanded forms
- **Self-Discovery Filtering**: Ignores own advertisements
- **Mesh Integration**: Using same WEARTAK genesis as watches (mesh ID: 29C916FA)
- **GATT Client Connections**: Full outbound connection support to discovered watches
- **Service/Characteristic Discovery**: Discovers HIVE service (F47AC10B) and characteristics
- **Sync Data Read**: Reads sync_data (F47A0003) from connected watches
- **Document Decryption**: Decrypts received documents via HiveMesh with shared encryption key
- **Data Extraction**: Extracts node_id, callsign, location, battery, heart rate, emergency status
- **Bidirectional Sync**: Writes our sync document back to watches after receiving theirs

### Discovered Devices (Test Results 2026-02-05)
- `WEAROS-4059`, `WEAROS-3301` - WearOS watches visible but not advertising as HIVE
- Various other BLE devices (TV, soundbar, HomePods, etc.)
- Self: `HIVE-BA5E0` (name truncated by CoreBluetooth)

**Note**: WearTAK watches only advertise HIVE service UUID when actively syncing. To test:
1. Open WearTAK app on watch
2. Trigger a sync or ensure BLE advertising is enabled
3. Watch should then appear as `HIVE_WEARTAK-xxxx`

### What Works (Linux)
- **Linux BLE Advertising**: Successfully advertising as `HIVE-BA5E0001` with GATT service
- **GATT Server**: 5 characteristics registered
- **Encrypted Documents**: Initial sync_state populated with 81-byte encrypted document
- **Discovery**: Successfully discovering WearTAK watches (e.g., `HIVE-C8E32F88`)

### What Doesn't Work (Linux/BlueZ)
- **Outbound Connections**: BlueZ consistently fails with `le-connection-abort-by-local`
  - Tried: Stopping scan before connect, retries, address type hints
  - Root cause unclear - may be BlueZ 5.64 limitation or adapter-specific issue
- **Inbound Connections**: Watches discover us but don't connect
  - Passive mode implemented but watches not initiating GATT connections
  - May need mesh ID in advertisement (currently omitted to fit 31-byte limit)

### Files Changed
- `src/platform/linux/adapter.rs`: Added helper methods for device access, discovery control, adapter alias, MTU tracking
- `src/platform/linux/connection.rs`: Added MTU discovery via GATT operations, better default MTU (185 bytes)
- `src/platform/apple/adapter.rs`: Fixed scan filter to use HIVE service UUID
- `examples/range_test_node.rs`: Range test orchestrator with active/passive connection modes (Linux)
- `examples/range_test_node_macos.rs`: Range test orchestrator for macOS using CoreBluetooth

## macOS Range Test Orchestrator (DONE)

The macOS range test node is implemented and working:
- **File**: `examples/range_test_node_macos.rs`
- **Build**: `cargo build --features macos --example range_test_node_macos`
- **Run**: `cargo run --features macos --example range_test_node_macos`

### What Works (macOS)
- CoreBluetooth adapter initialization
- GATT service registration with 5 characteristics
- Advertising as `HIVE-BA5E0001` with HIVE service UUID
- Scanning filtered by HIVE service UUID
- Discovery callback for found devices
- Periodic status updates
- **Full GATT Client (NEW)**:
  - Connect to discovered HIVE watches via `connect_by_identifier()`
  - Discover services and characteristics
  - Read sync_data from watches (F47A0003)
  - Decrypt documents via `mesh.on_ble_data_received_anonymous()`
  - Extract: node_id, callsign, lat/lon, battery%, heart rate, emergency
  - Write our sync document back for bidirectional sync

### Usage
```bash
# Basic run
cargo run --features macos --example range_test_node_macos

# With custom callsign and output file
cargo run --features macos --example range_test_node_macos -- --callsign RANGER --output test.log
```

### MTU Implementation (2026-02-05)
Implemented proper MTU handling to match Android behavior:

**Server-side (when watches connect to us):**
- GATT callbacks now capture MTU from `CharacteristicReadRequest.mtu` and `CharacteristicWriteRequest.mtu`
- Per-peer MTU tracked in `GattState.peer_mtu` HashMap
- MTU logged in GATT operations: "GATT read sync_state from XX:XX:XX:XX: 81 bytes (MTU=185)"
- Query via `adapter.get_peer_mtu(&address)` or `adapter.get_all_peer_mtus()`

**Client-side (when we connect to watches):**
- Default MTU increased from 23 to 185 bytes (matches WearTAK's request)
- Added `connection.discover_mtu()` to get actual negotiated value via `AcquireWrite`/`AcquireNotify`

**Constants:**
- `DEFAULT_BLE_MTU = 185` - Conservative default for BLE 4.2+ devices
- `MIN_BLE_MTU = 23` - ATT_MTU_MIN per Bluetooth spec

### Write Queue Implementation (2026-02-05)
Implemented per-connection write queue to serialize BLE writes (BLE only allows one pending write per connection):

**Key components:**
- `WriteQueueState` - Contains `VecDeque<QueuedWrite>` and `write_in_progress` flag
- `QueuedWrite` - Holds service UUID, char UUID, data, and completion oneshot sender

**Methods added to BluerConnection:**
- `write_characteristic_queued(service_uuid, char_uuid, data)` - Safe concurrent writes via queue
- `process_write_queue()` - Internal method to process queue items serially
- `write_queue_depth()` - Check pending write count (for backpressure monitoring)
- `write_in_progress()` - Check if a write is currently executing
- `clear_write_queue()` - Cancel all pending writes (called on disconnect)

**Usage:**
```rust
// Safe for concurrent calls - writes are serialized automatically
connection.write_characteristic_queued(service_uuid, char_uuid, &data).await?;

// Check queue depth for backpressure
if connection.write_queue_depth().await > 10 {
    log::warn!("Write queue backing up");
}
```

**Implementation notes:**
- Uses tokio::sync::Mutex for queue synchronization
- Each queued write gets a oneshot channel for completion notification
- Queue is cleared on disconnect, pending writes receive error
- Direct `write_characteristic()` still available but warns about concurrent use

### Auto-Reconnection Implementation (2026-02-05)
Implemented `ReconnectionManager` for automatic reconnection with exponential backoff:

**Configuration (`ReconnectionConfig`):**
- `base_delay` - Initial delay (default: 2 seconds)
- `max_delay` - Maximum delay cap (default: 60 seconds)
- `max_attempts` - Give up after N attempts (default: 10)
- `check_interval` - How often to check for peers to reconnect (default: 5 seconds)

**Key methods:**
- `track_disconnection(address)` - Start tracking a peer for reconnection
- `get_peers_to_reconnect()` - Get list of peers ready for attempt
- `record_attempt(address)` - Record that an attempt was made
- `on_connection_success(address)` - Clear tracking on successful reconnect
- `get_status(address)` - Check status (Ready, Waiting, Exhausted, NotTracked)

**Backoff formula:**
```
delay = min(base_delay * 2^attempts, max_delay)
```
With defaults: 2s, 4s, 8s, 16s, 32s, 60s, 60s, 60s, 60s, 60s (then exhausted)

**Usage:**
```rust
let mut manager = ReconnectionManager::with_defaults();

// On disconnect
manager.track_disconnection(peer_address.clone());

// Periodic check (every 5 seconds)
for peer in manager.get_peers_to_reconnect() {
    manager.record_attempt(&peer);
    if try_connect(&peer).await.is_ok() {
        manager.on_connection_success(&peer);
    }
}
```

### Peer Lifetime Management Implementation (2026-02-05)
Implemented `PeerLifetimeManager` for stale peer cleanup:

**Configuration (`PeerLifetimeConfig`):**
- `disconnected_timeout` - Remove disconnected peers after this (default: 30 seconds)
- `connected_timeout` - Remove "connected" peers with no activity (default: 60 seconds)
- `cleanup_interval` - How often to check for stale peers (default: 10 seconds)

**Key methods:**
- `on_peer_activity(address, connected)` - Update last seen time
- `on_peer_disconnected(address)` - Mark peer as disconnected (doesn't update last_seen)
- `get_stale_peers()` - Get list of stale peers with reasons
- `cleanup_stale_peers()` - Remove and return stale peers
- `stats()` - Get counts of connected/disconnected peers

**Stale detection:**
- Disconnected peers: stale after `disconnected_timeout` since last activity
- Connected peers: stale after `connected_timeout` since last activity (handles ghost connections)

**Usage:**
```rust
let mut manager = PeerLifetimeManager::with_defaults();

// On discovery/connection/data received
manager.on_peer_activity(&address, is_connected);

// On disconnect (note: doesn't update last_seen intentionally)
manager.on_peer_disconnected(&address);

// Periodic cleanup (every 10 seconds)
for stale in manager.cleanup_stale_peers() {
    log::info!("Removing stale peer {}: {:?}", stale.address, stale.reason);
    // Clean up your resources for this peer
}
```

### BLE Address Rotation Implementation (2026-02-05)
Implemented `AddressRotationHandler` for WearOS address rotation handling:

**The problem:**
WearOS devices rotate their BLE MAC addresses for privacy. The same device can appear with different addresses over time, causing duplicate peers.

**The solution:**
Use device name (which is stable) as a secondary key for identifying devices:

**Device patterns detected:**
- `WT-WEAROS-*` - WearTAK on WearOS (rotates addresses)
- `WEAROS-*` - Generic WearOS device (rotates addresses)
- `HIVE_*` / `HIVE-*` - HIVE mesh devices

**Key methods:**
- `register_device(name, address, node_id)` - Register a new device
- `on_device_discovered(name, address)` - Handle discovery with rotation detection
- `lookup_by_name(name)` / `lookup_by_address(address)` - Find known devices
- `update_address(name, new_address)` - Update after rotation detected
- `remove_device(node_id)` - Clean up all mappings

**Usage:**
```rust
let mut handler = AddressRotationHandler::new();

// On device discovery
if let Some(result) = handler.on_device_discovered(&name, &address) {
    // Known device
    if result.address_changed {
        log::info!("Address rotated: {} -> {}",
            result.previous_address.unwrap(), result.current_address);
        // Update your connection to use new address
    }
    // Use result.node_id for the existing peer
} else {
    // New device - register it
    let node_id = /* extract from advertisement */;
    handler.register_device(&name, &address, node_id);
}
```

**Helper functions:**
- `detect_device_pattern(name)` - Returns `DevicePattern::WearTak/WearOs/Hive/Unknown`
- `is_weartak_device(name)` - Quick check for WearTAK/WearOS
- `normalize_weartak_name(name)` - Strips "WT-" prefix for consistency

## Next Steps

### macOS - COMPLETE
The macOS GATT client is fully operational:
- Discovers watches advertising HIVE service UUID
- Connects, discovers services/characteristics, reads sync_data
- Decrypts via HiveMesh, extracts all CRDT data
- Writes back for bidirectional sync

**Note**: WearTAK watches expose sync_data (F47A0003) but NOT node_info (F47A0001). Node ID is extracted from the decrypted document header instead.

### Production Enhancements (Optional)
1. **Auto-reconnection**: Handle disconnections gracefully with exponential backoff
2. **Multiple simultaneous connections**: Connect to several watches in parallel
3. **RSSI tracking per connection**: Monitor signal strength over time
4. **Notification subscription**: Subscribe to sync_data notifications for push updates

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
