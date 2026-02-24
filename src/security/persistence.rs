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

//! Credential Persistence Layer
//!
//! Provides secure storage for mesh credentials, device identity, and security
//! state across reboots. This is critical for unattended devices (sensors, relays)
//! that must retain their identity and mesh membership after power cycles.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    PersistedState                           │
//! │  ┌─────────────┐  ┌─────────────┐  ┌──────────────────┐    │
//! │  │ DeviceKey   │  │ MeshGenesis │  │ IdentityRegistry │    │
//! │  │ (Ed25519)   │  │ (mesh seed) │  │ (TOFU cache)     │    │
//! │  └─────────────┘  └─────────────┘  └──────────────────────┘    │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    SecureStorage Trait                       │
//! └─────────────────────────────────────────────────────────────┘
//!          │              │              │              │
//!          ▼              ▼              ▼              ▼
//!     ┌────────┐    ┌────────┐    ┌────────┐    ┌────────┐
//!     │Android │    │  iOS   │    │ Linux  │    │ ESP32  │
//!     │Keystore│    │Keychain│    │  File  │    │  NVS   │
//!     └────────┘    └────────┘    └────────┘    └────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use eche_btle::security::{PersistedState, SecureStorage, DeviceIdentity, MeshGenesis};
//!
//! // On first boot: create and persist state
//! let identity = DeviceIdentity::generate();
//! let genesis = MeshGenesis::create("ALPHA", &identity, MembershipPolicy::Controlled);
//! let state = PersistedState::new(identity, genesis);
//! state.save(&storage)?;
//!
//! // On subsequent boots: restore state
//! let state = PersistedState::load(&storage)?;
//! let mesh = EcheMesh::from_persisted(state, config)?;
//! ```

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::{DeviceIdentity, IdentityRegistry, MeshGenesis};

/// Current version of the persisted state format.
///
/// Increment when making breaking changes to support migrations.
pub const PERSISTED_STATE_VERSION: u32 = 1;

/// Magic bytes to identify persisted state files.
const MAGIC: [u8; 4] = *b"ECHE";

/// Errors that can occur during persistence operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistenceError {
    /// Storage backend error
    StorageError(String),

    /// Data corruption or invalid format
    InvalidFormat,

    /// Version mismatch (stored version newer than supported)
    UnsupportedVersion {
        /// Version found in the stored data
        stored: u32,
        /// Maximum version supported by this code
        supported: u32,
    },

    /// Required data not found
    NotFound,

    /// Cryptographic operation failed
    CryptoError(String),
}

#[cfg(feature = "std")]
impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StorageError(msg) => write!(f, "storage error: {}", msg),
            Self::InvalidFormat => write!(f, "invalid format or corrupted data"),
            Self::UnsupportedVersion { stored, supported } => {
                write!(
                    f,
                    "unsupported version: stored={}, supported={}",
                    stored, supported
                )
            }
            Self::NotFound => write!(f, "persisted state not found"),
            Self::CryptoError(msg) => write!(f, "crypto error: {}", msg),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for PersistenceError {}

/// Platform-agnostic secure storage abstraction.
///
/// Implementations should use platform-specific secure storage:
/// - Android: EncryptedSharedPreferences / Keystore
/// - iOS: Keychain Services
/// - Linux: Encrypted file in XDG data directory
/// - ESP32: Encrypted NVS partition
/// - Windows: DPAPI
pub trait SecureStorage {
    /// Store bytes under the given key.
    ///
    /// The implementation should encrypt the data at rest using
    /// platform-appropriate mechanisms.
    fn store(&self, key: &str, value: &[u8]) -> Result<(), PersistenceError>;

    /// Retrieve bytes for the given key.
    ///
    /// Returns `Ok(None)` if the key doesn't exist.
    /// Returns `Err` if the key exists but cannot be decrypted.
    fn retrieve(&self, key: &str) -> Result<Option<Vec<u8>>, PersistenceError>;

    /// Delete the entry for the given key.
    ///
    /// Returns `Ok(())` even if the key didn't exist.
    fn delete(&self, key: &str) -> Result<(), PersistenceError>;

    /// Check if a key exists without retrieving its value.
    fn exists(&self, key: &str) -> Result<bool, PersistenceError> {
        Ok(self.retrieve(key)?.is_some())
    }
}

/// Complete persisted state for an Eche node.
///
/// Contains all security-critical data needed to restore a node
/// after reboot without network access.
#[derive(Debug, Clone)]
pub struct PersistedState {
    /// Format version for migration support
    pub version: u32,

    /// Device identity (Ed25519 private key)
    ///
    /// This is the node's long-term identity. The private key must be
    /// stored securely as it proves ownership of the node_id.
    device_private_key: [u8; 32],

    /// Mesh genesis block (contains mesh seed, name, policy)
    ///
    /// Optional - nodes may operate without mesh membership initially.
    genesis_data: Option<Vec<u8>>,

    /// TOFU identity registry (known peer public keys)
    ///
    /// Persisting this prevents "new device" warnings after reboot.
    registry_data: Vec<u8>,

    /// Revoked public keys
    ///
    /// Nodes that have been explicitly revoked from the mesh.
    revoked_keys: Vec<[u8; 32]>,

    /// Timestamp when state was last persisted
    pub persisted_at_ms: u64,
}

impl PersistedState {
    /// Create a new persisted state from components.
    pub fn new(identity: &DeviceIdentity, genesis: Option<&MeshGenesis>) -> Self {
        Self {
            version: PERSISTED_STATE_VERSION,
            device_private_key: identity.private_key_bytes(),
            genesis_data: genesis.map(|g| g.encode()),
            registry_data: Vec::new(),
            revoked_keys: Vec::new(),
            persisted_at_ms: 0,
        }
    }

    /// Create persisted state with an existing identity registry.
    pub fn with_registry(
        identity: &DeviceIdentity,
        genesis: Option<&MeshGenesis>,
        registry: &IdentityRegistry,
    ) -> Self {
        Self {
            version: PERSISTED_STATE_VERSION,
            device_private_key: identity.private_key_bytes(),
            genesis_data: genesis.map(|g| g.encode()),
            registry_data: registry.encode(),
            revoked_keys: Vec::new(),
            persisted_at_ms: 0,
        }
    }

    /// Restore the device identity from persisted state.
    pub fn restore_identity(&self) -> Result<DeviceIdentity, PersistenceError> {
        DeviceIdentity::from_private_key(&self.device_private_key)
            .map_err(|e| PersistenceError::CryptoError(format!("{:?}", e)))
    }

    /// Restore the mesh genesis from persisted state.
    pub fn restore_genesis(&self) -> Option<MeshGenesis> {
        self.genesis_data
            .as_ref()
            .and_then(|data| MeshGenesis::decode(data))
    }

    /// Restore the identity registry from persisted state.
    pub fn restore_registry(&self) -> IdentityRegistry {
        if self.registry_data.is_empty() {
            IdentityRegistry::new()
        } else {
            IdentityRegistry::decode(&self.registry_data).unwrap_or_default()
        }
    }

    /// Get the list of revoked public keys.
    pub fn revoked_keys(&self) -> &[[u8; 32]] {
        &self.revoked_keys
    }

    /// Add a revoked public key.
    pub fn add_revoked_key(&mut self, public_key: [u8; 32]) {
        if !self.revoked_keys.contains(&public_key) {
            self.revoked_keys.push(public_key);
        }
    }

    /// Update the identity registry data.
    pub fn update_registry(&mut self, registry: &IdentityRegistry) {
        self.registry_data = registry.encode();
    }

    /// Save state to secure storage.
    ///
    /// The storage key used is "hive_persisted_state".
    pub fn save(&self, storage: &dyn SecureStorage) -> Result<(), PersistenceError> {
        let encoded = self.encode();
        storage.store("hive_persisted_state", &encoded)
    }

    /// Load state from secure storage.
    ///
    /// Returns `Err(NotFound)` if no state has been persisted.
    pub fn load(storage: &dyn SecureStorage) -> Result<Self, PersistenceError> {
        let data = storage
            .retrieve("hive_persisted_state")?
            .ok_or(PersistenceError::NotFound)?;

        Self::decode(&data)
    }

    /// Delete persisted state from storage.
    ///
    /// Use with caution - this will require re-provisioning.
    pub fn delete(storage: &dyn SecureStorage) -> Result<(), PersistenceError> {
        storage.delete("hive_persisted_state")
    }

    /// Encode the state to bytes.
    ///
    /// Format:
    /// - Magic (4 bytes): "ECHE"
    /// - Version (4 bytes): u32 LE
    /// - Private key (32 bytes)
    /// - Persisted at (8 bytes): u64 LE timestamp
    /// - Genesis length (4 bytes): u32 LE (0 if none)
    /// - Genesis data (N bytes)
    /// - Registry length (4 bytes): u32 LE
    /// - Registry data (N bytes)
    /// - Revoked count (4 bytes): u32 LE
    /// - Revoked keys (32 bytes each)
    pub fn encode(&self) -> Vec<u8> {
        let genesis_len = self.genesis_data.as_ref().map(|d| d.len()).unwrap_or(0);
        let capacity = 4
            + 4
            + 32
            + 8
            + 4
            + genesis_len
            + 4
            + self.registry_data.len()
            + 4
            + self.revoked_keys.len() * 32;

        let mut buf = Vec::with_capacity(capacity);

        // Magic
        buf.extend_from_slice(&MAGIC);

        // Version
        buf.extend_from_slice(&self.version.to_le_bytes());

        // Private key
        buf.extend_from_slice(&self.device_private_key);

        // Timestamp
        buf.extend_from_slice(&self.persisted_at_ms.to_le_bytes());

        // Genesis
        buf.extend_from_slice(&(genesis_len as u32).to_le_bytes());
        if let Some(ref data) = self.genesis_data {
            buf.extend_from_slice(data);
        }

        // Registry
        buf.extend_from_slice(&(self.registry_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.registry_data);

        // Revoked keys
        buf.extend_from_slice(&(self.revoked_keys.len() as u32).to_le_bytes());
        for key in &self.revoked_keys {
            buf.extend_from_slice(key);
        }

        buf
    }

    /// Decode state from bytes.
    pub fn decode(data: &[u8]) -> Result<Self, PersistenceError> {
        // Minimum size: magic(4) + version(4) + key(32) + timestamp(8) + genesis_len(4) + registry_len(4) + revoked_count(4)
        if data.len() < 60 {
            return Err(PersistenceError::InvalidFormat);
        }

        let mut offset = 0;

        // Magic
        if data[offset..offset + 4] != MAGIC {
            return Err(PersistenceError::InvalidFormat);
        }
        offset += 4;

        // Version
        let version = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;

        if version > PERSISTED_STATE_VERSION {
            return Err(PersistenceError::UnsupportedVersion {
                stored: version,
                supported: PERSISTED_STATE_VERSION,
            });
        }

        // Private key
        let mut device_private_key = [0u8; 32];
        device_private_key.copy_from_slice(&data[offset..offset + 32]);
        offset += 32;

        // Timestamp
        let persisted_at_ms = u64::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]);
        offset += 8;

        // Genesis
        let genesis_len = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        if data.len() < offset + genesis_len {
            return Err(PersistenceError::InvalidFormat);
        }

        let genesis_data = if genesis_len > 0 {
            Some(data[offset..offset + genesis_len].to_vec())
        } else {
            None
        };
        offset += genesis_len;

        // Registry
        if data.len() < offset + 4 {
            return Err(PersistenceError::InvalidFormat);
        }

        let registry_len = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        if data.len() < offset + registry_len {
            return Err(PersistenceError::InvalidFormat);
        }

        let registry_data = data[offset..offset + registry_len].to_vec();
        offset += registry_len;

        // Revoked keys
        if data.len() < offset + 4 {
            return Err(PersistenceError::InvalidFormat);
        }

        let revoked_count = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        if data.len() < offset + revoked_count * 32 {
            return Err(PersistenceError::InvalidFormat);
        }

        let mut revoked_keys = Vec::with_capacity(revoked_count);
        for _ in 0..revoked_count {
            let mut key = [0u8; 32];
            key.copy_from_slice(&data[offset..offset + 32]);
            revoked_keys.push(key);
            offset += 32;
        }

        Ok(Self {
            version,
            device_private_key,
            genesis_data,
            registry_data,
            revoked_keys,
            persisted_at_ms,
        })
    }

    /// Set the persistence timestamp.
    pub fn set_persisted_at(&mut self, timestamp_ms: u64) {
        self.persisted_at_ms = timestamp_ms;
    }
}

/// In-memory storage for testing.
///
/// Not secure - only use for tests!
#[cfg(any(test, feature = "std"))]
pub struct MemoryStorage {
    data: std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>,
}

#[cfg(any(test, feature = "std"))]
impl MemoryStorage {
    /// Create a new in-memory storage.
    pub fn new() -> Self {
        Self {
            data: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

#[cfg(any(test, feature = "std"))]
impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "std"))]
impl SecureStorage for MemoryStorage {
    fn store(&self, key: &str, value: &[u8]) -> Result<(), PersistenceError> {
        let mut data = self
            .data
            .lock()
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;
        data.insert(key.to_string(), value.to_vec());
        Ok(())
    }

    fn retrieve(&self, key: &str) -> Result<Option<Vec<u8>>, PersistenceError> {
        let data = self
            .data
            .lock()
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;
        Ok(data.get(key).cloned())
    }

    fn delete(&self, key: &str) -> Result<(), PersistenceError> {
        let mut data = self
            .data
            .lock()
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;
        data.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{DeviceIdentity, MembershipPolicy, MeshGenesis};

    #[test]
    fn test_persisted_state_roundtrip() {
        let identity = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("TEST-MESH", &identity, MembershipPolicy::Controlled);

        let mut state = PersistedState::new(&identity, Some(&genesis));
        state.set_persisted_at(1234567890);
        state.add_revoked_key([0xAA; 32]);
        state.add_revoked_key([0xBB; 32]);

        let encoded = state.encode();
        let decoded = PersistedState::decode(&encoded).unwrap();

        assert_eq!(decoded.version, PERSISTED_STATE_VERSION);
        assert_eq!(decoded.persisted_at_ms, 1234567890);
        assert_eq!(decoded.revoked_keys.len(), 2);

        // Verify identity restoration
        let restored_identity = decoded.restore_identity().unwrap();
        assert_eq!(restored_identity.node_id(), identity.node_id());
        assert_eq!(restored_identity.public_key(), identity.public_key());

        // Verify genesis restoration
        let restored_genesis = decoded.restore_genesis().unwrap();
        assert_eq!(restored_genesis.mesh_id(), genesis.mesh_id());
    }

    #[test]
    fn test_persisted_state_without_genesis() {
        let identity = DeviceIdentity::generate();
        let state = PersistedState::new(&identity, None);

        let encoded = state.encode();
        let decoded = PersistedState::decode(&encoded).unwrap();

        assert!(decoded.restore_genesis().is_none());

        let restored_identity = decoded.restore_identity().unwrap();
        assert_eq!(restored_identity.node_id(), identity.node_id());
    }

    #[test]
    fn test_persisted_state_with_registry() {
        let identity1 = DeviceIdentity::generate();
        let identity2 = DeviceIdentity::generate();
        let identity3 = DeviceIdentity::generate();

        let mut registry = IdentityRegistry::new();
        registry.verify_or_register(&identity2.create_attestation(1000));
        registry.verify_or_register(&identity3.create_attestation(2000));

        let state = PersistedState::with_registry(&identity1, None, &registry);

        let encoded = state.encode();
        let decoded = PersistedState::decode(&encoded).unwrap();

        let restored_registry = decoded.restore_registry();
        assert_eq!(restored_registry.len(), 2);
        assert!(restored_registry.is_known(identity2.node_id()));
        assert!(restored_registry.is_known(identity3.node_id()));
    }

    #[test]
    fn test_memory_storage() {
        let storage = MemoryStorage::new();
        let identity = DeviceIdentity::generate();
        let state = PersistedState::new(&identity, None);

        // Save
        state.save(&storage).unwrap();

        // Load
        let loaded = PersistedState::load(&storage).unwrap();
        let restored = loaded.restore_identity().unwrap();
        assert_eq!(restored.node_id(), identity.node_id());

        // Delete
        PersistedState::delete(&storage).unwrap();
        assert!(matches!(
            PersistedState::load(&storage),
            Err(PersistenceError::NotFound)
        ));
    }

    #[test]
    fn test_invalid_magic() {
        let mut data = vec![0u8; 100];
        data[0..4].copy_from_slice(b"NOPE");

        assert!(matches!(
            PersistedState::decode(&data),
            Err(PersistenceError::InvalidFormat)
        ));
    }

    #[test]
    fn test_unsupported_version() {
        let identity = DeviceIdentity::generate();
        let state = PersistedState::new(&identity, None);
        let mut encoded = state.encode();

        // Set version to something higher than supported
        encoded[4..8].copy_from_slice(&999u32.to_le_bytes());

        assert!(matches!(
            PersistedState::decode(&encoded),
            Err(PersistenceError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn test_revoked_keys_deduplication() {
        let identity = DeviceIdentity::generate();
        let mut state = PersistedState::new(&identity, None);

        state.add_revoked_key([0xAA; 32]);
        state.add_revoked_key([0xAA; 32]); // Duplicate
        state.add_revoked_key([0xBB; 32]);

        assert_eq!(state.revoked_keys().len(), 2);
    }
}
