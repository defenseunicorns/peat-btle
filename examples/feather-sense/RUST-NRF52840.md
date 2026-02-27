# Pure-Rust Architecture for peat-btle on nRF52840

> Reference document for Claude Code and contributors working on the embedded Rust
> implementation of peat-btle targeting Nordic nRF52840 hardware.

## Why Pure Rust

The CircuitPython and SoftDevice paths on nRF52840 are dead ends for Peat:

- **SoftDevice** is Nordic's proprietary binary blob that owns the BLE radio, highest-priority
  interrupts, and the hardware crypto peripherals (AES-CCM, AES-ECB). It prevents application
  code from accessing those peripherals directly.
- **CircuitPython** runs on top of SoftDevice. It has no native AES/ChaCha20 support, no
  `cryptography` or `pycryptodome`, and the hardware crypto accelerator is locked behind
  SoftDevice. Pure-Python AES is too slow on a microcontroller.
- **The bind:** Can't use Python crypto libs (don't exist), can't access hardware crypto
  (SoftDevice owns it), can't do it in pure Python (too slow), can't bypass SoftDevice easily.

The pure-Rust stack eliminates SoftDevice entirely and gives full ownership of the hardware.

## The Stack

### 1. `nrf52840-hal` — Hardware Abstraction Layer

- **Crate:** [nrf52840-hal](https://crates.io/crates/nrf52840-hal) (v0.19.x, ~101K downloads)
- **Repo:** [nrf-rs/nrf-hal](https://github.com/nrf-rs/nrf-hal)
- **What it provides:** Direct access to all nRF52840 peripherals — GPIO, UART, SPI, I2C,
  timers, and critically the **AES-CCM and AES-ECB crypto peripherals**.
- **Key modules:**
  - `Ccm` — Safe, blocking wrapper around hardware AES-CCM encryption
  - `Ecb` — Safe, blocking wrapper around hardware AES-ECB encryption
  - `Rng` — Hardware random number generator
  - `Timer`, `Uarte`, `Spim`, `Twim` — Standard peripheral interfaces
- **Built on:** `nrf52840-pac` (Peripheral Access Crate, generated via `svd2rust` from Nordic's
  SVD files) and `embedded-hal` traits.

### 2. Embassy — Async Runtime for Embedded

- **Repo:** [embassy-rs/embassy](https://github.com/embassy-rs/embassy)
- **What it provides:** Cooperative multitasking on bare metal via Rust async/await. Maps well
  to mesh networking where you juggle BLE advertising, scanning, connection management, and
  data relay concurrently.
- **Key crates:**
  - `embassy-executor` — The async task executor
  - `embassy-nrf` — nRF-specific HAL with Embassy integration (features: `nrf52840`, `gpiote`,
    `time-driver-rtc1`)
  - `embassy-time` — Async timers and delays
- **Note:** `embassy-nrf` also supplies a standard `embedded-hal` style interface, so it works
  even outside pure-async contexts.

### 3. TrouBLE — Pure Rust BLE Host

- **Repo:** [embassy-rs/trouble](https://github.com/embassy-rs/trouble)
- **What it is:** A Bluetooth Low Energy Host implementation for embedded devices, written
  entirely in Rust. **Replaces SoftDevice.** No binary blob, no interrupt priority conflicts,
  no locked peripherals.
- **Architecture:** Implements the BLE Host side of the Host Controller Interface (HCI). The
  BLE spec splits implementations into a controller (lower layer) and host (upper layer)
  communicating via HCI, which can run over UART, USB, or custom in-memory IPC.
- **Status:** Working toward full BLE qualification. Fine for development and defense
  prototyping. Commercial certification may matter later.
- **Lineage:** Inspired by the `nrf-softdevice` project and `bleps`, adapted to work with
  `bt-hci` types/traits. Supports L2CAP connection-oriented channels.

### 4. Encryption

Two options, both available without SoftDevice:

- **Hardware AES-CCM** via `nrf52840-hal::Ccm` — uses the on-chip crypto accelerator directly
- **`chacha20poly1305`** crate — pure Rust AEAD, no hardware dependency

Peat minimum encryption requirement: AES-128-CCM or ChaCha20-Poly1305.

## Target Hardware

### Primary: Adafruit BlueFruit Sense (nRF52840)

- Ships with Arduino bootloader (bossac)
- Same nRF52840 SoC, just need to bypass SoftDevice
- USB-connected for flash/test loop

### Also Compatible

| Board | Price | Notes |
|-------|-------|-------|
| SparkFun Pro nRF52840 Mini | ~$30 | Cleaner for dev, LiPo charging, Qwiic, Raytac MDBT50Q-P1M module (FCC-approved) |
| Arduino Nano 33 BLE Sense Rev2 | ~$35-40 | Sensor-rich (IMU, temp, humidity, pressure, mic), good for field nodes |
| Nordic nRF52840-DK (PCA10056) | ~$40 | Reference dev kit, best probe-rs support |

All share the same nRF52840 SoC. Code targeting `nrf52840-hal` + TrouBLE runs on any of them.

### NDAA Compliance

- **Nordic Semiconductor:** Norwegian company (Trondheim). nRF52840 is not Chinese-sourced silicon.
- **SparkFun:** US company (Boulder, CO). NDAA-friendly board assembly.
- **ESP32 (Espressif):** Shanghai-based. Chinese sourced. **Not NDAA-compliant.**

The nRF52840 is a strong foundation for a custom Peat board with a trusted supply chain.

## Development Environment

```bash
# Rust toolchain with embedded target
rustup target add thumbv7em-none-eabihf

# Flashing/debugging tools
cargo install probe-rs-tools   # or use bossac for bootloader-based flashing

# Logging
# defmt + defmt-rtt for efficient structured logging over RTT
```

### Cargo.toml Dependencies

```toml
[dependencies]
embassy-executor = { version = "0.7", features = ["arch-cortex-m", "executor-thread"] }
embassy-nrf = { version = "0.3", features = ["nrf52840", "gpiote", "time-driver-rtc1"] }
embassy-time = "0.4"
trouble-host = "0.1"             # TrouBLE BLE host
nrf52840-hal = "0.19"
defmt = "0.3"
defmt-rtt = "0.4"
cortex-m = "0.7"
cortex-m-rt = "0.7"
```

> **Pin versions after validating.** The embedded Rust ecosystem moves fast. Check
> compatibility between Embassy, TrouBLE, and HAL versions before updating.

### Bootloader Decision

| Option | Pros | Cons |
|--------|------|------|
| Keep Arduino bootloader (bossac) | Easier setup, no extra hardware | Lose first 64K flash, no probe-rs debugging |
| Burn J-Link/DAPLink | Full flash, probe-rs, RTT logging, defmt | Requires ~$20 J-Link EDU Mini or second nRF board as debugger |

For PoC: bossac is fine. For sustained development: invest in probe-rs.

### Linker Script

The memory map **must** match whether you're using the bootloader or going bare-metal. If the
linker script is wrong, nothing works. This is the first thing to verify when things fail
silently.

## PoC Target

Minimum viable proof-of-concept — two BlueFruit boards, no SoftDevice, pure Rust:

1. **Board A** advertises an Peat beacon with a payload
2. **Board B** scans, discovers Board A, establishes a BLE connection
3. They exchange an encrypted message using hardware AES-CCM
4. LED blink or serial output confirms success

**This proves:** Pure Rust on nRF52840, BLE without SoftDevice via TrouBLE, hardware crypto
access, and bidirectional encrypted communication.

### Incremental Milestones

Each step is a checkpoint. Don't skip ahead — each depends on the previous one working on
real hardware.

1. Blinking LED via pure Rust (validates toolchain end-to-end)
2. BLE advertising on Board A
3. BLE scanning on Board B
4. Connection establishment between boards
5. Encrypted message exchange via hardware AES-CCM
6. Peat mesh protocol framing on top

## Project Location

```
peat-btle/
  examples/
    rust-nrf52840/       # ← this PoC lives here
      Cargo.toml
      src/
        main.rs          # or split into bin/advertiser.rs + bin/scanner.rs
      memory.x           # linker script
      .cargo/config.toml # target + runner config
```

This is a reference implementation / example within peat-btle, not a separate project.

## Known Pain Points

- **Two-board debugging:** When both boards run BLE simultaneously, knowing which one is
  misbehaving is hard. Good logging (defmt over RTT or serial) is essential from the start.
- **TrouBLE maturity:** Newer than nrf-softdevice bindings. Connection establishment and GATT
  should work, but edge cases may bite. Check the issue tracker.
- **Nordic does not officially support Rust.** Their supported SDK is C-based (nRF Connect SDK).
  Community support only. The `nrf-rs` GitHub org is the hub.
- **Async learning curve:** Embassy's executor/spawner pattern takes time to internalize if
  you're new to it. But it maps naturally to concurrent mesh operations.
- **EasyDMA RAM requirement:** On nRF52840, DMA buffers must reside in RAM (not flash). If
  using `embassy-nrf`, methods without `_from_ram` suffix auto-copy but allocate up to 512
  bytes. Watch memory usage.

## Strategic Context

This pure-Rust architecture solves every problem encountered with CircuitPython/SoftDevice:

| Problem | CircuitPython/SoftDevice | Pure Rust |
|---------|--------------------------|-----------|
| Crypto libraries | None available | Hardware AES-CCM via HAL, or chacha20poly1305 crate |
| Hardware crypto access | Locked behind SoftDevice | Direct via nrf52840-hal |
| Radio control | SoftDevice owns it | TrouBLE provides full BLE host |
| Interrupt conflicts | SoftDevice reserves highest priority | No binary blob, full control |
| Memory safety | Python GC on constrained hardware | Rust ownership model, zero-cost abstractions |
| Auditability | Proprietary binary blob | Fully open source, auditable stack |

### Long-Term Vision

peat-btle is a **protocol spec with reference implementations**, not a single codebase:

- **ESP32 / Arduino** — "get started in an afternoon" tier (mbedtls for crypto)
- **MicroPython** — Python accessibility tier (ucryptolib for AES)
- **Rust on nRF52840** — production/defense tier (this document)
- **CircuitPython** — education/maker tier (unencrypted demo mesh only)

The Rust implementation on nRF52840 is the **production reference** — no proprietary blobs,
hardware crypto, memory safety, NDAA-compliant silicon. This is what ships on custom Peat
hardware and what defense customers audit.

## References

- [nrf-rs/nrf-hal](https://github.com/nrf-rs/nrf-hal) — HAL source and examples
- [embassy-rs/embassy](https://github.com/embassy-rs/embassy) — Async runtime, nRF examples in `examples/nrf52840/`
- [embassy-rs/trouble](https://github.com/embassy-rs/trouble) — TrouBLE BLE host
- [nrf52840-hal docs](https://docs.rs/nrf52840-hal) — API reference (AES-CCM, AES-ECB modules)
- [probe-rs](https://probe.rs) — Flash and debug tool
- [Ferrous Systems Embedded Training](https://github.com/ferrous-systems/embedded-trainings-2020) — nRF52840-based Rust embedded course
- [Rust Embedded Book](https://docs.rust-embedded.org/book/) — General embedded Rust guide
