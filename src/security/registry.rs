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
//! use eche_btle::security::{DeviceIdentity, IdentityRegistry, RegistryResult};
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

use super::identity::{node_id_from_public_key, IdentityAttestation};
use super::membership_token::{MembershipToken, MAX_CALLSIGN_LEN};
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

    /// Optional callsign assigned via MembershipToken
    /// None = unknown (TOFU identity only), Some = verified member
    pub callsign: Option<[u8; MAX_CALLSIGN_LEN]>,

    /// When the membership token expires (0 = never, None = no token)
    pub token_expires_ms: Option<u64>,
}

impl IdentityRecord {
    /// Get the callsign as a string (trimmed of null padding)
    pub fn callsign_str(&self) -> Option<&str> {
        self.callsign.as_ref().map(|cs| {
            let len = cs.iter().position(|&b| b == 0).unwrap_or(MAX_CALLSIGN_LEN);
            core::str::from_utf8(&cs[..len]).unwrap_or("")
        })
    }

    /// Check if the membership token has expired
    pub fn is_token_expired(&self, now_ms: u64) -> bool {
        match self.token_expires_ms {
            Some(0) => false, // Never expires
            Some(expires) => now_ms > expires,
            None => false, // No token = not expired (just TOFU)
        }
    }
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
                    callsign: None,
                    token_expires_ms: None,
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
                callsign: None,
                token_expires_ms: None,
            },
        );
    }

    /// Register a member via MembershipToken
    ///
    /// Validates the token signature and stores the callsign binding.
    /// Returns the NodeId for the registered member.
    ///
    /// # Arguments
    /// * `token` - The membership token to register
    /// * `authority_public_key` - The mesh authority's public key for verification
    /// * `now_ms` - Current time for expiration checking
    ///
    /// # Returns
    /// * `Ok(NodeId)` - The node was registered successfully
    /// * `Err(RegistryResult)` - Registration failed (invalid signature or key mismatch)
    pub fn register_member(
        &mut self,
        token: &MembershipToken,
        authority_public_key: &[u8; 32],
        now_ms: u64,
    ) -> Result<NodeId, RegistryResult> {
        // Verify token signature
        if !token.verify(authority_public_key) {
            return Err(RegistryResult::InvalidSignature);
        }

        // Check expiration
        if token.is_expired(now_ms) {
            return Err(RegistryResult::InvalidSignature); // Reuse for now
        }

        let node_id = node_id_from_public_key(&token.public_key);

        // Check for key mismatch if already known
        if let Some(existing) = self.known.get(&node_id) {
            if existing.public_key != token.public_key {
                return Err(RegistryResult::KeyMismatch { node_id });
            }
        }

        // Register or update
        self.known.insert(
            node_id,
            IdentityRecord {
                public_key: token.public_key,
                first_seen_ms: now_ms,
                last_seen_ms: now_ms,
                verification_count: 1,
                callsign: Some(token.callsign),
                token_expires_ms: Some(token.expires_at_ms),
            },
        );

        Ok(node_id)
    }

    /// Get the callsign for a known node
    pub fn get_callsign(&self, node_id: NodeId) -> Option<&str> {
        self.known.get(&node_id).and_then(|r| r.callsign_str())
    }

    /// Find a node by callsign
    pub fn find_by_callsign(&self, callsign: &str) -> Option<NodeId> {
        for (node_id, record) in &self.known {
            if let Some(cs) = record.callsign_str() {
                if cs == callsign {
                    return Some(*node_id);
                }
            }
        }
        None
    }

    /// Encode registry for persistence
    ///
    /// Format v2:
    /// - version (1 byte) = 2
    /// - count (4 bytes)
    /// - Per entry (77 bytes):
    ///   - node_id (4 bytes)
    ///   - public_key (32 bytes)
    ///   - first_seen_ms (8 bytes)
    ///   - last_seen_ms (8 bytes)
    ///   - verification_count (4 bytes)
    ///   - has_callsign (1 byte): 0 = no callsign, 1 = has callsign
    ///   - callsign (12 bytes, only if has_callsign)
    ///   - token_expires_ms (8 bytes, only if has_callsign)
    pub fn encode(&self) -> Vec<u8> {
        // Calculate size: version + count + entries
        let entry_size = 4 + 32 + 8 + 8 + 4 + 1 + MAX_CALLSIGN_LEN + 8; // 77 bytes
        let mut buf = Vec::with_capacity(1 + 4 + self.known.len() * entry_size);

        // Version byte
        buf.push(2);

        // Number of entries
        buf.extend_from_slice(&(self.known.len() as u32).to_le_bytes());

        for (node_id, record) in &self.known {
            buf.extend_from_slice(&node_id.as_u32().to_le_bytes());
            buf.extend_from_slice(&record.public_key);
            buf.extend_from_slice(&record.first_seen_ms.to_le_bytes());
            buf.extend_from_slice(&record.last_seen_ms.to_le_bytes());
            buf.extend_from_slice(&record.verification_count.to_le_bytes());

            // Callsign and token expiration
            if let Some(callsign) = &record.callsign {
                buf.push(1); // has_callsign
                buf.extend_from_slice(callsign);
                buf.extend_from_slice(&record.token_expires_ms.unwrap_or(0).to_le_bytes());
            } else {
                buf.push(0); // no callsign
                buf.extend_from_slice(&[0u8; MAX_CALLSIGN_LEN]);
                buf.extend_from_slice(&0u64.to_le_bytes());
            }
        }

        buf
    }

    /// Decode registry from bytes (supports v1 and v2 formats)
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.is_empty() {
            return None;
        }

        // Check version byte
        let version = data[0];

        match version {
            2 => Self::decode_v2(data),
            // v1 format: first byte is part of count (no version byte)
            // v1 count is u32 LE, so if first byte is small (0-255), it's likely v1
            _ => Self::decode_v1(data),
        }
    }

    /// Decode v1 format (legacy, no callsign)
    fn decode_v1(data: &[u8]) -> Option<Self> {
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
                    callsign: None,
                    token_expires_ms: None,
                },
            );
        }

        Some(registry)
    }

    /// Decode v2 format (with callsign support)
    fn decode_v2(data: &[u8]) -> Option<Self> {
        if data.len() < 5 {
            return None;
        }

        // Skip version byte
        let count = u32::from_le_bytes([data[1], data[2], data[3], data[4]]) as usize;
        let entry_size = 77; // 4 + 32 + 8 + 8 + 4 + 1 + 12 + 8

        if data.len() < 5 + count * entry_size {
            return None;
        }

        let mut registry = Self::new();
        let mut offset = 5;

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

            let has_callsign = data[offset] != 0;
            offset += 1;

            let (callsign, token_expires_ms) = if has_callsign {
                let mut cs = [0u8; MAX_CALLSIGN_LEN];
                cs.copy_from_slice(&data[offset..offset + MAX_CALLSIGN_LEN]);
                offset += MAX_CALLSIGN_LEN;

                let expires = u64::from_le_bytes([
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

                (Some(cs), Some(expires))
            } else {
                offset += MAX_CALLSIGN_LEN + 8; // Skip empty fields
                (None, None)
            };

            registry.known.insert(
                node_id,
                IdentityRecord {
                    public_key,
                    first_seen_ms,
                    last_seen_ms,
                    verification_count,
                    callsign,
                    token_expires_ms,
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

        let _identity2 = DeviceIdentity::generate();
        let node_id = identity1.node_id();

        // Manually create a conflicting record
        registry.known.insert(
            node_id,
            IdentityRecord {
                public_key: [0xAA; 32], // Different key
                first_seen_ms: 0,
                last_seen_ms: 0,
                verification_count: 1,
                callsign: None,
                token_expires_ms: None,
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

    #[test]
    fn test_register_member_with_token() {
        use crate::security::{MembershipPolicy, MeshGenesis};

        let mut registry = IdentityRegistry::new();
        let authority = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("ALPHA", &authority, MembershipPolicy::Controlled);
        let member = DeviceIdentity::generate();

        let token = MembershipToken::issue(
            &authority,
            &genesis,
            member.public_key(),
            "BRAVO-07",
            3600_000, // 1 hour
        );

        let now = 1000u64;
        let result = registry.register_member(&token, &authority.public_key(), now);
        assert!(result.is_ok());

        let node_id = result.unwrap();
        assert!(registry.is_known(node_id));
        assert_eq!(registry.get_callsign(node_id), Some("BRAVO-07"));
    }

    #[test]
    fn test_find_by_callsign() {
        use crate::security::{MembershipPolicy, MeshGenesis};

        let mut registry = IdentityRegistry::new();
        let authority = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("ALPHA", &authority, MembershipPolicy::Controlled);

        // Register multiple members
        let member1 = DeviceIdentity::generate();
        let token1 =
            MembershipToken::issue(&authority, &genesis, member1.public_key(), "ALPHA-01", 0);
        let node1 = registry
            .register_member(&token1, &authority.public_key(), 0)
            .unwrap();

        let member2 = DeviceIdentity::generate();
        let token2 =
            MembershipToken::issue(&authority, &genesis, member2.public_key(), "BRAVO-02", 0);
        let _node2 = registry
            .register_member(&token2, &authority.public_key(), 0)
            .unwrap();

        // Find by callsign
        assert_eq!(registry.find_by_callsign("ALPHA-01"), Some(node1));
        assert_eq!(registry.find_by_callsign("CHARLIE-03"), None);
    }

    #[test]
    fn test_register_member_wrong_authority() {
        use crate::security::{MembershipPolicy, MeshGenesis};

        let mut registry = IdentityRegistry::new();
        let authority = DeviceIdentity::generate();
        let other = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("ALPHA", &authority, MembershipPolicy::Controlled);
        let member = DeviceIdentity::generate();

        let token =
            MembershipToken::issue(&authority, &genesis, member.public_key(), "BRAVO-07", 0);

        // Try to register with wrong authority key
        let result = registry.register_member(&token, &other.public_key(), 0);
        assert!(matches!(result, Err(RegistryResult::InvalidSignature)));
    }

    #[test]
    fn test_encode_decode_with_callsign() {
        use crate::security::{MembershipPolicy, MeshGenesis};

        let mut registry = IdentityRegistry::new();
        let authority = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("ALPHA", &authority, MembershipPolicy::Controlled);

        // Register member with callsign
        let member = DeviceIdentity::generate();
        let token =
            MembershipToken::issue(&authority, &genesis, member.public_key(), "ALPHA-01", 0);
        let node_id = registry
            .register_member(&token, &authority.public_key(), 0)
            .unwrap();

        // Also register a plain TOFU identity (no callsign)
        let plain = DeviceIdentity::generate();
        let attestation = plain.create_attestation(0);
        registry.verify_or_register(&attestation);

        // Encode and decode
        let encoded = registry.encode();
        let decoded = IdentityRegistry::decode(&encoded).unwrap();

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded.get_callsign(node_id), Some("ALPHA-01"));
        assert_eq!(decoded.get_callsign(plain.node_id()), None);
    }
}
