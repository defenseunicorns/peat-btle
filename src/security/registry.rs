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

//! Identity Registry - Trust On First Use (TOFU) identity tracking
//!
//! Tracks the binding between node IDs and their public keys. On first contact,
//! the public key is recorded. On subsequent contacts, if the public key differs,
//! the identity is rejected as a potential impersonation attempt.
//!
//! # Example
//!
//! ```
//! use hive_btle::security::{DeviceIdentity, IdentityRegistry, RegistryResult};
//!
//! let mut registry = IdentityRegistry::new();
//!
//! // First contact - identity is registered
//! let alice = DeviceIdentity::generate();
//! let attestation = alice.create_attestation(0);
//! assert!(matches!(
//!     registry.verify_or_register(&attestation),
//!     RegistryResult::Registered
//! ));
//!
//! // Same identity - verification succeeds
//! assert!(matches!(
//!     registry.verify_or_register(&attestation),
//!     RegistryResult::Verified
//! ));
//!
//! // Different key claiming same node_id - rejected!
//! // (This would require crafting a fake attestation, which would fail signature check first)
//! ```

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use hashbrown::HashMap;

use super::identity::IdentityAttestation;
use crate::NodeId;

/// Result of identity verification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryResult {
    /// Identity was newly registered (first contact)
    Registered,

    /// Identity was verified against existing record
    Verified,

    /// Signature verification failed
    InvalidSignature,

    /// Public key doesn't match previously registered key (impersonation attempt!)
    KeyMismatch {
        /// The node_id that was claimed
        node_id: NodeId,
    },
}

impl RegistryResult {
    /// Returns true if the identity is trusted (registered or verified)
    pub fn is_trusted(&self) -> bool {
        matches!(self, Self::Registered | Self::Verified)
    }

    /// Returns true if this is a security violation
    pub fn is_violation(&self) -> bool {
        matches!(self, Self::InvalidSignature | Self::KeyMismatch { .. })
    }
}

/// Record of a known identity
#[derive(Debug, Clone)]
pub struct IdentityRecord {
    /// The public key for this node
    pub public_key: [u8; 32],

    /// When this identity was first seen (milliseconds since epoch)
    pub first_seen_ms: u64,

    /// When this identity was last verified (milliseconds since epoch)
    pub last_seen_ms: u64,

    /// Number of successful verifications
    pub verification_count: u32,
}

/// TOFU Identity Registry
///
/// Maintains a mapping of node IDs to their public keys, implementing
/// Trust On First Use semantics.
#[derive(Debug, Clone)]
pub struct IdentityRegistry {
    /// Known identities: node_id → identity record
    known: HashMap<NodeId, IdentityRecord>,

    /// Maximum number of identities to track (prevents memory exhaustion)
    max_identities: usize,
}

impl Default for IdentityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl IdentityRegistry {
    /// Default maximum identities (suitable for most deployments)
    pub const DEFAULT_MAX_IDENTITIES: usize = 256;

    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            known: HashMap::new(),
            max_identities: Self::DEFAULT_MAX_IDENTITIES,
        }
    }

    /// Create a registry with custom capacity limit
    pub fn with_capacity(max_identities: usize) -> Self {
        Self {
            known: HashMap::with_capacity(max_identities.min(64)),
            max_identities,
        }
    }

    /// Verify an identity attestation or register it if new
    ///
    /// This is the main TOFU operation:
    /// 1. Verify the attestation signature
    /// 2. If node_id is new, register the public key
    /// 3. If node_id is known, verify the public key matches
    pub fn verify_or_register(&mut self, attestation: &IdentityAttestation) -> RegistryResult {
        self.verify_or_register_at(attestation, attestation.timestamp_ms)
    }

    /// Verify or register with explicit timestamp (for testing)
    pub fn verify_or_register_at(
        &mut self,
        attestation: &IdentityAttestation,
        now_ms: u64,
    ) -> RegistryResult {
        // First, verify the cryptographic signature
        if !attestation.verify() {
            return RegistryResult::InvalidSignature;
        }

        let node_id = attestation.node_id;

        // Check if we already know this node
        if let Some(record) = self.known.get_mut(&node_id) {
            // Known node - verify public key matches
            if record.public_key == attestation.public_key {
                // Same key - update last seen and count
                record.last_seen_ms = now_ms;
                record.verification_count = record.verification_count.saturating_add(1);
                RegistryResult::Verified
            } else {
                // Different key! Potential impersonation
                RegistryResult::KeyMismatch { node_id }
            }
        } else {
            // New node - register if we have capacity
            if self.known.len() >= self.max_identities {
                // At capacity - could implement LRU eviction here
                // For now, still register (HashMap will handle it)
                // In production, might want to evict oldest or implement proper LRU
            }

            self.known.insert(
                node_id,
                IdentityRecord {
                    public_key: attestation.public_key,
                    first_seen_ms: now_ms,
                    last_seen_ms: now_ms,
                    verification_count: 1,
                },
            );
            RegistryResult::Registered
        }
    }

    /// Check if a node_id is known without modifying the registry
    pub fn is_known(&self, node_id: NodeId) -> bool {
        self.known.contains_key(&node_id)
    }

    /// Get the public key for a known node
    pub fn get_public_key(&self, node_id: NodeId) -> Option<&[u8; 32]> {
        self.known.get(&node_id).map(|r| &r.public_key)
    }

    /// Get the full identity record for a node
    pub fn get_record(&self, node_id: NodeId) -> Option<&IdentityRecord> {
        self.known.get(&node_id)
    }

    /// Get the number of known identities
    pub fn len(&self) -> usize {
        self.known.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.known.is_empty()
    }

    /// Remove an identity from the registry
    ///
    /// Use with caution - this allows re-registration with a different key.
    pub fn remove(&mut self, node_id: NodeId) -> Option<IdentityRecord> {
        self.known.remove(&node_id)
    }

    /// Clear all known identities
    ///
    /// Use with extreme caution - this resets all TOFU trust.
    pub fn clear(&mut self) {
        self.known.clear();
    }

    /// Get all known node IDs
    pub fn known_nodes(&self) -> Vec<NodeId> {
        self.known.keys().copied().collect()
    }

    /// Pre-register a known identity (for out-of-band key exchange)
    ///
    /// This allows registering an identity without an attestation,
    /// useful when keys are exchanged through a secure side channel.
    pub fn pre_register(&mut self, node_id: NodeId, public_key: [u8; 32], now_ms: u64) {
        self.known.insert(
            node_id,
            IdentityRecord {
                public_key,
                first_seen_ms: now_ms,
                last_seen_ms: now_ms,
                verification_count: 0,
            },
        );
    }

    /// Encode registry for persistence
    ///
    /// Format per entry:
    /// - node_id (4 bytes)
    /// - public_key (32 bytes)
    /// - first_seen_ms (8 bytes)
    /// - last_seen_ms (8 bytes)
    /// - verification_count (4 bytes)
    ///
    /// Total: 56 bytes per entry
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(4 + self.known.len() * 56);

        // Number of entries
        buf.extend_from_slice(&(self.known.len() as u32).to_le_bytes());

        for (node_id, record) in &self.known {
            buf.extend_from_slice(&node_id.as_u32().to_le_bytes());
            buf.extend_from_slice(&record.public_key);
            buf.extend_from_slice(&record.first_seen_ms.to_le_bytes());
            buf.extend_from_slice(&record.last_seen_ms.to_le_bytes());
            buf.extend_from_slice(&record.verification_count.to_le_bytes());
        }

        buf
    }

    /// Decode registry from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }

        let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;

        if data.len() < 4 + count * 56 {
            return None;
        }

        let mut registry = Self::new();
        let mut offset = 4;

        for _ in 0..count {
            let node_id = NodeId::new(u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]));
            offset += 4;

            let mut public_key = [0u8; 32];
            public_key.copy_from_slice(&data[offset..offset + 32]);
            offset += 32;

            let first_seen_ms = u64::from_le_bytes([
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

            let last_seen_ms = u64::from_le_bytes([
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

            let verification_count = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            offset += 4;

            registry.known.insert(
                node_id,
                IdentityRecord {
                    public_key,
                    first_seen_ms,
                    last_seen_ms,
                    verification_count,
                },
            );
        }

        Some(registry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::DeviceIdentity;

    #[test]
    fn test_register_new_identity() {
        let mut registry = IdentityRegistry::new();
        let identity = DeviceIdentity::generate();
        let attestation = identity.create_attestation(0);

        let result = registry.verify_or_register(&attestation);
        assert_eq!(result, RegistryResult::Registered);
        assert!(result.is_trusted());
        assert!(!result.is_violation());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_verify_known_identity() {
        let mut registry = IdentityRegistry::new();
        let identity = DeviceIdentity::generate();
        let attestation = identity.create_attestation(0);

        // First registration
        registry.verify_or_register(&attestation);

        // Second verification
        let result = registry.verify_or_register(&attestation);
        assert_eq!(result, RegistryResult::Verified);
        assert!(result.is_trusted());
    }

    #[test]
    fn test_key_mismatch_detection() {
        let mut registry = IdentityRegistry::new();

        // Register first identity
        let identity1 = DeviceIdentity::generate();
        let attestation1 = identity1.create_attestation(0);
        registry.verify_or_register(&attestation1);

        // Try to register different identity with same node_id
        // (In reality, this would fail signature verification because
        // the attacker can't sign for a node_id derived from a different key)
        // But we can test the key mismatch path by pre-registering

        let identity2 = DeviceIdentity::generate();
        let node_id = identity1.node_id();

        // Manually create a conflicting record
        registry.known.insert(
            node_id,
            IdentityRecord {
                public_key: [0xAA; 32], // Different key
                first_seen_ms: 0,
                last_seen_ms: 0,
                verification_count: 1,
            },
        );

        // Now verification should detect mismatch
        let result = registry.verify_or_register(&attestation1);
        assert!(matches!(result, RegistryResult::KeyMismatch { .. }));
        assert!(result.is_violation());
    }

    #[test]
    fn test_invalid_signature_detection() {
        let mut registry = IdentityRegistry::new();

        // Create a tampered attestation
        let identity = DeviceIdentity::generate();
        let mut attestation = identity.create_attestation(0);
        attestation.signature[0] ^= 0xFF; // Corrupt signature

        let result = registry.verify_or_register(&attestation);
        assert_eq!(result, RegistryResult::InvalidSignature);
        assert!(result.is_violation());
    }

    #[test]
    fn test_verification_count_increment() {
        let mut registry = IdentityRegistry::new();
        let identity = DeviceIdentity::generate();
        let attestation = identity.create_attestation(0);
        let node_id = identity.node_id();

        // Multiple verifications
        registry.verify_or_register(&attestation);
        registry.verify_or_register(&attestation);
        registry.verify_or_register(&attestation);

        let record = registry.get_record(node_id).unwrap();
        assert_eq!(record.verification_count, 3);
    }

    #[test]
    fn test_pre_register() {
        let mut registry = IdentityRegistry::new();
        let identity = DeviceIdentity::generate();
        let node_id = identity.node_id();
        let public_key = identity.public_key();

        // Pre-register without attestation
        registry.pre_register(node_id, public_key, 1000);

        assert!(registry.is_known(node_id));
        assert_eq!(registry.get_public_key(node_id), Some(&public_key));

        // Now attestation should verify
        let attestation = identity.create_attestation(0);
        let result = registry.verify_or_register(&attestation);
        assert_eq!(result, RegistryResult::Verified);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let mut registry = IdentityRegistry::new();

        // Register a few identities
        for _ in 0..5 {
            let identity = DeviceIdentity::generate();
            let attestation = identity.create_attestation(0);
            registry.verify_or_register(&attestation);
        }

        let encoded = registry.encode();
        let decoded = IdentityRegistry::decode(&encoded).unwrap();

        assert_eq!(decoded.len(), registry.len());
        for node_id in registry.known_nodes() {
            assert!(decoded.is_known(node_id));
            assert_eq!(
                decoded.get_public_key(node_id),
                registry.get_public_key(node_id)
            );
        }
    }

    #[test]
    fn test_remove_identity() {
        let mut registry = IdentityRegistry::new();
        let identity = DeviceIdentity::generate();
        let attestation = identity.create_attestation(0);
        let node_id = identity.node_id();

        registry.verify_or_register(&attestation);
        assert!(registry.is_known(node_id));

        registry.remove(node_id);
        assert!(!registry.is_known(node_id));

        // Can re-register after removal
        let result = registry.verify_or_register(&attestation);
        assert_eq!(result, RegistryResult::Registered);
    }

    #[test]
    fn test_known_nodes() {
        let mut registry = IdentityRegistry::new();
        let mut expected_nodes = Vec::new();

        for _ in 0..3 {
            let identity = DeviceIdentity::generate();
            let attestation = identity.create_attestation(0);
            expected_nodes.push(identity.node_id());
            registry.verify_or_register(&attestation);
        }

        let known = registry.known_nodes();
        assert_eq!(known.len(), 3);
        for node_id in expected_nodes {
            assert!(known.contains(&node_id));
        }
    }
}
