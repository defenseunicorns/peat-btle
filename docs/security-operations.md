# Security Operations Guide

> For Users and Operators: Deploying, configuring, and managing secure HIVE meshes

## Concepts

### What is a Mesh?

A **mesh** is a group of HIVE devices that communicate with each other over Bluetooth Low Energy. Each mesh is identified by:

- **mesh_id**: A short identifier (e.g., "DEMO", "ALPHA", "OPS1")
- **shared secret**: Optional 32-byte key for encryption

Devices only communicate with others in the same mesh.

### Security Levels

| Level | Configuration | Protection | Use Case |
|-------|--------------|------------|----------|
| **None** | No secret | None | Development, demos |
| **Mesh-Wide** | Shared secret | External eavesdroppers | Semi-trusted operations |
| **Per-Peer E2EE** | + E2EE enabled | + Other mesh members | Sensitive point-to-point |

## Creating a Mesh

### Step 1: Choose a Mesh ID

The mesh_id appears in BLE advertisements and filters which devices can join.

**Guidelines:**
- 4 characters recommended (fits in BLE advertisement)
- Alphanumeric only
- Unique per deployment/exercise
- **Not secret** - visible to anyone scanning BLE

```
Good:  DEMO, OPS1, ALFA, TM42
Bad:   SECRET-MESH (visible anyway)
Bad:   test (too generic, conflicts likely)
```

### Step 2: Generate a Shared Secret (Recommended)

The shared secret provides encryption. All mesh members must have the same secret.

**Generating a secret:**

```bash
# Linux/macOS - generate random 32 bytes
openssl rand -hex 32

# Output example:
# a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456
```

**Store the secret securely:**
- Password manager
- Secure notes app
- Encrypted file
- **Never** in plain text files or emails

### Step 3: Distribute the Secret

**Secure distribution methods:**
- QR code scan (in-person)
- Encrypted messaging app
- Secure file transfer
- Physical USB drive

**Insecure methods (avoid):**
- Unencrypted email
- SMS/text message
- Shared documents
- Verbal over radio

### Step 4: Configure Each Device

Each device needs:
1. Unique node_id (usually auto-generated from MAC)
2. Callsign (human-readable name)
3. mesh_id (same for all devices in mesh)
4. Shared secret (same for all devices, if using encryption)

**Example configuration:**

| Device | node_id | callsign | mesh_id | secret |
|--------|---------|----------|---------|--------|
| Leader phone | 0x11111111 | ALPHA-1 | OPS1 | a1b2c3... |
| Team member 1 | 0x22222222 | ALPHA-2 | OPS1 | a1b2c3... |
| Team member 2 | 0x33333333 | ALPHA-3 | OPS1 | a1b2c3... |
| Sensor node | 0x44444444 | SENSOR-1 | OPS1 | a1b2c3... |

## Operating a Secure Mesh

### Verifying Encryption

After configuration, verify encryption is working:

1. **Check device status** - should show "Encryption: Enabled"
2. **Monitor BLE traffic** - encrypted packets start with `0xAE`
3. **Test with wrong key** - device with wrong secret cannot decode messages

### Monitoring Mesh Health

| Metric | Healthy | Investigate |
|--------|---------|-------------|
| Connected peers | Expected count | Missing devices |
| Sync frequency | Regular intervals | Long gaps |
| Decryption failures | 0 | Any failures (key mismatch?) |
| Peer timeouts | Occasional | Frequent (BLE interference?) |

### Handling Alerts

**Emergency Event Received:**
- Acknowledge via ACK response
- Verify source callsign
- Follow operational procedures

**Decryption Failed:**
- Verify sender is using correct mesh_id
- Verify sender has correct shared secret
- Check for data corruption (retry)

**Peer Lost:**
- Device moved out of BLE range
- Device powered off or battery dead
- BLE interference

## Membership Management

### Current Limitation

**Important:** hive-btle does not currently support:
- Removing specific devices from a mesh
- Blocking compromised devices
- Key rotation

If a device is compromised:
1. Generate new shared secret
2. Distribute to all **other** devices
3. Reconfigure all devices with new secret
4. Compromised device is now excluded

### Adding a New Device

1. Configure device with mesh_id and secret
2. Power on within BLE range of mesh
3. Device auto-discovers and joins
4. Verify device appears in peer list

### Removing a Device (Planned)

Currently requires changing the secret for all remaining devices. Future versions will support explicit revocation.

## Troubleshooting

### Device Not Joining Mesh

**Check:**
1. mesh_id matches exactly (case-sensitive)
2. Shared secret is identical (all 32 bytes)
3. BLE is enabled and functioning
4. Device is within BLE range (~100m line-of-sight)

**Common mistakes:**
- Typo in mesh_id
- Partial secret copy (need all 64 hex characters)
- BLE turned off
- Battery too low for BLE operation

### Cannot Decrypt Messages

**Symptoms:**
- Documents received but show as encrypted
- Decryption error in logs
- No sync data appearing

**Causes:**
1. **Wrong secret** - verify byte-for-byte match
2. **Wrong mesh_id** - key is derived from mesh_id + secret
3. **Encryption not enabled** - sender isn't encrypting
4. **Data corruption** - BLE transmission error

### Messages Not Reaching All Devices

**Check:**
1. All devices have encryption enabled (or all disabled)
2. Mixed encrypted/unencrypted doesn't work well
3. BLE range and line-of-sight
4. Peer timeout settings (default 45s)

### Per-Peer E2EE Not Working

**Symptoms:**
- Key exchange messages sent but session not established
- E2EE messages not decrypting

**Causes:**
1. E2EE not enabled on one or both sides
2. Max sessions limit reached (default 16)
3. Session timed out (default 30 minutes)
4. Key exchange messages not delivered

## Security Checklist

### Pre-Deployment

- [ ] Generated unique mesh_id for this operation
- [ ] Generated cryptographically secure 32-byte secret
- [ ] Distributed secret through secure channel
- [ ] Configured all devices with identical secret
- [ ] Verified encryption enabled on all devices
- [ ] Tested communication between all device pairs
- [ ] Documented device node_ids and callsigns

### During Operation

- [ ] Monitor peer count matches expected
- [ ] Watch for decryption errors (key mismatch)
- [ ] Note any devices dropping repeatedly
- [ ] Keep spare configured devices ready

### Post-Operation

- [ ] Change shared secret if devices were at risk
- [ ] Review any security alerts
- [ ] Update documentation with lessons learned
- [ ] Securely delete old secrets

## Reference

### Configuration Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| mesh_id | "DEMO" | Network identifier |
| encryption_secret | None | 32-byte shared key |
| strict_encryption | false | Reject unencrypted when encryption enabled |
| peer_timeout_ms | 45000 | Remove stale peers after 45s |
| max_peers | 8 | Maximum tracked peers |
| sync_interval_ms | 5000 | Broadcast every 5s |
| e2ee_session_timeout | 1800000 | E2EE session expires after 30m |

### Strict Encryption Mode

Enable strict mode to reject unencrypted documents (prevents downgrade attacks):

```
with_encryption(secret)     # Enable encryption
with_strict_encryption()    # Also reject unencrypted
```

**Recommended deployment path:**
1. Deploy with encryption enabled, strict=false
2. Verify all nodes are encrypting (monitor logs)
3. Enable strict mode once all nodes confirmed
4. Monitor SecurityViolation events for attacks

### Power Profiles

| Profile | Duty Cycle | Battery Life* | Use Case |
|---------|------------|---------------|----------|
| Aggressive | 20% | ~6 hours | High-activity, fast sync |
| Balanced | 10% | ~12 hours | Normal operations |
| LowPower | 2% | ~20 hours | Extended operations |
| UltraLow | 0.5% | ~36 hours | Minimal activity |

*Estimated for 300mAh smartwatch battery

### BLE Range

| PHY Mode | Typical Range | Battery Impact |
|----------|---------------|----------------|
| LE 1M | ~100m | Baseline |
| LE 2M | ~50m | Lower (faster) |
| LE Coded S=2 | ~200m | Higher |
| LE Coded S=8 | ~400m | Highest |

### Document Size Limits

| Component | Size |
|-----------|------|
| BLE MTU | ~244 bytes typical |
| Encryption overhead | 30 bytes |
| E2EE overhead | 46 bytes |
| **Available payload** | **~170 bytes** |

## Glossary

| Term | Definition |
|------|------------|
| **mesh_id** | Identifier that groups devices into a network |
| **node_id** | Unique 32-bit identifier for each device |
| **callsign** | Human-readable name (e.g., "ALPHA-1") |
| **shared secret** | 32-byte key for mesh encryption |
| **E2EE** | End-to-end encryption between two specific peers |
| **CRDT** | Conflict-free data type that merges without conflicts |
| **peer** | Another device in the mesh |
| **stale** | A peer not seen within timeout period |

## Getting Help

- **Technical issues**: Check logs for error messages
- **Configuration help**: Review this guide and examples
- **Bug reports**: https://github.com/r-evolve/hive-btle/issues
- **Security concerns**: Contact security@revolveteam.com
