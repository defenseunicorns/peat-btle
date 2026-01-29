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

//! Signed payload utilities for transport-agnostic message authentication
//!
//! Provides helpers for encoding and verifying signed messages that work
//! across transport layers (BLE, WiFi, IP-based networks).
//!
//! # Wire Format
//!
//! ```text
//! [marker:1][payload:N][signature:64]
//! ```
//!
//! - **marker**: Single byte identifying the message type
//! - **payload**: Variable-length application data (type-specific)
//! - **signature**: Ed25519 signature over (marker || payload)
//!
//! The signature covers both the marker and payload, binding the message
//! type to the content to prevent cross-protocol attacks.
//!
//! # Example
//!
//! ```
//! use hive_btle::security::{DeviceIdentity, SignedPayload};
//!
//! let identity = DeviceIdentity::generate();
//!
//! // Encode a signed message
//! let marker = 0xAF;
//! let payload = [0x01, 0x02, 0x03, 0x04];
//! let wire = SignedPayload::encode(marker, &payload, &identity);
//!
//! // Decode and verify
//! let pubkey = identity.public_key();
//! assert!(SignedPayload::verify(&wire, &pubkey));
//!
//! // Extract components
//! let decoded = SignedPayload::decode(&wire).unwrap();
//! assert_eq!(decoded.marker, marker);
//! assert_eq!(decoded.payload, &payload);
//! ```

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::identity::{verify_signature, DeviceIdentity};

/// Signature size in bytes (Ed25519)
pub const SIGNATURE_SIZE: usize = 64;

/// Minimum wire size: marker (1) + signature (64)
pub const MIN_WIRE_SIZE: usize = 1 + SIGNATURE_SIZE;

/// Decoded signed payload
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedPayload<'a> {
    /// Message type marker
    pub marker: u8,
    /// Payload bytes (between marker and signature)
    pub payload: &'a [u8],
    /// Ed25519 signature
    pub signature: &'a [u8; 64],
}

/// Signed payload encoding and verification utilities
///
/// Transport-agnostic helpers for creating and verifying signed messages.
/// Used by hive-lite CannedMessage and HIVE protocol messages.
pub struct SignedPayload;

impl SignedPayload {
    /// Encode a signed payload
    ///
    /// Creates wire format: `[marker:1][payload:N][signature:64]`
    ///
    /// The signature covers `marker || payload`, binding the message type
    /// to the content.
    ///
    /// # Arguments
    /// * `marker` - Message type identifier
    /// * `payload` - Application data to sign
    /// * `identity` - Signer's identity (holds private key)
    ///
    /// # Returns
    /// Wire bytes ready for transmission
    pub fn encode(marker: u8, payload: &[u8], identity: &DeviceIdentity) -> Vec<u8> {
        // Build message to sign: marker || payload
        let mut to_sign = Vec::with_capacity(1 + payload.len());
        to_sign.push(marker);
        to_sign.extend_from_slice(payload);

        // Sign
        let signature = identity.sign(&to_sign);

        // Build wire: marker || payload || signature
        let mut wire = Vec::with_capacity(1 + payload.len() + SIGNATURE_SIZE);
        wire.push(marker);
        wire.extend_from_slice(payload);
        wire.extend_from_slice(&signature);

        wire
    }

    /// Encode with pre-computed signature
    ///
    /// Use when the signature is computed externally (e.g., by secure enclave).
    ///
    /// # Arguments
    /// * `marker` - Message type identifier
    /// * `payload` - Application data
    /// * `signature` - Pre-computed Ed25519 signature over (marker || payload)
    pub fn encode_with_signature(marker: u8, payload: &[u8], signature: &[u8; 64]) -> Vec<u8> {
        let mut wire = Vec::with_capacity(1 + payload.len() + SIGNATURE_SIZE);
        wire.push(marker);
        wire.extend_from_slice(payload);
        wire.extend_from_slice(signature);
        wire
    }

    /// Decode a signed payload without verification
    ///
    /// Extracts marker, payload, and signature from wire format.
    /// Does NOT verify the signature - call `verify()` separately.
    ///
    /// # Arguments
    /// * `wire` - Wire bytes in format `[marker:1][payload:N][signature:64]`
    ///
    /// # Returns
    /// `Some(DecodedPayload)` if wire is at least 65 bytes, `None` otherwise
    pub fn decode(wire: &[u8]) -> Option<DecodedPayload<'_>> {
        if wire.len() < MIN_WIRE_SIZE {
            return None;
        }

        let marker = wire[0];
        let payload_end = wire.len() - SIGNATURE_SIZE;
        let payload = &wire[1..payload_end];

        // Safe because we checked length above
        let signature: &[u8; 64] = wire[payload_end..].try_into().ok()?;

        Some(DecodedPayload {
            marker,
            payload,
            signature,
        })
    }

    /// Verify a signed payload
    ///
    /// Checks that the signature is valid for the given public key.
    ///
    /// # Arguments
    /// * `wire` - Wire bytes in format `[marker:1][payload:N][signature:64]`
    /// * `public_key` - Signer's Ed25519 public key
    ///
    /// # Returns
    /// `true` if signature is valid, `false` otherwise
    pub fn verify(wire: &[u8], public_key: &[u8; 32]) -> bool {
        let Some(decoded) = Self::decode(wire) else {
            return false;
        };

        // Reconstruct signed message: marker || payload
        let signed_len = wire.len() - SIGNATURE_SIZE;
        let to_verify = &wire[..signed_len];

        verify_signature(public_key, to_verify, decoded.signature)
    }

    /// Decode and verify in one step
    ///
    /// Convenience method that decodes and verifies, returning the decoded
    /// payload only if verification succeeds.
    ///
    /// # Arguments
    /// * `wire` - Wire bytes
    /// * `public_key` - Expected signer's public key
    ///
    /// # Returns
    /// `Some(DecodedPayload)` if valid, `None` if malformed or signature invalid
    pub fn decode_verified<'a>(wire: &'a [u8], public_key: &[u8; 32]) -> Option<DecodedPayload<'a>> {
        if !Self::verify(wire, public_key) {
            return None;
        }
        Self::decode(wire)
    }

    /// Get the payload size from total wire size
    ///
    /// Useful for pre-allocating buffers.
    #[inline]
    pub const fn payload_size(wire_size: usize) -> usize {
        wire_size.saturating_sub(MIN_WIRE_SIZE)
    }

    /// Get the wire size from payload size
    ///
    /// Useful for pre-allocating buffers.
    #[inline]
    pub const fn wire_size(payload_size: usize) -> usize {
        1 + payload_size + SIGNATURE_SIZE
    }

    /// Extract the marker byte without full decode
    ///
    /// Quick check for message type routing.
    #[inline]
    pub fn peek_marker(wire: &[u8]) -> Option<u8> {
        wire.first().copied()
    }

    /// Extract signature bytes without full verification
    ///
    /// Useful for caching or deferred verification.
    pub fn extract_signature(wire: &[u8]) -> Option<&[u8; 64]> {
        if wire.len() < MIN_WIRE_SIZE {
            return None;
        }
        let sig_start = wire.len() - SIGNATURE_SIZE;
        wire[sig_start..].try_into().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let identity = DeviceIdentity::generate();
        let marker = 0xAF;
        let payload = [0x01, 0x02, 0x03, 0x04, 0x05];

        let wire = SignedPayload::encode(marker, &payload, &identity);

        // Check wire size
        assert_eq!(wire.len(), SignedPayload::wire_size(payload.len()));
        assert_eq!(wire.len(), 1 + 5 + 64);

        // Decode
        let decoded = SignedPayload::decode(&wire).unwrap();
        assert_eq!(decoded.marker, marker);
        assert_eq!(decoded.payload, &payload);
    }

    #[test]
    fn test_verify_valid_signature() {
        let identity = DeviceIdentity::generate();
        let marker = 0xAF;
        let payload = b"Hello, mesh!";

        let wire = SignedPayload::encode(marker, payload, &identity);
        let pubkey = identity.public_key();

        assert!(SignedPayload::verify(&wire, &pubkey));
    }

    #[test]
    fn test_verify_wrong_pubkey() {
        let identity1 = DeviceIdentity::generate();
        let identity2 = DeviceIdentity::generate();

        let wire = SignedPayload::encode(0xAF, b"test", &identity1);
        let wrong_pubkey = identity2.public_key();

        assert!(!SignedPayload::verify(&wire, &wrong_pubkey));
    }

    #[test]
    fn test_verify_tampered_payload() {
        let identity = DeviceIdentity::generate();
        let mut wire = SignedPayload::encode(0xAF, b"original", &identity);
        let pubkey = identity.public_key();

        // Tamper with payload
        wire[1] ^= 0xFF;

        assert!(!SignedPayload::verify(&wire, &pubkey));
    }

    #[test]
    fn test_verify_tampered_marker() {
        let identity = DeviceIdentity::generate();
        let mut wire = SignedPayload::encode(0xAF, b"test", &identity);
        let pubkey = identity.public_key();

        // Change marker
        wire[0] = 0xBF;

        assert!(!SignedPayload::verify(&wire, &pubkey));
    }

    #[test]
    fn test_decode_verified() {
        let identity = DeviceIdentity::generate();
        let marker = 0xAF;
        let payload = b"verified content";

        let wire = SignedPayload::encode(marker, payload, &identity);
        let pubkey = identity.public_key();

        let decoded = SignedPayload::decode_verified(&wire, &pubkey).unwrap();
        assert_eq!(decoded.marker, marker);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_decode_verified_fails_bad_sig() {
        let identity1 = DeviceIdentity::generate();
        let identity2 = DeviceIdentity::generate();

        let wire = SignedPayload::encode(0xAF, b"test", &identity1);
        let wrong_pubkey = identity2.public_key();

        assert!(SignedPayload::decode_verified(&wire, &wrong_pubkey).is_none());
    }

    #[test]
    fn test_empty_payload() {
        let identity = DeviceIdentity::generate();
        let marker = 0x00;
        let payload: &[u8] = &[];

        let wire = SignedPayload::encode(marker, payload, &identity);
        assert_eq!(wire.len(), MIN_WIRE_SIZE);

        let decoded = SignedPayload::decode(&wire).unwrap();
        assert_eq!(decoded.marker, marker);
        assert!(decoded.payload.is_empty());

        assert!(SignedPayload::verify(&wire, &identity.public_key()));
    }

    #[test]
    fn test_peek_marker() {
        let identity = DeviceIdentity::generate();
        let wire = SignedPayload::encode(0xAB, b"test", &identity);

        assert_eq!(SignedPayload::peek_marker(&wire), Some(0xAB));
        assert_eq!(SignedPayload::peek_marker(&[]), None);
    }

    #[test]
    fn test_extract_signature() {
        let identity = DeviceIdentity::generate();
        let wire = SignedPayload::encode(0xAF, b"test", &identity);

        let sig = SignedPayload::extract_signature(&wire).unwrap();
        assert_eq!(sig.len(), 64);

        // Too short
        assert!(SignedPayload::extract_signature(&[0x01; 10]).is_none());
    }

    #[test]
    fn test_encode_with_signature() {
        let identity = DeviceIdentity::generate();
        let marker = 0xAF;
        let payload = b"external sig";

        // Compute signature externally
        let mut to_sign = Vec::new();
        to_sign.push(marker);
        to_sign.extend_from_slice(payload);
        let signature = identity.sign(&to_sign);

        // Encode with pre-computed signature
        let wire = SignedPayload::encode_with_signature(marker, payload, &signature);

        // Should verify
        assert!(SignedPayload::verify(&wire, &identity.public_key()));
    }

    #[test]
    fn test_wire_size_calculation() {
        assert_eq!(SignedPayload::wire_size(0), 65);
        assert_eq!(SignedPayload::wire_size(21), 86); // CannedMessage size
        assert_eq!(SignedPayload::wire_size(100), 165);

        assert_eq!(SignedPayload::payload_size(65), 0);
        assert_eq!(SignedPayload::payload_size(86), 21);
        assert_eq!(SignedPayload::payload_size(165), 100);
    }

    #[test]
    fn test_canned_message_size() {
        // CannedMessage: [0xAF][msg_code:1][src:4][tgt:4][timestamp:8][seq:4] = 22 bytes unsigned
        // Signed: [0xAF][msg_code:1][src:4][tgt:4][timestamp:8][seq:4][signature:64] = 86 bytes
        let payload_size = 1 + 4 + 4 + 8 + 4; // 21 bytes (without marker)
        assert_eq!(SignedPayload::wire_size(payload_size), 86);
    }
}
