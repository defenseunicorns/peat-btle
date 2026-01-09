# ESP32 Platform Integration Guide

This guide covers integrating `hive-btle` into ESP32 projects using the ESP-IDF NimBLE stack.

## Requirements

| Requirement | Details |
|-------------|---------|
| Hardware | ESP32, ESP32-S3, ESP32-C3 |
| ESP-IDF | v5.0+ recommended |
| Rust | Nightly (esp fork) |
| BLE Stack | NimBLE (integrated in ESP-IDF) |

### Hardware Tested

- M5Stack Core2 (ESP32-D0WDQ6-V3)
- ESP32-DevKitC
- ESP32-S3-DevKitC

**Note:** Original ESP32 does not support Coded PHY. Use ESP32-S3 or ESP32-C3 for BLE 5.0 features.

## Architecture

```
┌─────────────────────────────────────────┐
│         Application (main.rs)           │
├─────────────────────────────────────────┤
│           Esp32Adapter (Rust)           │
├─────────────────────────────────────────┤
│            NimBLE FFI Bindings          │
├─────────────────────────────────────────┤
│         ESP-IDF NimBLE Stack            │
│    (GAP, GATT, Security Manager)        │
└─────────────────────────────────────────┘
```

## Project Setup

### 1. Install ESP-IDF and Rust Toolchain

```bash
# Install ESP-IDF
git clone --recursive https://github.com/espressif/esp-idf.git
cd esp-idf
./install.sh esp32
source export.sh

# Install Rust ESP toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install nightly
cargo install espup
espup install
source ~/export-esp.sh

# Install cargo-espflash
cargo install espflash
```

### 2. Create Project

```bash
cargo new --bin hive-esp32
cd hive-esp32
```

### 3. Configure Cargo.toml

```toml
[package]
name = "hive-esp32"
version = "0.1.0"
edition = "2021"

[dependencies]
esp-idf-svc = { version = "0.48", features = ["binstart", "std"] }
esp-idf-hal = "0.43"
log = "0.4"
hive-btle = { version = "0.1", features = ["esp32", "std"] }
embedded-svc = "0.27"

[build-dependencies]
embuild = "0.31"

[profile.release]
opt-level = "s"
lto = "thin"
```

### 4. Configure ESP-IDF (sdkconfig.defaults)

```ini
# Enable Bluetooth
CONFIG_BT_ENABLED=y
CONFIG_BT_NIMBLE_ENABLED=y
CONFIG_BT_CONTROLLER_ENABLED=y

# NimBLE settings
CONFIG_BT_NIMBLE_ROLE_CENTRAL=y
CONFIG_BT_NIMBLE_ROLE_PERIPHERAL=y
CONFIG_BT_NIMBLE_ROLE_OBSERVER=y
CONFIG_BT_NIMBLE_ROLE_BROADCASTER=y
CONFIG_BT_NIMBLE_MAX_CONNECTIONS=4
CONFIG_BT_NIMBLE_ATT_PREFERRED_MTU=128

# GAP settings
CONFIG_BT_NIMBLE_GAP_DEVICE_NAME_MAX_LEN=20

# Logging
CONFIG_LOG_DEFAULT_LEVEL_INFO=y

# For M5Stack display (optional)
CONFIG_LV_USE_LOG=y
```

### 5. Create .cargo/config.toml

```toml
[build]
target = "xtensa-esp32-espidf"

[target.xtensa-esp32-espidf]
linker = "ldproxy"

[unstable]
build-std = ["std", "panic_abort"]

[env]
ESP_IDF_VERSION = "v5.1"
```

## Basic Usage

```rust
use esp_idf_svc::hal::prelude::*;
use esp_idf_svc::log::EspLogger;
use hive_btle::{HiveMesh, HiveMeshConfig, NodeId};
use hive_btle::platform::esp32::Esp32Adapter;
use log::info;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    // Initialize ESP-IDF
    esp_idf_svc::sys::link_patches();
    EspLogger::initialize_default();

    info!("Starting HIVE BLE on ESP32...");

    // Get MAC address for node ID
    let mut mac = [0u8; 6];
    unsafe {
        esp_idf_svc::sys::esp_efuse_mac_get_default(mac.as_mut_ptr());
    }
    let node_id = NodeId::from_mac_address(&mac);

    info!("Node ID: {:08X}", node_id.as_u32());

    // Create mesh configuration
    let config = HiveMeshConfig::new(node_id, "ESP32-1", "DEMO");
    let mesh = HiveMesh::new(config);

    // Create and initialize ESP32 adapter
    let mut adapter = Esp32Adapter::new(node_id, "HIVE_DEMO")?;
    let ble_config = hive_btle::BleConfig::hive_lite(node_id);

    // Block on async init
    esp_idf_svc::hal::task::block_on(async {
        adapter.init(&ble_config).await?;
        adapter.start().await?;
        Ok::<_, anyhow::Error>(())
    })?;

    info!("BLE adapter started");

    // Main loop
    loop {
        let now_ms = esp_idf_svc::sys::esp_timer_get_time() as u64 / 1000;

        // Check for received documents
        if let Some(data) = adapter.take_pending_document() {
            info!("Received {} bytes", data.len());
            if let Some(result) = mesh.on_ble_data("unknown", &data, now_ms) {
                if result.is_emergency {
                    info!("EMERGENCY from {:08X}!", result.source_node.as_u32());
                    // Trigger LED, buzzer, etc.
                }
            }
        }

        // Periodic tick
        if let Some(doc) = mesh.tick(now_ms) {
            let sent = adapter.gossip_document(&doc);
            info!("Gossiped {} bytes to {} peers", doc.len(), sent);
        }

        // Small delay
        std::thread::sleep(Duration::from_millis(100));
    }
}
```

## M5Stack Core2 Integration

For M5Stack with display and buttons:

```rust
use esp_idf_svc::hal::gpio::*;
use esp_idf_svc::hal::peripherals::Peripherals;

fn main() -> anyhow::Result<()> {
    let peripherals = Peripherals::take()?;

    // Initialize display (M5Stack specific)
    // ... display init code ...

    // Button pins (M5Stack Core2)
    let btn_a = PinDriver::input(peripherals.pins.gpio39)?; // Button A
    let btn_b = PinDriver::input(peripherals.pins.gpio38)?; // Button B
    let btn_c = PinDriver::input(peripherals.pins.gpio37)?; // Button C

    // ... mesh setup ...

    loop {
        // Check button presses
        if btn_a.is_low() {
            info!("Button A pressed - sending EMERGENCY");
            let doc = mesh.send_emergency(now_ms);
            adapter.gossip_document(&doc);
        }

        if btn_b.is_low() {
            info!("Button B pressed - sending ACK");
            let doc = mesh.send_ack(now_ms);
            adapter.gossip_document(&doc);
        }

        if btn_c.is_low() {
            info!("Button C pressed - clearing event");
            mesh.clear_event();
        }

        // ... rest of main loop ...
    }
}
```

## NimBLE API Reference

### GAP Events

The `Esp32Adapter` handles these GAP events internally:

| Event | Description |
|-------|-------------|
| `BLE_GAP_EVENT_CONNECT` | Connection established |
| `BLE_GAP_EVENT_DISCONNECT` | Connection lost |
| `BLE_GAP_EVENT_DISC` | Device discovered |
| `BLE_GAP_EVENT_ADV_COMPLETE` | Advertising finished |

### GATT Service

The adapter automatically creates:

| UUID | Type | Properties |
|------|------|------------|
| `f47ac10b-...` | Service | Primary |
| `f47ac10b-...-0003` | Characteristic | Read, Write, Notify |

### Key Functions

```rust
impl Esp32Adapter {
    /// Create new adapter
    pub fn new(node_id: NodeId, device_name: &str) -> Result<Self>;

    /// Create with HIVE-Lite defaults
    pub fn hive_lite(node_id: NodeId) -> Result<Self>;

    /// Take pending received document (non-blocking)
    pub fn take_pending_document(&self) -> Option<Vec<u8>>;

    /// Send document to all connected peers via GATT notify
    pub fn gossip_document(&self, data: &[u8]) -> usize;

    /// Update local document for GATT reads
    pub fn set_document(&self, data: &[u8]);
}
```

## Power Management

### Low Power Configuration

```rust
// Ultra low power for smartwatch use
let config = BleConfig::hive_lite(node_id);

// Custom power profile
let mut config = BleConfig::new(node_id);
config.power_profile = PowerProfile::Custom {
    scan_interval_ms: 10000,   // Scan every 10 seconds
    scan_window_ms: 200,       // Scan for 200ms
    adv_interval_ms: 5000,     // Advertise every 5 seconds
    conn_interval_ms: 200,     // 200ms connection interval
};
```

### Sleep Modes

```rust
use esp_idf_svc::sys::*;

fn enter_light_sleep(duration_ms: u64) {
    unsafe {
        // Configure light sleep
        esp_sleep_enable_timer_wakeup(duration_ms * 1000);
        esp_light_sleep_start();
    }
}

// Main loop with sleep
loop {
    // Process BLE events
    handle_ble_events(&mesh, &adapter);

    // Enter light sleep between events
    enter_light_sleep(900); // Sleep 900ms, active 100ms = 10% duty cycle
}
```

## Memory Optimization

### Stack Configuration

In `sdkconfig.defaults`:

```ini
CONFIG_ESP_MAIN_TASK_STACK_SIZE=8192
CONFIG_BT_NIMBLE_TASK_STACK_SIZE=4096
```

### Heap Usage

```rust
// Monitor heap usage
fn print_heap_info() {
    unsafe {
        let free = esp_idf_svc::sys::esp_get_free_heap_size();
        let min = esp_idf_svc::sys::esp_get_minimum_free_heap_size();
        info!("Heap: {} free, {} minimum", free, min);
    }
}
```

## Building and Flashing

```bash
# Build
cargo build --release

# Flash and monitor
espflash flash --monitor target/xtensa-esp32-espidf/release/hive-esp32

# Or separately
espflash flash target/xtensa-esp32-espidf/release/hive-esp32
espflash monitor
```

## Troubleshooting

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| BLE stack reset | Stack overflow | Increase `CONFIG_BT_NIMBLE_TASK_STACK_SIZE` |
| No advertising | Wrong GAP mode | Check `CONFIG_BT_NIMBLE_ROLE_BROADCASTER` |
| No scanning | Wrong GAP mode | Check `CONFIG_BT_NIMBLE_ROLE_OBSERVER` |
| Connection drops | Range issues | Reduce distance, check antenna |
| OOM errors | Too many connections | Reduce `CONFIG_BT_NIMBLE_MAX_CONNECTIONS` |

### Debug Logging

In `sdkconfig.defaults`:

```ini
CONFIG_LOG_DEFAULT_LEVEL_DEBUG=y
CONFIG_BT_NIMBLE_DEBUG=y
CONFIG_BT_NIMBLE_LOG_LEVEL_DEBUG=y
```

### Monitor NimBLE Events

```rust
// Enable verbose logging
unsafe {
    esp_log_level_set(b"NimBLE\0".as_ptr() as *const _, ESP_LOG_DEBUG);
}
```

## Testing with Other Platforms

### iOS/Android Discovery

1. Flash ESP32 with HIVE firmware
2. It will advertise as `HIVE_DEMO-XXXXXXXX`
3. Use iOS/Android HIVE app to discover
4. Connect and sync documents

### Linux Testing

```bash
# Scan for HIVE devices
sudo hcitool lescan | grep HIVE

# Connect with gatttool
sudo gatttool -b XX:XX:XX:XX:XX:XX -I
> connect
> primary
> characteristics
```

## References

- [ESP-IDF NimBLE Guide](https://docs.espressif.com/projects/esp-idf/en/latest/esp32/api-reference/bluetooth/nimble/index.html)
- [esp-idf-svc crate](https://github.com/esp-rs/esp-idf-svc)
- [M5Stack Core2](https://docs.m5stack.com/en/core/core2)
- [NimBLE Documentation](https://mynewt.apache.org/latest/network/index.html)
