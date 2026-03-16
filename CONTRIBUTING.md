# Contributing to peat-btle

Thank you for your interest in contributing to peat-btle. This document covers development setup, testing, and the pull request process.

## Getting Started

1. Fork the repository and clone your fork
2. Create a feature branch from `main`
3. Make your changes
4. Run pre-commit checks
5. Submit a pull request

## Development Setup

### Prerequisites

- Rust stable toolchain (install via [rustup](https://rustup.rs))
- Platform-specific dependencies:
  - **Linux**: BlueZ 5.48+, D-Bus development libraries (`libdbus-1-dev`)
  - **macOS**: Xcode Command Line Tools
  - **ESP32**: ESP-IDF toolchain
  - **Android**: Android NDK + UniFFI

### Feature Flags

| Feature | Description |
|---------|-------------|
| `linux` | Linux BlueZ adapter |
| `macos` | macOS CoreBluetooth adapter |
| `ios` | iOS CoreBluetooth adapter (Beta) |
| `android` | Android BLE adapter with UniFFI bindings |
| `windows` | Windows WinRT adapter (Planned) |
| `esp32` | ESP32 NimBLE adapter |
| `transport-only` | Core transport without mesh management |

### Building

```bash
# Build for your platform
cargo build --features linux
cargo build --features macos

# Android AAR
cd android && ./gradlew assembleRelease

# ESP32
cargo +esp build --target xtensa-esp32-espidf --features esp32
```

## Testing

```bash
# Unit tests (no hardware required)
cargo test

# Tests with platform features
cargo test --features linux

# Specific test module
cargo test sync::

# Integration tests
cargo test --test mesh_sync
cargo test --test emergency_flow
```

Integration tests in `tests/` cover mesh sync, peer connections, emergency flow, and encryption. Test on real hardware when possible — BLE simulators have limitations.

## Pre-Commit Checks

Before submitting a PR, ensure all of the following pass locally:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

The CI pipeline runs these same checks on every PR.

## Branching Strategy

We use **trunk-based development** on `main` with short-lived feature branches:

- Branch from `main` for all changes
- Keep branches small and focused (prefer multiple small PRs over one large one)
- Squash-and-merge to `main`

## Commit Requirements

- **GPG-signed commits are required.** Configure commit signing per [GitHub's documentation](https://docs.github.com/en/authentication/managing-commit-signature-verification).
- Write clear, descriptive commit messages

## Pull Request Access

Submitting pull requests requires contributor access to the repository. If you're interested in contributing, please open an issue to introduce yourself and discuss the change you'd like to make. A maintainer will grant PR access to active contributors.

## Pull Request Access

Submitting pull requests requires contributor access to the repository. If you're interested in contributing, please open an issue to introduce yourself and discuss the change you'd like to make. A maintainer will grant PR access to active contributors.

## Pull Request Process

1. Open a PR against `main` with a clear description of the change
2. Focus each PR on a single concern
3. Ensure CI passes (fmt, clippy, tests)
4. PRs require at least one approving review from a CODEOWNERS member
5. PRs are squash-merged to maintain a clean history

## Radicle Patches

peat-btle also accepts contributions via [Radicle](https://radicle.xyz). The repository includes a `.goa` CI script that automatically runs format checks, clippy, tests, and example builds against incoming patches. Community patches require a delegate to comment `ok-to-test` before CI runs.

## Architectural Changes

For significant architectural changes, open an issue first to discuss the approach. Reference the relevant ADR in `docs/adr/` if one exists, or propose a new one.

## Reporting Issues

Use GitHub Issues to report bugs or request features. Include platform/OS version, Bluetooth hardware details, steps to reproduce, expected vs. actual behavior, and relevant log output.

## License

By contributing, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).
