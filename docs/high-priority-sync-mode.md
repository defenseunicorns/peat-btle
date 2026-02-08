# High-Priority Sync Mode

## Problem Statement

WearOS aggressively manages power, which causes critical issues for mesh networking:

1. **Silent BLE scan termination** - Scans stop after ~5 minutes with no error callback
2. **Connection supervision timeouts** - Long delays before detecting peer disconnection
3. **Background service throttling** - Reduced CPU time for background operations
4. **WiFi/BLE coexistence issues** - Radio contention causes missed packets

For tactical/emergency use cases, reliable peer communication is more important than battery life.

---

## Solution: High-Priority Sync Mode

A configurable mode that trades battery life for communication reliability.

### Behavior Changes

| Setting | Normal Mode | High-Priority Mode |
|---------|-------------|-------------------|
| Scan restart interval | 2 minutes | 30 seconds |
| Keep-alive ping interval | 10 seconds | 3 seconds |
| Reconnection polling | 3 seconds | 1 second |
| Connection interval request | Default (~30ms) | Minimum (7.5ms) |
| Wake lock | None | Partial wake lock |
| Battery optimization | System default | Request exemption |
| Sync interval | 3 seconds | 1 second |

### Expected Battery Impact

- **Normal mode**: ~8-12 hours typical use
- **High-priority mode**: ~4-6 hours typical use (estimated 40-50% reduction)

---

## Systems Touched

### 1. hive-btle Library (`HiveBtle.kt`)

New configuration class:

```kotlin
data class HiveMeshConfig(
    val highPriorityMode: Boolean = false,
    val scanRestartIntervalMs: Long = if (highPriorityMode) 30000 else 120000,
    val keepAliveIntervalMs: Long = if (highPriorityMode) 3000 else 10000,
    val reconnectIntervalMs: Long = if (highPriorityMode) 1000 else 3000,
    val syncIntervalMs: Long = if (highPriorityMode) 1000 else 3000,
    val requestConnectionInterval: Int? = if (highPriorityMode) 6 else null, // 7.5ms units
    val useWakeLock: Boolean = highPriorityMode,
    val requestBatteryExemption: Boolean = highPriorityMode
)
```

New methods:

```kotlin
fun setHighPriorityMode(enabled: Boolean)
fun isHighPriorityMode(): Boolean
fun requestBatteryOptimizationExemption(activity: Activity)
```

### 2. WearTAK Service (`HiveBtleService.kt`)

- Acquire/release `PowerManager.PARTIAL_WAKE_LOCK` based on mode
- Update notification to indicate high-priority mode active
- Persist mode preference to SharedPreferences

### 3. WearTAK Repository (`HiveBtleRepository.kt`)

New state:

```kotlin
private val _highPriorityMode = MutableStateFlow(false)
val highPriorityMode: StateFlow<Boolean> = _highPriorityMode

fun setHighPriorityMode(enabled: Boolean)
```

### 4. WearTAK UI (`HiveMeshActivity.kt`)

Add toggle button at top of peer list:

```
+----------------------------------+
|  [PRIORITY SYNC: OFF]            |  <- Toggle button
+----------------------------------+
|  Mesh Peers (2 connected)        |
+----------------------------------+
|  WEAROS-4059  -62dBm  100%  [*]  |
|  WEAROS-3301  -55dBm   87%  [*]  |
+----------------------------------+
```

When enabled:
- Button shows "PRIORITY SYNC: ON" with warning color (orange/yellow)
- Optional: Show battery warning toast on first enable

### 5. Watchface Complication

Expose high-priority state for watchface display:

- Normal mode: Green mesh icon
- High-priority mode: Orange/yellow mesh icon with "!" indicator
- Tapping complication could toggle mode (optional)

---

## Implementation Steps

### Phase 1: Core Library Support

1. Add `HiveMeshConfig` data class to HiveBtle
2. Add configuration setters/getters
3. Implement dynamic interval updates (scan restart, sync, reconnect)
4. Add connection parameter request on connect
5. Add wake lock support (passed from app context)

### Phase 2: WearTAK Service Integration

1. Add SharedPreferences persistence for mode
2. Implement wake lock acquisition/release
3. Update foreground notification with mode indicator
4. Add battery optimization exemption request flow

### Phase 3: UI Integration

1. Add toggle button to HiveMeshActivity
2. Add confirmation dialog with battery warning
3. Update HiveBtleRepository with mode state
4. Add mode indicator to existing HIVE icons

### Phase 4: Watchface Integration

1. Expose mode via complication data
2. Update icon color based on mode
3. Optional: Add tap-to-toggle functionality

---

## Android APIs Used

### Wake Lock
```kotlin
val powerManager = getSystemService(Context.POWER_SERVICE) as PowerManager
val wakeLock = powerManager.newWakeLock(
    PowerManager.PARTIAL_WAKE_LOCK,
    "WearTAK::HiveMeshWakeLock"
)
wakeLock.acquire()  // On enable
wakeLock.release()  // On disable
```

### Battery Optimization Exemption
```kotlin
val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS)
intent.data = Uri.parse("package:$packageName")
startActivity(intent)
```

### BLE Connection Parameters
```kotlin
// Request faster connection interval after connecting
if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
    gatt.requestConnectionPriority(BluetoothGatt.CONNECTION_PRIORITY_HIGH)
}
```

### Foreground Service Type
```kotlin
// In AndroidManifest.xml
<service
    android:name=".service.HiveBtleService"
    android:foregroundServiceType="connectedDevice|location" />
```

---

## Risk Mitigation

1. **Battery drain warning** - Show clear warning when enabling
2. **Auto-disable after timeout** - Optionally disable after 1-2 hours
3. **Charging detection** - Could auto-enable when charging
4. **Visual indicators** - Always show when mode is active
5. **Easy toggle** - Make it simple to turn off quickly

---

## Design Decisions

1. **Auto-disable timeout** - Configurable option (default: 1 hour, options: off, 30min, 1hr, 2hr)
2. **Auto-enable on emergency** - YES, automatically enable when any peer enters SOS/emergency state
3. **Watchface tap confirmation** - YES, show confirmation dialog before toggling
4. **Battery estimate** - Future consideration, not in initial implementation

---

## Configuration Options

```kotlin
data class HighPriorityConfig(
    val enabled: Boolean = false,
    val autoDisableAfterMs: Long? = 3600000,  // 1 hour, null = never
    val autoEnableOnEmergency: Boolean = true,
    val showBatteryWarning: Boolean = true
)
```

### Auto-Disable Options
- Off (manual only)
- 30 minutes
- 1 hour (default)
- 2 hours

### Emergency Auto-Enable Behavior
When any mesh peer enters emergency/SOS state:
1. Automatically enable high-priority mode
2. Show notification: "Priority sync enabled - peer emergency detected"
3. Reset auto-disable timer
4. Stays enabled until:
   - Emergency is cleared AND auto-disable timeout reached, OR
   - User manually disables

---

*Document created: 2026-02-05*
*Updated: 2026-02-05 - Added design decisions*
