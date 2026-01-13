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


//! Per-Peer E2EE Example
//!
//! Demonstrates end-to-end encryption between specific peers.
//! Only the sender and recipient can decrypt these messages -
//! other mesh members cannot, even if they have the mesh-wide key.
//!
//! Run with: cargo run --example peer_e2ee

use hive_btle::observer::{HiveEvent, HiveObserver};
use hive_btle::security::PeerSessionManager;
use hive_btle::{HiveMesh, HiveMeshConfig, NodeId};
use std::sync::Arc;

/// Observer that tracks E2EE events
struct E2eeObserver {
    name: &'static str,
}

impl E2eeObserver {
    fn new(name: &'static str) -> Self {
        Self { name }
    }
}

impl HiveObserver for E2eeObserver {
    fn on_event(&self, event: HiveEvent) {
        match event {
            HiveEvent::PeerE2eeEstablished { peer_node_id } => {
                println!(
                    "[{}] E2EE session ESTABLISHED with {:08X}",
                    self.name,
                    peer_node_id.as_u32()
                );
            }
            HiveEvent::PeerE2eeMessageReceived { from_node, data } => {
                let message = String::from_utf8_lossy(&data);
                println!(
                    "[{}] E2EE message from {:08X}: \"{}\"",
                    self.name,
                    from_node.as_u32(),
                    message
                );
            }
            HiveEvent::PeerE2eeClosed { peer_node_id } => {
                println!(
                    "[{}] E2EE session CLOSED with {:08X}",
                    self.name,
                    peer_node_id.as_u32()
                );
            }
            _ => {}
        }
    }
}

fn main() {
    println!("=== HIVE-BTLE Per-Peer E2EE Example ===\n");

    // Create mesh nodes (using HiveMesh's built-in E2EE support)
    let config_alice = HiveMeshConfig::new(NodeId::new(0xAAAA1111), "ALICE", "DEMO");
    let config_bob = HiveMeshConfig::new(NodeId::new(0xBBBB2222), "BOB", "DEMO");

    let mesh_alice = HiveMesh::new(config_alice);
    let mesh_bob = HiveMesh::new(config_bob);

    mesh_alice.add_observer(Arc::new(E2eeObserver::new("ALICE")));
    mesh_bob.add_observer(Arc::new(E2eeObserver::new("BOB")));

    println!("Created nodes: ALICE, BOB\n");

    println!("--- Enabling Per-Peer E2EE ---");

    mesh_alice.enable_peer_e2ee();
    mesh_bob.enable_peer_e2ee();

    println!("Alice E2EE enabled: {}", mesh_alice.is_peer_e2ee_enabled());
    println!("Bob E2EE enabled: {}", mesh_bob.is_peer_e2ee_enabled());

    if let Some(pk) = mesh_alice.peer_e2ee_public_key() {
        println!(
            "Alice public key: {:02X}{:02X}{:02X}...",
            pk[0], pk[1], pk[2]
        );
    }
    if let Some(pk) = mesh_bob.peer_e2ee_public_key() {
        println!("Bob public key: {:02X}{:02X}{:02X}...", pk[0], pk[1], pk[2]);
    }
    println!();

    println!("--- HiveMesh E2EE API ---");
    let now_ms = 1000u64;

    // Alice initiates E2EE session with Bob
    let key_exchange = mesh_alice
        .initiate_peer_e2ee(NodeId::new(0xBBBB2222), now_ms)
        .expect("Alice should initiate");
    println!("Alice initiated key exchange: {} bytes", key_exchange.len());
    println!("Session count: {}", mesh_alice.peer_e2ee_session_count());
    println!();

    // Note: In real usage, this key_exchange would be sent to Bob via BLE,
    // and Bob would receive it via on_ble_data_received() which handles
    // the key exchange internally.

    println!("--- Direct PeerSessionManager API (Low-Level) ---\n");

    // For demonstration, let's show the low-level API directly
    demonstrate_low_level_e2ee();

    println!("--- Closing HiveMesh Session ---");
    mesh_alice.close_peer_e2ee(NodeId::new(0xBBBB2222));
    println!("Alice sessions: {}", mesh_alice.peer_e2ee_session_count());

    println!("\n=== Example Complete ===");
}

/// Demonstrates the low-level PeerSessionManager API
fn demonstrate_low_level_e2ee() {
    let now_ms = 1000u64;

    // Create session managers for Alice and Bob
    let mut alice = PeerSessionManager::new(NodeId::new(0xAAAA1111));
    let mut bob = PeerSessionManager::new(NodeId::new(0xBBBB2222));

    println!("Alice node ID: {:08X}", alice.our_node_id().as_u32());
    println!("Bob node ID: {:08X}", bob.our_node_id().as_u32());
    println!();

    // Step 1: Alice initiates key exchange
    println!("Step 1: Alice initiates key exchange");
    let alice_msg = alice.initiate_session(NodeId::new(0xBBBB2222), now_ms);
    println!("  Alice -> Bob: KeyExchangeMessage");
    println!("    sender: {:08X}", alice_msg.sender_node_id.as_u32());
    println!(
        "    public_key: {:02X}{:02X}...",
        alice_msg.public_key[0], alice_msg.public_key[1]
    );
    println!("    ephemeral: {}", alice_msg.is_ephemeral);
    println!();

    // Step 2: Bob receives and responds
    println!("Step 2: Bob receives and responds");
    let (bob_response, bob_established) = bob
        .handle_key_exchange(&alice_msg, now_ms + 100)
        .expect("Bob should handle key exchange");
    println!("  Bob session established: {}", bob_established);
    println!("  Bob -> Alice: KeyExchangeMessage (response)");
    println!();

    // Step 3: Alice receives Bob's response
    println!("Step 3: Alice receives Bob's response");
    let (_, alice_established) = alice
        .handle_key_exchange(&bob_response, now_ms + 200)
        .expect("Alice should complete handshake");
    println!("  Alice session established: {}", alice_established);
    println!();

    // Verify sessions
    println!("Session verification:");
    println!(
        "  Alice has session with Bob: {}",
        alice.has_session(NodeId::new(0xBBBB2222))
    );
    println!(
        "  Bob has session with Alice: {}",
        bob.has_session(NodeId::new(0xAAAA1111))
    );
    println!();

    // Step 4: Send encrypted messages
    println!("Step 4: Alice sends encrypted message to Bob");
    let plaintext = b"Hello Bob! This is a secret message.";
    let encrypted = alice
        .encrypt_for_peer(NodeId::new(0xBBBB2222), plaintext, now_ms + 300)
        .expect("Alice should encrypt");

    println!("  Plaintext: \"{}\"", String::from_utf8_lossy(plaintext));
    println!("  Encrypted: {} bytes", encrypted.encode().len());
    println!("    sender: {:08X}", encrypted.sender_node_id.as_u32());
    println!(
        "    recipient: {:08X}",
        encrypted.recipient_node_id.as_u32()
    );
    println!("    counter: {}", encrypted.counter);
    println!();

    // Step 5: Bob decrypts
    println!("Step 5: Bob decrypts the message");
    let decrypted = bob
        .decrypt_from_peer(&encrypted, now_ms + 400)
        .expect("Bob should decrypt");
    println!("  Decrypted: \"{}\"", String::from_utf8_lossy(&decrypted));
    assert_eq!(decrypted, plaintext);
    println!();

    // Step 6: Bidirectional communication
    println!("Step 6: Bob responds to Alice");
    let response = b"Hi Alice! Got your message.";
    let encrypted_response = bob
        .encrypt_for_peer(NodeId::new(0xAAAA1111), response, now_ms + 500)
        .expect("Bob should encrypt");

    let decrypted_response = alice
        .decrypt_from_peer(&encrypted_response, now_ms + 600)
        .expect("Alice should decrypt");
    println!(
        "  Alice received: \"{}\"",
        String::from_utf8_lossy(&decrypted_response)
    );
    println!();

    // Overhead calculation
    println!("Encryption overhead:");
    let overhead = encrypted.encode().len() - plaintext.len();
    println!("  Plaintext: {} bytes", plaintext.len());
    println!("  Ciphertext: {} bytes", encrypted.encode().len());
    println!("  Overhead: {} bytes", overhead);
    println!("    - 4 bytes: recipient node ID");
    println!("    - 4 bytes: sender node ID");
    println!("    - 8 bytes: counter");
    println!("    - 12 bytes: nonce");
    println!("    - 16 bytes: authentication tag");
    println!();

    // Close session
    println!("Closing sessions:");
    alice.close_session(NodeId::new(0xBBBB2222));
    println!("  Alice sessions: {}", alice.session_count());
    println!("  Bob sessions: {}", bob.session_count());
}
