# ADR-002: Mesh Provisioning and Node Onboarding

**Status**: Draft
**Date**: 2026-01-19
**Authors**: (r)evolve Team
**Depends On**: ADR-001 (Trust Architecture)
**Related Issues**: 97f090e (Identity), 3920c2c (Membership)

---

## Executive Summary

This ADR defines how HIVE meshes are created, how nodes are provisioned to join, and how trust is established in different operational scenarios. It distinguishes between **user-attended nodes** (phones, tablets) and **userless nodes** (sensors, beacons, wearables) with different provisioning requirements.

The key insight: **The mesh doesn't exist until the first node creates it.** That genesis moment establishes the security parameters that all subsequent nodes must conform to.

---

## The Bootstrap Problem

### Current State

Today, configuring a hive-btle node requires manually providing:

```kotlin
val config = HiveMeshConfig(
    nodeId = NodeId.fromMacAddress(bluetoothAdapter.address),
    callsign = "ALPHA-1",
    meshId = "DEMO",
    encryptionSecret = someSecretBytes  // Where does this come from?
)
```

**Unanswered questions:**

1. Who decides the `mesh_id`?
2. Who generates the `encryption_secret`?
3. How does node #2 get the secret from node #1?
4. What if node #1 is a sensor with no UI?
5. How do we know the secret wasn't intercepted?

### The Trust Chain Gap

```
┌─────────────────────────────────────────────────────────────────┐
│                    WHERE DOES TRUST START?                       │
│                                                                  │
│     ┌──────────┐         ????          ┌──────────┐             │
│     │  Node A  │ ──────────────────▶   │  Node B  │             │
│     │ (phone)  │    How does B get    │ (sensor) │             │
│     └──────────┘    the secret?        └──────────┘             │
│                                                                  │
│  Options today:                                                  │
│  • Pre-shared (deployed with same secret) - No revocation       │
│  • QR code scan (requires UI on both) - Doesn't work for sensors│
│  • Bluetooth pairing (OS-level) - Doesn't bind to mesh          │
│  • Manual entry (type 64 hex chars) - Error-prone, insecure     │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Decision

### Node Classification

We define three node classes with different provisioning capabilities:

| Class | Examples | Has UI | Has User | Provisioning |
|-------|----------|--------|----------|--------------|
| **Controller** | Phone, Tablet, Laptop | Yes | Yes | Can create mesh, provision others |
| **Attended** | Smartwatch, HUD | Limited | Yes | Receives provisioning via UI |
| **Unattended** | Sensor, Beacon, Relay | No | No | Factory provisioned or auto-enrolled |

### Mesh Lifecycle

```
┌─────────────────────────────────────────────────────────────────┐
│                      MESH LIFECYCLE                              │
│                                                                  │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐         │
│   │   GENESIS   │───▶│   ACTIVE    │───▶│  ROTATION   │──┐      │
│   │ (1st node)  │    │ (operating) │    │ (key change)│  │      │
│   └─────────────┘    └─────────────┘    └─────────────┘  │      │
│         │                   │                  │          │      │
│         │                   ▼                  │          │      │
│         │           ┌─────────────┐            │          │      │
│         │           │   ENROLL    │◀───────────┘          │      │
│         │           │ (new nodes) │                       │      │
│         │           └─────────────┘                       │      │
│         │                   │                             │      │
│         │                   ▼                             │      │
│         │           ┌─────────────┐                       │      │
│         └──────────▶│   REVOKE    │───────────────────────┘      │
│                     │ (remove bad)│                              │
│                     └─────────────┘                              │
└─────────────────────────────────────────────────────────────────┘
```

---

## Genesis: Creating a New Mesh

### Who Can Create?

Only **Controller** class nodes can create a mesh (requires UI for confirmation).

### What Happens at Genesis?

```rust
/// Create a new mesh as the founding controller
pub struct MeshGenesis {
    /// Human-readable mesh name (becomes mesh_id hash)
    pub mesh_name: String,

    /// Generated cryptographic seed (256 bits of entropy)
    pub mesh_seed: [u8; 32],

    /// Creator's device identity (from ADR-001)
    pub creator_identity: DeviceIdentity,

    /// Timestamp of creation
    pub created_at: u64,

    /// Initial membership policy
    pub policy: MembershipPolicy,
}

impl MeshGenesis {
    /// Generate new mesh (called by controller UI)
    pub fn create(mesh_name: &str, creator: &DeviceIdentity) -> Self {
        let mesh_seed = generate_entropy();  // Platform CSPRNG

        Self {
            mesh_name: mesh_name.into(),
            mesh_seed,
            creator_identity: creator.clone(),
            created_at: now_ms(),
            policy: MembershipPolicy::default(),  // Creator is sole authority
        }
    }

    /// Derive the mesh_id (deterministic from name + seed)
    pub fn mesh_id(&self) -> String {
        let hash = blake3::keyed_hash(&self.mesh_seed, self.mesh_name.as_bytes());
        // First 4 bytes as hex = 8 character mesh_id
        hex::encode(&hash.as_bytes()[0..4]).to_uppercase()
    }

    /// Derive the encryption secret
    pub fn encryption_secret(&self) -> [u8; 32] {
        blake3::derive_key("HIVE-mesh-encryption-v1", &self.mesh_seed)
    }

    /// Derive the beacon key base
    pub fn beacon_key_base(&self) -> [u8; 32] {
        blake3::derive_key("HIVE-beacon-key-v1", &self.mesh_seed)
    }
}
```

### Genesis UI Flow (Controller)

```
┌─────────────────────────────────────────────────────────────────┐
│                    CREATE NEW MESH                               │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  Mesh Name: [ALPHA-TEAM________________]                │    │
│  │                                                         │    │
│  │  Your Callsign: [ALPHA-1_______________]                │    │
│  │                                                         │    │
│  │  Security Level:                                        │    │
│  │    ○ Open (anyone with mesh ID can attempt join)        │    │
│  │    ● Controlled (explicit enrollment required)          │    │
│  │    ○ Strict (pre-provisioned devices only)              │    │
│  │                                                         │    │
│  │  [ ] Enable encrypted advertisements                    │    │
│  │  [x] Require identity attestation                       │    │
│  │                                                         │    │
│  │              [CREATE MESH]                              │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│  ⚠️  You will be the only authority for this mesh.              │
│     Keep the mesh seed secure - it cannot be recovered.         │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Enrollment: Adding Nodes to Existing Mesh

### Enrollment Methods by Node Class

| Method | Controller→Controller | Controller→Attended | Controller→Unattended |
|--------|----------------------|---------------------|----------------------|
| **QR Code** | ✓ Recommended | ✓ If has camera | ✗ No UI |
| **NFC Tap** | ✓ Alternative | ✓ Recommended | ✓ If NFC equipped |
| **BLE Provisioning** | ✓ Backup | ✓ Backup | ✓ Primary |
| **Pre-shared** | ✗ Avoid | ✗ Avoid | ✓ Factory deploy |

### Method 1: QR Code Enrollment (Controller → Attended)

The enrolling controller generates a time-limited enrollment token:

```rust
pub struct EnrollmentToken {
    /// Mesh identification
    mesh_id: String,

    /// Encrypted mesh seed (encrypted to enrollee's ephemeral key)
    encrypted_seed: Vec<u8>,

    /// Controller's public key (for verification)
    controller_pubkey: [u8; 32],

    /// Expiration timestamp
    expires_at: u64,

    /// Signature over all fields
    signature: [u8; 64],
}

impl EnrollmentToken {
    /// Generate QR code content
    pub fn to_qr(&self) -> String {
        // Format: HIVE://enroll/v1/<base64url-encoded-token>
        format!("HIVE://enroll/v1/{}", base64url::encode(&self.encode()))
    }
}
```

**QR Content** (fits in QR code < 500 bytes):

```
HIVE://enroll/v1/eyJtIjoiQUxQSEEiLCJlIjoiYmFzZTY0Li4uIiwiayI6IjAxMjM0NTY3ODlhYmNkZWYuLi4iLCJ4IjoxNzA1NjgwMDAwLCJzIjoiYmFzZTY0Li4uIn0
```

**Enrollment Flow:**

```
┌──────────────────┐                    ┌──────────────────┐
│    Controller    │                    │    New Node      │
│   (has mesh)     │                    │  (wants to join) │
└────────┬─────────┘                    └────────┬─────────┘
         │                                       │
         │  1. Controller selects "Add Device"   │
         │                                       │
         │  2. New node generates ephemeral      │
         │     keypair, displays QR with pubkey  │
         │                              ◀────────┤
         │                                       │
         │  3. Controller scans new node's QR    │
         ├────────▶                              │
         │                                       │
         │  4. Controller generates enrollment   │
         │     token encrypted to ephemeral key  │
         │                                       │
         │  5. Controller displays enrollment QR │
         │                              ◀────────┤
         │                                       │
         │  6. New node scans enrollment QR      │
         ├────────▶                              │
         │                                       │
         │  7. New node decrypts seed, derives   │
         │     all mesh keys, joins mesh         │
         │                                       │
         │  8. New node broadcasts attestation   │
         │     with its device identity          │
         │                              ─────────┤
         │                                       │
         │  9. Controller receives attestation,  │
         │     adds to membership roster         │
         ├──────────                             │
         │                                       │
         ▼                                       ▼
    ┌─────────┐                            ┌─────────┐
    │  MESH   │◀──────────────────────────▶│  MESH   │
    │ MEMBER  │     Now synchronized       │ MEMBER  │
    └─────────┘                            └─────────┘
```

### Method 2: BLE Provisioning (Controller → Unattended)

For sensors/beacons with no UI, the controller pushes provisioning data over a secure BLE channel.

```rust
/// BLE Provisioning Service UUID: 0xF47B (distinct from mesh sync)
const PROVISIONING_SERVICE_UUID: u16 = 0xF47B;

/// Provisioning characteristic
const PROVISIONING_CHAR_UUID: Uuid = uuid!("f47b0001-...");

pub struct ProvisioningSession {
    /// ECDH ephemeral for this session
    our_ephemeral: x25519_dalek::EphemeralSecret,

    /// Device being provisioned
    target_device: BluetoothDevice,

    /// Session key (after ECDH)
    session_key: Option<[u8; 32]>,
}

impl ProvisioningSession {
    /// Initiate provisioning to a device
    pub async fn provision(
        &mut self,
        genesis: &MeshGenesis,
        target_pubkey: [u8; 32],
    ) -> Result<(), ProvisionError> {
        // 1. ECDH key exchange
        let shared = self.our_ephemeral.diffie_hellman(&target_pubkey);
        self.session_key = Some(derive_session_key(&shared));

        // 2. Build provisioning packet
        let packet = ProvisioningPacket {
            mesh_seed: genesis.mesh_seed,
            mesh_name: genesis.mesh_name.clone(),
            controller_pubkey: genesis.creator_identity.public_key(),
            timestamp: now_ms(),
        };

        // 3. Encrypt and send
        let encrypted = encrypt_provisioning(&self.session_key.unwrap(), &packet);
        self.write_characteristic(PROVISIONING_CHAR_UUID, &encrypted).await?;

        // 4. Wait for attestation response
        let response = self.read_characteristic(PROVISIONING_CHAR_UUID).await?;

        Ok(())
    }
}
```

**Physical Security:**

For unattended nodes, BLE provisioning requires **physical proximity** (RSSI > -50 dBm enforced). This prevents remote provisioning of sensors across a building.

```rust
impl ProvisioningSession {
    fn verify_proximity(&self, rssi: i8) -> Result<(), ProvisionError> {
        const MIN_PROVISIONING_RSSI: i8 = -50;  // ~1 meter
        if rssi < MIN_PROVISIONING_RSSI {
            return Err(ProvisionError::TooFar {
                rssi,
                required: MIN_PROVISIONING_RSSI
            });
        }
        Ok(())
    }
}
```

### Method 3: Factory Pre-Provisioning (Unattended)

For large deployments, devices can be provisioned at manufacturing/staging time:

```rust
/// Pre-provisioning data burned into device at factory
pub struct FactoryProvision {
    /// Mesh seed (encrypted with device-specific key)
    encrypted_mesh_seed: [u8; 48],  // 32 + 16 (nonce + tag)

    /// Expected mesh_id (for verification)
    expected_mesh_id: String,

    /// Device identity (generated at factory, key in secure element)
    device_identity: DeviceIdentity,

    /// Provisioning authority's public key
    authority_pubkey: [u8; 32],
}
```

**Factory Flow:**

1. Generate device identity on secure manufacturing station
2. Encrypt mesh seed with device-unique key
3. Flash encrypted provision + device identity to NVS
4. Device private key never leaves secure element

---

## User vs Userless Nodes: Trust Implications

### User-Attended Nodes (Phones, Watches)

- **Identity**: Bound to user account (ATAK login, WearOS profile)
- **Authority**: User can approve/deny operations
- **Revocation**: User can self-revoke if compromised
- **Provisioning**: Interactive (QR, NFC, manual)

```rust
pub struct UserNode {
    device_identity: DeviceIdentity,
    user_binding: Option<UserBinding>,  // ATAK/WearTAK user ID
}

pub struct UserBinding {
    user_id: String,           // e.g., "alpha1@unit.mil"
    binding_timestamp: u64,
    binding_signature: [u8; 64],  // User signs device pubkey
}
```

### Userless Nodes (Sensors, Relays)

- **Identity**: Device identity only (no user)
- **Authority**: None - follows mesh policy
- **Revocation**: Must be revoked by authority
- **Provisioning**: Automated (BLE, factory)

```rust
pub struct SensorNode {
    device_identity: DeviceIdentity,
    provisioner: [u8; 32],      // Pubkey of who provisioned
    provisioned_at: u64,
    capabilities: SensorCapabilities,
}
```

### Trust Hierarchy

```
┌─────────────────────────────────────────────────────────────────┐
│                     TRUST HIERARCHY                              │
│                                                                  │
│                    ┌───────────────┐                            │
│                    │  Mesh Creator │  ← Sole initial authority  │
│                    │  (Controller) │                            │
│                    └───────┬───────┘                            │
│                            │                                     │
│              ┌─────────────┼─────────────┐                      │
│              │             │             │                      │
│              ▼             ▼             ▼                      │
│     ┌────────────┐ ┌────────────┐ ┌────────────┐               │
│     │ Delegated  │ │   User     │ │   User     │               │
│     │ Authority  │ │  Node A    │ │  Node B    │               │
│     │(can revoke)│ │ (member)   │ │ (member)   │               │
│     └─────┬──────┘ └────────────┘ └────────────┘               │
│           │                                                     │
│           │ can provision                                       │
│           ▼                                                     │
│     ┌────────────┐ ┌────────────┐ ┌────────────┐               │
│     │  Sensor 1  │ │  Sensor 2  │ │   Relay    │               │
│     │ (unattend) │ │ (unattend) │ │ (unattend) │               │
│     └────────────┘ └────────────┘ └────────────┘               │
│                                                                  │
│  Trust flows DOWN. Sensors cannot provision or revoke.          │
│  Users can self-revoke. Authorities can revoke anyone below.    │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Offline/Disconnected Scenarios

### Scenario: New Node Joins While Creator Offline

If the mesh creator is offline, can new nodes still join?

**Policy-dependent:**

| Policy | Behavior |
|--------|----------|
| Open | Yes - any node with mesh_id can attempt discovery |
| Controlled | Partial - can join if another authority is online |
| Strict | No - requires authority signature on enrollment |

### Scenario: Sensor Reboots Without Network

Sensors must persist provisioning data:

```rust
pub struct PersistedState {
    // Stored in encrypted NVS
    mesh_seed: [u8; 32],
    device_identity: DeviceIdentity,
    known_peers: Vec<(NodeId, [u8; 32])>,  // TOFU cache
    revocation_list: Vec<[u8; 32]>,        // Revoked pubkeys
}
```

On boot, sensor:
1. Loads persisted state
2. Derives mesh keys
3. Starts advertising (encrypted beacon)
4. Syncs revocation list when peers connect

---

## Security Considerations

### Provisioning Attack Vectors

| Attack | Mitigation |
|--------|------------|
| Evil twin (fake provisioner) | Verify provisioner's identity attestation |
| MITM on QR exchange | QR contains pubkeys, ECDH prevents MITM |
| Stolen enrollment token | Tokens expire (5 min default), single-use |
| Factory backdoor | Device key generated on-device, never exported |
| Proximity spoofing | Require RSSI > -50 dBm, physical verification |

### Revocation of Provisioner

If a device that provisioned others is revoked:
- Its provisioned devices are NOT automatically revoked
- Authority must explicitly revoke each if needed
- Provisioning relationships are logged for audit

---

## Implementation Phases

### Phase 1: Genesis & Manual Provisioning (MVP)
- Controller can create mesh
- QR code enrollment for attended devices
- Pre-shared secret for unattended (existing behavior)

### Phase 2: BLE Provisioning
- Secure BLE provisioning protocol
- Proximity verification
- Provisioning audit log

### Phase 3: Factory Provisioning Support
- Secure element integration
- Batch provisioning tools
- Certificate-based (for enterprise)

---

## API Surface

```rust
// Mesh creation (Controller only)
impl HiveMesh {
    /// Create a new mesh as founder
    pub fn create_mesh(name: &str, policy: MembershipPolicy) -> Result<MeshGenesis, Error>;

    /// Join existing mesh with provisioning data
    pub fn join_mesh(provisioning: ProvisioningData) -> Result<Self, Error>;
}

// Enrollment (Controller/Authority only)
impl HiveMesh {
    /// Generate QR enrollment token
    pub fn generate_enrollment_qr(&self, duration_secs: u64) -> Result<String, Error>;

    /// Parse scanned enrollment QR
    pub fn parse_enrollment_qr(qr_data: &str) -> Result<EnrollmentToken, Error>;

    /// Accept enrollment token and join mesh
    pub fn accept_enrollment(&mut self, token: EnrollmentToken) -> Result<(), Error>;

    /// Provision unattended device over BLE
    pub async fn provision_device(&self, device: BluetoothDevice) -> Result<(), Error>;
}

// Status
impl HiveMesh {
    /// Check if we are the mesh creator
    pub fn is_mesh_creator(&self) -> bool;

    /// Get our role in the trust hierarchy
    pub fn trust_role(&self) -> TrustRole;

    /// List devices we provisioned
    pub fn provisioned_devices(&self) -> Vec<ProvisionedDevice>;
}
```

---

## References

- ADR-001: Trust Architecture (identity, attestation)
- Issue 97f090e: Identity Binding
- Issue 3920c2c: Membership Control
- BLE Core Spec 5.0: Advertising, GATT
- NIST SP 800-57: Key Management

---

## Open Questions

1. **Multi-mesh devices**: Can a sensor belong to multiple meshes?
   - *Tentative*: No, single mesh binding for simplicity.

2. **Mesh merging**: Can two meshes merge?
   - *Tentative*: Not in v1. Requires authority negotiation.

3. **Provisioning delegation depth**: How deep can provisioning authority be delegated?
   - *Tentative*: 1 level (creator → delegate → cannot delegate further).

4. **Emergency provisioning**: What if all authorities are lost?
   - *Proposed*: Document as "re-genesis" procedure. Existing nodes cannot recover.
