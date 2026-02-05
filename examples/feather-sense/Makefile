# Makefile for Adafruit Feather nRF52840 Sense
#
# Two approaches:
#   1. PURE RUST (recommended) - No SoftDevice, requires debug probe (SWD)
#   2. SOFTDEVICE - Uses Nordic's BLE stack, works with UF2 bootloader
#
# Pure Rust Targets (require probe-rs + SWD connection):
#   make probe BIN=raw_blinky     - Flash minimal blinky via SWD
#   make probe BIN=pure_blinky    - Flash embassy blinky via SWD
#   make run BIN=raw_blinky       - Flash and show RTT output
#
# SoftDevice Targets (UF2 bootloader - currently broken due to SD interference):
#   make flash-sd BIN=sd_minimal  - Flash via UF2 bootloader
#
# Other:
#   make info    - Show available binaries
#   make clean   - Clean build artifacts
#
# Hardware Setup for Pure Rust:
#   Connect debug probe to Feather SWD pads:
#     - SWDIO (data)
#     - SWCLK (clock)
#     - GND
#   Then: probe-rs will erase SoftDevice and flash at 0x0

# =============================================================================
# Configuration
# =============================================================================

BIN ?= raw_blinky
TARGET = thumbv7em-none-eabihf
RELEASE_DIR = target/$(TARGET)/release
CHIP = nRF52840_xxAA

# UF2 settings (for SoftDevice builds only)
UF2_FAMILY = 0xADA52840
UF2_BASE_SD = 0x26000
UF2_BASE_PURE = 0x0
BOOTLOADER_MOUNT = /run/media/$(USER)/FTHRSNSBOOT

# Detect bootloader mount point
MOUNT_POINT := $(shell \
	if [ -d "$(BOOTLOADER_MOUNT)" ]; then echo "$(BOOTLOADER_MOUNT)"; \
	elif [ -d "/media/$(USER)/FTHRSNSBOOT" ]; then echo "/media/$(USER)/FTHRSNSBOOT"; \
	elif [ -d "/mnt/FTHRSNSBOOT" ]; then echo "/mnt/FTHRSNSBOOT"; \
	fi)

.PHONY: build build-sd build-ble build-hive probe run ble hive flash-sd uf2 uf2-sd clean info help

# =============================================================================
# Pure Rust Targets (recommended)
# =============================================================================

# Build pure Rust binary (default features include cortex-m critical section)
build:
	cargo build --release --bin $(BIN)

# Flash via probe-rs (erases SoftDevice, flashes at 0x0)
probe: build
	probe-rs run --chip $(CHIP) $(RELEASE_DIR)/$(BIN)

# Flash and attach to RTT for debug output
run: build
	probe-rs run --chip $(CHIP) $(RELEASE_DIR)/$(BIN)

# =============================================================================
# BLE Targets (nrf-sdc - pure Rust BLE)
# =============================================================================

# Environment for nrf-sdc bindgen (requires arm-none-eabi-gcc)
export BINDGEN_EXTRA_CLANG_ARGS = --sysroot=/usr/arm-none-eabi -I/usr/arm-none-eabi/include -I/usr/lib/gcc/arm-none-eabi/14.2.0/include

# Build BLE advertiser
build-ble:
	cargo build --release --no-default-features --features trouble --bin ble_advertiser

# Flash BLE advertiser
ble: build-ble
	probe-rs run --chip $(CHIP) $(RELEASE_DIR)/ble_advertiser

# Build HIVE mesh node (full GATT server for WearTAK)
build-hive:
	cargo build --release --no-default-features --features trouble --bin hive_node

# Flash HIVE mesh node
hive: build-hive
	probe-rs run --chip $(CHIP) $(RELEASE_DIR)/hive_node

# Just attach to RTT (board already running)
attach:
	probe-rs attach --chip $(CHIP) $(RELEASE_DIR)/$(BIN)

# Erase entire chip (removes SoftDevice and bootloader)
erase:
	probe-rs erase --chip $(CHIP) --allow-erase-all

# =============================================================================
# SoftDevice Targets (UF2 bootloader - currently broken)
# =============================================================================

# Build with SoftDevice support
build-sd:
	cargo build --release --no-default-features --features softdevice --bin $(BIN)

# Convert to UF2 for SoftDevice builds (base 0x26000)
uf2-sd: build-sd
	@mkdir -p target
	llvm-objcopy -O binary $(RELEASE_DIR)/$(BIN) target/$(BIN).bin
	uf2conv target/$(BIN).bin --base $(UF2_BASE_SD) --family $(UF2_FAMILY) --output target/$(BIN).uf2
	@echo "Created target/$(BIN).uf2 (base=$(UF2_BASE_SD))"
	@ls -lh target/$(BIN).uf2

# Flash via UF2 bootloader (double-tap reset first)
flash-sd: uf2-sd
	@if [ -z "$(MOUNT_POINT)" ]; then \
		echo "Error: Bootloader not mounted."; \
		echo "1. Double-tap reset to enter bootloader mode (green LED)"; \
		echo "2. Run: udisksctl mount -b /dev/sda"; \
		echo "3. Re-run this command"; \
		exit 1; \
	fi
	@echo "Copying $(BIN).uf2 to $(MOUNT_POINT)..."
	cp target/$(BIN).uf2 "$(MOUNT_POINT)/"
	sync
	@echo "Done! Board will reboot automatically."

# Mount bootloader drive (helper)
mount:
	@echo "Looking for bootloader device..."
	@lsblk -o NAME,SIZE,LABEL | grep -E "FTHR|32.1M" || true
	@echo ""
	@echo "If you see a ~32M device, mount it with:"
	@echo "  udisksctl mount -b /dev/sdX"

# =============================================================================
# Utility Targets
# =============================================================================

# Show available binaries and build info
info:
	@echo "=== Pure Rust Binaries (use with: make probe BIN=xxx) ==="
	@echo "  raw_blinky   - Minimal GPIO blinky, no embassy/async"
	@echo "  pure_blinky  - Embassy async blinky, no SoftDevice"
	@echo ""
	@echo "=== BLE Binaries (nrf-sdc, requires arm-none-eabi-gcc) ==="
	@echo "  make ble   - BLE advertising as HIVE-xxxx"
	@echo "  make hive  - HIVE GATT server for WearTAK (recommended)"
	@echo ""
	@echo "=== SoftDevice Binaries (use with: make flash-sd BIN=xxx) ==="
	@echo "  sd_minimal   - Minimal SoftDevice + embassy blinky"
	@echo "  blinky       - SoftDevice blinky test"
	@echo "  main (default binary) - Full HIVE SOS beacon"
	@echo ""
	@echo "Current: BIN=$(BIN)"
	@echo "Target:  $(TARGET)"
	@echo "Chip:    $(CHIP)"

help:
	@echo "Feather nRF52840 Sense - Pure Rust Development"
	@echo ""
	@echo "PURE RUST (requires SWD debug probe):"
	@echo "  make probe BIN=raw_blinky  - Flash minimal blinky"
	@echo "  make run BIN=raw_blinky    - Flash + RTT debug output"
	@echo "  make erase                 - Erase chip (removes SoftDevice)"
	@echo ""
	@echo "BLE (requires arm-none-eabi-gcc for nrf-sdc):"
	@echo "  make hive                  - Flash HIVE GATT server (WearTAK)"
	@echo "  make ble                   - Flash BLE advertiser only"
	@echo ""
	@echo "SOFTDEVICE (UF2 bootloader - currently broken):"
	@echo "  make flash-sd BIN=sd_minimal"
	@echo ""
	@echo "OTHER:"
	@echo "  make info   - Show available binaries"
	@echo "  make clean  - Clean build artifacts"

clean:
	cargo clean
	rm -f target/*.bin target/*.uf2
