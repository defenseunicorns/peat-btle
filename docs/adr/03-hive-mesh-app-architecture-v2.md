# ADR-03: HIVE Mesh Application Architecture (Revised)

**Organization:** (r)evolve - Revolve Team LLC  
**URL:** https://revolveteam.com  
**Date:** January 2026  
**Status:** Draft  
**Depends On:** ADR-01 (Capability Validation), ADR-02 (Feature Requirements)

---

## Executive Summary

This ADR defines the application architecture for HIVE Mesh—a family of applications built on hive-btle. All platforms (iOS, Android, M5Stack Core2, bare ESP32) are **first-class mesh participants** sharing the same CRDT data structures and sync protocol. The architecture lives in a single codebase within hive-btle, with platform-specific UI layers.

---

## Key Design Decisions

### Decision 1: Single Codebase in hive-btle

All application code lives in the hive-btle repository:

```
hive-btle/
├── src/
│   ├── lib.rs                 # Core library
│   ├── crdt/                  # CRDT implementations
│   ├── sync/                  # Sync protocol
│   ├── platform/
│   │   ├── apple/             # iOS/macOS (CoreBluetooth)
│   │   ├── android/           # Android (JNI)  
│   │   ├── linux/             # Linux (BlueZ)
│   │   └── esp32/             # ESP32 (existing work)
│   └── app/                   # Application layer (NEW)
│       ├── messages.rs        # Message CRDT + logic
│       ├── alerts.rs          # Alert CRDT + logic
│       ├── markers.rs         # Map marker CRDT + logic
│       ├── files.rs           # File sharing CRDT + logic
│       └── peers.rs           # Peer presence CRDT + logic
├── ios/
│   ├── hive-apple-ffi/        # UniFFI bindings (exists)
│   └── HiveTest/              # iOS app → HiveMesh
├── android/                   # Android app (future)
├── examples/
│   └── m5stack-core2/         # M5Stack firmware (move from hive repo)
└── ...
```

### Decision 2: M5Stack Core2 = First-Class Mesh Node

The M5Stack is not a gateway or bridge—it's a full participant:
- Same CRDT sync as phones
- WiFi + BLE connectivity (both for itself, not bridging)
- Local UI for interaction
- Can originate messages, alerts, markers

### Decision 3: No Cloud Gateway (For Now)

The mesh is self-contained. No Nostr bridge, no cloud backend, no internet dependency. This can be added later as an optional feature, but MVP is pure mesh.

### Decision 4: embedded-graphics for M5Stack Display

The M5Stack Core2 uses an ILI9342 display (320x240). We'll use:
- `embedded-graphics` for 2D graphics primitives
- `mipidsi` crate for display driver (supports ILI9341/ILI9342)
- Existing Rust AXP192 driver for power management

This keeps the entire stack in Rust, consistent with hive-btle.

---

## Device Capabilities Matrix

All devices are first-class mesh participants. Differences are in UI/UX, not protocol:

| Capability | iOS/Android | M5Stack Core2 | Bare ESP32 |
|------------|-------------|---------------|------------|
| **Mesh Role** | Full participant | Full participant | Full participant |
| **BLE** | ✓ | ✓ | ✓ |
| **WiFi** | ✓ | ✓ | Optional |
| **Messages** | Full UI | List view + canned replies | Relay only |
| **Alerts** | Full UI | Full UI + audio/vibration | Relay + LED |
| **Markers** | Map view | List view | Relay only |
| **Files** | Full transfer | Metadata only | Relay only |
| **Sensors** | Read from mesh | Read + originate | Originate |
| **Display** | Full touch | 320x240 touch | None |
| **Input** | Full keyboard | 3 virtual buttons + touch | 0-2 buttons |

---

## M5Stack Core2 Specification

### Hardware Summary

| Component | Specification | Use in HIVE Mesh |
|-----------|---------------|------------------|
| CPU | ESP32 dual-core 240MHz | BLE + UI processing |
| RAM | 8MB PSRAM | CRDT buffers, display framebuffer |
| Flash | 16MB | Firmware + message cache |
| Display | 2" ILI9342 320x240 IPS | Status UI, messages, alerts |
| Touch | FT6336U capacitive | UI navigation |
| Virtual Buttons | 3 zones (40px strip) | Quick actions |
| Speaker | NS4168 I2S amplifier | Alert tones |
| Vibration | Motor | Haptic alerts |
| IMU | MPU6886 6-axis | Motion detection (future) |
| Microphone | SPM1423 PDM | Voice notes (future) |
| RTC | BM8563 battery-backed | Accurate timestamps |
| Power | AXP192 PMU + 390mAh LiPo | Battery management |
| Storage | microSD slot | Extended logging (optional) |

### Display Architecture

**Screen Layout (320x240 pixels):**

```
┌────────────────────────────────────────┐ ─┐
│ ⚡ 87%   HIVE MESH   📶 5 peers  12:34 │  │ Status bar (24px)
├────────────────────────────────────────┤ ─┤
│                                        │  │
│                                        │  │
│         Main Content Area              │  │ Content (176px)
│         (View-specific)                │  │
│                                        │  │
│                                        │  │
├────────────────────────────────────────┤ ─┤
│   [MSG]        [SOS]        [SET]     │  │ Virtual buttons (40px)
└────────────────────────────────────────┘ ─┘
```

### View Hierarchy

```
┌─────────────────┐
│   Dashboard     │ ← Default view
│   (home)        │
└────────┬────────┘
         │
    ┌────┴────┬──────────┐
    ▼         ▼          ▼
┌───────┐ ┌───────┐ ┌─────────┐
│ Msgs  │ │Alerts │ │Settings │
└───┬───┘ └───┬───┘ └─────────┘
    │         │
    ▼         ▼
┌───────┐ ┌───────┐
│ Detail│ │ Detail│
└───────┘ └───────┘
```

### Views

**1. Dashboard (Default)**
```
┌────────────────────────────────────────┐
│ ⚡ 87%   HIVE MESH   📶 5 peers  12:34 │
├────────────────────────────────────────┤
│  Mesh Status: HEALTHY                  │
│  ├─ Direct peers: 3                    │
│  ├─ Reachable: 5                       │
│  └─ Messages today: 12                 │
│                                        │
│  Last Alert: None                      │
│  Last Message: "Copy that" (2m ago)    │
├────────────────────────────────────────┤
│   [MSG]        [SOS]        [SET]     │
└────────────────────────────────────────┘
```

**2. Message List**
```
┌────────────────────────────────────────┐
│ ⚡ 87%   MESSAGES        ← Back  12:34 │
├────────────────────────────────────────┤
│ ┌────────────────────────────────────┐ │
│ │ Alpha-1           2m ago           │ │
│ │ Copy that, moving to rally point   │ │
│ └────────────────────────────────────┘ │
│ ┌────────────────────────────────────┐ │
│ │ Bravo-2           5m ago           │ │
│ │ Need assistance at grid 123456     │ │
│ └────────────────────────────────────┘ │
│ ┌────────────────────────────────────┐ │
│ │ Command          10m ago           │ │
│ │ All units check in                 │ │
│ └────────────────────────────────────┘ │
├────────────────────────────────────────┤
│  [BACK]      [REPLY]      [SCROLL]    │
└────────────────────────────────────────┘
```

**3. Canned Reply Selection**
```
┌────────────────────────────────────────┐
│ ⚡ 87%   QUICK REPLY     ← Back  12:34 │
├────────────────────────────────────────┤
│                                        │
│   [ Copy           ]                   │
│   [ Negative       ]                   │
│   [ En route       ]                   │
│   [ Need assist    ]                   │
│   [ All clear      ]                   │
│   [ Custom...      ]                   │
│                                        │
├────────────────────────────────────────┤
│  [BACK]      [SEND]       [SCROLL]    │
└────────────────────────────────────────┘
```

**4. Alert Display (Full Screen Takeover)**
```
┌────────────────────────────────────────┐
│ ⚠️ ⚠️ ⚠️  EMERGENCY ALERT  ⚠️ ⚠️ ⚠️  │
├────────────────────────────────────────┤
│                                        │
│           🆘 SOS 🆘                    │
│                                        │
│        From: Alpha-1                   │
│        Time: 12:34:56                  │
│                                        │
│        Grid: 33.7490, -84.3880         │
│                                        │
│        "Need immediate evac"           │
│                                        │
├────────────────────────────────────────┤
│  [ACK]       [LOCATE]     [DISMISS]   │
└────────────────────────────────────────┘
```
*Audio tone plays, vibration motor activates*

**5. Settings**
```
┌────────────────────────────────────────┐
│ ⚡ 87%   SETTINGS        ← Back  12:34 │
├────────────────────────────────────────┤
│                                        │
│  Callsign: [M5-Node-1    ]            │
│                                        │
│  Display:                              │
│    Brightness: [████████░░] 80%        │
│    Auto-dim: [ON ]                     │
│                                        │
│  Audio:                                │
│    Alert volume: [████████░░] 80%      │
│    Vibration: [ON ]                    │
│                                        │
├────────────────────────────────────────┤
│  [BACK]      [EDIT]       [SAVE]      │
└────────────────────────────────────────┘
```

### Button Mappings

| View | BTN1 (Left) | BTN2 (Center) | BTN3 (Right) |
|------|-------------|---------------|--------------|
| Dashboard | Messages | **SOS** (hold 2s) | Settings |
| Messages | Back | Reply | Scroll Down |
| Reply Select | Back | Send | Scroll Down |
| Alert | ACK | Locate | Dismiss |
| Settings | Back | Edit/Toggle | Save |

**SOS Button Behavior:**
- Tap: Show alert history
- **Hold 2 seconds**: Send emergency alert (with vibration confirmation)

### Software Architecture (M5Stack)

```
┌─────────────────────────────────────────────────────────────┐
│  M5Stack Core2 Firmware                                     │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────┐   │
│  │  UI Layer (embedded-graphics)                        │   │
│  │  ├─ views/dashboard.rs                               │   │
│  │  ├─ views/messages.rs                                │   │
│  │  ├─ views/alerts.rs                                  │   │
│  │  ├─ views/settings.rs                                │   │
│  │  └─ widgets/ (status_bar, button_bar, list, etc.)   │   │
│  └─────────────────────────────────────────────────────┘   │
│                          │                                  │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  App Layer (shared with mobile)                      │   │
│  │  ├─ messages.rs                                      │   │
│  │  ├─ alerts.rs                                        │   │
│  │  ├─ markers.rs                                       │   │
│  │  └─ peers.rs                                         │   │
│  └─────────────────────────────────────────────────────┘   │
│                          │                                  │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  hive-btle Core                                      │   │
│  │  ├─ CRDT sync                                        │   │
│  │  ├─ BLE mesh                                         │   │
│  │  └─ Platform: ESP32                                  │   │
│  └─────────────────────────────────────────────────────┘   │
│                          │                                  │
│  ┌─────────────────────────────────────────────────────┐   │
│  │  HAL (Hardware Abstraction)                          │   │
│  │  ├─ display.rs    (mipidsi + ILI9342)               │   │
│  │  ├─ touch.rs      (FT6336U driver)                  │   │
│  │  ├─ audio.rs      (I2S + NS4168)                    │   │
│  │  ├─ power.rs      (AXP192 driver)                   │   │
│  │  └─ vibration.rs  (GPIO motor control)              │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

---

## CRDT Data Model

### Shared Across All Platforms

```rust
// Message - append-only log
struct Message {
    id: Uuid,
    author: NodeId,
    timestamp: Timestamp,
    content: String,        // max 1000 chars
    reply_to: Option<Uuid>, // threading
}

// Alert - LWW register + ACK set
struct Alert {
    id: Uuid,
    author: NodeId,
    timestamp: Timestamp,
    alert_type: AlertType,  // SOS, Medical, Evacuation, AllClear, CheckIn
    location: Option<Location>,
    message: Option<String>, // max 280 chars
    acks: HashSet<(NodeId, Timestamp)>, // ORSet for ACKs
}

// Marker - LWW map entry
struct MapMarker {
    id: Uuid,
    author: NodeId,
    created_at: Timestamp,
    updated_at: Timestamp,
    latitude: f64,
    longitude: f64,
    title: String,          // max 100 chars
    description: String,    // max 500 chars
    category: MarkerCategory,
    status: MarkerStatus,   // Active, Resolved, Expired
    photo_ids: Vec<FileId>, // references
}

// File manifest - metadata only syncs, binary transfers separately
struct FileManifest {
    id: FileId,
    author: NodeId,
    timestamp: Timestamp,
    name: String,
    size: u32,
    mime_type: String,
    hash: [u8; 32],         // SHA-256
    chunk_count: u16,
}

// Peer presence - ORSet
struct Peer {
    id: NodeId,
    callsign: String,       // max 20 chars
    last_seen: Timestamp,
    capabilities: PeerCapabilities,
}
```

### Platform-Specific Handling

| CRDT | iOS/Android | M5Stack Core2 | Bare ESP32 |
|------|-------------|---------------|------------|
| Messages | Store all, full UI | Store last 50, list UI | Relay only, no storage |
| Alerts | Store all, full UI | Store last 20, full UI | Relay + LED blink on alert |
| Markers | Store all, map UI | Store last 20, list UI | Relay only |
| Files | Full transfer + storage | Metadata only | Relay only |
| Peers | Store all | Store all | Store direct peers only |

---

## Implementation Phases

### Phase 1: Move M5Stack Example to hive-btle (1 week)

**Tasks:**
1. Move `hive/examples/m5stack-core2-hive` to `hive-btle/examples/m5stack-core2/`
2. Update imports/dependencies
3. Verify existing functionality still works
4. Document current state

**Deliverables:**
- [ ] M5Stack example builds from hive-btle repo
- [ ] BLE mesh works between M5Stack and other platforms
- [ ] README documents build process

### Phase 2: App Data Layer (2 weeks)

**Tasks:**
1. Define CRDT schemas in `src/app/`
2. Implement Message, Alert, Marker, Peer CRDTs
3. Add sync protocol for each type
4. Unit tests for merge behavior

**Deliverables:**
- [ ] All CRDT types implemented
- [ ] Sync works across platforms
- [ ] Tests pass

### Phase 3: M5Stack Display UI (3 weeks)

**Tasks:**
1. Set up embedded-graphics with mipidsi driver
2. Implement HAL layer (display, touch, audio, power, vibration)
3. Build widget library (status bar, button bar, list, etc.)
4. Implement all views (dashboard, messages, alerts, settings)
5. Touch input handling
6. Audio/vibration for alerts

**Deliverables:**
- [ ] All views render correctly
- [ ] Touch navigation works
- [ ] Alerts trigger audio + vibration
- [ ] Battery indicator accurate

### Phase 4: iOS App Update (2 weeks)

**Tasks:**
1. Update HiveTest → HiveMesh
2. Integrate new CRDT data layer
3. Add messaging UI
4. Add alert UI
5. Add marker/map UI

**Deliverables:**
- [ ] Full messaging works
- [ ] Alerts send/receive with ACK
- [ ] Map markers display and create

### Phase 5: Cross-Platform Validation (1 week)

**Tasks:**
1. Test iOS ↔ Android ↔ M5Stack ↔ ESP32
2. Verify CRDT sync across all platforms
3. Battery life testing
4. Range testing with Coded PHY
5. Document results

**Deliverables:**
- [ ] All platforms sync correctly
- [ ] Performance metrics documented
- [ ] Demo script works

---

## Open Questions

1. **Touch driver:** Is there an existing Rust FT6336U driver, or do we write one?
2. **Font rendering:** Which font for embedded-graphics? Bitmap vs vector?
3. **Canned replies:** Hardcoded or configurable via settings?
4. **Message threading:** Include reply_to in MVP or defer?
5. **File transfer:** Include in MVP or defer to later phase?

---

## References

- ADR-01: hive-btle Capability Validation Plan
- ADR-02: Messaging App Feature Requirements
- M5Stack Core2 Docs: https://docs.m5stack.com/en/core/core2
- embedded-graphics: https://github.com/embedded-graphics/embedded-graphics
- mipidsi: https://github.com/almindor/mipidsi
- Rust ESP32 Book: https://esp-rs.github.io/book/
