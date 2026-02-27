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

//! Basic Mesh Example
//!
//! Demonstrates the core PeatMesh API for CRDT-based mesh synchronization.
//! This example creates two mesh nodes and shows how they synchronize state.
//!
//! Run with: cargo run --example basic_mesh

use peat_btle::observer::{PeatEvent, PeatObserver};
use peat_btle::{NodeId, PeatMesh, PeatMeshConfig};
use std::sync::Arc;

/// Observer that prints all mesh events
struct PrintObserver {
    name: &'static str,
}

impl PrintObserver {
    fn new(name: &'static str) -> Self {
        Self { name }
    }
}

impl PeatObserver for PrintObserver {
    fn on_event(&self, event: PeatEvent) {
        match event {
            PeatEvent::PeerDiscovered { peer } => {
                println!("[{}] Discovered peer: {}", self.name, peer.display_name());
            }
            PeatEvent::PeerConnected { node_id } => {
                println!("[{}] Connected to: {:08X}", self.name, node_id.as_u32());
            }
            PeatEvent::PeerDisconnected { node_id, reason } => {
                println!(
                    "[{}] Disconnected from: {:08X} ({:?})",
                    self.name,
                    node_id.as_u32(),
                    reason
                );
            }
            PeatEvent::EmergencyReceived { from_node } => {
                println!(
                    "[{}] *** EMERGENCY from {:08X} ***",
                    self.name,
                    from_node.as_u32()
                );
            }
            PeatEvent::AckReceived { from_node } => {
                println!("[{}] ACK from {:08X}", self.name, from_node.as_u32());
            }
            PeatEvent::DocumentSynced {
                from_node,
                total_count,
            } => {
                println!(
                    "[{}] Synced with {:08X}, total count: {}",
                    self.name,
                    from_node.as_u32(),
                    total_count
                );
            }
            PeatEvent::MeshStateChanged {
                peer_count,
                connected_count,
            } => {
                println!(
                    "[{}] Mesh state: {} peers, {} connected",
                    self.name, peer_count, connected_count
                );
            }
            _ => {
                println!("[{}] Event: {:?}", self.name, event);
            }
        }
    }
}

fn main() {
    println!("=== PEAT-BTLE Basic Mesh Example ===\n");

    // Create two mesh nodes
    let config_alpha = PeatMeshConfig::new(NodeId::new(0x11111111), "ALPHA-1", "DEMO");
    let config_bravo = PeatMeshConfig::new(NodeId::new(0x22222222), "BRAVO-1", "DEMO");

    let mesh_alpha = PeatMesh::new(config_alpha);
    let mesh_bravo = PeatMesh::new(config_bravo);

    // Add observers
    mesh_alpha.add_observer(Arc::new(PrintObserver::new("ALPHA")));
    mesh_bravo.add_observer(Arc::new(PrintObserver::new("BRAVO")));

    println!("Created mesh nodes:");
    println!(
        "  - {} (Node ID: {:08X})",
        mesh_alpha.callsign(),
        mesh_alpha.node_id().as_u32()
    );
    println!(
        "  - {} (Node ID: {:08X})",
        mesh_bravo.callsign(),
        mesh_bravo.node_id().as_u32()
    );
    println!();

    // Simulate discovery and connection
    // In a real scenario, this happens via BLE callbacks
    let now_ms = 1000u64;

    println!("--- Simulating BLE Discovery ---");
    mesh_alpha.on_ble_discovered(
        "bravo-uuid",
        Some("PEAT_DEMO-22222222"),
        -65,
        Some("DEMO"),
        now_ms,
    );

    mesh_bravo.on_ble_discovered(
        "alpha-uuid",
        Some("PEAT_DEMO-11111111"),
        -60,
        Some("DEMO"),
        now_ms,
    );
    println!();

    println!("--- Simulating BLE Connection ---");
    mesh_alpha.on_ble_connected("bravo-uuid", now_ms + 100);
    mesh_bravo.on_incoming_connection("alpha-conn", NodeId::new(0x11111111), now_ms + 100);
    println!();

    // Show current state
    println!("Current state:");
    println!("  Alpha peers: {}", mesh_alpha.peer_count());
    println!("  Bravo peers: {}", mesh_bravo.peer_count());
    println!();

    // Simulate document sync
    println!("--- Document Synchronization ---");
    let alpha_doc = mesh_alpha.build_document();
    println!("Alpha document: {} bytes", alpha_doc.len());

    // Bravo receives Alpha's document
    if let Some(result) =
        mesh_bravo.on_ble_data_received_from_node(NodeId::new(0x11111111), &alpha_doc, now_ms + 200)
    {
        println!(
            "Bravo received doc from {:08X}, total_count: {}",
            result.source_node.as_u32(),
            result.total_count
        );
    }
    println!();

    // Simulate emergency flow
    println!("--- Emergency Flow ---");
    let emergency_doc = mesh_alpha.send_emergency(now_ms + 300);
    println!("Alpha sent EMERGENCY ({} bytes)", emergency_doc.len());

    if let Some(result) = mesh_bravo.on_ble_data_received_from_node(
        NodeId::new(0x11111111),
        &emergency_doc,
        now_ms + 400,
    ) {
        println!("Bravo received emergency: {}", result.is_emergency);
    }

    // Bravo sends ACK
    let ack_doc = mesh_bravo.send_ack(now_ms + 500);
    println!("Bravo sent ACK ({} bytes)", ack_doc.len());

    if let Some(result) =
        mesh_alpha.on_ble_data_received_from_node(NodeId::new(0x22222222), &ack_doc, now_ms + 600)
    {
        println!("Alpha received ACK: {}", result.is_ack);
    }
    println!();

    // Show final state
    println!("--- Final State ---");
    println!(
        "Alpha: emergency_active={}, ack_active={}",
        mesh_alpha.is_emergency_active(),
        mesh_alpha.is_ack_active()
    );
    println!(
        "Bravo: emergency_active={}, ack_active={}",
        mesh_bravo.is_emergency_active(),
        mesh_bravo.is_ack_active()
    );
    println!("Alpha total_count: {}", mesh_alpha.total_count());
    println!("Bravo total_count: {}", mesh_bravo.total_count());

    println!("\n=== Example Complete ===");
}
