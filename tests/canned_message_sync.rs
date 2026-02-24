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

//! CannedMessage round-trip over BLE mesh (encrypted)
//!
//! End-to-end tests verifying CannedMessage sync through the eche-btle delta
//! document pipeline with mesh-wide encryption.
//!
//! Run with: `cargo test --features eche-lite-sync --test canned_message_sync`

use eche_btle::eche_lite_sync::CannedMessageDocument;
use eche_btle::eche_mesh::{EcheMesh, EcheMeshConfig};
use eche_btle::NodeId;
use eche_lite::{CannedMessage, CannedMessageAckEvent, NodeId as EcheLiteNodeId};

const TEST_SECRET: [u8; 32] = [
    0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20,
];

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Create an encrypted mesh node with CannedMessageDocument auto-registered
/// (the `eche-lite-sync` feature enables auto-registration in EcheMesh::new).
fn make_node(node_id: u32, callsign: &str) -> EcheMesh {
    let config =
        EcheMeshConfig::new(NodeId::new(node_id), callsign, "TEST").with_encryption(TEST_SECRET);
    EcheMesh::new(config)
}

/// Simulate BLE discovery + connection so `on_ble_data_received` can resolve the sender.
fn discover_and_connect(receiver: &EcheMesh, sender_node_id: u32, now: u64) {
    let device_id = format!("device-{:08X}", sender_node_id);
    let adv_name = format!("ECHE_TEST-{:08X}", sender_node_id);

    receiver.on_ble_discovered(&device_id, Some(&adv_name), -60, Some("TEST"), now);
    receiver.on_ble_connected(&device_id, now);
}

#[test]
fn test_canned_message_round_trip_encrypted() {
    let now = now_ms();
    let sender = make_node(0x111, "ALPHA");
    let receiver = make_node(0x222, "BRAVO");

    // Sender stores a CheckIn CannedMessage
    let event = CannedMessageAckEvent::new(
        CannedMessage::CheckIn,
        EcheLiteNodeId::new(0x111),
        None,
        now,
    );
    let doc = CannedMessageDocument::new(event);
    assert!(sender.store_app_document(doc));
    assert_eq!(sender.app_document_count(), 1);

    // Build encrypted delta document (includes app ops)
    let delta_bytes = sender.build_full_delta_document(now);
    assert!(!delta_bytes.is_empty());
    // Encrypted documents start with 0xAE marker
    assert_eq!(delta_bytes[0], 0xAE, "Delta should be encrypted");

    // Receiver discovers + connects sender
    discover_and_connect(&receiver, 0x111, now);

    // Receiver processes encrypted delta
    let result =
        receiver.on_ble_data_received(&format!("device-{:08X}", 0x111u32), &delta_bytes, now + 100);
    assert!(
        result.is_some(),
        "Receiver should decrypt and process delta"
    );

    // Verify receiver now has the CannedMessage document
    let docs = receiver.get_all_app_documents_of_type::<CannedMessageDocument>();
    assert_eq!(docs.len(), 1, "Receiver should have 1 CannedMessage");

    let received = &docs[0];
    assert_eq!(received.source_node(), 0x111);
    assert_eq!(received.message_code(), CannedMessage::CheckIn.as_u8());
    assert_eq!(received.timestamp(), now);
}

#[test]
fn test_canned_message_ack_round_trip() {
    let now = now_ms();
    let sender = make_node(0x111, "ALPHA");
    let receiver = make_node(0x222, "BRAVO");

    // Sender stores a CheckIn
    let event = CannedMessageAckEvent::new(
        CannedMessage::CheckIn,
        EcheLiteNodeId::new(0x111),
        None,
        now,
    );
    sender.store_app_document(CannedMessageDocument::new(event));

    // Forward to receiver
    let delta_bytes = sender.build_full_delta_document(now);
    discover_and_connect(&receiver, 0x111, now);
    receiver.on_ble_data_received(&format!("device-{:08X}", 0x111u32), &delta_bytes, now + 100);

    // Receiver ACKs the message
    let mut docs = receiver.get_all_app_documents_of_type::<CannedMessageDocument>();
    assert_eq!(docs.len(), 1);
    let ack_ts = now + 200;
    assert!(docs[0].ack(0x222, ack_ts), "ACK should be new");
    // Re-store the updated document
    receiver.store_app_document(docs.remove(0));

    // Receiver sends delta back to sender
    let ack_delta = receiver.build_full_delta_document(now + 300);
    discover_and_connect(&sender, 0x222, now + 300);
    let result =
        sender.on_ble_data_received(&format!("device-{:08X}", 0x222u32), &ack_delta, now + 400);
    assert!(result.is_some(), "Sender should process ACK delta");

    // Verify sender's copy now has the receiver's ACK
    let sender_docs = sender.get_all_app_documents_of_type::<CannedMessageDocument>();
    assert_eq!(sender_docs.len(), 1);
    assert!(
        sender_docs[0].has_acked(0x222),
        "Sender should see receiver's ACK"
    );
    // ack_count: source auto-acks (0x111) + receiver ACK (0x222) = 2
    assert_eq!(sender_docs[0].ack_count(), 2);
}

#[test]
fn test_canned_message_deduplication() {
    let now = now_ms();
    let sender = make_node(0x111, "ALPHA");
    let receiver = make_node(0x222, "BRAVO");

    // Sender stores a single message
    let event =
        CannedMessageAckEvent::new(CannedMessage::Alert, EcheLiteNodeId::new(0x111), None, now);
    sender.store_app_document(CannedMessageDocument::new(event));

    // Build delta and send twice
    let delta_bytes = sender.build_full_delta_document(now);
    discover_and_connect(&receiver, 0x111, now);

    receiver.on_ble_data_received(&format!("device-{:08X}", 0x111u32), &delta_bytes, now + 100);
    receiver.on_ble_data_received(&format!("device-{:08X}", 0x111u32), &delta_bytes, now + 200);

    // Should still be exactly 1 document (dedup by identity: source_node + timestamp)
    let docs = receiver.get_all_app_documents_of_type::<CannedMessageDocument>();
    assert_eq!(
        docs.len(),
        1,
        "Duplicate delta should not create a second document"
    );
    assert_eq!(docs[0].message_code(), CannedMessage::Alert.as_u8());
}
