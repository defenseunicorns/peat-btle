// Copyright (c) 2025-2026 (r)evolve - Revolve Team LLC
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Encrypted BLE Advertisement Beacons
//!
//! Protects mesh and node identity from passive observers while allowing
//! mesh members to identify each other. Non-members see random data.
//!
//! # Privacy Properties
//!
//! - Device name is generic ("HIVE") - no identifying information
//! - mesh_id and node_id are encrypted in service data
//! - Nonce rotates to prevent tracking across advertisements
//! - Only nodes with beacon_key can decrypt
//!
//! # Wire Format
//!
//! ```text
//! Encrypted Beacon (21 bytes):
//! ┌─────────┬─────────┬──────────────────┬─────┬──────┬─────┬─────┐
//! │ Version │  Nonce  │ Encrypted Identity│ MAC │ Caps │Hier │ Bat │
//! │ 1 byte  │ 4 bytes │     8 bytes      │4 byt│2 byt │1 byt│1 byt│
//! └─────────┴─────────┴──────────────────┴─────┴──────┴─────┴─────┘
//!
//! Encrypted Identity = XOR(mesh_id[4] || node_id[4], keystream)
//! MAC = BLAKE3(beacon_key || nonce || encrypted)[0..4]
//! ```
//!
//! # Example
//!
//! ```ignore
//! use hive_btle::discovery::{EncryptedBeacon, BeaconKey};
//!
//! // Derive beacon key from genesis
//! let beacon_key = BeaconKey::from_base(&genesis.beacon_key_base());
//!
//! // Create and encrypt beacon
//! let beacon = EncryptedBeacon::new(node_id, capabilities, hierarchy, battery);
//! let encrypted = beacon.encrypt(&beacon_key, &mesh_id_bytes);
//!
//! // Decrypt received beacon
//! if let Some((beacon, mesh_id)) = EncryptedBeacon::decrypt(&encrypted, &beacon_key) {
//!     println!("Received from mesh {:08X}, node {:08X}", mesh_id, beacon.node_id);
//! }
//! ```

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::NodeId;

/// Version byte for encrypted beacon format
pub const ENCRYPTED_BEACON_VERSION: u8 = 0x02;

/// Size of encrypted beacon in bytes
pub const ENCRYPTED_BEACON_SIZE: usize = 21;

/// Size of the encrypted identity portion
const ENCRYPTED_IDENTITY_SIZE: usize = 8;

/// Size of the MAC
const MAC_SIZE: usize = 4;

/// Size of the nonce
const NONCE_SIZE: usize = 4;

/// Beacon encryption key derived from mesh genesis
#[derive(Clone)]
pub struct BeaconKey {
    /// The 32-byte key used for encryption and MAC
    key: [u8; 32],
}

impl BeaconKey {
    /// Create a beacon key from the base key (from MeshGenesis::beacon_key_base())
    pub fn from_base(base: &[u8; 32]) -> Self {
        Self { key: *base }
    }

    /// Get the raw key bytes (for testing)
    #[cfg(test)]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }

    /// Derive keystream for XOR encryption
    fn derive_keystream(&self, nonce: &[u8; NONCE_SIZE]) -> [u8; ENCRYPTED_IDENTITY_SIZE] {
        // Use BLAKE3 keyed hash to derive keystream
        let mut input = [0u8; 36];
        input[..32].copy_from_slice(&self.key);
        input[32..].copy_from_slice(nonce);

        let hash = blake3::hash(&input);
        let mut keystream = [0u8; ENCRYPTED_IDENTITY_SIZE];
        keystream.copy_from_slice(&hash.as_bytes()[..ENCRYPTED_IDENTITY_SIZE]);
        keystream
    }

    /// Compute truncated MAC over nonce and encrypted data
    fn compute_mac(
        &self,
        nonce: &[u8; NONCE_SIZE],
        encrypted: &[u8; ENCRYPTED_IDENTITY_SIZE],
    ) -> [u8; MAC_SIZE] {
        let hash = blake3::keyed_hash(
            &self.key,
            &[nonce.as_slice(), encrypted.as_slice()].concat(),
        );
        let mut mac = [0u8; MAC_SIZE];
        mac.copy_from_slice(&hash.as_bytes()[..MAC_SIZE]);
        mac
    }
}

/// Encrypted beacon for privacy-preserving advertisements
///
/// Contains node identification and status that can only be read
/// by mesh members with the beacon key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedBeacon {
    /// Node identifier (encrypted in wire format)
    pub node_id: NodeId,

    /// Node capabilities bitmap (public)
    pub capabilities: u16,

    /// Hierarchy level (public, for parent selection)
    pub hierarchy_level: u8,

    /// Battery percentage 0-100 (public)
    pub battery_percent: u8,
}

impl EncryptedBeacon {
    /// Create a new beacon with the given parameters
    pub fn new(
        node_id: NodeId,
        capabilities: u16,
        hierarchy_level: u8,
        battery_percent: u8,
    ) -> Self {
        Self {
            node_id,
            capabilities,
            hierarchy_level,
            battery_percent,
        }
    }

    /// Encrypt the beacon for transmission
    ///
    /// # Arguments
    /// * `key` - Beacon encryption key from mesh genesis
    /// * `mesh_id_bytes` - First 4 bytes of mesh_id hash (for identification)
    ///
    /// # Returns
    /// 21-byte encrypted beacon ready for BLE service data
    pub fn encrypt(&self, key: &BeaconKey, mesh_id_bytes: &[u8; 4]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(ENCRYPTED_BEACON_SIZE);

        // Version
        buf.push(ENCRYPTED_BEACON_VERSION);

        // Generate random nonce
        let mut nonce = [0u8; NONCE_SIZE];
        rand_core::OsRng.fill_bytes(&mut nonce);
        buf.extend_from_slice(&nonce);

        // Build plaintext: mesh_id[4] || node_id[4]
        let mut plaintext = [0u8; ENCRYPTED_IDENTITY_SIZE];
        plaintext[..4].copy_from_slice(mesh_id_bytes);
        plaintext[4..].copy_from_slice(&self.node_id.as_u32().to_be_bytes());

        // Encrypt with XOR keystream
        let keystream = key.derive_keystream(&nonce);
        let mut encrypted = [0u8; ENCRYPTED_IDENTITY_SIZE];
        for i in 0..ENCRYPTED_IDENTITY_SIZE {
            encrypted[i] = plaintext[i] ^ keystream[i];
        }
        buf.extend_from_slice(&encrypted);

        // MAC
        let mac = key.compute_mac(&nonce, &encrypted);
        buf.extend_from_slice(&mac);

        // Public fields (not encrypted, needed for filtering)
        buf.extend_from_slice(&self.capabilities.to_be_bytes());
        buf.push(self.hierarchy_level);
        buf.push(self.battery_percent);

        buf
    }

    /// Attempt to decrypt a beacon
    ///
    /// # Arguments
    /// * `data` - Raw encrypted beacon bytes (21 bytes)
    /// * `key` - Beacon encryption key to try
    ///
    /// # Returns
    /// * `Some((beacon, mesh_id_bytes))` if decryption succeeds and MAC is valid
    /// * `None` if data is invalid or MAC doesn't match (wrong mesh)
    pub fn decrypt(data: &[u8], key: &BeaconKey) -> Option<(Self, [u8; 4])> {
        if data.len() < ENCRYPTED_BEACON_SIZE {
            return None;
        }

        // Check version
        if data[0] != ENCRYPTED_BEACON_VERSION {
            return None;
        }

        // Extract components
        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(&data[1..5]);

        let mut encrypted = [0u8; ENCRYPTED_IDENTITY_SIZE];
        encrypted.copy_from_slice(&data[5..13]);

        let mut received_mac = [0u8; MAC_SIZE];
        received_mac.copy_from_slice(&data[13..17]);

        // Verify MAC first (quick rejection for wrong mesh)
        let expected_mac = key.compute_mac(&nonce, &encrypted);
        if received_mac != expected_mac {
            return None;
        }

        // Decrypt
        let keystream = key.derive_keystream(&nonce);
        let mut plaintext = [0u8; ENCRYPTED_IDENTITY_SIZE];
        for i in 0..ENCRYPTED_IDENTITY_SIZE {
            plaintext[i] = encrypted[i] ^ keystream[i];
        }

        // Extract mesh_id and node_id
        let mut mesh_id_bytes = [0u8; 4];
        mesh_id_bytes.copy_from_slice(&plaintext[..4]);

        let node_id = NodeId::new(u32::from_be_bytes([
            plaintext[4],
            plaintext[5],
            plaintext[6],
            plaintext[7],
        ]));

        // Extract public fields
        let capabilities = u16::from_be_bytes([data[17], data[18]]);
        let hierarchy_level = data[19];
        let battery_percent = data[20];

        Some((
            Self {
                node_id,
                capabilities,
                hierarchy_level,
                battery_percent,
            },
            mesh_id_bytes,
        ))
    }

    /// Check if data looks like an encrypted beacon (quick check)
    pub fn is_encrypted_beacon(data: &[u8]) -> bool {
        data.len() >= ENCRYPTED_BEACON_SIZE && data[0] == ENCRYPTED_BEACON_VERSION
    }
}

/// Convert mesh_id string to 4-byte identifier for beacon
///
/// Uses first 4 bytes of BLAKE3 hash of the mesh_id string.
pub fn mesh_id_to_bytes(mesh_id: &str) -> [u8; 4] {
    let hash = blake3::hash(mesh_id.as_bytes());
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&hash.as_bytes()[..4]);
    bytes
}

/// Generic device name for encrypted beacons
pub const ENCRYPTED_DEVICE_NAME: &str = "HIVE";

use rand_core::RngCore;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = BeaconKey::from_base(&[0x42; 32]);
        let mesh_id_bytes = mesh_id_to_bytes("TEST-MESH");
        let node_id = NodeId::new(0x12345678);

        let beacon = EncryptedBeacon::new(node_id, 0x0F00, 2, 85);
        let encrypted = beacon.encrypt(&key, &mesh_id_bytes);

        assert_eq!(encrypted.len(), ENCRYPTED_BEACON_SIZE);
        assert_eq!(encrypted[0], ENCRYPTED_BEACON_VERSION);

        let (decrypted, decrypted_mesh_id) = EncryptedBeacon::decrypt(&encrypted, &key).unwrap();

        assert_eq!(decrypted.node_id, node_id);
        assert_eq!(decrypted.capabilities, 0x0F00);
        assert_eq!(decrypted.hierarchy_level, 2);
        assert_eq!(decrypted.battery_percent, 85);
        assert_eq!(decrypted_mesh_id, mesh_id_bytes);
    }

    #[test]
    fn test_wrong_key_fails() {
        let key1 = BeaconKey::from_base(&[0x42; 32]);
        let key2 = BeaconKey::from_base(&[0x99; 32]);
        let mesh_id_bytes = mesh_id_to_bytes("TEST-MESH");
        let node_id = NodeId::new(0x12345678);

        let beacon = EncryptedBeacon::new(node_id, 0x0F00, 2, 85);
        let encrypted = beacon.encrypt(&key1, &mesh_id_bytes);

        // Decryption with wrong key should fail (MAC mismatch)
        assert!(EncryptedBeacon::decrypt(&encrypted, &key2).is_none());
    }

    #[test]
    fn test_tampered_data_fails() {
        let key = BeaconKey::from_base(&[0x42; 32]);
        let mesh_id_bytes = mesh_id_to_bytes("TEST-MESH");
        let node_id = NodeId::new(0x12345678);

        let beacon = EncryptedBeacon::new(node_id, 0x0F00, 2, 85);
        let mut encrypted = beacon.encrypt(&key, &mesh_id_bytes);

        // Tamper with encrypted data
        encrypted[7] ^= 0xFF;

        // Should fail MAC check
        assert!(EncryptedBeacon::decrypt(&encrypted, &key).is_none());
    }

    #[test]
    fn test_different_nonces_produce_different_ciphertext() {
        let key = BeaconKey::from_base(&[0x42; 32]);
        let mesh_id_bytes = mesh_id_to_bytes("TEST-MESH");
        let node_id = NodeId::new(0x12345678);

        let beacon = EncryptedBeacon::new(node_id, 0x0F00, 2, 85);
        let encrypted1 = beacon.encrypt(&key, &mesh_id_bytes);
        let encrypted2 = beacon.encrypt(&key, &mesh_id_bytes);

        // Nonces should differ (bytes 1-4)
        assert_ne!(&encrypted1[1..5], &encrypted2[1..5]);

        // Encrypted portions should differ
        assert_ne!(&encrypted1[5..13], &encrypted2[5..13]);

        // Both should decrypt correctly
        assert!(EncryptedBeacon::decrypt(&encrypted1, &key).is_some());
        assert!(EncryptedBeacon::decrypt(&encrypted2, &key).is_some());
    }

    #[test]
    fn test_is_encrypted_beacon() {
        let key = BeaconKey::from_base(&[0x42; 32]);
        let mesh_id_bytes = mesh_id_to_bytes("TEST-MESH");
        let beacon = EncryptedBeacon::new(NodeId::new(1), 0, 0, 0);
        let encrypted = beacon.encrypt(&key, &mesh_id_bytes);

        assert!(EncryptedBeacon::is_encrypted_beacon(&encrypted));
        assert!(!EncryptedBeacon::is_encrypted_beacon(&[0x01; 21])); // Wrong version
        assert!(!EncryptedBeacon::is_encrypted_beacon(&[0x02; 10])); // Too short
    }

    #[test]
    fn test_mesh_id_to_bytes_deterministic() {
        let bytes1 = mesh_id_to_bytes("ALPHA-TEAM");
        let bytes2 = mesh_id_to_bytes("ALPHA-TEAM");
        let bytes3 = mesh_id_to_bytes("BRAVO-TEAM");

        assert_eq!(bytes1, bytes2);
        assert_ne!(bytes1, bytes3);
    }
}
