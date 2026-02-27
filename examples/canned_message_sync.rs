// Copyright (c) 2025-2026 (r)evolve - Revolve Team LLC
// SPDX-License-Identifier: Apache-2.0
//
// Example: peat-lite CannedMessage sync over peat-btle mesh
//
// Demonstrates how to implement DocumentType for peat-lite's
// CannedMessageAckEvent and sync it through the peat-btle mesh.
//
// Run with: `cargo run --features linux --example canned_message_sync`

use peat_btle::peat_mesh::{PeatMesh, PeatMeshConfig};
use peat_btle::registry::{AppOperation, DocumentType};
use peat_btle::NodeId;
use peat_lite::{CannedMessage, CannedMessageAckEvent, NodeId as PeatLiteNodeId};

// ---------------------------------------------------------------------------
// CannedMessageDocument: DocumentType adapter for peat-lite CannedMessages
// ---------------------------------------------------------------------------

/// App-layer type ID for CannedMessage documents (first app-layer slot).
pub const CANNED_MESSAGE_TYPE_ID: u8 = 0xC0;

/// Delta operation codes for CannedMessage.
pub mod op_code {
    pub const FULL_STATE: u8 = 0x00;
    pub const ACK_UPDATE: u8 = 0x01;
}

/// Wrapper around [`CannedMessageAckEvent`] that implements [`DocumentType`],
/// enabling CannedMessage events to sync through the peat-btle mesh.
#[derive(Debug, Clone)]
pub struct CannedMessageDocument {
    inner: CannedMessageAckEvent,
}

impl CannedMessageDocument {
    pub fn new(event: CannedMessageAckEvent) -> Self {
        Self { inner: event }
    }

    pub fn inner(&self) -> &CannedMessageAckEvent {
        &self.inner
    }

    pub fn into_inner(self) -> CannedMessageAckEvent {
        self.inner
    }

    pub fn ack(&mut self, node_id: u32, ack_timestamp: u64) -> bool {
        self.inner.ack(PeatLiteNodeId::new(node_id), ack_timestamp)
    }

    pub fn has_acked(&self, node_id: u32) -> bool {
        self.inner.has_acked(PeatLiteNodeId::new(node_id))
    }

    pub fn ack_count(&self) -> usize {
        self.inner.ack_count()
    }

    pub fn source_node(&self) -> u32 {
        self.inner.source_node.as_u32()
    }

    pub fn timestamp(&self) -> u64 {
        self.inner.timestamp
    }

    pub fn message_code(&self) -> u8 {
        self.inner.message.as_u8()
    }
}

impl From<CannedMessageAckEvent> for CannedMessageDocument {
    fn from(event: CannedMessageAckEvent) -> Self {
        Self::new(event)
    }
}

impl DocumentType for CannedMessageDocument {
    const TYPE_ID: u8 = CANNED_MESSAGE_TYPE_ID;
    const TYPE_NAME: &'static str = "CannedMessage";

    fn identity(&self) -> (u32, u64) {
        (self.inner.source_node.as_u32(), self.inner.timestamp)
    }

    fn encode(&self) -> Vec<u8> {
        let full = self.inner.encode();
        if full.len() > 1 {
            full[1..].to_vec()
        } else {
            Vec::new()
        }
    }

    fn decode(data: &[u8]) -> Option<Self> {
        let mut with_marker = Vec::with_capacity(1 + data.len());
        with_marker.push(0xAF);
        with_marker.extend_from_slice(data);
        CannedMessageAckEvent::decode(&with_marker).map(Self::new)
    }

    fn merge(&mut self, other: &Self) -> bool {
        self.inner.merge(&other.inner)
    }

    fn to_delta_op(&self) -> Option<AppOperation> {
        let (source, doc_timestamp) = self.identity();
        let sync_timestamp =
            (doc_timestamp & 0x0000_FFFF_FFFF_FFFF) | ((self.inner.ack_count() as u64) << 48);
        Some(
            AppOperation::new(Self::TYPE_ID, op_code::FULL_STATE, source, sync_timestamp)
                .with_payload(self.encode()),
        )
    }

    fn apply_delta_op(&mut self, op: &AppOperation) -> bool {
        if op.type_id != Self::TYPE_ID {
            return false;
        }

        match op.op_code {
            op_code::ACK_UPDATE => {
                if op.payload.len() < 2 {
                    return false;
                }
                let num_acks = u16::from_le_bytes([op.payload[0], op.payload[1]]) as usize;
                let expected_len = 2 + num_acks * 12;
                if op.payload.len() < expected_len {
                    return false;
                }
                let mut changed = false;
                let mut offset = 2;
                for _ in 0..num_acks {
                    let acker_node = u32::from_le_bytes([
                        op.payload[offset],
                        op.payload[offset + 1],
                        op.payload[offset + 2],
                        op.payload[offset + 3],
                    ]);
                    let ack_ts = u64::from_le_bytes([
                        op.payload[offset + 4],
                        op.payload[offset + 5],
                        op.payload[offset + 6],
                        op.payload[offset + 7],
                        op.payload[offset + 8],
                        op.payload[offset + 9],
                        op.payload[offset + 10],
                        op.payload[offset + 11],
                    ]);
                    offset += 12;
                    if self.inner.ack(PeatLiteNodeId::new(acker_node), ack_ts) {
                        changed = true;
                    }
                }
                changed
            }
            op_code::FULL_STATE => {
                if let Some(other) = Self::decode(&op.payload) {
                    self.inner.merge(&other.inner)
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Demo: two mesh nodes syncing a CannedMessage
// ---------------------------------------------------------------------------

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

fn make_node(node_id: u32, callsign: &str) -> PeatMesh {
    let config =
        PeatMeshConfig::new(NodeId::new(node_id), callsign, "TEST").with_encryption(TEST_SECRET);
    let mesh = PeatMesh::new(config);
    // Register CannedMessageDocument with the mesh's document registry
    mesh.document_registry().register::<CannedMessageDocument>();
    mesh
}

fn discover_and_connect(receiver: &PeatMesh, sender_node_id: u32, now: u64) {
    let device_id = format!("device-{:08X}", sender_node_id);
    let adv_name = format!("PEAT_TEST-{:08X}", sender_node_id);
    receiver.on_ble_discovered(&device_id, Some(&adv_name), -60, Some("TEST"), now);
    receiver.on_ble_connected(&device_id, now);
}

fn main() {
    env_logger::init();
    let now = now_ms();

    println!("=== peat-lite CannedMessage sync over peat-btle ===\n");

    // Create two mesh nodes
    let sender = make_node(0x111, "ALPHA");
    let receiver = make_node(0x222, "BRAVO");

    // Sender stores a CheckIn CannedMessage
    let event = CannedMessageAckEvent::new(
        CannedMessage::CheckIn,
        PeatLiteNodeId::new(0x111),
        None,
        now,
    );
    let doc = CannedMessageDocument::new(event);
    sender.store_app_document(doc);
    println!("ALPHA stored CheckIn message (source=0x111, ts={})", now);

    // Build encrypted delta document
    let delta_bytes = sender.build_full_delta_document(now);
    println!("Built encrypted delta: {} bytes", delta_bytes.len());

    // Simulate BLE discovery + connection
    discover_and_connect(&receiver, 0x111, now);

    // Receiver processes the encrypted delta
    let result =
        receiver.on_ble_data_received(&format!("device-{:08X}", 0x111u32), &delta_bytes, now + 100);
    println!("Receiver processed delta: {}", result.is_some());

    // Verify receiver has the document
    let docs = receiver.get_all_app_documents_of_type::<CannedMessageDocument>();
    println!("Receiver has {} CannedMessage doc(s)", docs.len());

    if let Some(doc) = docs.first() {
        println!(
            "  source=0x{:08X}, message=CheckIn({}), acks={}",
            doc.source_node(),
            doc.message_code(),
            doc.ack_count(),
        );
    }

    // Receiver ACKs the message
    let mut docs = receiver.get_all_app_documents_of_type::<CannedMessageDocument>();
    if let Some(doc) = docs.first_mut() {
        doc.ack(0x222, now + 200);
        println!("\nBRAVO ACKed the message (ack_count={})", doc.ack_count());
        receiver.store_app_document(docs.remove(0));
    }

    // Send ACK back to sender
    let ack_delta = receiver.build_full_delta_document(now + 300);
    discover_and_connect(&sender, 0x222, now + 300);
    sender.on_ble_data_received(&format!("device-{:08X}", 0x222u32), &ack_delta, now + 400);

    let sender_docs = sender.get_all_app_documents_of_type::<CannedMessageDocument>();
    if let Some(doc) = sender_docs.first() {
        println!(
            "ALPHA now sees {} ACKs (BRAVO acked: {})",
            doc.ack_count(),
            doc.has_acked(0x222)
        );
    }

    println!("\n=== Done ===");
}
