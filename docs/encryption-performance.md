# Encryption Performance Analysis

> Benchmark results and QoS considerations for ChaCha20-Poly1305 encryption

## Benchmark Results

Tested on Linux x86_64, debug build, 100 iterations per test.

### Latency Per Operation

| Document Size | Plain Build | Encrypted Build | Overhead | Plain Sync | Encrypted Sync | Overhead |
|--------------|-------------|-----------------|----------|------------|----------------|----------|
| Minimal (counter only) | 1µs | 64µs | +63µs | 1µs | 105µs | +104µs |
| Small (1 chat) | <1µs | 77µs | +76µs | 2µs | 146µs | +144µs |
| Medium (5 chats) | 1µs | 131µs | +130µs | 4µs | 255µs | +251µs |
| Large (10 chats) | 4µs | 147µs | +143µs | 8µs | 290µs | +282µs |
| Very Large (20 chats) | 4µs | 145µs | +141µs | 8µs | 288µs | +280µs |

### Size Overhead

| Component | Size |
|-----------|------|
| Encrypted marker | 2 bytes |
| Nonce | 12 bytes |
| Auth tag | 16 bytes |
| **Total overhead** | **30 bytes** |

### Throughput (10 chat messages, 632 bytes)

| Mode | Docs/sec | KB/sec | Throughput |
|------|----------|--------|------------|
| Unencrypted | 247,743 | 152,904 | 1.25 Gbps |
| Encrypted | 7,148 | 4,621 | 38 Mbps |
| **Reduction** | **97.1%** | | |

## Analysis

### Acceptable for BLE Mesh

The encryption overhead is acceptable for tactical BLE mesh operations:

1. **Sync intervals**: Typical sync every 100-500ms; 100-300µs overhead is <1% of cycle
2. **BLE bottleneck**: BLE 4.x max ~1-2 Mbps; encryption's 38 Mbps throughput exceeds this
3. **Size overhead**: 30 bytes is ~5% for typical documents (500-600 bytes)

### Concerns for High-Frequency Operations

For scenarios requiring high-frequency updates or resource-constrained devices:

1. **Emergency broadcasts**: May need <10ms latency for rapid ACK propagation
2. **Location updates**: High-frequency GPS tracks could stress encryption
3. **Battery impact**: Crypto operations consume CPU cycles on watches/sensors
4. **Multi-hop relay**: Each hop adds encryption overhead

## QoS Encryption Strategies

### Proposed: Selective Encryption

Not all data requires the same security level. A QoS-aware encryption system could:

```
┌─────────────────────────────────────────────────────────────────┐
│                    ENCRYPTION QoS LEVELS                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Level 0: NONE (Development/Testing)                            │
│  - No encryption overhead                                       │
│  - Use for: demos, debugging, trusted networks                  │
│                                                                 │
│  Level 1: MESH-WIDE (Current default)                           │
│  - ChaCha20-Poly1305 with shared secret                         │
│  - Use for: general mesh traffic                                │
│                                                                 │
│  Level 2: SELECTIVE                                             │
│  - Encrypt sensitive data (chat, locations)                     │
│  - Skip encryption for: counters, heartbeats                    │
│  - Use for: battery-constrained devices                         │
│                                                                 │
│  Level 3: PER-PEER E2EE                                         │
│  - Full end-to-end encryption per peer pair                     │
│  - Use for: sensitive comms, untrusted relays                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Implementation Options

#### Option A: Data Type Classification

```rust
enum EncryptionPolicy {
    /// Never encrypt (counters, heartbeats)
    Never,
    /// Encrypt with mesh-wide key (default)
    MeshWide,
    /// Encrypt with per-peer session key
    PeerE2EE,
}

// Example classification
fn encryption_policy_for(data_type: DataType) -> EncryptionPolicy {
    match data_type {
        DataType::Counter => EncryptionPolicy::Never,
        DataType::Heartbeat => EncryptionPolicy::Never,
        DataType::Location => EncryptionPolicy::MeshWide,
        DataType::Chat => EncryptionPolicy::MeshWide,
        DataType::Emergency => EncryptionPolicy::MeshWide,
        DataType::SensitiveChat => EncryptionPolicy::PeerE2EE,
    }
}
```

#### Option B: Connection-Based Policy

```rust
enum ConnectionEncryption {
    /// Trusted local connection (same device, loopback)
    None,
    /// Trusted mesh peer (known, authenticated)
    MeshWide,
    /// Untrusted relay or unknown peer
    PeerE2EE,
}
```

#### Option C: Adaptive Based on Conditions

```rust
struct AdaptiveEncryption {
    /// Disable encryption when battery below threshold
    battery_threshold_percent: u8,
    /// Disable for high-frequency updates (>10 Hz)
    high_frequency_threshold_hz: f32,
    /// Always encrypt these data types regardless
    always_encrypt: Vec<DataType>,
}
```

### Security vs Performance Tradeoffs

| Strategy | Latency | Security | Battery | Complexity |
|----------|---------|----------|---------|------------|
| Always encrypt | High | Maximum | High drain | Low |
| Selective by data type | Medium | Good | Medium | Medium |
| Selective by connection | Medium | Good | Medium | Medium |
| Adaptive | Low-Medium | Variable | Optimal | High |
| Never encrypt | Lowest | None | Minimal | Lowest |

## Recommendations

### Short Term (v0.1.x)

1. Keep current mesh-wide encryption as default
2. Add `strict_encryption: false` option to accept unencrypted from legacy
3. Document performance characteristics for integrators

### Medium Term (v0.2.x)

1. Implement data type classification
2. Allow per-CRDT encryption policy
3. Add battery-aware encryption toggle

### Long Term (v0.3.x)

1. Full adaptive encryption system
2. Per-connection encryption negotiation
3. Hardware crypto acceleration support (ESP32, ARM TrustZone)

## Running the Benchmark

```bash
# Run all encryption benchmarks
cargo test --test encryption_benchmark --features linux -- --nocapture

# Run specific test
cargo test --test encryption_benchmark --features linux benchmark_encryption_latency -- --nocapture
```

## References

- [ChaCha20-Poly1305 RFC 8439](https://datatracker.ietf.org/doc/html/rfc8439)
- [Security Architecture](./security-architecture.md)
- [ADR-002: Mesh Provisioning](./adr/02-mesh-provisioning.md)
