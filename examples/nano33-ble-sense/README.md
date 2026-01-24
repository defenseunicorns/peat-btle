# HIVE Sensor Beacon - Arduino Nano 33 BLE Sense

Transmit-only HIVE mesh node using the nRF52840-based Arduino Nano 33 BLE Sense.

## What It Does

- Advertises as a HIVE node via BLE
- Serves sensor data via GATT characteristic
- Syncs CRDT documents with mesh peers
- Low power, headless operation

## Hardware

- **Board**: Arduino Nano 33 BLE Sense
- **MCU**: Nordic nRF52840 (Cortex-M4F @ 64MHz)
- **Memory**: 1MB Flash, 256KB RAM
- **BLE**: 5.0 via SoftDevice S140

### Onboard Sensors

| Sensor | Function |
|--------|----------|
| LSM9DS1 / BMI270+BMM150 | 9-axis IMU |
| APDS9960 | Gesture, light, proximity |
| LPS22HB | Pressure, temperature |
| HTS221 | Humidity, temperature |
| MP34DT05 | Microphone |

## Prerequisites

### 1. Install Rust embedded toolchain

```bash
# Add ARM Cortex-M target
rustup target add thumbv7em-none-eabihf

# Install probe-rs for flashing
cargo install probe-rs-tools
```

### 2. Install SoftDevice

The Nordic SoftDevice S140 must be flashed first (one-time):

```bash
# Download S140 from Nordic (or use probe-rs)
probe-rs download --chip nRF52840_xxAA s140_nrf52_7.3.0_softdevice.hex
```

## Building

```bash
cd examples/nano33-ble-sense
cargo build --release
```

## Flashing

```bash
# Build and flash
cargo run --release

# Or just flash
probe-rs run --chip nRF52840_xxAA target/thumbv7em-none-eabihf/release/nano33-ble-sense-hive
```

## Viewing Logs

Logs are output via RTT (Real-Time Transfer):

```bash
# In another terminal
probe-rs attach --chip nRF52840_xxAA
```

## HIVE Integration

This node:
1. Advertises with HIVE service UUID (0xF47A)
2. Exposes GATT service with document + sensor characteristics
3. Accepts document writes from mesh peers (merges CRDT)
4. Broadcasts sensor readings to subscribed peers

## Power Consumption

| Mode | Current |
|------|---------|
| Advertising (1s interval) | ~15μA |
| Connected, idle | ~1mA |
| Sensor read + transmit | ~5mA |

## License

Apache-2.0
