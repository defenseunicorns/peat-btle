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


//! Mesh-Wide Encryption Example
//!
//! Demonstrates how to use mesh-wide encryption to protect documents
//! from external eavesdroppers. All mesh members share a secret key.
//!
//! Run with: cargo run --example encryption_demo

use hive_btle::observer::{HiveEvent, HiveObserver, SecurityViolationKind};
use hive_btle::{HiveMesh, HiveMeshConfig, NodeId};
use std::sync::Arc;

/// Observer that reports security events
struct SecurityObserver {
    name: &'static str,
}

impl SecurityObserver {
    fn new(name: &'static str) -> Self {
        Self { name }
    }
}

impl HiveObserver for SecurityObserver {
    fn on_event(&self, event: HiveEvent) {
        match event {
            HiveEvent::EmergencyReceived { from_node } => {
                println!(
                    "[{}] Received EMERGENCY from {:08X}",
                    self.name,
                    from_node.as_u32()
                );
            }
            HiveEvent::SecurityViolation { kind, source } => {
                let kind_str = match kind {
                    SecurityViolationKind::DecryptionFailed => "Decryption failed (wrong key?)",
                    SecurityViolationKind::UnencryptedInStrictMode => "Unencrypted in strict mode",
                    SecurityViolationKind::ReplayDetected => "Replay attack detected",
                    SecurityViolationKind::UnauthorizedNode => "Unauthorized node",
                };
                println!(
                    "[{}] SECURITY VIOLATION: {} (source: {:?})",
                    self.name, kind_str, source
                );
            }
            HiveEvent::DocumentSynced { from_node, .. } => {
                println!(
                    "[{}] Successfully synced with {:08X}",
                    self.name,
                    from_node.as_u32()
                );
            }
            _ => {}
        }
    }
}

fn main() {
    println!("=== HIVE-BTLE Mesh-Wide Encryption Example ===\n");

    // Shared secret for the mesh (in practice, distribute securely)
    let mesh_secret: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];

    // Different secret for an outsider
    let wrong_secret: [u8; 32] = [0xFFu8; 32];

    println!("--- Creating Encrypted Mesh Nodes ---");

    // Create encrypted mesh nodes with shared secret
    let config_alpha = HiveMeshConfig::new(NodeId::new(0x11111111), "ALPHA-1", "SECURE")
        .with_encryption(mesh_secret);
    let config_bravo = HiveMeshConfig::new(NodeId::new(0x22222222), "BRAVO-1", "SECURE")
        .with_encryption(mesh_secret);

    let mesh_alpha = HiveMesh::new(config_alpha);
    let mesh_bravo = HiveMesh::new(config_bravo);

    mesh_alpha.add_observer(Arc::new(SecurityObserver::new("ALPHA")));
    mesh_bravo.add_observer(Arc::new(SecurityObserver::new("BRAVO")));

    println!(
        "Alpha encryption enabled: {}",
        mesh_alpha.is_encryption_enabled()
    );
    println!(
        "Bravo encryption enabled: {}",
        mesh_bravo.is_encryption_enabled()
    );
    println!();

    // Create an outsider with wrong key
    let config_eve = HiveMeshConfig::new(NodeId::new(0xEEEEEEEE), "EVE-1", "SECURE")
        .with_encryption(wrong_secret);
    let mesh_eve = HiveMesh::new(config_eve);
    mesh_eve.add_observer(Arc::new(SecurityObserver::new("EVE")));

    // Create an unencrypted node
    let config_plain = HiveMeshConfig::new(NodeId::new(0xAAAAAAAA), "PLAIN-1", "SECURE");
    let mesh_plain = HiveMesh::new(config_plain);
    mesh_plain.add_observer(Arc::new(SecurityObserver::new("PLAIN")));

    println!("--- Scenario 1: Encrypted Communication ---");
    let now_ms = 1000u64;

    // Alpha sends encrypted emergency
    let encrypted_doc = mesh_alpha.send_emergency(now_ms);
    println!(
        "Alpha sent encrypted document: {} bytes",
        encrypted_doc.len()
    );
    println!("First byte (marker): 0x{:02X}", encrypted_doc[0]);

    // Bravo (same key) can decrypt
    let result = mesh_bravo.on_ble_data_received_from_node(
        NodeId::new(0x11111111),
        &encrypted_doc,
        now_ms + 100,
    );
    println!("Bravo can decrypt: {}", result.is_some());
    println!();

    println!("--- Scenario 2: Wrong Key Rejection ---");

    // Eve (wrong key) cannot decrypt
    let result = mesh_eve.on_ble_data_received_from_node(
        NodeId::new(0x11111111),
        &encrypted_doc,
        now_ms + 200,
    );
    println!("Eve can decrypt: {}", result.is_some());
    println!();

    println!("--- Scenario 3: Unencrypted Node ---");

    // Unencrypted node sends plain document
    let plain_doc = mesh_plain.build_document();
    println!("Plain node document: {} bytes", plain_doc.len());
    println!("First byte: 0x{:02X} (not encrypted marker)", plain_doc[0]);

    // Encrypted node (non-strict mode) can receive unencrypted
    let result = mesh_bravo.on_ble_data_received_from_node(
        NodeId::new(0xAAAAAAAA),
        &plain_doc,
        now_ms + 300,
    );
    println!(
        "Bravo (non-strict) accepts unencrypted: {}",
        result.is_some()
    );
    println!();

    println!("--- Scenario 4: Strict Encryption Mode ---");

    // Create strict mode node
    let config_strict = HiveMeshConfig::new(NodeId::new(0x33333333), "STRICT-1", "SECURE")
        .with_encryption(mesh_secret)
        .with_strict_encryption();
    let mesh_strict = HiveMesh::new(config_strict);
    mesh_strict.add_observer(Arc::new(SecurityObserver::new("STRICT")));

    println!(
        "Strict mode enabled: {}",
        mesh_strict.is_strict_encryption_enabled()
    );

    // Strict mode rejects unencrypted documents
    let result = mesh_strict.on_ble_data_received_from_node(
        NodeId::new(0xAAAAAAAA),
        &plain_doc,
        now_ms + 400,
    );
    println!("Strict node accepts unencrypted: {}", result.is_some());
    println!();

    println!("--- Encryption Overhead ---");
    // Create unencrypted version of Alpha's state for comparison
    let mesh_alpha_unencrypted = HiveMesh::new(HiveMeshConfig::new(
        NodeId::new(0x11111111),
        "ALPHA-1",
        "SECURE",
    ));
    mesh_alpha_unencrypted.send_emergency(now_ms + 500); // Match state
    let unencrypted_doc = mesh_alpha_unencrypted.build_document();
    let encrypted_doc_sample = mesh_alpha.build_document();
    let overhead = encrypted_doc_sample.len() - unencrypted_doc.len();
    println!("Same content unencrypted: {} bytes", unencrypted_doc.len());
    println!(
        "Same content encrypted: {} bytes",
        encrypted_doc_sample.len()
    );
    println!("Encryption overhead: {} bytes", overhead);
    println!("  (2 bytes marker + 12 bytes nonce + 16 bytes auth tag = 30 bytes)");

    println!("\n=== Example Complete ===");
}
