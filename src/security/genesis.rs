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

//! Mesh genesis protocol for creating new Eche meshes
//!
//! A mesh is created through a genesis event where:
//! - A cryptographic seed is generated (256 bits of entropy)
//! - The mesh_id is derived from the name and seed
//! - Encryption keys are derived from the seed
//! - The creator becomes the initial authority
//!
//! # Example
//!
//! ```
//! use eche_btle::security::{DeviceIdentity, MeshGenesis, MembershipPolicy};
//!
//! // Create the founder's identity
//! let founder = DeviceIdentity::generate();
//!
//! // Create a new mesh
//! let genesis = MeshGenesis::create("ALPHA-TEAM", &founder, MembershipPolicy::Controlled);
//!
//! // Get derived values
//! let mesh_id = genesis.mesh_id();           // e.g., "A1B2C3D4"
//! let secret = genesis.encryption_secret();   // 32-byte key
//! let beacon_key = genesis.beacon_key_base(); // For encrypted beacons
//! ```

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use rand_core::{OsRng, RngCore};

use super::identity::DeviceIdentity;

/// Membership policy controlling how nodes can join the mesh
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MembershipPolicy {
    /// Anyone with mesh_id can attempt to discover and join
    /// Least secure, useful for demos and open networks
    Open,

    /// Explicit enrollment by an authority is required
    /// Balanced security for most deployments
    #[default]
    Controlled,

    /// Only pre-provisioned devices can join
    /// Highest security for sensitive operations
    Strict,
}

/// Genesis event for creating a new mesh
///
/// Contains all the cryptographic material needed to bootstrap a mesh.
/// The creator should securely store this for recovery purposes.
#[derive(Clone)]
pub struct MeshGenesis {
    /// Human-readable mesh name
    pub mesh_name: String,

    /// 256-bit cryptographic seed (generated from CSPRNG)
    mesh_seed: [u8; 32],

    /// Creator's device identity
    creator_identity: DeviceIdentity,

    /// Timestamp of creation (milliseconds since Unix epoch)
    pub created_at_ms: u64,

    /// Membership policy for this mesh
    pub policy: MembershipPolicy,
}

impl MeshGenesis {
    /// HKDF context for encryption key derivation
    const ENCRYPTION_CONTEXT: &'static [u8] = b"ECHE-mesh-encryption-v1";

    /// HKDF context for beacon key derivation
    const BEACON_CONTEXT: &'static [u8] = b"ECHE-beacon-key-v1";

    /// Create a new mesh as the founding controller
    ///
    /// # Arguments
    /// * `mesh_name` - Human-readable name for the mesh
    /// * `creator` - Device identity of the mesh creator
    /// * `policy` - Membership policy for the mesh
    ///
    /// # Returns
    /// A new MeshGenesis with cryptographically secure seed
    pub fn create(mesh_name: &str, creator: &DeviceIdentity, policy: MembershipPolicy) -> Self {
        let mut mesh_seed = [0u8; 32];
        OsRng.fill_bytes(&mut mesh_seed);

        Self {
            mesh_name: mesh_name.into(),
            mesh_seed,
            creator_identity: creator.clone(),
            created_at_ms: Self::now_ms(),
            policy,
        }
    }

    /// Create genesis with a specific seed (for testing or deterministic creation)
    ///
    /// # Safety
    /// Only use with cryptographically random seeds in production.
    pub fn with_seed(
        mesh_name: &str,
        seed: [u8; 32],
        creator: &DeviceIdentity,
        policy: MembershipPolicy,
    ) -> Self {
        Self {
            mesh_name: mesh_name.into(),
            mesh_seed: seed,
            creator_identity: creator.clone(),
            created_at_ms: Self::now_ms(),
            policy,
        }
    }

    /// Derive the mesh_id from name and seed
    ///
    /// The mesh_id is 8 hex characters derived from BLAKE3 keyed hash.
    /// Format: uppercase hex, e.g., "A1B2C3D4"
    pub fn mesh_id(&self) -> String {
        let hash = blake3::keyed_hash(&self.mesh_seed, self.mesh_name.as_bytes());
        let hash_bytes = hash.as_bytes();

        // First 4 bytes as uppercase hex = 8 characters
        format!(
            "{:02X}{:02X}{:02X}{:02X}",
            hash_bytes[0], hash_bytes[1], hash_bytes[2], hash_bytes[3]
        )
    }

    /// Derive the mesh-wide encryption secret
    ///
    /// Uses BLAKE3 key derivation with a specific context.
    pub fn encryption_secret(&self) -> [u8; 32] {
        blake3::derive_key(
            core::str::from_utf8(Self::ENCRYPTION_CONTEXT).unwrap(),
            &self.mesh_seed,
        )
    }

    /// Derive the base key for encrypted beacons
    ///
    /// Beacon keys are rotated, but this is the base from which they're derived.
    pub fn beacon_key_base(&self) -> [u8; 32] {
        blake3::derive_key(
            core::str::from_utf8(Self::BEACON_CONTEXT).unwrap(),
            &self.mesh_seed,
        )
    }

    /// Get the mesh seed for secure storage
    ///
    /// **Security**: This is the root secret. Protect it carefully.
    pub fn mesh_seed(&self) -> &[u8; 32] {
        &self.mesh_seed
    }

    /// Get the creator's identity
    pub fn creator(&self) -> &DeviceIdentity {
        &self.creator_identity
    }

    /// Get the creator's public key
    pub fn creator_public_key(&self) -> [u8; 32] {
        self.creator_identity.public_key()
    }

    /// Check if a given identity is the mesh creator
    pub fn is_creator(&self, identity: &DeviceIdentity) -> bool {
        self.creator_identity.public_key() == identity.public_key()
    }

    /// Encode genesis data for persistence
    ///
    /// Format:
    /// - mesh_name length (2 bytes, LE)
    /// - mesh_name (variable)
    /// - mesh_seed (32 bytes)
    /// - creator public key (32 bytes)
    /// - creator private key (32 bytes) - SENSITIVE!
    /// - created_at_ms (8 bytes, LE)
    /// - policy (1 byte)
    ///
    /// Total: 107 + mesh_name.len() bytes
    pub fn encode(&self) -> Vec<u8> {
        let name_bytes = self.mesh_name.as_bytes();
        let mut buf = Vec::with_capacity(107 + name_bytes.len());

        // Mesh name (length-prefixed)
        buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(name_bytes);

        // Mesh seed
        buf.extend_from_slice(&self.mesh_seed);

        // Creator identity (public + private keys)
        buf.extend_from_slice(&self.creator_identity.public_key());
        buf.extend_from_slice(&self.creator_identity.private_key_bytes());

        // Timestamp
        buf.extend_from_slice(&self.created_at_ms.to_le_bytes());

        // Policy
        buf.push(match self.policy {
            MembershipPolicy::Open => 0,
            MembershipPolicy::Controlled => 1,
            MembershipPolicy::Strict => 2,
        });

        buf
    }

    /// Decode genesis data from bytes
    ///
    /// Returns None if data is invalid or too short.
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 107 {
            return None;
        }

        // Mesh name
        let name_len = u16::from_le_bytes([data[0], data[1]]) as usize;
        if data.len() < 107 + name_len {
            return None;
        }

        let mesh_name = String::from_utf8(data[2..2 + name_len].to_vec()).ok()?;
        let offset = 2 + name_len;

        // Mesh seed
        let mut mesh_seed = [0u8; 32];
        mesh_seed.copy_from_slice(&data[offset..offset + 32]);

        // Creator public key (we'll reconstruct from private)
        let _public_key = &data[offset + 32..offset + 64];

        // Creator private key
        let mut private_key = [0u8; 32];
        private_key.copy_from_slice(&data[offset + 64..offset + 96]);
        let creator_identity = DeviceIdentity::from_private_key(&private_key).ok()?;

        // Timestamp
        let created_at_ms = u64::from_le_bytes([
            data[offset + 96],
            data[offset + 97],
            data[offset + 98],
            data[offset + 99],
            data[offset + 100],
            data[offset + 101],
            data[offset + 102],
            data[offset + 103],
        ]);

        // Policy
        let policy = match data[offset + 104] {
            0 => MembershipPolicy::Open,
            1 => MembershipPolicy::Controlled,
            2 => MembershipPolicy::Strict,
            _ => return None,
        };

        Some(Self {
            mesh_name,
            mesh_seed,
            creator_identity,
            created_at_ms,
            policy,
        })
    }

    /// Get current timestamp in milliseconds
    #[cfg(feature = "std")]
    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    #[cfg(not(feature = "std"))]
    fn now_ms() -> u64 {
        0 // Platform should provide timestamp
    }
}

impl core::fmt::Debug for MeshGenesis {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MeshGenesis")
            .field("mesh_name", &self.mesh_name)
            .field("mesh_id", &self.mesh_id())
            .field("creator_node_id", &self.creator_identity.node_id())
            .field("created_at_ms", &self.created_at_ms)
            .field("policy", &self.policy)
            .field("mesh_seed", &"[REDACTED]")
            .finish()
    }
}

/// Shareable mesh credentials (without creator's private key)
///
/// This can be shared with nodes joining the mesh.
#[derive(Debug, Clone)]
pub struct MeshCredentials {
    /// The mesh_id (derived, for verification)
    pub mesh_id: String,

    /// Mesh name
    pub mesh_name: String,

    /// Encryption secret for mesh-wide encryption
    pub encryption_secret: [u8; 32],

    /// Creator's public key (for verification)
    pub creator_public_key: [u8; 32],

    /// Membership policy
    pub policy: MembershipPolicy,
}

impl MeshCredentials {
    /// Create credentials from genesis data
    pub fn from_genesis(genesis: &MeshGenesis) -> Self {
        Self {
            mesh_id: genesis.mesh_id(),
            mesh_name: genesis.mesh_name.clone(),
            encryption_secret: genesis.encryption_secret(),
            creator_public_key: genesis.creator_public_key(),
            policy: genesis.policy,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_genesis() {
        let creator = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("ALPHA-TEAM", &creator, MembershipPolicy::Controlled);

        assert_eq!(genesis.mesh_name, "ALPHA-TEAM");
        assert_eq!(genesis.policy, MembershipPolicy::Controlled);
        assert!(genesis.is_creator(&creator));
    }

    #[test]
    fn test_mesh_id_format() {
        let creator = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("TEST", &creator, MembershipPolicy::Open);

        let mesh_id = genesis.mesh_id();

        // Should be 8 uppercase hex characters
        assert_eq!(mesh_id.len(), 8);
        assert!(mesh_id
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_lowercase()));
    }

    #[test]
    fn test_mesh_id_deterministic() {
        let creator = DeviceIdentity::generate();
        let seed = [0x42u8; 32];
        let genesis = MeshGenesis::with_seed("TEST", seed, &creator, MembershipPolicy::Open);

        // Multiple calls return same mesh_id
        let id1 = genesis.mesh_id();
        let id2 = genesis.mesh_id();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_different_names_different_ids() {
        let creator = DeviceIdentity::generate();
        let seed = [0x42u8; 32];

        let genesis1 = MeshGenesis::with_seed("ALPHA", seed, &creator, MembershipPolicy::Open);
        let genesis2 = MeshGenesis::with_seed("BRAVO", seed, &creator, MembershipPolicy::Open);

        assert_ne!(genesis1.mesh_id(), genesis2.mesh_id());
    }

    #[test]
    fn test_different_seeds_different_ids() {
        let creator = DeviceIdentity::generate();

        let genesis1 =
            MeshGenesis::with_seed("TEST", [0x42u8; 32], &creator, MembershipPolicy::Open);
        let genesis2 =
            MeshGenesis::with_seed("TEST", [0x43u8; 32], &creator, MembershipPolicy::Open);

        assert_ne!(genesis1.mesh_id(), genesis2.mesh_id());
    }

    #[test]
    fn test_encryption_secret_deterministic() {
        let creator = DeviceIdentity::generate();
        let seed = [0x42u8; 32];
        let genesis = MeshGenesis::with_seed("TEST", seed, &creator, MembershipPolicy::Open);

        let secret1 = genesis.encryption_secret();
        let secret2 = genesis.encryption_secret();

        assert_eq!(secret1, secret2);
        assert_ne!(secret1, seed); // Derived, not the same as seed
    }

    #[test]
    fn test_beacon_key_different_from_encryption() {
        let creator = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("TEST", &creator, MembershipPolicy::Open);

        let encryption = genesis.encryption_secret();
        let beacon = genesis.beacon_key_base();

        assert_ne!(encryption, beacon);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let creator = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("ALPHA-TEAM", &creator, MembershipPolicy::Strict);

        let encoded = genesis.encode();
        let decoded = MeshGenesis::decode(&encoded).unwrap();

        assert_eq!(decoded.mesh_name, genesis.mesh_name);
        assert_eq!(decoded.mesh_id(), genesis.mesh_id());
        assert_eq!(decoded.encryption_secret(), genesis.encryption_secret());
        assert_eq!(decoded.policy, genesis.policy);
        assert!(decoded.is_creator(&creator));
    }

    #[test]
    fn test_decode_too_short() {
        let short_data = [0u8; 50];
        assert!(MeshGenesis::decode(&short_data).is_none());
    }

    #[test]
    fn test_credentials_from_genesis() {
        let creator = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("TEST", &creator, MembershipPolicy::Controlled);

        let creds = MeshCredentials::from_genesis(&genesis);

        assert_eq!(creds.mesh_id, genesis.mesh_id());
        assert_eq!(creds.mesh_name, genesis.mesh_name);
        assert_eq!(creds.encryption_secret, genesis.encryption_secret());
        assert_eq!(creds.creator_public_key, genesis.creator_public_key());
        assert_eq!(creds.policy, genesis.policy);
    }

    #[test]
    fn test_policy_default() {
        assert_eq!(MembershipPolicy::default(), MembershipPolicy::Controlled);
    }

    #[test]
    fn test_debug_redacts_seed() {
        let creator = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("TEST", &creator, MembershipPolicy::Open);

        let debug_str = format!("{:?}", genesis);
        assert!(debug_str.contains("REDACTED"));
        assert!(debug_str.contains("mesh_id"));
    }
}
