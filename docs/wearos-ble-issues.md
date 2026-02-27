# WearOS BLE Issues and Workarounds

## Overview

WearOS aggressively manages power, which causes several issues with continuous BLE operations required for mesh networking. This document captures the issues we've encountered and the workarounds implemented in peat-btle.

---

## Issue 1: Silent BLE Scan Termination

### Symptoms
- BLE scanning starts successfully
- After 5-10 minutes, scan callbacks stop firing
- No error is reported - `onScanFailed()` is never called
- Scanning appears to be running (`isScanning = true`) but no results are received
- Peers that walk out of range and return are never rediscovered

### Root Cause
WearOS power management silently terminates BLE scans to conserve battery. This happens even when:
- The app is running as a foreground service
- The scan was started with `SCAN_MODE_LOW_LATENCY`
- The screen is on

### Evidence from Logs
```
02-05 15:25:14.383 I PeatBtle: Started scanning for Peat devices (no UUID filter)
02-05 15:30:14.109 D PeatBtle.ScanCallback: Scan result: 68:6D:2F:50:CF:30 (WEAROS-3301)
# ... no more scan results after this point ...
# 25+ minutes of silence, no onScanFailed callback
```

### Workaround
Periodically restart the BLE scan every 2 minutes:

```kotlin
private val SCAN_RESTART_INTERVAL_MS = 120000L // 2 minutes

private val scanRestartRunnable = object : Runnable {
    override fun run() {
        if (isMeshRunning && isScanning) {
            Log.i(TAG, "[SCAN-RESTART] Restarting BLE scan (WearOS workaround)")
            stopScan()
            handler.postDelayed({
                if (isMeshRunning) {
                    startScan(discoveryCallback)
                }
            }, 500)
        }
        if (isMeshRunning) {
            handler.postDelayed(this, SCAN_RESTART_INTERVAL_MS)
        }
    }
}
```

### Log Indicator
When the workaround triggers, you'll see:
```
[SCAN-RESTART] Restarting BLE scan (WearOS workaround)
```

---

## Issue 2: Wireless Debugging Disconnections

### Symptoms
- ADB WiFi connections to watches drop frequently
- Port numbers change on each reconnection
- Happens even when watches are on chargers

### Root Cause
WearOS aggressively puts WiFi to sleep and terminates debugging connections to save power.

### Workaround
- Enable "Stay awake while charging" in Developer Options
- Be prepared to reconnect with new port numbers
- Keep watches on charger during development

---

## Issue 3: BLE Address Rotation

### Symptoms
- A previously connected peer appears as a new device
- Connection attempts to old address fail
- Peer tracking becomes inconsistent

### Root Cause
WearOS rotates BLE MAC addresses for privacy. The random address changes periodically.

### Workaround
peat-btle tracks peers by `nodeId` (derived from service data) rather than BLE address:

```kotlin
// Update address when peer is rediscovered with new address
if (oldAddress != device.address) {
    addressToNodeId.remove(oldAddress)
    existingPeer.address = device.address
    addressToNodeId[device.address] = peerNodeId
}
```

---

## Issue 4: Connection vs Discovery Range Mismatch

### Symptoms
- Can see peer advertisements at longer range
- Connection attempts fail until devices are very close (~2 meters)
- Reconnection only succeeds at close range

### Root Cause
BLE connections require stronger signal than advertisements:
- Advertisements can be received at -90 dBm
- Connections typically require -70 to -80 dBm or stronger
- WearOS may have additional power-saving restrictions on connection attempts

### Workarounds

1. **Use `autoConnect=true` for reconnection attempts:**
```kotlin
// autoConnect tells Android to queue the connection and complete
// it when signal is adequate, rather than failing immediately
device.connectGatt(context, true, callback, BluetoothDevice.TRANSPORT_LE)
```

2. **Clean up stale pending connections:**
```kotlin
// Check if actually connected (not just pending)
if (connections.containsKey(peer.address) && peer.isConnected) {
    return // Already connected
}

// If we have a pending/stale GATT but not actually connected, close it
if (connections.containsKey(peer.address) && !peer.isConnected) {
    disconnect(peer.address)
}
```

---

## Recommended WearOS Developer Settings

For best results during development and testing:

1. **Developer Options → Stay awake while charging**: ON
2. **Developer Options → Wireless debugging**: Enabled
3. **Keep watches on charger** during extended testing
4. **Test reconnection scenarios** by physically moving devices apart and back

---

## Configuration Constants

Current values tuned for WearOS reliability:

```kotlin
PEER_TIMEOUT_MS = 120000L         // 2 min - keep disconnected peers longer
CONNECTED_PEER_TIMEOUT_MS = 300000L // 5 min - handle stale connections
RECONNECT_INTERVAL_MS = 3000L     // 3s - faster reconnection polling
RECONNECT_BASE_DELAY_MS = 1000L   // 1s - quick first retry
RECONNECT_MAX_DELAY_MS = 15000L   // 15s - cap on backoff
RECONNECT_MAX_ATTEMPTS = 20       // More attempts before giving up
SCAN_RESTART_INTERVAL_MS = 120000L // 2 min - restart scan periodically
```

---

## Future Improvements

1. **Investigate WearOS Companion App permissions** - May provide less restricted BLE access
2. **Test with `SCAN_MODE_LOW_POWER`** - May be less likely to be killed (at cost of latency)
3. **Implement scan result caching** - Remember recent peers even if scan is killed
4. **Add connection retry with exponential backoff on GATT failure**

---

*Last updated: 2026-02-05*
