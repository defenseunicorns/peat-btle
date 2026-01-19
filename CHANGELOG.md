# Changelog

All notable changes to hive-btle will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- HIVE beacon format and discovery protocol
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
