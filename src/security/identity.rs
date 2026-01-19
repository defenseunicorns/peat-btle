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

//! Device identity management using Ed25519 signatures
//!
//! Each device in a HIVE mesh has a cryptographic identity consisting of:
//! - An Ed25519 signing keypair (private key stored securely)
//! - A derived NodeId (first 4 bytes of BLAKE3 hash of public key)
//!
//! This enables:
//! - Cryptographic binding between node_id and device
//! - Document signing to prove authorship
//! - Identity attestation to prevent impersonation
//!
//! # Example
//!
//! ```
//! use hive_btle::security::DeviceIdentity;
//!
//! // Generate a new identity
//! let identity = DeviceIdentity::generate();
//!
//! // Get the derived node_id
//! let node_id = identity.node_id();
//!
//! // Sign a message
//! let message = b"Hello, mesh!";
//! let signature = identity.sign(message);
//!
//! // Verify signature with public key
//! assert!(identity.verify(message, &signature));
//! ```

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;

use crate::NodeId;

/// Errors that can occur during identity operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityError {
    /// Invalid signature format or verification failed
    InvalidSignature,
    /// Invalid public key format
    InvalidPublicKey,
    /// Invalid private key format
    InvalidPrivateKey,
    /// Serialization/deserialization error
    SerializationError,
}

impl core::fmt::Display for IdentityError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidSignature => write!(f, "invalid signature"),
            Self::InvalidPublicKey => write!(f, "invalid public key"),
            Self::InvalidPrivateKey => write!(f, "invalid private key"),
            Self::SerializationError => write!(f, "serialization error"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for IdentityError {}

/// A device's cryptographic identity
///
/// Contains an Ed25519 signing key and derives a unique NodeId from it.
/// The private key should be stored securely (platform secure enclave if available).
pub struct DeviceIdentity {
    /// Ed25519 signing key (contains both private and public key)
    signing_key: SigningKey,
}

impl DeviceIdentity {
    /// Generate a new random device identity
    ///
    /// Uses the platform's cryptographically secure random number generator.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self { signing_key }
    }

    /// Create identity from existing private key bytes
    ///
    /// # Arguments
    /// * `private_key` - 32-byte Ed25519 private key
    ///
    /// # Returns
    /// * `Ok(DeviceIdentity)` - If key is valid
    /// * `Err(IdentityError)` - If key format is invalid
    pub fn from_private_key(private_key: &[u8; 32]) -> Result<Self, IdentityError> {
        let signing_key = SigningKey::from_bytes(private_key);
        Ok(Self { signing_key })
    }

    /// Get the private key bytes for secure storage
    ///
    /// **Security**: This exposes the private key. Only use for persisting
    /// to secure storage (keychain, secure enclave, encrypted NVS).
    pub fn private_key_bytes(&self) -> [u8; 32] {
        self.signing_key.to_bytes()
    }

    /// Get the public key bytes
    ///
    /// This can be shared freely to allow others to verify signatures.
    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Get the verifying key for signature verification
    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Derive the NodeId from the public key
    ///
    /// The NodeId is the first 4 bytes of the BLAKE3 hash of the public key,
    /// interpreted as a little-endian u32. This provides:
    /// - Deterministic derivation (same key = same node_id)
    /// - Collision resistance (BLAKE3 is cryptographically secure)
    /// - Compact representation (4 bytes vs 32 bytes)
    pub fn node_id(&self) -> NodeId {
        let public_key = self.public_key();
        let hash = blake3::hash(&public_key);
        let hash_bytes = hash.as_bytes();

        // First 4 bytes as little-endian u32
        let id = u32::from_le_bytes([hash_bytes[0], hash_bytes[1], hash_bytes[2], hash_bytes[3]]);

        NodeId::new(id)
    }

    /// Sign a message
    ///
    /// # Arguments
    /// * `message` - Arbitrary bytes to sign
    ///
    /// # Returns
    /// 64-byte Ed25519 signature
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        let signature = self.signing_key.sign(message);
        signature.to_bytes()
    }

    /// Verify a signature made by this identity
    ///
    /// # Arguments
    /// * `message` - Original message that was signed
    /// * `signature` - 64-byte signature to verify
    ///
    /// # Returns
    /// `true` if signature is valid, `false` otherwise
    pub fn verify(&self, message: &[u8], signature: &[u8; 64]) -> bool {
        let sig = Signature::from_bytes(signature);
        self.signing_key
            .verifying_key()
            .verify(message, &sig)
            .is_ok()
    }

    /// Create an identity attestation
    ///
    /// An attestation proves that the holder of this identity controls
    /// the claimed node_id at a specific point in time.
    pub fn create_attestation(&self, timestamp_ms: u64) -> IdentityAttestation {
        let node_id = self.node_id();
        let public_key = self.public_key();

        // Sign: node_id || public_key || timestamp
        let mut message = Vec::with_capacity(4 + 32 + 8);
        message.extend_from_slice(&node_id.as_u32().to_le_bytes());
        message.extend_from_slice(&public_key);
        message.extend_from_slice(&timestamp_ms.to_le_bytes());

        let signature = self.sign(&message);

        IdentityAttestation {
            node_id,
            public_key,
            timestamp_ms,
            signature,
        }
    }
}

impl Clone for DeviceIdentity {
    fn clone(&self) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(&self.signing_key.to_bytes()),
        }
    }
}

impl core::fmt::Debug for DeviceIdentity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeviceIdentity")
            .field("node_id", &self.node_id())
            .field("public_key", &hex_short(&self.public_key()))
            .field("private_key", &"[REDACTED]")
            .finish()
    }
}

/// An identity attestation proving ownership of a node_id
///
/// Used during mesh joining and periodic re-attestation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityAttestation {
    /// The claimed node_id
    pub node_id: NodeId,
    /// Public key (node_id is derived from this)
    pub public_key: [u8; 32],
    /// Timestamp when attestation was created (milliseconds since epoch)
    pub timestamp_ms: u64,
    /// Signature over (node_id || public_key || timestamp)
    pub signature: [u8; 64],
}

impl IdentityAttestation {
    /// Verify this attestation is valid
    ///
    /// Checks:
    /// 1. Signature is valid for the public key
    /// 2. node_id correctly derives from public key
    ///
    /// Does NOT check timestamp freshness (caller should do that).
    pub fn verify(&self) -> bool {
        // Verify node_id derives from public_key
        let hash = blake3::hash(&self.public_key);
        let hash_bytes = hash.as_bytes();
        let expected_id =
            u32::from_le_bytes([hash_bytes[0], hash_bytes[1], hash_bytes[2], hash_bytes[3]]);

        if self.node_id.as_u32() != expected_id {
            return false;
        }

        // Verify signature
        let verifying_key = match VerifyingKey::from_bytes(&self.public_key) {
            Ok(k) => k,
            Err(_) => return false,
        };

        let signature = Signature::from_bytes(&self.signature);

        // Reconstruct signed message
        let mut message = Vec::with_capacity(4 + 32 + 8);
        message.extend_from_slice(&self.node_id.as_u32().to_le_bytes());
        message.extend_from_slice(&self.public_key);
        message.extend_from_slice(&self.timestamp_ms.to_le_bytes());

        verifying_key.verify(&message, &signature).is_ok()
    }

    /// Encode attestation to bytes for wire transmission
    ///
    /// Format: node_id (4) || public_key (32) || timestamp (8) || signature (64) = 108 bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(108);
        buf.extend_from_slice(&self.node_id.as_u32().to_le_bytes());
        buf.extend_from_slice(&self.public_key);
        buf.extend_from_slice(&self.timestamp_ms.to_le_bytes());
        buf.extend_from_slice(&self.signature);
        buf
    }

    /// Decode attestation from bytes
    ///
    /// Returns None if data is not exactly 108 bytes.
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() != 108 {
            return None;
        }

        let node_id = NodeId::new(u32::from_le_bytes([data[0], data[1], data[2], data[3]]));

        let mut public_key = [0u8; 32];
        public_key.copy_from_slice(&data[4..36]);

        let timestamp_ms = u64::from_le_bytes([
            data[36], data[37], data[38], data[39], data[40], data[41], data[42], data[43],
        ]);

        let mut signature = [0u8; 64];
        signature.copy_from_slice(&data[44..108]);

        Some(Self {
            node_id,
            public_key,
            timestamp_ms,
            signature,
        })
    }
}

/// Verify a signature from a known public key
///
/// Utility function for verifying signatures without a full DeviceIdentity.
pub fn verify_signature(public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
    let verifying_key = match VerifyingKey::from_bytes(public_key) {
        Ok(k) => k,
        Err(_) => return false,
    };

    let sig = Signature::from_bytes(signature);

    verifying_key.verify(message, &sig).is_ok()
}

/// Derive NodeId from a public key
///
/// Utility function for deriving node_id without a full DeviceIdentity.
pub fn node_id_from_public_key(public_key: &[u8; 32]) -> NodeId {
    let hash = blake3::hash(public_key);
    let hash_bytes = hash.as_bytes();

    let id = u32::from_le_bytes([hash_bytes[0], hash_bytes[1], hash_bytes[2], hash_bytes[3]]);

    NodeId::new(id)
}

// Helper for debug output
fn hex_short(bytes: &[u8]) -> String {
    if bytes.len() <= 4 {
        hex::encode(bytes)
    } else {
        format!(
            "{}..{}",
            hex::encode(&bytes[..2]),
            hex::encode(&bytes[bytes.len() - 2..])
        )
    }
}

// Need hex for debug output
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_identity() {
        let identity = DeviceIdentity::generate();

        // node_id should be non-zero (extremely unlikely to be zero)
        assert_ne!(identity.node_id().as_u32(), 0);

        // Public key should be 32 bytes
        assert_eq!(identity.public_key().len(), 32);
    }

    #[test]
    fn test_identity_from_private_key() {
        let identity1 = DeviceIdentity::generate();
        let private_key = identity1.private_key_bytes();

        let identity2 = DeviceIdentity::from_private_key(&private_key).unwrap();

        // Same private key = same public key = same node_id
        assert_eq!(identity1.public_key(), identity2.public_key());
        assert_eq!(identity1.node_id(), identity2.node_id());
    }

    #[test]
    fn test_node_id_deterministic() {
        let identity = DeviceIdentity::generate();

        // Multiple calls return same node_id
        let id1 = identity.node_id();
        let id2 = identity.node_id();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_different_identities_different_node_ids() {
        let identity1 = DeviceIdentity::generate();
        let identity2 = DeviceIdentity::generate();

        // Different identities should have different node_ids
        // (collision probability is ~1 in 4 billion)
        assert_ne!(identity1.node_id(), identity2.node_id());
    }

    #[test]
    fn test_sign_verify() {
        let identity = DeviceIdentity::generate();
        let message = b"Test message for signing";

        let signature = identity.sign(message);
        assert!(identity.verify(message, &signature));
    }

    #[test]
    fn test_verify_wrong_message() {
        let identity = DeviceIdentity::generate();
        let message = b"Original message";
        let wrong_message = b"Wrong message";

        let signature = identity.sign(message);
        assert!(!identity.verify(wrong_message, &signature));
    }

    #[test]
    fn test_verify_wrong_key() {
        let identity1 = DeviceIdentity::generate();
        let identity2 = DeviceIdentity::generate();
        let message = b"Test message";

        let signature = identity1.sign(message);
        assert!(!identity2.verify(message, &signature));
    }

    #[test]
    fn test_attestation_create_verify() {
        let identity = DeviceIdentity::generate();
        let timestamp = 1705680000000u64; // Some timestamp

        let attestation = identity.create_attestation(timestamp);

        assert!(attestation.verify());
        assert_eq!(attestation.node_id, identity.node_id());
        assert_eq!(attestation.public_key, identity.public_key());
        assert_eq!(attestation.timestamp_ms, timestamp);
    }

    #[test]
    fn test_attestation_encode_decode() {
        let identity = DeviceIdentity::generate();
        let attestation = identity.create_attestation(1705680000000);

        let encoded = attestation.encode();
        assert_eq!(encoded.len(), 108);

        let decoded = IdentityAttestation::decode(&encoded).unwrap();
        assert_eq!(decoded, attestation);
        assert!(decoded.verify());
    }

    #[test]
    fn test_attestation_tampered() {
        let identity = DeviceIdentity::generate();
        let mut attestation = identity.create_attestation(1705680000000);

        // Tamper with timestamp
        attestation.timestamp_ms += 1;

        // Verification should fail
        assert!(!attestation.verify());
    }

    #[test]
    fn test_node_id_from_public_key() {
        let identity = DeviceIdentity::generate();
        let public_key = identity.public_key();

        let derived_id = node_id_from_public_key(&public_key);
        assert_eq!(derived_id, identity.node_id());
    }

    #[test]
    fn test_verify_signature_utility() {
        let identity = DeviceIdentity::generate();
        let message = b"Test with utility function";

        let signature = identity.sign(message);
        let public_key = identity.public_key();

        assert!(verify_signature(&public_key, message, &signature));
    }

    #[test]
    fn test_identity_clone() {
        let identity1 = DeviceIdentity::generate();
        let identity2 = identity1.clone();

        assert_eq!(identity1.public_key(), identity2.public_key());
        assert_eq!(identity1.node_id(), identity2.node_id());

        // Both can sign and verify each other
        let message = b"Clone test";
        let sig1 = identity1.sign(message);
        let sig2 = identity2.sign(message);

        assert!(identity1.verify(message, &sig2));
        assert!(identity2.verify(message, &sig1));
    }

    #[test]
    fn test_debug_redacts_private_key() {
        let identity = DeviceIdentity::generate();
        let debug_str = format!("{:?}", identity);

        assert!(debug_str.contains("REDACTED"));
        assert!(debug_str.contains("node_id"));
    }
}
