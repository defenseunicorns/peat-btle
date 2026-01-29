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

//! Membership tokens for tactical trust in HIVE meshes
//!
//! A `MembershipToken` is an authority-signed credential that binds:
//! - A device's public key to a human-readable callsign
//! - The mesh this membership is valid for
//! - Expiration time for the credential
//!
//! Tokens are issued by the mesh authority (creator) and can be verified
//! by any node that knows the authority's public key.
//!
//! # Wire Format
//!
//! ```text
//! ┌──────────────┬─────────┬──────────┬────────────┬─────────────┬───────────────────┐
//! │ public_key   │ mesh_id │ callsign │ issued_at  │ expires_at  │ authority_sig     │
//! │ 32 bytes     │ 4 bytes │ 12 bytes │ 8 bytes    │ 8 bytes     │ 64 bytes          │
//! └──────────────┴─────────┴──────────┴────────────┴─────────────┴───────────────────┘
//! Total: 128 bytes
//! ```
//!
//! # Example
//!
//! ```
//! use hive_btle::security::{DeviceIdentity, MembershipToken, MeshGenesis, MembershipPolicy};
//!
//! // Authority creates the mesh
//! let authority = DeviceIdentity::generate();
//! let genesis = MeshGenesis::create("ALPHA", &authority, MembershipPolicy::Controlled);
//!
//! // New member generates identity
//! let member = DeviceIdentity::generate();
//!
//! // Authority issues token
//! let token = MembershipToken::issue(
//!     &authority,
//!     &genesis,
//!     member.public_key(),
//!     "BRAVO-07",
//!     3600 * 24 * 30 * 1000, // 30 days in ms
//! );
//!
//! // Anyone can verify with authority's public key
//! assert!(token.verify(&authority.public_key()));
//!
//! // Get the callsign
//! assert_eq!(token.callsign_str(), "BRAVO-07");
//! ```

#[cfg(not(feature = "std"))]
use alloc::string::String;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use super::genesis::MeshGenesis;
use super::identity::{verify_signature, DeviceIdentity};

/// Maximum callsign length (null-padded in wire format)
pub const MAX_CALLSIGN_LEN: usize = 12;

/// Size of mesh_id in bytes (matches MeshGenesis 8-char hex = 4 bytes)
pub const MESH_ID_SIZE: usize = 4;

/// Total wire size of a MembershipToken
pub const TOKEN_WIRE_SIZE: usize = 32 + 4 + 12 + 8 + 8 + 64; // 128 bytes

/// A membership token binding a device to a callsign within a mesh
///
/// Issued by the mesh authority and verifiable by any node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipToken {
    /// Member's Ed25519 public key
    pub public_key: [u8; 32],

    /// Mesh ID this token is valid for (4 bytes from MeshGenesis)
    pub mesh_id: [u8; MESH_ID_SIZE],

    /// Assigned callsign (up to 12 chars, null-padded)
    pub callsign: [u8; MAX_CALLSIGN_LEN],

    /// When this token was issued (milliseconds since Unix epoch)
    pub issued_at_ms: u64,

    /// When this token expires (milliseconds since Unix epoch)
    /// 0 means no expiration
    pub expires_at_ms: u64,

    /// Authority's Ed25519 signature over the above fields
    pub authority_signature: [u8; 64],
}

impl MembershipToken {
    /// Issue a new membership token
    ///
    /// # Arguments
    /// * `authority` - The mesh authority's identity (must be mesh creator)
    /// * `genesis` - The mesh genesis containing mesh_id
    /// * `member_public_key` - The new member's public key
    /// * `callsign` - Human-readable callsign (max 12 chars)
    /// * `validity_ms` - How long the token is valid (0 = forever)
    ///
    /// # Panics
    /// Panics if callsign is longer than 12 characters.
    pub fn issue(
        authority: &DeviceIdentity,
        genesis: &MeshGenesis,
        member_public_key: [u8; 32],
        callsign: &str,
        validity_ms: u64,
    ) -> Self {
        assert!(
            callsign.len() <= MAX_CALLSIGN_LEN,
            "callsign must be <= {} chars",
            MAX_CALLSIGN_LEN
        );

        let mesh_id = Self::mesh_id_bytes(&genesis.mesh_id());
        let mut callsign_bytes = [0u8; MAX_CALLSIGN_LEN];
        callsign_bytes[..callsign.len()].copy_from_slice(callsign.as_bytes());

        let now_ms = Self::now_ms();
        let expires_at_ms = if validity_ms == 0 {
            0
        } else {
            now_ms.saturating_add(validity_ms)
        };

        let mut token = Self {
            public_key: member_public_key,
            mesh_id,
            callsign: callsign_bytes,
            issued_at_ms: now_ms,
            expires_at_ms,
            authority_signature: [0u8; 64],
        };

        // Sign the token
        let signable = token.signable_bytes();
        token.authority_signature = authority.sign(&signable);

        token
    }

    /// Issue a token with explicit timestamps (for testing)
    pub fn issue_at(
        authority: &DeviceIdentity,
        mesh_id: [u8; MESH_ID_SIZE],
        member_public_key: [u8; 32],
        callsign: &str,
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> Self {
        assert!(
            callsign.len() <= MAX_CALLSIGN_LEN,
            "callsign must be <= {} chars",
            MAX_CALLSIGN_LEN
        );

        let mut callsign_bytes = [0u8; MAX_CALLSIGN_LEN];
        callsign_bytes[..callsign.len()].copy_from_slice(callsign.as_bytes());

        let mut token = Self {
            public_key: member_public_key,
            mesh_id,
            callsign: callsign_bytes,
            issued_at_ms,
            expires_at_ms,
            authority_signature: [0u8; 64],
        };

        let signable = token.signable_bytes();
        token.authority_signature = authority.sign(&signable);

        token
    }

    /// Verify the token's authority signature
    ///
    /// # Arguments
    /// * `authority_public_key` - The mesh authority's public key
    ///
    /// # Returns
    /// `true` if the signature is valid
    pub fn verify(&self, authority_public_key: &[u8; 32]) -> bool {
        let signable = self.signable_bytes();
        verify_signature(authority_public_key, &signable, &self.authority_signature)
    }

    /// Check if the token has expired
    ///
    /// # Arguments
    /// * `now_ms` - Current time in milliseconds since epoch
    ///
    /// # Returns
    /// `true` if the token has expired (expires_at_ms != 0 and now_ms > expires_at_ms)
    pub fn is_expired(&self, now_ms: u64) -> bool {
        self.expires_at_ms != 0 && now_ms > self.expires_at_ms
    }

    /// Check if the token is valid (signature OK and not expired)
    ///
    /// # Arguments
    /// * `authority_public_key` - The mesh authority's public key
    /// * `now_ms` - Current time in milliseconds since epoch
    pub fn is_valid(&self, authority_public_key: &[u8; 32], now_ms: u64) -> bool {
        self.verify(authority_public_key) && !self.is_expired(now_ms)
    }

    /// Get the callsign as a string (trimmed of null padding)
    pub fn callsign_str(&self) -> &str {
        let len = self
            .callsign
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(MAX_CALLSIGN_LEN);
        // Safety: We control callsign creation and only allow valid UTF-8
        core::str::from_utf8(&self.callsign[..len]).unwrap_or("")
    }

    /// Get the mesh_id as a hex string (e.g., "A1B2C3D4")
    pub fn mesh_id_hex(&self) -> String {
        format!(
            "{:02X}{:02X}{:02X}{:02X}",
            self.mesh_id[0], self.mesh_id[1], self.mesh_id[2], self.mesh_id[3]
        )
    }

    /// Encode token to wire format (128 bytes)
    pub fn encode(&self) -> [u8; TOKEN_WIRE_SIZE] {
        let mut buf = [0u8; TOKEN_WIRE_SIZE];
        let mut offset = 0;

        buf[offset..offset + 32].copy_from_slice(&self.public_key);
        offset += 32;

        buf[offset..offset + MESH_ID_SIZE].copy_from_slice(&self.mesh_id);
        offset += MESH_ID_SIZE;

        buf[offset..offset + MAX_CALLSIGN_LEN].copy_from_slice(&self.callsign);
        offset += MAX_CALLSIGN_LEN;

        buf[offset..offset + 8].copy_from_slice(&self.issued_at_ms.to_le_bytes());
        offset += 8;

        buf[offset..offset + 8].copy_from_slice(&self.expires_at_ms.to_le_bytes());
        offset += 8;

        buf[offset..offset + 64].copy_from_slice(&self.authority_signature);

        buf
    }

    /// Decode token from wire format
    ///
    /// Returns `None` if data is not exactly 128 bytes.
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() != TOKEN_WIRE_SIZE {
            return None;
        }

        let mut offset = 0;

        let mut public_key = [0u8; 32];
        public_key.copy_from_slice(&data[offset..offset + 32]);
        offset += 32;

        let mut mesh_id = [0u8; MESH_ID_SIZE];
        mesh_id.copy_from_slice(&data[offset..offset + MESH_ID_SIZE]);
        offset += MESH_ID_SIZE;

        let mut callsign = [0u8; MAX_CALLSIGN_LEN];
        callsign.copy_from_slice(&data[offset..offset + MAX_CALLSIGN_LEN]);
        offset += MAX_CALLSIGN_LEN;

        let issued_at_ms = u64::from_le_bytes([
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

        let expires_at_ms = u64::from_le_bytes([
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

        let mut authority_signature = [0u8; 64];
        authority_signature.copy_from_slice(&data[offset..offset + 64]);

        Some(Self {
            public_key,
            mesh_id,
            callsign,
            issued_at_ms,
            expires_at_ms,
            authority_signature,
        })
    }

    /// Get the bytes that are signed (everything except the signature)
    fn signable_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(TOKEN_WIRE_SIZE - 64);
        buf.extend_from_slice(&self.public_key);
        buf.extend_from_slice(&self.mesh_id);
        buf.extend_from_slice(&self.callsign);
        buf.extend_from_slice(&self.issued_at_ms.to_le_bytes());
        buf.extend_from_slice(&self.expires_at_ms.to_le_bytes());
        buf
    }

    /// Convert mesh_id hex string to bytes
    fn mesh_id_bytes(hex: &str) -> [u8; MESH_ID_SIZE] {
        let mut bytes = [0u8; MESH_ID_SIZE];
        // MeshGenesis returns 8 hex chars = 4 bytes
        if hex.len() == 8 {
            for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
                if i < MESH_ID_SIZE {
                    let s = core::str::from_utf8(chunk).unwrap_or("00");
                    bytes[i] = u8::from_str_radix(s, 16).unwrap_or(0);
                }
            }
        }
        bytes
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::MembershipPolicy;

    #[test]
    fn test_issue_and_verify() {
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

        assert!(token.verify(&authority.public_key()));
        assert_eq!(token.callsign_str(), "BRAVO-07");
        assert_eq!(token.public_key, member.public_key());
    }

    #[test]
    fn test_wrong_authority_fails() {
        let authority = DeviceIdentity::generate();
        let other = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("ALPHA", &authority, MembershipPolicy::Controlled);
        let member = DeviceIdentity::generate();

        let token = MembershipToken::issue(
            &authority,
            &genesis,
            member.public_key(),
            "BRAVO-07",
            3600_000,
        );

        // Verification with wrong authority should fail
        assert!(!token.verify(&other.public_key()));
    }

    #[test]
    fn test_tampered_token_fails() {
        let authority = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("ALPHA", &authority, MembershipPolicy::Controlled);
        let member = DeviceIdentity::generate();

        let mut token = MembershipToken::issue(
            &authority,
            &genesis,
            member.public_key(),
            "BRAVO-07",
            3600_000,
        );

        // Tamper with callsign
        token.callsign[0] = b'X';

        assert!(!token.verify(&authority.public_key()));
    }

    #[test]
    fn test_expiration() {
        let authority = DeviceIdentity::generate();
        let mesh_id = [0x12, 0x34, 0x56, 0x78];
        let member = DeviceIdentity::generate();

        let token = MembershipToken::issue_at(
            &authority,
            mesh_id,
            member.public_key(),
            "ALPHA-01",
            1000, // issued at
            2000, // expires at
        );

        // Before expiration
        assert!(!token.is_expired(1500));
        assert!(token.is_valid(&authority.public_key(), 1500));

        // After expiration
        assert!(token.is_expired(2500));
        assert!(!token.is_valid(&authority.public_key(), 2500));
    }

    #[test]
    fn test_no_expiration() {
        let authority = DeviceIdentity::generate();
        let mesh_id = [0x12, 0x34, 0x56, 0x78];
        let member = DeviceIdentity::generate();

        let token = MembershipToken::issue_at(
            &authority,
            mesh_id,
            member.public_key(),
            "ALPHA-01",
            1000,
            0, // Never expires
        );

        // Should never expire
        assert!(!token.is_expired(u64::MAX));
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let authority = DeviceIdentity::generate();
        let genesis = MeshGenesis::create("ALPHA", &authority, MembershipPolicy::Controlled);
        let member = DeviceIdentity::generate();

        let token = MembershipToken::issue(
            &authority,
            &genesis,
            member.public_key(),
            "CHARLIE-12",
            86400_000, // 24 hours
        );

        let encoded = token.encode();
        assert_eq!(encoded.len(), TOKEN_WIRE_SIZE);

        let decoded = MembershipToken::decode(&encoded).unwrap();
        assert_eq!(decoded, token);
        assert!(decoded.verify(&authority.public_key()));
    }

    #[test]
    fn test_callsign_str_trimmed() {
        let authority = DeviceIdentity::generate();
        let mesh_id = [0x12, 0x34, 0x56, 0x78];
        let member = DeviceIdentity::generate();

        // Short callsign
        let token =
            MembershipToken::issue_at(&authority, mesh_id, member.public_key(), "A-1", 0, 0);
        assert_eq!(token.callsign_str(), "A-1");

        // Max length callsign
        let token = MembershipToken::issue_at(
            &authority,
            mesh_id,
            member.public_key(),
            "ALPHA-BRAVO1",
            0,
            0,
        );
        assert_eq!(token.callsign_str(), "ALPHA-BRAVO1");
    }

    #[test]
    fn test_mesh_id_hex() {
        let authority = DeviceIdentity::generate();
        let mesh_id = [0xAB, 0xCD, 0xEF, 0x12];
        let member = DeviceIdentity::generate();

        let token =
            MembershipToken::issue_at(&authority, mesh_id, member.public_key(), "TEST", 0, 0);

        assert_eq!(token.mesh_id_hex(), "ABCDEF12");
    }

    #[test]
    fn test_wire_size() {
        // Verify our constant matches actual struct encoding
        assert_eq!(TOKEN_WIRE_SIZE, 128);
        assert_eq!(
            TOKEN_WIRE_SIZE,
            32 + MESH_ID_SIZE + MAX_CALLSIGN_LEN + 8 + 8 + 64
        );
    }

    #[test]
    #[should_panic(expected = "callsign must be <= 12 chars")]
    fn test_callsign_too_long_panics() {
        let authority = DeviceIdentity::generate();
        let mesh_id = [0x12, 0x34, 0x56, 0x78];
        let member = DeviceIdentity::generate();

        MembershipToken::issue_at(
            &authority,
            mesh_id,
            member.public_key(),
            "THIS-IS-TOO-LONG",
            0,
            0,
        );
    }
}
