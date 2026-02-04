# nRF52840 Platform Limitations

This document captures the limitations and requirements discovered while attempting to port hive-btle to Nordic nRF52840-based boards (Adafruit CLUE, Feather nRF52840 Sense).

## Why nRF52840?

- **NDAA Compliance**: Nordic Semiconductor is non-Chinese manufacture, required for government/defense applications
- **Low Power**: Microcontroller-based for extended battery life (days/weeks vs hours)
- **BLE Mesh**: Ideal for daisy-chaining data over long distances
- **Cost Effective**: More resource-efficient than Linux SBCs for simple mesh nodes

## SoftDevice Limitations

Nordic's SoftDevice is a proprietary closed-source BLE stack that must be flashed separately from application code.

### Key Issues

1. **Separate Flash Requirement**: SoftDevice must be flashed to address 0x0 via SWD/J-Link programmer. UF2 bootloader cannot flash SoftDevice.

2. **Memory Layout Mismatch**:
   - S140 v6.1.1: Flash starts at 0x26000
   - S140 v7.3.0: Flash starts at 0x27000
   - Application must match the installed SoftDevice version exactly

3. **Critical Section Conflicts**: The `nrf-softdevice` crate's `critical-section-impl` conflicts with `cortex-m`'s `critical-section-single-core`. Only one can be active.

4. **PAC Conflicts**: `nrf52840-pac` and Embassy's `nrf-pac` define different interrupt symbols, causing linker errors if both are present.

5. **No SoftDevice in CircuitPython/UF2 Boards**: Adafruit boards ship with CircuitPython or Arduino bootloaders that use their own BLE stack, not SoftDevice. Flashing an app that expects SoftDevice results in crashes.

### Requirements for SoftDevice Development

- **SWD Programmer**: J-Link, J-Link EDU Mini ($18), or compatible debugger
- **probe-rs**: Rust-native flash/debug tool (replaces UF2 workflow)
- **SoftDevice Hex**: Download from [Nordic](https://www.nordicsemi.com/Products/Development-software/S140/Download)
- **Correct Memory Layout**: `memory.x` must match SoftDevice version

### Flash Procedure (Correct Way)

```bash
# 1. Flash SoftDevice first
probe-rs download --chip nRF52840_xxAA s140_nrf52_7.3.0_softdevice.hex

# 2. Flash application
cargo run --release  # with .cargo/config.toml configured for probe-rs
```

## CircuitPython Limitations

CircuitPython provides an easier development experience but is unsuitable for production hive-btle deployment.

### Key Issues

1. **No ChaCha20-Poly1305**: CircuitPython lacks the cryptographic primitives required for hive-btle mesh encryption. The ATAK plugin requires encrypted connections.

2. **No X25519 Key Exchange**: Required for per-peer end-to-end encryption sessions.

3. **Performance**: Python interpreter overhead reduces battery life and throughput compared to compiled Rust.

4. **Memory Constraints**: CircuitPython's runtime leaves less RAM for application data structures (CRDT state, peer sessions).

5. **Different BLE Stack**: Uses Adafruit's BLE library, not SoftDevice. Would require rewriting all GATT service code.

## Working Platforms

hive-btle currently works on:

| Platform | Status | Notes |
|----------|--------|-------|
| Linux (BlueZ) | ✅ Working | Raspberry Pi, x86 |
| Android | ✅ Working | Via JNI bindings |
| macOS | ✅ Working | CoreBluetooth |
| iOS | ✅ Working | CoreBluetooth |
| Windows | ✅ Working | WinRT |
| ESP32 (M5Stack) | ✅ Working | NimBLE stack |
| nRF52840 | ❌ Blocked | Requires SWD + SoftDevice |

## Path Forward for nRF52840

1. **Acquire SWD Programmer**: J-Link EDU Mini or similar
2. **Use Development Kit**: Nordic nRF52840-DK has proper debug header
3. **Flash SoftDevice First**: Before any application code
4. **Use probe-rs**: Not UF2 bootloader
5. **Match Memory Layout**: Verify `memory.x` against SoftDevice version

## Alternative NDAA-Compliant Options

If nRF52840 proves too difficult:

- **Nordic nRF52840-DK**: Has proper SWD header, designed for development
- **Particle Xenon**: nRF52840 with accessible debug pins (discontinued but available)
- **Fanstel BT840/BT840F**: nRF52840 modules with debug access
- **Laird BL654**: nRF52840 with better debug support

## References

- [nrf-softdevice README](https://github.com/embassy-rs/nrf-softdevice)
- [S140 SoftDevice Downloads](https://www.nordicsemi.com/Products/Development-software/S140/Download)
- [probe-rs](https://probe.rs/)
- [Embassy nRF Examples](https://github.com/embassy-rs/embassy/tree/main/examples/nrf52840)
