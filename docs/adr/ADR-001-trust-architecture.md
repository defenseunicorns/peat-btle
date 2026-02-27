# ADR-001: Trust Architecture for Tactical BLE Mesh Networks

**Status**: Proposed
**Date**: 2026-01-14
**Authors**: (r)evolve Team
**Reviewers**: Android-ATAK, WearTAK, Core Platform
**Related Issues**: c3d5b46, 97f090e, 3920c2c, ba4bcb5
**HIVE Framework ADRs**: ADR-006 (Security), ADR-039 (BLE Transport), ADR-044 (E2E Encryption)

---

## Executive Summary

This ADR proposes a unified trust architecture for peat-btle that addresses four critical security gaps: identity binding, encrypted advertisements, membership control, and key rotation. Rather than treating these as independent features, we present a cohesive cryptographic identity model where each component builds upon the others.

The architecture introduces **device-bound Ed25519 identities** as the foundation, enabling:
- Cryptographic proof of node identity (eliminates impersonation)
- Private mesh discovery (encrypted BLE advertisements)
- Granular membership control (revoke individuals, not secrets)
- Secure key rotation (distribute new keys only to verified members)

**Implementation Timeline**: 4 phases over ~8 weeks of focused development.

---

## Context

### The Problem

peat-btle currently operates on a **shared secret model**: if you know the 32-byte mesh secret, you're a member. This creates a binary trust boundary with no gradations:

```
┌─────────────────────────────────────────────────────────────────┐
│                    CURRENT TRUST MODEL                          │
│                                                                 │
│   Outside                    │                   Inside          │
│   ─────────                  │                   ──────          │
│   • Cannot decrypt           │   • Full mesh access             │
│   • Cannot participate       │   • Can claim ANY identity       │
│   • Can still detect mesh    │   • Cannot be individually       │
│     (traffic analysis)       │     removed                      │
│                              │   • Shares fate with all         │
│                              │     other members                │
│                              │                                  │
│              SECRET BOUNDARY (single point of failure)          │
└─────────────────────────────────────────────────────────────────┘
```

This model has four critical vulnerabilities:

| Vulnerability | Impact | Exploitability |
|--------------|--------|----------------|
| **Identity Spoofing** | Attacker with secret impersonates any node | High - no verification |
| **Mesh Detection** | Adversary maps all Peat networks in area | High - cleartext ads |
| **No Revocation** | Compromised device = permanent breach | Critical - no recovery |
| **Static Keys** | Key compromise decrypts all past traffic | High - no rotation |

### Why Now?

As peat-btle moves toward production deployment with ATAK/WearTAK integration, the stakes increase. A mesh network carrying tactical situational awareness data requires trust guarantees beyond "you have the password."

### Guiding Principles

1. **Defense in Depth**: Multiple layers, each independently valuable
2. **Graceful Degradation**: Partial implementation still improves security
3. **Embedded-First**: Must work on ESP32 with 256KB RAM
4. **Zero Trust Relay**: Intermediate nodes learn nothing they don't need
5. **Backward Compatibility**: Phased rollout without flag day

---

## Decision

### The Trust Model

We introduce **cryptographic device identity** as the foundational primitive. Every node generates a persistent Ed25519 keypair, and the node_id becomes a *derivation* of that public key rather than a self-asserted value.

```
┌─────────────────────────────────────────────────────────────────┐
│                    PROPOSED TRUST MODEL                         │
│                                                                 │
│                    ┌─────────────────────┐                      │
│                    │   Device Identity   │                      │
│                    │   (Ed25519 keypair) │                      │
│                    └──────────┬──────────┘                      │
│                               │                                 │
│              ┌────────────────┼────────────────┐                │
│              │                │                │                │
│              ▼                ▼                ▼                │
│    ┌─────────────────┐ ┌───────────┐ ┌─────────────────┐        │
│    │  Node Identity  │ │  Message  │ │   Membership    │        │
│    │  (node_id from  │ │ Signatures│ │   Attestation   │        │
│    │   pubkey hash)  │ │           │ │                 │        │
│    └────────┬────────┘ └─────┬─────┘ └────────┬────────┘        │
│             │                │                │                 │
│             └────────────────┴────────────────┘                 │
│                              │                                  │
│              ┌───────────────┼───────────────┐                  │
│              ▼               ▼               ▼                  │
│    ┌──────────────┐  ┌──────────────┐  ┌──────────────┐         │
│    │  Encrypted   │  │  Membership  │  │     Key      │         │
│    │    Beacons   │  │   Control    │  │   Rotation   │         │
│    └──────────────┘  └──────────────┘  └──────────────┘         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Core Components

#### 1. Device Identity (Foundation)

Each device generates a persistent Ed25519 keypair on first run:

```rust
pub struct DeviceIdentity {
    /// Ed25519 signing key (32 bytes, stored in secure enclave)
    signing_key: ed25519_dalek::SigningKey,

    /// Cached public key (32 bytes)
    public_key: [u8; 32],

    /// Derived node_id (first 4 bytes of BLAKE3(public_key))
    node_id: NodeId,
}

impl DeviceIdentity {
    /// Generate new identity (called once per device lifetime)
    pub fn generate() -> Self;

    /// Load from platform secure storage
    pub fn load(storage: &dyn SecureStorage) -> Result<Self, IdentityError>;

    /// Sign arbitrary data
    pub fn sign(&self, message: &[u8]) -> Signature;

    /// Verify signature from another identity
    pub fn verify(public_key: &[u8; 32], message: &[u8], sig: &Signature) -> bool;
}
```

**Node ID Derivation** (deterministic, collision-resistant):

```rust
fn derive_node_id(public_key: &[u8; 32]) -> NodeId {
    let hash = blake3::hash(public_key);
    NodeId::new(u32::from_le_bytes(hash.as_bytes()[0..4].try_into().unwrap()))
}
```

**Storage by Platform**:

| Platform | Secure Storage | Fallback |
|----------|---------------|----------|
| iOS | Secure Enclave / Keychain | None (require hardware) |
| Android | Android Keystore (TEE) | Encrypted SharedPrefs |
| ESP32 | NVS with encryption | Plain NVS (dev only) |

#### 2. Identity Attestation (Wire Format)

Documents carry cryptographic proof of authorship:

```
┌─────────────────────────────────────────────────────────────────┐
│ IDENTITY_ATTESTATION_MARKER (0xB3)                              │
├─────────────────────────────────────────────────────────────────┤
│ marker:          1 byte  (0xB3)                                 │
│ flags:           1 byte  (bit 0: includes_pubkey)               │
│ node_id:         4 bytes (LE u32)                               │
│ timestamp:       8 bytes (LE u64, milliseconds)                 │
│ public_key:      32 bytes (if flags.includes_pubkey)            │
│ signature:       64 bytes (Ed25519 over preceding fields)       │
├─────────────────────────────────────────────────────────────────┤
│ Total: 78 bytes (with pubkey) or 46 bytes (without)             │
└─────────────────────────────────────────────────────────────────┘
```

**Signature Computation**:

```rust
// Sign: marker || flags || node_id || timestamp || [public_key]
let mut signing_input = Vec::with_capacity(46);
signing_input.push(IDENTITY_ATTESTATION_MARKER);
signing_input.push(flags);
signing_input.extend_from_slice(&node_id.to_le_bytes());
signing_input.extend_from_slice(&timestamp.to_le_bytes());
if flags & 0x01 != 0 {
    signing_input.extend_from_slice(&public_key);
}
let signature = identity.sign(&signing_input);
```

**Trust-On-First-Use (TOFU)**:

```rust
pub struct IdentityRegistry {
    /// Known identities: node_id → public_key
    known: HashMap<NodeId, [u8; 32]>,

    /// When each identity was first seen
    first_seen: HashMap<NodeId, u64>,
}

impl IdentityRegistry {
    /// Returns Ok(true) if new, Ok(false) if known, Err if mismatch
    pub fn verify_or_register(
        &mut self,
        attestation: &IdentityAttestation,
    ) -> Result<bool, IdentityMismatch>;
}
```

#### 3. Encrypted Beacons (Privacy)

BLE advertisements reveal nothing to passive observers:

```
┌─────────────────────────────────────────────────────────────────┐
│ BLE ADVERTISEMENT (31 bytes max)                                │
├─────────────────────────────────────────────────────────────────┤
│ Flags:              3 bytes (0x02 0x01 0x06)                    │
│ Complete Local Name: 6 bytes (0x05 0x09 "PEAT")                 │
│ Service Data:       22 bytes                                    │
│   └─ Length:        1 byte  (0x15 = 21)                         │
│   └─ Type:          1 byte  (0x16 = Service Data)               │
│   └─ UUID:          2 bytes (0x7AF4 = Peat service)             │
│   └─ Version:       1 byte  (0x01)                              │
│   └─ Encrypted:     12 bytes (ChaCha8 ciphertext)               │
│   └─ MAC:           4 bytes (truncated Poly1305)                │
├─────────────────────────────────────────────────────────────────┤
│ Total: 31 bytes                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Encrypted Payload** (12 bytes plaintext → 12 bytes ciphertext):

```rust
struct BeaconPayload {
    mesh_id_hash: [u8; 4],  // BLAKE3(mesh_id)[0..4]
    node_id: [u8; 4],       // Node identifier
    epoch: [u8; 2],         // Key epoch (for rotation)
    capabilities: [u8; 2],  // Bitflags (relay, gateway, etc.)
}
```

**Beacon Key Derivation**:

```rust
// Beacon key rotates with time slot (default: 15 minutes)
fn derive_beacon_key(mesh_secret: &[u8; 32], time_slot: u64) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key("PEAT-beacon-v1");
    hasher.update(mesh_secret);
    hasher.update(&time_slot.to_le_bytes());
    *hasher.finalize().as_bytes()
}
```

**Scanning Behavior**:

```rust
impl PeatMesh {
    /// Attempt to decrypt beacon; returns None for non-mesh devices
    pub fn try_decrypt_beacon(&self, service_data: &[u8]) -> Option<BeaconInfo>;

    /// Generate our encrypted beacon
    pub fn build_beacon(&self, now_ms: u64) -> [u8; 17];
}
```

#### 4. Membership Control (Authorization)

Fine-grained control over who can participate:

```rust
pub struct MembershipPolicy {
    /// Operating mode
    mode: MembershipMode,

    /// Authorized public keys (if mode = AllowList)
    allow_list: HashSet<[u8; 32]>,

    /// Revoked public keys (always enforced)
    revoked: HashSet<[u8; 32]>,

    /// Policy version (monotonic, for sync)
    version: u64,
}

pub enum MembershipMode {
    /// Anyone with mesh secret can join (current behavior)
    Open,

    /// Only pre-authorized public keys can join
    AllowList,

    /// Anyone except revoked keys can join
    DenyList,
}
```

**Revocation Message** (Wire Format 0xB4):

```
┌─────────────────────────────────────────────────────────────────┐
│ REVOCATION_MARKER (0xB4)                                        │
├─────────────────────────────────────────────────────────────────┤
│ marker:          1 byte  (0xB4)                                 │
│ flags:           1 byte  (bit 0: is_self_revocation)            │
│ revoked_pubkey:  32 bytes                                       │
│ reason:          1 byte  (0=unspecified, 1=compromised,         │
│                           2=lost, 3=administrative)             │
│ timestamp:       8 bytes (LE u64)                               │
│ issuer_pubkey:   32 bytes                                       │
│ signature:       64 bytes (Ed25519)                             │
├─────────────────────────────────────────────────────────────────┤
│ Total: 139 bytes                                                │
└─────────────────────────────────────────────────────────────────┘
```

**Revocation Authority Model**:

We implement **threshold revocation** with configurable policy:

```rust
pub struct RevocationPolicy {
    /// Minimum signatures required to revoke
    threshold: u8,

    /// Public keys authorized to issue revocations
    authorities: HashSet<[u8; 32]>,

    /// Allow self-revocation (device can remove itself)
    allow_self_revoke: bool,
}
```

Default: `threshold=1` with mesh creator as sole authority (simple leader model).

#### 5. Key Rotation (Recovery)

Secure transition to new mesh encryption key:

```
┌─────────────────────────────────────────────────────────────────┐
│ KEY_ROTATION_MARKER (0xB5)                                      │
├─────────────────────────────────────────────────────────────────┤
│ marker:          1 byte  (0xB5)                                 │
│ flags:           1 byte  (bit 0: is_announcement,               │
│                           bit 1: is_commitment,                 │
│                           bit 2: is_completion)                 │
│ epoch:           4 bytes (LE u32, new key epoch)                │
│ transition_end:  8 bytes (LE u64, grace period end timestamp)   │
│ key_commitment:  32 bytes (BLAKE3 hash of new key)              │
│ issuer_pubkey:   32 bytes                                       │
│ signature:       64 bytes (Ed25519)                             │
├─────────────────────────────────────────────────────────────────┤
│ Total: 142 bytes                                                │
└─────────────────────────────────────────────────────────────────┘
```

**Rotation Protocol**:

```
Phase 1: Announcement (Leader → All)
─────────────────────────────────────
Leader broadcasts signed rotation announcement with:
  - New epoch number
  - Transition end timestamp (now + grace_period)
  - Commitment to new key (hash only, not the key itself)

Phase 2: Key Distribution (Leader → Each Member via E2EE)
─────────────────────────────────────────────────────────
Leader sends new key to each verified member via per-peer E2EE.
Only nodes with valid identity attestation receive the new key.
Revoked nodes are explicitly excluded.

Phase 3: Transition (All Nodes)
───────────────────────────────
During grace period, nodes accept BOTH old and new epoch keys.
Nodes begin encrypting with new key immediately upon receipt.

Phase 4: Completion (Leader → All)
─────────────────────────────────
After grace period, leader broadcasts completion message.
Old epoch key is deleted; messages with old epoch rejected.
```

**Key Derivation Chain**:

```rust
// Each epoch derives from previous (forward secrecy)
fn derive_epoch_key(
    base_secret: &[u8; 32],
    mesh_id: &str,
    epoch: u32,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key("PEAT-epoch-key-v1");
    hasher.update(base_secret);
    hasher.update(mesh_id.as_bytes());
    hasher.update(&epoch.to_le_bytes());
    *hasher.finalize().as_bytes()
}
```

---

## Implementation Plan

### Phase 1: Device Identity (Foundation)

**Duration**: 2 weeks
**Issue**: 97f090e
**Dependencies**: None

| Task | Effort | Output |
|------|--------|--------|
| `DeviceIdentity` struct and generation | 2d | `src/identity/mod.rs` |
| Platform secure storage traits | 2d | `src/identity/storage.rs` |
| Android Keystore implementation | 2d | `src/platform/android/keystore.rs` |
| iOS Keychain implementation | 2d | `src/platform/ios/keychain.rs` |
| ESP32 NVS implementation | 1d | `src/platform/esp32/nvs_identity.rs` |
| Node ID derivation migration | 1d | Update `PeatMeshConfig` |
| Identity attestation wire format | 1d | `src/identity/attestation.rs` |
| TOFU registry | 1d | `src/identity/registry.rs` |
| Integration tests | 2d | `tests/identity_*.rs` |

**Acceptance Criteria**:
- [ ] Each device generates persistent Ed25519 identity
- [ ] node_id derived from public key hash
- [ ] Identity attestation in documents (optional, feature-flagged)
- [ ] TOFU registry detects key changes
- [ ] All platform storage implementations pass tests

**Wire Format Marker**: `0xB3` (IDENTITY_ATTESTATION_MARKER)

### Phase 2: Encrypted Beacons (Privacy)

**Duration**: 1.5 weeks
**Issue**: c3d5b46
**Dependencies**: Phase 1 (identity for beacon signing)

| Task | Effort | Output |
|------|--------|--------|
| `BeaconPayload` struct | 0.5d | `src/beacon/mod.rs` |
| ChaCha8 beacon encryption | 1d | `src/beacon/crypto.rs` |
| Time-slotted key derivation | 0.5d | `src/beacon/keys.rs` |
| Android advertisement update | 1d | `PeatBtle.kt` |
| iOS advertisement update | 1d | `PeatBLE.swift` |
| ESP32 NimBLE update | 1d | Platform adapter |
| Backward compat scanner | 1d | Detect old vs new format |
| Integration tests | 1d | `tests/beacon_*.rs` |

**Acceptance Criteria**:
- [ ] Device name is generic "PEAT" (no identifiers)
- [ ] Mesh identity encrypted in service data
- [ ] Only secret-holders can identify mesh members
- [ ] Scanning still works with service UUID filter
- [ ] Backward compatible with legacy advertisements

**Wire Format**: Service Data UUID 0xF47A with encrypted payload

### Phase 3: Membership Control (Authorization)

**Duration**: 2 weeks
**Issue**: 3920c2c
**Dependencies**: Phase 1 (identity for revocation signatures)

| Task | Effort | Output |
|------|--------|--------|
| `MembershipPolicy` struct | 1d | `src/membership/mod.rs` |
| Allow/deny list filtering | 1d | `src/membership/filter.rs` |
| Revocation message format | 1d | `src/membership/revocation.rs` |
| Revocation CRDT (grow-only set) | 1d | `src/membership/crdt.rs` |
| Revocation propagation | 1d | Relay through mesh |
| Persistent revocation storage | 1d | Platform storage |
| Policy sync protocol | 2d | `src/membership/sync.rs` |
| Android/iOS API exposure | 1d | JNI + Swift bindings |
| Integration tests | 2d | `tests/membership_*.rs` |

**Acceptance Criteria**:
- [ ] Static allow-list restricts discovery
- [ ] Deny-list rejects known-bad nodes
- [ ] Revocation messages propagate mesh-wide
- [ ] Revoked nodes excluded after propagation
- [ ] Revocations persist across restart
- [ ] Cannot self-revoke (DoS prevention)

**Wire Format Marker**: `0xB4` (REVOCATION_MARKER)

### Phase 4: Key Rotation (Recovery)

**Duration**: 2.5 weeks
**Issue**: ba4bcb5
**Dependencies**: Phase 1 (identity), Phase 3 (membership for distribution)

| Task | Effort | Output |
|------|--------|--------|
| Epoch key derivation | 1d | `src/rotation/keys.rs` |
| Rotation announcement message | 1d | `src/rotation/messages.rs` |
| Multi-epoch decryption support | 1d | Accept old + new during transition |
| E2EE key distribution | 2d | Leverage existing per-peer E2EE |
| Rotation state machine | 2d | `src/rotation/state.rs` |
| Leader rotation initiation | 1d | `PeatMesh::initiate_rotation()` |
| Automatic old-key purge | 1d | After transition period |
| Platform API exposure | 1d | JNI + Swift bindings |
| Integration tests | 2d | `tests/rotation_*.rs` |
| Chaos testing (partitions) | 1d | Verify convergence |

**Acceptance Criteria**:
- [ ] Leader can initiate rotation
- [ ] All verified members receive new key via E2EE
- [ ] Both keys valid during grace period
- [ ] Old key rejected after transition
- [ ] Works despite partial mesh connectivity
- [ ] Revoked nodes excluded from key distribution

**Wire Format Marker**: `0xB5` (KEY_ROTATION_MARKER)

---

## Wire Format Summary

| Marker | Hex | Name | Size | Phase |
|--------|-----|------|------|-------|
| `IDENTITY_ATTESTATION_MARKER` | `0xB3` | Identity proof | 46-78 bytes | 1 |
| `REVOCATION_MARKER` | `0xB4` | Membership revocation | 139 bytes | 3 |
| `KEY_ROTATION_MARKER` | `0xB5` | Key rotation control | 142 bytes | 4 |
| (beacon) | - | Encrypted advertisement | 17 bytes | 2 |

---

## Security Analysis

### Threat Mitigation Matrix

| Threat | Before | After | Residual Risk |
|--------|--------|-------|---------------|
| Identity spoofing | Trivial | Requires private key theft | Platform secure enclave |
| Mesh detection | Passive scan | Requires secret + timing | Traffic volume analysis |
| Node revocation | Impossible | Per-device revocation | Authority compromise |
| Key compromise | Permanent | Recoverable via rotation | Transition window |
| Replay attacks | Counter-based | Counter + identity binding | Clock skew |

### Attack Surface Changes

**Reduced**:
- Impersonation (now requires device private key)
- Traffic analysis (encrypted beacons)
- Permanent compromise (key rotation)
- Blast radius (individual revocation)

**Unchanged**:
- Physical device compromise
- Side-channel attacks on crypto
- Denial of service (RF jamming)

**New**:
- TOFU first-contact vulnerability (mitigated by out-of-band verification)
- Authority key compromise (mitigated by threshold revocation)
- Transition window exposure (minimized by short grace period)

### Formal Properties

1. **Identity Binding**: ∀ messages m, verify(m.signature, m.pubkey, m.content) ⟹ sender controls private key
2. **Revocation Consistency**: Once node N is revoked, ∀ honest nodes eventually reject N
3. **Key Freshness**: After rotation completes, messages with old epoch are rejected
4. **Beacon Privacy**: Without mesh secret, beacon payload is indistinguishable from random

---

## HIVE Framework Integration

peat-btle is the BLE transport layer within the broader HIVE (Hierarchical Intelligence for Versatile Entities) framework. This trust architecture must integrate seamlessly with HIVE's existing security model while supporting standalone operation.

### Credential Integration

HIVE uses `HiveCredentials` for backend authentication:

```rust
// hive-protocol/src/credentials.rs
pub struct HiveCredentials {
    app_id: String,           // Application identifier (HIVE_APP_ID)
    secret_key: Option<String>, // Shared secret (HIVE_SECRET_KEY, base64)
    offline_token: Option<String>,
}
```

peat-btle maps these credentials as follows:

| HIVE Credential | peat-btle Mapping | Usage |
|-----------------|-------------------|-------|
| `HIVE_APP_ID` | `mesh_id` derivation | `mesh_id = BLAKE3(app_id)[0..4]` as hex |
| `HIVE_SECRET_KEY` | `encryption_secret` | Mesh-wide encryption key |
| Device keypair | `DeviceIdentity` | Ed25519 identity (new) |

### Integration Modes

#### Mode 1: HIVE-Managed (Recommended)

When peat-btle operates as a transport under hive-protocol:

```rust
impl PeatBtleTransport {
    /// Create transport from HIVE credentials
    pub fn from_hive_credentials(
        creds: &HiveCredentials,
        device_identity: DeviceIdentity,
    ) -> Result<Self> {
        let mesh_id = derive_mesh_id(creds.app_id());
        let encryption_secret = creds.require_secret_key()?
            .as_bytes()
            .try_into()
            .map_err(|_| Error::InvalidSecret)?;

        Ok(Self::new(PeatMeshConfig::new(
            device_identity.node_id(),
            &device_identity.callsign(),
            &mesh_id,
        ).with_encryption(&encryption_secret)
         .with_identity(device_identity)))
    }
}
```

In this mode:
- Credentials flow from hive-protocol
- Device identity may be provisioned by higher-level PKI (ADR-006)
- peat-btle provides transport-layer trust; hive-protocol provides application-layer trust

#### Mode 2: Standalone Operation

When peat-btle operates independently (e.g., pure BLE mesh without HIVE backend):

```rust
// Environment variables for standalone operation
// HIVE_BTLE_MESH_ID     - 4-character mesh identifier
// HIVE_BTLE_SECRET      - 32-byte hex-encoded encryption secret
// HIVE_BTLE_CALLSIGN    - Human-readable device name

impl PeatMeshConfig {
    /// Load configuration from environment (standalone mode)
    pub fn from_env() -> Result<Self> {
        let mesh_id = env::var("HIVE_BTLE_MESH_ID")
            .or_else(|_| env::var("HIVE_APP_ID").map(|id| derive_mesh_id(&id)))
            .unwrap_or_else(|_| "DEMO".to_string());

        let secret = env::var("HIVE_BTLE_SECRET")
            .or_else(|_| env::var("HIVE_SECRET_KEY"))
            .ok()
            .and_then(|s| hex::decode(&s).ok());

        // Generate or load device identity
        let identity = DeviceIdentity::load_or_generate()?;

        let mut config = Self::new(
            identity.node_id(),
            &env::var("PEAT_BTLE_CALLSIGN").unwrap_or_else(|_| "PEAT".to_string()),
            &mesh_id,
        ).with_identity(identity);

        if let Some(secret) = secret {
            config = config.with_encryption(&secret.try_into()?);
        }

        Ok(config)
    }
}
```

### Identity Bridging

When connected to HIVE-protocol, device identities can be upgraded:

```
┌─────────────────────────────────────────────────────────────────┐
│                    IDENTITY HIERARCHY                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │           HIVE-Protocol Layer (ADR-006)                  │   │
│  │  X.509 Certificates, PKI, Challenge-Response Auth       │   │
│  │  DeviceId → full certificate chain verification         │   │
│  └───────────────────────────┬─────────────────────────────┘   │
│                              │                                  │
│                              │ Identity Binding                 │
│                              │ (pubkey attestation)             │
│                              ▼                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │           peat-btle Transport Layer (This ADR)           │   │
│  │  Ed25519 Device Identity, TOFU, Mesh-Scoped Trust       │   │
│  │  NodeId → pubkey hash (standalone) or cert-bound        │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Identity Binding Protocol**:

When a node has both peat-btle identity and hive-protocol identity:

```rust
/// Bind peat-btle identity to hive-protocol certificate
pub struct IdentityBinding {
    /// peat-btle Ed25519 public key
    btle_pubkey: [u8; 32],

    /// hive-protocol X.509 certificate hash
    cert_hash: [u8; 32],

    /// Signature by X.509 private key over (btle_pubkey || cert_hash)
    binding_signature: Vec<u8>,
}
```

This allows:
1. Standalone nodes to use TOFU-based trust
2. HIVE-connected nodes to verify via certificate chain
3. Graceful upgrade when BLE-only nodes join HIVE network

### Membership Synchronization

When peat-btle is managed by hive-protocol:

```rust
/// Callback interface for HIVE-managed membership
pub trait MembershipDelegate {
    /// Called when a new node requests to join (for approval)
    fn on_join_request(&self, pubkey: &[u8; 32]) -> JoinDecision;

    /// Called to get current membership roster
    fn get_roster(&self) -> Vec<[u8; 32]>;

    /// Called when peat-btle wants to revoke a node
    fn request_revocation(&self, pubkey: &[u8; 32], reason: RevocationReason);
}

impl PeatMesh {
    /// Set delegate for HIVE-managed membership
    pub fn set_membership_delegate(&mut self, delegate: Arc<dyn MembershipDelegate>);
}
```

This allows hive-protocol to:
- Approve/deny join requests based on PKI verification
- Push roster updates from cell membership changes
- Coordinate revocations across HIVE and peat-btle layers

### Key Rotation Coordination

When rotation is initiated at the HIVE level:

```rust
/// Key rotation can be initiated by either layer
pub enum RotationSource {
    /// Initiated by peat-btle (e.g., local compromise detected)
    BtleLocal,

    /// Initiated by hive-protocol (e.g., cell key rotation)
    HiveProtocol { new_secret: [u8; 32] },
}

impl PeatMesh {
    /// Handle rotation from HIVE protocol layer
    pub fn on_hive_key_rotation(&mut self, new_secret: &[u8; 32]) -> Result<()> {
        // Skip announcement phase - HIVE already coordinated
        // Go directly to key update
        self.rotation_state = RotationState::Transitioning {
            old_epoch: self.current_epoch,
            new_epoch: self.current_epoch + 1,
            new_key: derive_epoch_key(new_secret, &self.mesh_id, self.current_epoch + 1),
            transition_end: now_ms() + self.config.rotation_grace_period_ms,
        };
        Ok(())
    }
}
```

---

## Alternatives Considered

### PKI/Certificate Authority

**Rejected**: Too complex for embedded devices, requires online CA or pre-provisioned certs.

### BLE Bonding/Pairing

**Rejected**: Platform-specific behavior, doesn't protect at relay nodes, doesn't scale.

### Web of Trust

**Rejected**: Complex key management, unclear trust semantics for tactical use.

### Pre-Shared Keys Per Pair

**Rejected**: O(n²) key management, doesn't scale beyond small meshes.

---

## Backward Compatibility

### Migration Strategy

```
Week 0-2: Deploy Phase 1 (identity) with feature flag disabled
Week 2-4: Enable identity attestation, monitor for issues
Week 4-5: Deploy Phase 2 (beacons) with legacy fallback
Week 5-7: Deploy Phase 3 (membership) in permissive mode
Week 7-9: Deploy Phase 4 (rotation), validate with test rotation
Week 9+:  Enable strict mode, remove legacy code paths
```

### Feature Flags

```rust
pub struct SecurityFeatures {
    /// Require identity attestation on all documents
    pub require_identity: bool,

    /// Use encrypted beacons (disable for legacy interop)
    pub encrypted_beacons: bool,

    /// Enforce membership policy
    pub enforce_membership: bool,

    /// Enable key rotation protocol
    pub enable_rotation: bool,
}
```

### Version Negotiation

Nodes advertise capability bits in beacon:

| Bit | Capability |
|-----|------------|
| 0 | Supports identity attestation |
| 1 | Supports encrypted beacons |
| 2 | Supports membership control |
| 3 | Supports key rotation |

---

## Open Questions

1. **Authority Bootstrap**: How is the initial rotation authority designated?
   - *Proposed*: First node to create mesh becomes authority; can delegate.

2. **Cross-Mesh Trust**: Can identities be trusted across different meshes?
   - *Proposed*: Identity is device-bound, not mesh-bound; trust is mesh-scoped.

3. **Recovery from Total Authority Compromise**: What if all authority keys are lost?
   - *Proposed*: Emergency re-bootstrap with new mesh_id; document as operational procedure.

4. **Identity Backup/Restore**: Can a device identity be backed up?
   - *Proposed*: Platform-dependent; generally discouraged for security.

---

## Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Identity adoption | 100% of v0.1.0+ nodes | Telemetry |
| Beacon privacy | 0 cleartext advertisements | Field audit |
| Revocation latency | <30s mesh-wide propagation | Test harness |
| Rotation success | 100% of verified nodes | Test rotation |
| Code size increase | <20KB on ESP32 | Build metrics |
| Signature verification | <10ms on ESP32 | Benchmarks |

---

## References

- [Ed25519 (RFC 8032)](https://datatracker.ietf.org/doc/html/rfc8032)
- [BLAKE3 Specification](https://github.com/BLAKE3-team/BLAKE3-specs)
- [ChaCha20-Poly1305 (RFC 8439)](https://datatracker.ietf.org/doc/html/rfc8439)
- [BLE 5.0 Core Specification](https://www.bluetooth.com/specifications/specs/)
- [Android Keystore](https://developer.android.com/training/articles/keystore)
- [iOS Secure Enclave](https://support.apple.com/guide/security/secure-enclave-sec59b0b31ff/web)
- Existing: `docs/security-architecture.md`

---

## Appendix A: Cryptographic Overhead Budget

| Operation | ESP32 (160MHz) | Phone | Size |
|-----------|---------------|-------|------|
| Ed25519 keygen | ~50ms | <1ms | 64B privkey |
| Ed25519 sign | ~15ms | <1ms | 64B signature |
| Ed25519 verify | ~25ms | <1ms | - |
| ChaCha8 beacon encrypt | <1ms | <0.1ms | 16B overhead |
| BLAKE3 hash (32B) | <1ms | <0.1ms | - |

**Total per-document overhead**: 46-78 bytes (identity attestation)
**Total per-beacon overhead**: 4 bytes (MAC)
**RAM for identity registry**: ~40 bytes per known peer

---

## Appendix B: Issue Rework Recommendations

Based on this unified architecture, the existing issues should be updated:

### c3d5b46 (Encrypted Advertisements)
- Update wire format to match beacon specification above
- Add dependency on Phase 1 for beacon signing
- Remove Option A/B; proceed with unified approach

### 97f090e (Identity Binding)
- Expand to include TOFU registry specification
- Add platform storage requirements
- Mark as **foundation** for other phases

### 3920c2c (Membership Control)
- Change wire format marker from 0xB1 to 0xB4 (0xB1 is relay)
- Add threshold revocation as default policy
- Specify CRDT semantics for revocation propagation

### ba4bcb5 (Key Rotation)
- Change wire format marker from 0xB0 to 0xB5 (0xB0 is key exchange)
- Add dependency on Phase 1 (identity) and Phase 3 (membership)
- Specify E2EE key distribution mechanism

---

*This document represents our commitment to building security that protects the people who depend on it. Every design decision prioritizes the humans at the edge of the network over architectural elegance. The best security is security that works when everything else has failed.*
