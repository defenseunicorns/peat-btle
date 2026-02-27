# Security Architecture

> For Architects: Design decisions, threat model, and security boundaries

## Overview

peat-btle implements a layered security model designed for tactical mesh networks operating on resource-constrained devices. This document covers the architecture decisions, trade-offs, and known limitations.

## Threat Model

### In Scope

| Threat | Mitigation |
|--------|-----------|
| Passive eavesdropping | Mesh-wide encryption (Phase 1) |
| Message tampering | AEAD authentication (Poly1305) |
| Replay attacks | Per-peer message counters (Phase 2) |
| Unauthorized mesh participation | Shared secret requirement |
| Inter-member message interception | Per-peer E2EE (Phase 2) |

### Out of Scope (Current Implementation)

| Threat | Status | Notes |
|--------|--------|-------|
| Active man-in-the-middle | Partially mitigated | DeviceIdentity provides TOFU-based protection |
| Node impersonation | **Mitigated** | Ed25519 DeviceIdentity binds node_id to keypair |
| Traffic analysis | Not mitigated | mesh_id broadcast in clear |
| Compromised node revocation | Not mitigated | Requires secret rotation for all nodes |
| Key compromise recovery | Not mitigated | No key rotation mechanism |
| Side-channel attacks | Not mitigated | Standard crypto implementations |
| BLE pairing attacks | **Mitigated** | Application-layer encryption; BLE is not trust boundary |

### BLE Pairing Attack Resilience

**Threat**: Attacks like WhisperPair (CVE-2024-XXXXX) can downgrade BLE pairing
security by manipulating key exchange timing, resulting in weaker session keys.

**PEAT-BTLE Mitigation**: BLE link security is **not** the trust boundary.

1. **Discovery-only dependency**: Peat uses BLE for proximity detection and
   initial rendezvous. Security-critical operations require application-layer
   authentication per ADR-006.

2. **PKI verification**: Device identity is established via Ed25519 keypairs,
   verified at connection establishment before any CRDT sync occurs.

3. **Mesh-wide encryption**: ChaCha20-Poly1305 encrypts all sync payloads
   regardless of BLE security level.

4. **Defense in depth**: Even a fully compromised BLE link exposes only
   encrypted, authenticated traffic that cannot be injected into the Peat mesh.

**Recommendation**: For maximum security, require BLE Security Level 3+ for
sync operations (MITM-protected pairing) but design assuming it's compromised.

## Security Layers

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        APPLICATION BOUNDARY                              │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                    Phase 2: Per-Peer E2EE                          │  │
│  │  • X25519 key exchange per peer pair                               │  │
│  │  • ChaCha20-Poly1305 session encryption                            │  │
│  │  • Replay protection via monotonic counters                        │  │
│  │  • 46 bytes overhead                                               │  │
│  │  • Protects: other mesh members, compromised relays                │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                          │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                  Phase 1: Mesh-Wide Encryption                     │  │
│  │  • Shared 32-byte secret (out-of-band distribution)                │  │
│  │  • HKDF-SHA256 key derivation (secret + mesh_id)                   │  │
│  │  • ChaCha20-Poly1305 AEAD encryption                               │  │
│  │  • 30 bytes overhead                                               │  │
│  │  • Protects: external eavesdroppers                                │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                          │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                      Mesh Filtering                                │  │
│  │  • mesh_id in BLE advertisement (cleartext!)                       │  │
│  │  • Device name format: HIVE_<mesh_id>-<node_id>                    │  │
│  │  • Connection-time filtering only                                  │  │
│  │  • Protects: accidental cross-mesh pollution                       │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                          │
│                          BLE TRANSPORT                                   │
│                    (no native security used)                             │
└─────────────────────────────────────────────────────────────────────────┘
```

## Mesh Creation

### Identity Components

| Component | Size | Source | Purpose |
|-----------|------|--------|---------|
| `DeviceIdentity` | Ed25519 keypair | Generated per-device | Cryptographic identity |
| `node_id` | 32-bit | BLAKE3(public_key) | Unique node identifier |
| `callsign` | String | User-assigned | Human-readable name |
| `mesh_id` | 8-char hex | MeshGenesis | Network segregation |
| `encryption_secret` | 256-bit | MeshGenesis HKDF | Cryptographic access |

### Device Identity (New)

Each device generates a persistent Ed25519 keypair (`DeviceIdentity`):

```rust
use peat_btle::security::DeviceIdentity;

let identity = DeviceIdentity::generate();
let node_id = identity.node_id();        // Derived from public key
let attestation = identity.create_attestation();  // Signed proof
```

The `IdentityAttestation` contains:
- `node_id`: 4 bytes
- `public_key`: 32 bytes
- `timestamp_ms`: 8 bytes
- `signature`: 64 bytes (signs all above fields)

### Mesh Formation Flow

```
┌─────────────┐         ┌─────────────┐         ┌─────────────┐
│   Node A    │         │   Node B    │         │   Node C    │
│  (Leader)   │         │  (Member)   │         │  (Member)   │
└──────┬──────┘         └──────┬──────┘         └──────┬──────┘
       │                       │                       │
       │  ┌────────────────────────────────────────┐   │
       │  │     OUT-OF-BAND SECRET DISTRIBUTION    │   │
       │  │  (QR code, secure channel, physical)   │   │
       │  └────────────────────────────────────────┘   │
       │                       │                       │
       │ Configure:            │ Configure:            │ Configure:
       │ - mesh_id="ALPHA"     │ - mesh_id="ALPHA"     │ - mesh_id="ALPHA"
       │ - secret=0x42...      │ - secret=0x42...      │ - secret=0x42...
       │                       │                       │
       │─── BLE Advertise ────>│                       │
       │    "HIVE_ALPHA-AAA"   │                       │
       │                       │                       │
       │<── BLE Discover ──────│                       │
       │    mesh_id matches    │                       │
       │                       │                       │
       │<═══ GATT Connect ═════│                       │
       │                       │                       │
       │<── Encrypted Doc ─────│                       │
       │    (if secret valid,  │                       │
       │     merge succeeds)   │                       │
       │                       │                       │
```

### Key Derivation

```rust
// Mesh encryption key derivation
let hkdf = Hkdf::<Sha256>::new(
    Some(mesh_id.as_bytes()),  // Salt: mesh identifier
    &shared_secret             // IKM: 32-byte shared secret
);
let mut key = [0u8; 32];
hkdf.expand(b"PEAT-BTLE-mesh-encryption-v1", &mut key);
```

**Design Decision**: Using `mesh_id` as HKDF salt ensures that the same shared secret produces different keys for different meshes. This allows a single organization to use one master secret across multiple deployments.

## Access Control

### Current Model: Symmetric Shared Secret

```
┌─────────────────────────────────────────────────────────────┐
│                    ACCESS CONTROL MODEL                      │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│   "You're in the mesh if you know the secret"               │
│                                                              │
│   Membership = Possession of 32-byte secret                 │
│   Authentication = Ability to decrypt documents             │
│   Authorization = None (all members equivalent)             │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### Implications

| Aspect | Current Behavior | Security Implication |
|--------|-----------------|---------------------|
| Node joining | Automatic if secret matches | No approval workflow |
| Node removal | Impossible without secret change | No revocation |
| Role differentiation | None | All nodes are peers |
| Audit trail | None | No accountability |
| Key compromise | Total mesh compromise | Single point of failure |

## Membership Management

### Peer Discovery and Tracking

```rust
// PeerManager tracks discovered nodes
struct PeerManager {
    peers: BTreeMap<NodeId, PeatPeer>,      // Known peers
    identifier_map: BTreeMap<String, NodeId>, // BLE ID → NodeId
    sync_history: BTreeMap<NodeId, u64>,     // Last sync timestamps
}

// Peer lifecycle
on_discovered() → Add to peers (if mesh_id matches)
on_connected()  → Mark as connected
on_disconnected() → Mark as disconnected (still tracked)
cleanup_stale() → Remove after peer_timeout_ms (default: 45s)
```

### Membership Boundaries

```
┌─────────────────────────────────────────────────────────────┐
│                    MEMBERSHIP CONCEPT                        │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│   CURRENT: Dynamic, opportunistic                           │
│   - No predefined membership roster                         │
│   - Nodes join/leave based on BLE proximity                 │
│   - Staleness-based cleanup (timeout)                       │
│                                                              │
│   MISSING: Explicit membership control                      │
│   - No allow-list of authorized node_ids                    │
│   - No deny-list for revoked nodes                          │
│   - No membership attestation                               │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Cryptographic Details

### Phase 1: Mesh-Wide Encryption

| Property | Value |
|----------|-------|
| Algorithm | ChaCha20-Poly1305 |
| Key Size | 256 bits |
| Nonce | 96 bits (random per message) |
| Auth Tag | 128 bits |
| Wire Overhead | 30 bytes (2 marker + 12 nonce + 16 tag) |

**Wire Format**:
```
┌────────┬──────────┬───────────────┬─────────────────────────┐
│ 0xAE   │ Reserved │ Nonce (12B)   │ Ciphertext + Tag        │
│ 1 byte │ 1 byte   │ 12 bytes      │ plaintext_len + 16      │
└────────┴──────────┴───────────────┴─────────────────────────┘
```

### Phase 2: Per-Peer E2EE

| Property | Value |
|----------|-------|
| Key Exchange | X25519 ECDH |
| Key Derivation | HKDF-SHA256 |
| Encryption | ChaCha20-Poly1305 |
| Replay Protection | 64-bit monotonic counter |
| Session Timeout | 30 minutes (default) |
| Max Sessions | 16 (default) |

**Wire Format**:
```
┌────────┬──────────┬───────────┬───────────┬──────────┬───────────┬─────────────┐
│ 0xAF   │ Reserved │ Recipient │ Sender    │ Counter  │ Nonce     │ Ciphertext  │
│ 1 byte │ 1 byte   │ 4 bytes   │ 4 bytes   │ 8 bytes  │ 12 bytes  │ + Tag       │
└────────┴──────────┴───────────┴───────────┴──────────┴───────────┴─────────────┘
```

**Session Key Derivation**:
```rust
// Bind session key to both parties (prevents key confusion)
let info = format!(
    "PEAT-peer-session-{:08X}-{:08X}",
    min(our_id, peer_id),
    max(our_id, peer_id)
);
hkdf.expand(info.as_bytes(), &mut session_key);
```

## Design Trade-offs

### Why Symmetric Shared Secret?

| Alternative | Rejected Because |
|-------------|------------------|
| PKI/Certificates | Too complex for embedded, requires CA infrastructure |
| BLE Pairing | Platform-specific, can't relay encrypted across mesh |
| Pre-shared keys per pair | O(n²) key management for n nodes |
| Identity-based encryption | Complex, large code footprint |

**Chosen approach**: Single shared secret is simple, fits in embedded constraints (~2KB code), and provides adequate security for semi-trusted tactical environments.

### Why Application-Layer Encryption?

BLE provides native encryption (AES-CCM), but:
1. **Multi-hop**: BLE encryption terminates at each hop; messages would be plaintext at relay nodes
2. **Platform variance**: Pairing behavior differs across iOS/Android/ESP32
3. **Control**: Application-layer gives us consistent behavior everywhere

### Why Optional Encryption?

```rust
// Encrypted nodes can receive unencrypted (backward compat)
fn decrypt_document(&self, data: &[u8]) -> Option<Cow<[u8]>> {
    if data[0] == ENCRYPTED_MARKER {
        // Decrypt
    } else {
        // Accept unencrypted for gradual rollout
        Some(Cow::Borrowed(data))
    }
}
```

This enables:
- Development/testing without crypto overhead
- Gradual deployment (some nodes encrypted before all)
- Interop with legacy devices

**Risk**: Downgrade attacks possible without strict mode.

**Mitigation**: Enable strict encryption mode to reject unencrypted documents:

```rust
let config = PeatMeshConfig::new(node_id, callsign, mesh_id)
    .with_encryption(secret)
    .with_strict_encryption();  // Reject unencrypted
```

When strict mode is enabled, `SecurityViolation::UnencryptedInStrictMode` events are emitted for rejected documents.

## Known Limitations

### Critical Gaps

1. **No Key Rotation**
   - Secret is static for mesh lifetime
   - Compromise = permanent breach until manual re-key
   - **Mitigation**: Implement periodic key rotation protocol

2. **No Revocation**
   - Cannot remove a node without changing secret
   - Malicious node with secret stays forever
   - **Mitigation**: Implement deny-list with signed revocation

3. **Identity Binding** (Implemented)
   - Ed25519 `DeviceIdentity` binds node_id to public key
   - `IdentityAttestation` provides signed proof of identity
   - node_id derived from public key hash (BLAKE3)
   - **Status**: Core primitives implemented, integration pending

4. **mesh_id in Cleartext**
   - BLE advertisements expose network identifier
   - Enables traffic analysis, targeting
   - **Mitigation**: Randomized advertisement with encrypted payload

5. **No Forward Secrecy (Phase 1)**
   - Compromise of shared secret decrypts all past messages
   - **Mitigation**: Per-peer E2EE provides forward secrecy for sensitive comms

### Resource Constraints

| Platform | RAM Limit | Crypto Code | Sessions |
|----------|-----------|-------------|----------|
| ESP32 | ~256KB | ~15KB | 4-8 |
| Smartwatch | ~64KB | ~15KB | 2-4 |
| Phone | Unlimited | ~15KB | 16+ |

## Recommendations

### For Demo/Development
- Use mesh_id="DEMO" with no encryption
- Focus on functionality, not security

### For Semi-Trusted Environments
- Enable mesh-wide encryption
- Use unique secret per mesh
- Accept current limitations

### For Adversarial Environments
**Use with caution.** Available security features:
- Identity binding via `DeviceIdentity` (Ed25519)
- Mesh genesis with `MeshGenesis` protocol

Still required:
- Key rotation protocol
- Revocation mechanism
- Encrypted advertisements
- Audit logging
- TOFU registry integration

## Future Work

### Planned Improvements

1. **v0.2: Key Rotation**
   - Leader-initiated rotation protocol
   - Graceful transition period
   - Backward-compatible wire format

2. **v0.3: Membership Control**
   - Signed membership roster
   - Join approval workflow
   - Revocation propagation

3. **v0.4: Identity Binding** (Implemented in v0.0.12)
   - ~~Device attestation~~ `DeviceIdentity` with Ed25519
   - ~~Node certificate chain~~ `IdentityAttestation` with signatures
   - ~~Impersonation detection~~ node_id derived from public key
   - Pending: TOFU registry, document signing integration

### Research Areas

- Post-quantum key exchange (Kyber)
- Deniable authentication
- Anonymous mesh membership
- Hardware security module integration

## References

- [Ed25519 (RFC 8032)](https://datatracker.ietf.org/doc/html/rfc8032) - Device identity signatures
- [BLAKE3](https://github.com/BLAKE3-team/BLAKE3) - Node ID derivation, key derivation
- [ChaCha20-Poly1305 (RFC 8439)](https://datatracker.ietf.org/doc/html/rfc8439)
- [X25519 (RFC 7748)](https://datatracker.ietf.org/doc/html/rfc7748)
- [HKDF (RFC 5869)](https://datatracker.ietf.org/doc/html/rfc5869)
- [BLE 5.0 Specification](https://www.bluetooth.com/specifications/specs/)
