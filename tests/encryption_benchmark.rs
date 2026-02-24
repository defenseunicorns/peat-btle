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

//! Benchmark tests comparing sync latency with and without encryption
//!
//! Measures the performance impact of ChaCha20-Poly1305 encryption on
//! document sync operations across different payload sizes.
//!
//! Run with: cargo test --test encryption_benchmark --features linux -- --nocapture

use eche_btle::eche_mesh::{EcheMesh, EcheMeshConfig};
use eche_btle::NodeId;
use std::time::{Duration, Instant};

/// Test document sizes (approximate payload sizes in bytes)
const DOCUMENT_SIZES: &[(usize, &str)] = &[
    (0, "minimal (counter only)"),
    (1, "small (1 chat message)"),
    (5, "medium (5 chat messages)"),
    (10, "large (10 chat messages)"),
    (20, "very large (20 chat messages)"),
];

/// Number of iterations for each benchmark
const ITERATIONS: usize = 100;

/// Shared secret for encryption tests
const TEST_SECRET: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

/// Create a mesh node without encryption
fn create_unencrypted_mesh(node_id: u32, callsign: &str) -> EcheMesh {
    let config = EcheMeshConfig::new(NodeId::new(node_id), callsign, "BENCH");
    EcheMesh::new(config)
}

/// Create a mesh node with encryption enabled
fn create_encrypted_mesh(node_id: u32, callsign: &str) -> EcheMesh {
    let config =
        EcheMeshConfig::new(NodeId::new(node_id), callsign, "BENCH").with_encryption(TEST_SECRET);
    EcheMesh::new(config)
}

/// Add chat messages to increase document size
#[cfg(feature = "legacy-chat")]
fn populate_mesh_with_chats(mesh: &EcheMesh, count: usize) {
    for i in 0..count {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Add a chat message with varying content
        let message = format!("Test message {} with some padding content", i);
        mesh.send_chat("BENCH", &message, now + i as u64);
    }
}

/// Benchmark document building (serialization)
fn benchmark_build_document(mesh: &EcheMesh, iterations: usize) -> Duration {
    let start = Instant::now();
    for _ in 0..iterations {
        let _doc = mesh.build_document();
    }
    start.elapsed()
}

/// Benchmark full sync cycle: build -> "transmit" -> process
fn benchmark_sync_cycle(sender: &EcheMesh, receiver: &EcheMesh, iterations: usize) -> Duration {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let start = Instant::now();
    for i in 0..iterations {
        // Sender builds document (includes encryption if enabled)
        let doc_bytes = sender.build_document();

        // Simulate BLE transmission (just a memory copy here)
        let received = doc_bytes.clone();

        // Receiver processes document (includes decryption if enabled)
        let _ = receiver.on_ble_data("bench-peer", &received, now_ms + i as u64);
    }
    start.elapsed()
}

/// Print benchmark results in a table format
fn print_results(
    label: &str,
    unencrypted_build: Duration,
    encrypted_build: Duration,
    unencrypted_sync: Duration,
    encrypted_sync: Duration,
    unencrypted_size: usize,
    encrypted_size: usize,
) {
    let build_overhead_us =
        encrypted_build.as_micros() as i64 - unencrypted_build.as_micros() as i64;
    let build_overhead_pct = if unencrypted_build.as_micros() > 0 {
        (build_overhead_us as f64 / unencrypted_build.as_micros() as f64) * 100.0
    } else {
        0.0
    };

    let sync_overhead_us = encrypted_sync.as_micros() as i64 - unencrypted_sync.as_micros() as i64;
    let sync_overhead_pct = if unencrypted_sync.as_micros() > 0 {
        (sync_overhead_us as f64 / unencrypted_sync.as_micros() as f64) * 100.0
    } else {
        0.0
    };

    let size_overhead = encrypted_size as i64 - unencrypted_size as i64;

    println!(
        "| {:30} | {:>8} | {:>8} | {:>+8} ({:>+6.1}%) | {:>8} | {:>8} | {:>+8} ({:>+6.1}%) | {:>6} | {:>6} | {:>+5} |",
        label,
        format!("{}µs", unencrypted_build.as_micros() / ITERATIONS as u128),
        format!("{}µs", encrypted_build.as_micros() / ITERATIONS as u128),
        format!("{}µs", build_overhead_us / ITERATIONS as i64),
        build_overhead_pct,
        format!("{}µs", unencrypted_sync.as_micros() / ITERATIONS as u128),
        format!("{}µs", encrypted_sync.as_micros() / ITERATIONS as u128),
        format!("{}µs", sync_overhead_us / ITERATIONS as i64),
        sync_overhead_pct,
        unencrypted_size,
        encrypted_size,
        size_overhead,
    );
}

#[test]
fn benchmark_encryption_latency() {
    println!("\n");
    println!("{}", "=".repeat(180));
    println!("ECHE-BTLE Encryption Benchmark: Sync Latency Comparison");
    println!("{}", "=".repeat(180));
    println!("Iterations per test: {}", ITERATIONS);
    println!("Encryption: ChaCha20-Poly1305 (30 byte overhead)");
    println!();

    // Table header
    println!(
        "| {:30} | {:>8} | {:>8} | {:>20} | {:>8} | {:>8} | {:>20} | {:>6} | {:>6} | {:>5} |",
        "Document Size",
        "Build",
        "Build",
        "Build Overhead",
        "Sync",
        "Sync",
        "Sync Overhead",
        "Size",
        "Size",
        "Size"
    );
    println!(
        "| {:30} | {:>8} | {:>8} | {:>20} | {:>8} | {:>8} | {:>20} | {:>6} | {:>6} | {:>5} |",
        "", "Plain", "Enc", "(per op)", "Plain", "Enc", "(per op)", "Plain", "Enc", "Δ"
    );
    println!("{}", "-".repeat(180));

    for (_chat_count, label) in DOCUMENT_SIZES {
        // Create unencrypted meshes
        let sender_plain = create_unencrypted_mesh(0x1111, "SEND");
        let receiver_plain = create_unencrypted_mesh(0x2222, "RECV");

        // Create encrypted meshes
        let sender_enc = create_encrypted_mesh(0x3333, "SEND");
        let receiver_enc = create_encrypted_mesh(0x4444, "RECV");

        // Populate with chat messages
        #[cfg(feature = "legacy-chat")]
        {
            populate_mesh_with_chats(&sender_plain, *_chat_count);
            populate_mesh_with_chats(&sender_enc, *_chat_count);
        }

        // Get document sizes
        let plain_doc = sender_plain.build_document();
        let enc_doc = sender_enc.build_document();
        let plain_size = plain_doc.len();
        let enc_size = enc_doc.len();

        // Benchmark build operations
        let plain_build = benchmark_build_document(&sender_plain, ITERATIONS);
        let enc_build = benchmark_build_document(&sender_enc, ITERATIONS);

        // Benchmark full sync cycle
        let plain_sync = benchmark_sync_cycle(&sender_plain, &receiver_plain, ITERATIONS);
        let enc_sync = benchmark_sync_cycle(&sender_enc, &receiver_enc, ITERATIONS);

        print_results(
            label,
            plain_build,
            enc_build,
            plain_sync,
            enc_sync,
            plain_size,
            enc_size,
        );
    }

    println!("{}", "-".repeat(180));
    println!();
    println!("Notes:");
    println!("  - Build: Time to serialize document (includes encryption if enabled)");
    println!(
        "  - Sync: Full cycle: build + transmit (memory copy) + process (includes decryption)"
    );
    println!("  - Size overhead is ~30 bytes (2 byte marker + 12 byte nonce + 16 byte auth tag)");
    println!("  - Latency overhead is primarily from ChaCha20-Poly1305 AEAD operations");
    println!();
}

#[test]
fn benchmark_encryption_throughput() {
    println!("\n");
    println!("{}", "=".repeat(100));
    println!("ECHE-BTLE Encryption Throughput Benchmark");
    println!("{}", "=".repeat(100));

    // Create meshes
    let sender_plain = create_unencrypted_mesh(0x1111, "SEND");
    let sender_enc = create_encrypted_mesh(0x2222, "SEND");

    // Add 10 chat messages for a realistic document
    #[cfg(feature = "legacy-chat")]
    {
        populate_mesh_with_chats(&sender_plain, 10);
        populate_mesh_with_chats(&sender_enc, 10);
    }

    let plain_size = sender_plain.build_document().len();
    let enc_size = sender_enc.build_document().len();

    // Measure how many documents we can build per second
    let duration = Duration::from_secs(1);

    // Unencrypted throughput
    let start = Instant::now();
    let mut plain_count = 0u64;
    while start.elapsed() < duration {
        let _doc = sender_plain.build_document();
        plain_count += 1;
    }
    let plain_elapsed = start.elapsed();

    // Encrypted throughput
    let start = Instant::now();
    let mut enc_count = 0u64;
    while start.elapsed() < duration {
        let _doc = sender_enc.build_document();
        enc_count += 1;
    }
    let enc_elapsed = start.elapsed();

    let plain_rate = plain_count as f64 / plain_elapsed.as_secs_f64();
    let enc_rate = enc_count as f64 / enc_elapsed.as_secs_f64();
    let plain_throughput_kbps = (plain_rate * plain_size as f64 * 8.0) / 1000.0;
    let enc_throughput_kbps = (enc_rate * enc_size as f64 * 8.0) / 1000.0;

    println!();
    println!(
        "Document size: {} bytes (plain), {} bytes (encrypted)",
        plain_size, enc_size
    );
    println!();
    println!(
        "| {:20} | {:>15} | {:>15} | {:>15} |",
        "Mode", "Docs/sec", "KB/sec", "Throughput"
    );
    println!("{}", "-".repeat(75));
    println!(
        "| {:20} | {:>15.0} | {:>15.2} | {:>15.2} kbps |",
        "Unencrypted",
        plain_rate,
        plain_rate * plain_size as f64 / 1024.0,
        plain_throughput_kbps
    );
    println!(
        "| {:20} | {:>15.0} | {:>15.2} | {:>15.2} kbps |",
        "Encrypted",
        enc_rate,
        enc_rate * enc_size as f64 / 1024.0,
        enc_throughput_kbps
    );
    println!("{}", "-".repeat(75));
    println!();
    println!(
        "Encryption overhead: {:.1}% reduction in throughput",
        (1.0 - enc_rate / plain_rate) * 100.0
    );
    println!();
}

#[cfg(feature = "legacy-chat")]
#[test]
fn test_encryption_correctness() {
    // Verify that encrypted documents can be correctly decrypted
    let sender = create_encrypted_mesh(0x1111, "SEND");
    let receiver = create_encrypted_mesh(0x2222, "RECV");

    // Add some data
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    sender.send_chat("ALICE", "Hello encrypted world!", now);

    // Build and sync
    let doc_bytes = sender.build_document();

    // Verify it's encrypted (starts with marker 0xAE)
    assert_eq!(
        doc_bytes[0], 0xAE,
        "Document should be encrypted (marker 0xAE)"
    );

    // Receiver should be able to process it
    let result = receiver.on_ble_data("sender", &doc_bytes, now);
    assert!(result.is_some(), "Encrypted document should be processable");

    // Verify the data was received
    assert!(receiver.chat_count() > 0, "Chat should have been synced");

    println!("Encryption correctness test passed!");
}
