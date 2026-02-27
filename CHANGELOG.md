# Changelog

All notable changes to peat-btle will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-02-12

### Added
- **Android**: High-priority sync mode for time-critical state updates
- **Android**: WearOS reliability improvements (reconnect on re-discovery, stale peer cleanup, address rotation handling, auto-reconnect with exponential backoff)
- **feather-sense**: Peat GATT server for WearTAK connectivity on nRF52840
- **feather-sense**: Pure Rust BLE advertising with nrf-sdc
- **feather-sense**: probe-rs build support and BLE target in Makefile
- **macOS**: GATT client with bidirectional sync
- BLE connection management infrastructure with UniFFI exports
- Range test node for WearTAK field testing
- Adafruit Feather Sense (nRF52840) example support
- CannedMessage CRDT document sync via delta mechanism
- Extensible document registry for CRDT sync
- nRF52840 sensor beacon example

### Fixed
- **Android**: Reconnect to peers when re-discovered after disconnect
- **Android**: Clean up stale connected peers and improve display names
- **Android**: Peer tracking with name-based deduplication
- **Android**: Handle WearOS BLE address rotation for stable mesh state
- **Android**: Prevent GATT server registration leaks
- **Android**: Prevent unwanted BLE pairing requests on Samsung devices
- **Apple**: CoreBluetooth memory management segfault
- **Linux**: BLE connection handling and advertisement improvements
- Relay deduplication and callsign cache
- CI checkout and format/clippy/macOS example gating fixes

### Changed
- **feather-sense**: Corrected GPIO P1 base address for raw_blinky

## [0.1.0-rc.30] - 2026-01-29

### Added
- `MembershipToken`: Lightweight authority-signed tokens (128 bytes) for constrained devices
  - Binds public_key to callsign with mesh_id and expiration
  - Wire format: `[pubkey:32][mesh_id:4][callsign:12][issued:8][expires:8][sig:64]`
- `SignedPayload`: Transport-agnostic signing utilities for BLE and WiFi/IP
  - Wire format: `[marker:1][payload:N][signature:64]`
- `IdentityRegistry` extended with callsign support:
  - `register_member()` validates and stores membership tokens
  - `get_callsign()` / `find_by_callsign()` for lookups
  - Persistence format v2 with backwards compatibility

## [0.1.0-rc.29] - 2026-01-27

### Added
- Functional BLE loopback test automation (kitlab ↔ Pi)
- `ble_responder` and `ble_test_client` example binaries
- Delta sync auto-registration on peer connect/disconnect
- `decrypt_only()` API for transport-only decryption

### Fixed
- UInt formatting in Android logs (`.toLong()` for `String.format`)
- Added `updatePeripheralState()` convenience method (ATAK team contribution)

### Changed
- CI now runs functional BLE test via SSH to Raspberry Pi

## [0.1.0-rc.28] - 2026-01-26

### Changed
- **BREAKING**: Migrated Android bindings from manual JNI to UniFFI
  - All Rust types now accessed via `uniffi.peat_btle` package
  - PeatMesh construction uses `newFromGenesis()` or `newWithPeripheral()` factory methods
  - Method parameters now use Kotlin unsigned types (UInt, ULong, UByte)
  - BLE callback timestamps require `.toULong()` conversion

### Added
- UniFFI bindings module (`src/uniffi_bindings.rs`) with full PeatMesh API
- Generated Kotlin bindings (`android/src/main/kotlin/uniffi/peat_btle/peat_btle.kt`)
- Chat methods exposed via UniFFI: `sendChat`, `sendChatReply`, `chatCount`, `getAllChatMessages`, `getChatMessagesSince`
- `updatePeripheralState` method for efficient encrypted state updates
- `deriveNodeIdFromMac` standalone function
- Peer state types via UniFFI: `ConnectionState`, `PeerConnectionState`, `StateCountSummary`, `FullStateCountSummary`, `IndirectPeer`, `ViaPeerRoute`
- Peer state methods: `getPeerConnectionState`, `getDegradedPeers`, `getLostPeers`, `getConnectionStateCounts`, `getIndirectPeers`, `getFullStateCounts`

### Fixed
- Removed JNI native method calls from callback proxies (`ScanCallbackProxy`, `GattCallbackProxy`, `AdvertiseCallbackProxy`) that caused `UnsatisfiedLinkError` at runtime

### Removed
- Manual JNI bridge (`src/platform/android/jni_bridge.rs`)
- JNI-based Kotlin files: `PeatMesh.kt`, `DeviceIdentity.kt`, `MeshGenesis.kt`, `IdentityAttestation.kt`
- JNI native method declarations from callback proxy classes
- `System.loadLibrary("peat_btle")` calls (UniFFI/JNA handles library loading automatically)
- `jni` and `ndk` crate dependencies

### Migration Guide
See `docs/UNIFFI_MIGRATION.md` for Android integration updates.

## [0.0.12] - 2026-01-19

### Added
- ADR-002: Mesh Provisioning and Node Onboarding architecture
- Codex.md with Radicle workflow guide and CI documentation
- Security implementation roadmap with 8 tracked issues

### Fixed
- Clippy warnings in linux adapter (derivable_impls, type_complexity, manual_strip, clone_on_copy)
- Range contains checks in CRDT validation
- linux_scanner example (rand dependency, callback types)
- Code formatting across multiple files

## [0.0.11] - 2026-01-18

### Added
- ChatCRDT for persistent mesh chat with reply threading
- Chat message deduplication in Android bindings
- MTU overflow protection (CHAT_SYNC_LIMIT=8)
- Profiling stress test example

### Fixed
- BLE MTU overflow crash with large chat histories
- Duplicate chat notifications in Android

## [0.1.0] - 2024-12-01

### Added
- Initial release
- Linux platform support with BlueZ/bluer
- Core BLE transport architecture
- BleAdapter trait for platform abstraction
- Peat beacon format and discovery protocol
- GATT service definition (0xF47A)
- Power profile management (Aggressive, Balanced, LowPower, UltraLow)
- Lightweight CRDT implementations (GCounter, LWWRegister, ORSet)
- Hierarchical mesh topology support
- Delta-based synchronization protocol
- Coded PHY support for extended range (BLE 5.0+)

### Platform Support
- Linux (BlueZ 5.48+) - Complete
- macOS (CoreBluetooth) - Complete
- iOS (CoreBluetooth) - Complete
- ESP32 (NimBLE) - Complete
- Android - In Progress
- Windows - Planned
