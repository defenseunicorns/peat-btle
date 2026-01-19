//! Profiling stress test for hive-btle
//!
//! This example exercises all major code paths for profiling:
//! - CRDT operations (counter, emergency, chat)
//! - Document encoding/decoding
//! - Mesh sync and merge
//! - Peer management
//! - Encryption/decryption
//!
//! Run with profiling tools:
//!
//! ```bash
//! # Memory leaks (Valgrind)
//! cargo build --example profile_stress --release
//! valgrind --leak-check=full --show-leak-kinds=all \
//!     target/release/examples/profile_stress
//!
//! # Memory growth (heaptrack)
//! heaptrack target/release/examples/profile_stress
//! heaptrack_gui heaptrack.profile_stress.*.zst
//!
//! # Memory growth (Valgrind massif)
//! valgrind --tool=massif target/release/examples/profile_stress
//! ms_print massif.out.*
//!
//! # CPU profiling (flamegraph)
//! cargo flamegraph --example profile_stress
//! ```

use hive_btle::observer::DisconnectReason;
use hive_btle::{HiveMesh, HiveMeshConfig, NodeId};
use std::time::Instant;

/// Number of simulated nodes in the mesh
const NUM_NODES: usize = 10;

/// Number of iterations for each operation type
const ITERATIONS: usize = 1000;

/// Number of chat messages to generate
const CHAT_MESSAGES: usize = 500;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    println!("=== HIVE-BTLE Profiling Stress Test ===\n");

    let total_start = Instant::now();

    // Phase 1: Create mesh nodes
    println!("[Phase 1] Creating {} mesh nodes...", NUM_NODES);
    let phase_start = Instant::now();
    let meshes = create_meshes(NUM_NODES);
    println!(
        "  Created {} nodes in {:?}\n",
        meshes.len(),
        phase_start.elapsed()
    );

    // Phase 2: Document sync stress
    println!(
        "[Phase 2] Document sync ({} sync cycles)...",
        ITERATIONS / 10
    );
    let phase_start = Instant::now();
    test_document_sync(&meshes, ITERATIONS / 10);
    println!("  Completed in {:?}\n", phase_start.elapsed());

    // Phase 3: Chat CRDT operations
    println!("[Phase 3] Chat CRDT ({} messages)...", CHAT_MESSAGES);
    let phase_start = Instant::now();
    test_chat_operations(&meshes, CHAT_MESSAGES);
    println!("  Completed in {:?}\n", phase_start.elapsed());

    // Phase 4: Emergency/ACK cycle
    println!(
        "[Phase 4] Emergency/ACK cycles ({} cycles)...",
        ITERATIONS / 10
    );
    let phase_start = Instant::now();
    test_emergency_cycles(&meshes, ITERATIONS / 10);
    println!("  Completed in {:?}\n", phase_start.elapsed());

    // Phase 5: Peer churn simulation
    println!(
        "[Phase 5] Peer churn simulation ({} connect/disconnect cycles)...",
        ITERATIONS
    );
    let phase_start = Instant::now();
    test_peer_churn(&meshes, ITERATIONS);
    println!("  Completed in {:?}\n", phase_start.elapsed());

    // Phase 6: Large document handling
    println!("[Phase 6] Document size verification...");
    let phase_start = Instant::now();
    test_document_sizes(&meshes);
    println!("  Completed in {:?}\n", phase_start.elapsed());

    // Summary
    println!("=== Profiling Complete ===");
    println!("Total time: {:?}", total_start.elapsed());
    println!("\nMemory stats (if available):");
    print_memory_stats();
}

fn create_meshes(count: usize) -> Vec<HiveMesh> {
    (0..count)
        .map(|i| {
            let node_id = NodeId::new(0x10000000 + i as u32);
            let config = HiveMeshConfig::new(node_id, &format!("NODE-{:02}", i), "PROFILE_TEST")
                .with_encryption(*b"profile-test-key-32-bytes-pad!!!");
            HiveMesh::new(config)
        })
        .collect()
}

fn test_document_sync(meshes: &[HiveMesh], iterations: usize) {
    let now_ms = now();

    for i in 0..iterations {
        // Each node builds a document
        let documents: Vec<_> = meshes.iter().map(|m| m.build_document()).collect();

        // Cross-sync: each node receives documents from all others
        for (recv_idx, receiver) in meshes.iter().enumerate() {
            for (send_idx, doc) in documents.iter().enumerate() {
                if recv_idx != send_idx {
                    // First discover and connect the peer
                    let peer_id = format!("peer-{}", send_idx);
                    receiver.on_ble_discovered(
                        &peer_id,
                        Some(&format!("HIVE_TEST-{:08X}", 0x10000000 + send_idx as u32)),
                        -60,
                        Some("PROFILE_TEST"),
                        now_ms + i as u64 * 100,
                    );
                    receiver.on_ble_connected(&peer_id, now_ms + i as u64 * 100);
                    let _ = receiver.on_ble_data_received(&peer_id, doc, now_ms + i as u64 * 100);
                }
            }
        }
    }
}

fn test_chat_operations(meshes: &[HiveMesh], message_count: usize) {
    let now_ms = now();

    // First establish peer connections
    for (i, mesh) in meshes.iter().enumerate() {
        for (j, _) in meshes.iter().enumerate() {
            if i != j {
                let peer_id = format!("chat-peer-{}", j);
                mesh.on_ble_discovered(
                    &peer_id,
                    Some(&format!("HIVE_TEST-{:08X}", 0x10000000 + j as u32)),
                    -60,
                    Some("PROFILE_TEST"),
                    now_ms,
                );
                mesh.on_ble_connected(&peer_id, now_ms);
            }
        }
    }

    for i in 0..message_count {
        let sender_idx = i % meshes.len();
        let sender = &meshes[sender_idx];
        let timestamp = now_ms + i as u64;

        // Send a chat message
        if let Some(doc) = sender.send_chat(
            &format!("NODE-{:02}", sender_idx),
            &format!("Test message #{} from node {}", i, sender_idx),
            timestamp,
        ) {
            // Broadcast to all other nodes
            for (recv_idx, receiver) in meshes.iter().enumerate() {
                if recv_idx != sender_idx {
                    let peer_id = format!("chat-peer-{}", sender_idx);
                    let _ = receiver.on_ble_data_received(&peer_id, &doc, timestamp);
                }
            }
        }

        // Every 10th message is a reply
        if i > 0 && i % 10 == 0 {
            let replier_idx = (sender_idx + 1) % meshes.len();
            let replier = &meshes[replier_idx];
            let reply_timestamp = timestamp + 1;

            if let Some(doc) = replier.send_chat_reply(
                &format!("NODE-{:02}", replier_idx),
                "ACK",
                0x10000000 + sender_idx as u32,
                timestamp,
                reply_timestamp,
            ) {
                for (recv_idx, receiver) in meshes.iter().enumerate() {
                    if recv_idx != replier_idx {
                        let peer_id = format!("chat-peer-{}", replier_idx);
                        let _ = receiver.on_ble_data_received(&peer_id, &doc, reply_timestamp);
                    }
                }
            }
        }
    }

    // Query chat messages
    for mesh in meshes {
        let _ = mesh.all_chat_messages();
        let _ = mesh.chat_messages_since(now_ms + (message_count / 2) as u64);
    }
}

fn test_emergency_cycles(meshes: &[HiveMesh], cycles: usize) {
    let now_ms = now();

    // Establish connections first
    for (i, mesh) in meshes.iter().enumerate() {
        for (j, _) in meshes.iter().enumerate() {
            if i != j {
                let peer_id = format!("emerg-peer-{}", j);
                mesh.on_ble_discovered(
                    &peer_id,
                    Some(&format!("HIVE_TEST-{:08X}", 0x10000000 + j as u32)),
                    -60,
                    Some("PROFILE_TEST"),
                    now_ms,
                );
                mesh.on_ble_connected(&peer_id, now_ms);
            }
        }
    }

    for i in 0..cycles {
        let sender_idx = i % meshes.len();
        let sender = &meshes[sender_idx];
        let timestamp = now_ms + i as u64 * 1000;

        // Send emergency
        let emergency_doc = sender.send_emergency(timestamp);

        // Broadcast emergency
        for (recv_idx, receiver) in meshes.iter().enumerate() {
            if recv_idx != sender_idx {
                let peer_id = format!("emerg-peer-{}", sender_idx);
                let _ = receiver.on_ble_data_received(&peer_id, &emergency_doc, timestamp);
            }
        }

        // All nodes send ACKs
        for (acker_idx, acker) in meshes.iter().enumerate() {
            if acker_idx != sender_idx {
                let ack_doc = acker.send_ack(timestamp + acker_idx as u64);

                // Broadcast ACK
                for (recv_idx, receiver) in meshes.iter().enumerate() {
                    if recv_idx != acker_idx {
                        let peer_id = format!("emerg-peer-{}", acker_idx);
                        let _ = receiver.on_ble_data_received(&peer_id, &ack_doc, timestamp + 100);
                    }
                }
            }
        }

        // Clear emergency
        sender.clear_emergency();
    }
}

fn test_peer_churn(meshes: &[HiveMesh], cycles: usize) {
    let now_ms = now();

    for i in 0..cycles {
        let mesh = &meshes[i % meshes.len()];
        let peer_id = format!("churn-peer-{}", i % 50); // Reuse peer IDs to test cleanup
        let peer_node = NodeId::new(0x20000000 + (i % 50) as u32);

        // Discover peer
        mesh.on_ble_discovered(
            &peer_id,
            Some(&format!("HIVE_TEST-{:08X}", peer_node.as_u32())),
            -60 - (i % 40) as i8,
            Some("PROFILE_TEST"),
            now_ms + i as u64,
        );

        // Connect
        mesh.on_ble_connected(&peer_id, now_ms + i as u64 + 1);

        // Simulate some data exchange
        let doc = mesh.build_document();
        let _ = mesh.on_ble_data_received(&peer_id, &doc, now_ms + i as u64 + 2);

        // Disconnect
        mesh.on_ble_disconnected(&peer_id, DisconnectReason::RemoteRequest);

        // Periodic tick
        if i % 10 == 0 {
            let _ = mesh.tick(now_ms + i as u64 + 100);
        }
    }
}

fn test_document_sizes(_meshes: &[HiveMesh]) {
    let now_ms = now();

    // Create fresh nodes for accurate size testing (previous phases accumulate state)
    println!("  Creating fresh nodes for size verification...");
    let fresh_meshes = create_meshes(2);

    // Establish connections
    let peer_id = "size-peer-1";
    fresh_meshes[0].on_ble_discovered(
        peer_id,
        Some(&format!("HIVE_TEST-{:08X}", 0x10000001)),
        -60,
        Some("PROFILE_TEST"),
        now_ms,
    );
    fresh_meshes[0].on_ble_connected(peer_id, now_ms);

    fresh_meshes[1].on_ble_discovered(
        "size-peer-0",
        Some(&format!("HIVE_TEST-{:08X}", 0x10000000)),
        -60,
        Some("PROFILE_TEST"),
        now_ms,
    );
    fresh_meshes[1].on_ble_connected("size-peer-0", now_ms);

    // Measure baseline document size (no chat)
    let baseline_doc = fresh_meshes[0].build_document();
    println!(
        "  Baseline document size (no chat): {} bytes",
        baseline_doc.len()
    );

    // Fill chat to capacity (will be pruned for sync)
    for i in 0..50 {
        if let Some(doc) = fresh_meshes[0].send_chat("TEST", &format!("Msg {}", i), now_ms + i) {
            let _ = fresh_meshes[1].on_ble_data_received("size-peer-0", &doc, now_ms + i);
        }
    }

    // Verify document sizes stay within BLE MTU limits
    println!("  Document sizes after 50 chat messages:");
    for (i, mesh) in fresh_meshes.iter().enumerate() {
        let doc = mesh.build_document();
        let status = if doc.len() <= 512 { "OK" } else { "OVER MTU!" };
        println!("    Fresh Node {}: {} bytes [{}]", i, doc.len(), status);
    }

    // Verify chat count (local should have more than sync limit)
    println!("\n  Chat message counts:");
    println!("    Node 0 local: {}", fresh_meshes[0].chat_count());
    println!("    Expected sync limit: 8 (CHAT_SYNC_LIMIT), local max: 32 (CHAT_MAX_MESSAGES)");

    // Show size breakdown
    let chat_doc = fresh_meshes[0].build_document();
    let chat_overhead = chat_doc.len() - baseline_doc.len();
    println!("\n  Size breakdown:");
    println!(
        "    Baseline (counter + peripheral + encryption): {} bytes",
        baseline_doc.len()
    );
    println!(
        "    Chat overhead (8 sync messages): {} bytes",
        chat_overhead
    );
    println!("    Total: {} bytes (MTU limit: 512)", chat_doc.len());
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn print_memory_stats() {
    // Try to read /proc/self/status for memory info (Linux)
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmRSS:")
                || line.starts_with("VmHWM:")
                || line.starts_with("VmSize:")
            {
                println!("  {}", line);
            }
        }
    }
}
