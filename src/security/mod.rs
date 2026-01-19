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

//! Security module for HIVE-BTLE
//!
//! Provides two layers of encryption:
//!
//! ## Phase 1: Mesh-Wide Encryption
//!
//! All formation members share a secret and can encrypt/decrypt documents.
//! Protects against external eavesdroppers.
//!
//! ```ignore
//! use hive_btle::security::MeshEncryptionKey;
//!
//! let secret = [0x42u8; 32];
//! let key = MeshEncryptionKey::from_shared_secret("DEMO", &secret);
//! let encrypted = key.encrypt(b"document").unwrap();
//! ```
//!
//! ## Phase 2: Per-Peer E2EE
//!
//! Two specific peers establish a unique session via X25519 key exchange.
//! Only sender and recipient can decrypt - other mesh members cannot.
//!
//! ```ignore
//! use hive_btle::security::PeerSessionManager;
//! use hive_btle::NodeId;
//!
//! let mut alice = PeerSessionManager::new(NodeId::new(0x11111111));
//! let mut bob = PeerSessionManager::new(NodeId::new(0x22222222));
//!
//! // Key exchange
//! let alice_msg = alice.initiate_session(NodeId::new(0x22222222), now_ms);
//! let (bob_response, _) = bob.handle_key_exchange(&alice_msg, now_ms).unwrap();
//! alice.handle_key_exchange(&bob_response, now_ms).unwrap();
//!
//! // Now Alice and Bob can communicate securely
//! let encrypted = alice.encrypt_for_peer(NodeId::new(0x22222222), b"secret", now_ms).unwrap();
//! let decrypted = bob.decrypt_from_peer(&encrypted, now_ms).unwrap();
//! ```
//!
//! ## Encryption Layers
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  Phase 1: Mesh-Wide (Formation Key)                             │
//! │  ┌─────────────────────────────────────────────────────────┐    │
//! │  │  All formation members can decrypt                       │    │
//! │  │  Protects: External eavesdroppers                        │    │
//! │  │  Overhead: 30 bytes                                      │    │
//! │  └─────────────────────────────────────────────────────────┘    │
//! │                                                                  │
//! │  Phase 2: Per-Peer E2EE (Session Key)                           │
//! │  ┌─────────────────────────────────────────────────────────┐    │
//! │  │  Only sender + recipient can decrypt                     │    │
//! │  │  Protects: Other mesh members, compromised relays        │    │
//! │  │  Overhead: 44 bytes                                      │    │
//! │  └─────────────────────────────────────────────────────────┘    │
//! └─────────────────────────────────────────────────────────────────┘
//! ```

mod genesis;
mod identity;
mod mesh_key;
mod peer_key;
mod peer_session;
mod registry;

// Device identity and attestation
pub use identity::{
    node_id_from_public_key, verify_signature, DeviceIdentity, IdentityAttestation, IdentityError,
};

// TOFU Identity Registry
pub use registry::{IdentityRecord, IdentityRegistry, RegistryResult};

// Mesh genesis and credentials
pub use genesis::{MembershipPolicy, MeshCredentials, MeshGenesis};

// Phase 1: Mesh-wide encryption
pub use mesh_key::{EncryptedDocument, EncryptionError, MeshEncryptionKey};

// Phase 2: Per-peer E2EE
pub use peer_key::{
    EphemeralKey, KeyExchangeMessage, PeerIdentityKey, PeerSessionKey, SharedSecret,
};
pub use peer_session::{
    PeerEncryptedMessage, PeerSession, PeerSessionManager, SessionState, DEFAULT_MAX_SESSIONS,
    DEFAULT_SESSION_TIMEOUT_MS,
};
