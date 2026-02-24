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

//! Integration between eche-btle and eche-lite.
//!
//! This module provides [`DocumentType`] implementations for eche-lite types,
//! enabling them to sync through the eche-btle mesh using the extensible
//! document registry.
//!
//! ## Usage
//!
//! Enable the `eche-lite-sync` feature in your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! eche-btle = { version = "0.1", features = ["eche-lite-sync"] }
//! ```
//!
//! Then register the CannedMessage type with the mesh:
//!
//! ```ignore
//! use eche_btle::{EcheMesh, DocumentRegistry};
//! use eche_btle::eche_lite_sync::CannedMessageDocument;
//!
//! // Register the CannedMessage document type
//! mesh.document_registry().register::<CannedMessageDocument>();
//!
//! // Store a canned message for sync
//! let event = eche_lite::CannedMessageAckEvent::new(
//!     eche_lite::CannedMessage::CheckIn,
//!     eche_lite::NodeId::new(my_node_id),
//!     None,
//!     timestamp_ms,
//! );
//! mesh.store_document(CannedMessageDocument::from(event));
//! ```

use crate::registry::{AppOperation, DocumentType};
use eche_lite::{CannedMessageAckEvent, NodeId as EcheLiteNodeId};

/// App-layer type ID for CannedMessage documents.
///
/// Uses 0xC0, the first app-layer slot.
pub const CANNED_MESSAGE_TYPE_ID: u8 = 0xC0;

/// Delta operation codes for CannedMessage.
pub mod op_code {
    /// Full state update (used when delta not available).
    pub const FULL_STATE: u8 = 0x00;
    /// ACK update only (efficient delta for ACK additions).
    pub const ACK_UPDATE: u8 = 0x01;
}

/// Wrapper around [`CannedMessageAckEvent`] that implements [`DocumentType`].
///
/// This enables CannedMessage events to sync through the eche-btle mesh
/// using the extensible document registry.
#[derive(Debug, Clone)]
pub struct CannedMessageDocument {
    inner: CannedMessageAckEvent,
}

impl CannedMessageDocument {
    /// Create a new document from a CannedMessageAckEvent.
    pub fn new(event: CannedMessageAckEvent) -> Self {
        Self { inner: event }
    }

    /// Get a reference to the inner event.
    pub fn inner(&self) -> &CannedMessageAckEvent {
        &self.inner
    }

    /// Get a mutable reference to the inner event.
    pub fn inner_mut(&mut self) -> &mut CannedMessageAckEvent {
        &mut self.inner
    }

    /// Consume and return the inner event.
    pub fn into_inner(self) -> CannedMessageAckEvent {
        self.inner
    }

    /// Record an ACK from a node.
    ///
    /// Delegates to [`CannedMessageAckEvent::ack`].
    pub fn ack(&mut self, node_id: u32, ack_timestamp: u64) -> bool {
        self.inner.ack(EcheLiteNodeId::new(node_id), ack_timestamp)
    }

    /// Check if a node has acknowledged.
    pub fn has_acked(&self, node_id: u32) -> bool {
        self.inner.has_acked(EcheLiteNodeId::new(node_id))
    }

    /// Get the number of ACKs.
    pub fn ack_count(&self) -> usize {
        self.inner.ack_count()
    }

    /// Get the source node ID.
    pub fn source_node(&self) -> u32 {
        self.inner.source_node.as_u32()
    }

    /// Get the timestamp.
    pub fn timestamp(&self) -> u64 {
        self.inner.timestamp
    }

    /// Get the message code.
    pub fn message_code(&self) -> u8 {
        self.inner.message.as_u8()
    }
}

impl From<CannedMessageAckEvent> for CannedMessageDocument {
    fn from(event: CannedMessageAckEvent) -> Self {
        Self::new(event)
    }
}

impl From<CannedMessageDocument> for CannedMessageAckEvent {
    fn from(doc: CannedMessageDocument) -> Self {
        doc.into_inner()
    }
}

impl DocumentType for CannedMessageDocument {
    const TYPE_ID: u8 = CANNED_MESSAGE_TYPE_ID;
    const TYPE_NAME: &'static str = "CannedMessage";

    fn identity(&self) -> (u32, u64) {
        (self.inner.source_node.as_u32(), self.inner.timestamp)
    }

    fn encode(&self) -> Vec<u8> {
        // Convert from heapless::Vec to std::Vec, skipping the 0xAF marker
        // since the document registry adds its own type header (0xC0)
        let full = self.inner.encode();
        if full.len() > 1 {
            full[1..].to_vec()
        } else {
            Vec::new()
        }
    }

    fn decode(data: &[u8]) -> Option<Self> {
        // Prepend the 0xAF marker that eche-lite expects
        // (it was stripped when we encoded for the registry)
        let mut with_marker = Vec::with_capacity(1 + data.len());
        with_marker.push(0xAF);
        with_marker.extend_from_slice(data);
        CannedMessageAckEvent::decode(&with_marker).map(Self::new)
    }

    fn merge(&mut self, other: &Self) -> bool {
        self.inner.merge(&other.inner)
    }

    fn to_delta_op(&self) -> Option<AppOperation> {
        // Send full document state for reliable sync.
        // The delta encoder filters by (key, timestamp) - we encode ack_count
        // in the upper bits so changes trigger re-sync.
        let (source, doc_timestamp) = self.identity();

        // Combine document timestamp with ack_count for versioning:
        // - Lower 48 bits: original document timestamp
        // - Upper 16 bits: ack_count (version indicator)
        // This ensures the delta encoder re-sends when ACKs change.
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
                // Parse ACK entries from payload
                // Format: [num_acks: 2B] [entries: 12B each]
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

                    if self.inner.ack(EcheLiteNodeId::new(acker_node), ack_ts) {
                        changed = true;
                    }
                }

                changed
            }
            op_code::FULL_STATE => {
                // Full state replacement
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

#[cfg(test)]
mod tests {
    use super::*;
    use eche_lite::CannedMessage;

    #[test]
    fn test_document_identity() {
        let event = CannedMessageAckEvent::new(
            CannedMessage::CheckIn,
            EcheLiteNodeId::new(0x12345678),
            None,
            1706234567000,
        );
        let doc = CannedMessageDocument::new(event);

        assert_eq!(doc.identity(), (0x12345678, 1706234567000));
    }

    #[test]
    fn test_document_encode_decode() {
        let event = CannedMessageAckEvent::new(
            CannedMessage::Emergency,
            EcheLiteNodeId::new(0xAABBCCDD),
            Some(EcheLiteNodeId::new(0x11223344)),
            1000,
        );
        let doc = CannedMessageDocument::new(event);

        let encoded = doc.encode();
        let decoded = CannedMessageDocument::decode(&encoded).unwrap();

        assert_eq!(decoded.identity(), doc.identity());
        assert_eq!(decoded.message_code(), doc.message_code());
    }

    #[test]
    fn test_document_merge() {
        let source = EcheLiteNodeId::new(0x111);
        let acker = EcheLiteNodeId::new(0x222);

        let mut doc1 = CannedMessageDocument::new(CannedMessageAckEvent::new(
            CannedMessage::Alert,
            source,
            None,
            1000,
        ));

        let mut event2 = CannedMessageAckEvent::new(CannedMessage::Alert, source, None, 1000);
        event2.ack(acker, 1500);
        let doc2 = CannedMessageDocument::new(event2);

        // Merge should add the ACK
        assert!(doc1.merge(&doc2));
        assert!(doc1.has_acked(acker.as_u32()));
        assert_eq!(doc1.ack_count(), 2);

        // Merging again should not change
        assert!(!doc1.merge(&doc2));
    }

    #[test]
    fn test_delta_op_encode_decode() {
        let source = EcheLiteNodeId::new(0x12345678);
        let acker1 = EcheLiteNodeId::new(0xAAAA);
        let acker2 = EcheLiteNodeId::new(0xBBBB);

        let mut event = CannedMessageAckEvent::new(CannedMessage::NeedSupport, source, None, 2000);
        event.ack(acker1, 2100);
        event.ack(acker2, 2200);

        let doc = CannedMessageDocument::new(event);
        let op = doc.to_delta_op().unwrap();

        assert_eq!(op.type_id, CANNED_MESSAGE_TYPE_ID);
        assert_eq!(op.op_code, op_code::FULL_STATE);
        assert_eq!(op.source_node, 0x12345678);

        // Timestamp encodes version (ack_count=3) in upper bits, doc timestamp in lower
        // ack_count = 3 (source auto-acks + acker1 + acker2)
        let expected_timestamp = 2000u64 | (3u64 << 48);
        assert_eq!(op.timestamp, expected_timestamp);

        // Extract original doc timestamp for document identity
        let doc_timestamp = op.timestamp & 0x0000_FFFF_FFFF_FFFF;
        assert_eq!(doc_timestamp, 2000);

        // Verify we can apply the delta to a fresh event
        let mut fresh = CannedMessageDocument::new(CannedMessageAckEvent::new(
            CannedMessage::NeedSupport,
            source,
            None,
            2000,
        ));

        // FULL_STATE merges the complete document state
        assert!(fresh.apply_delta_op(&op));
        assert!(fresh.has_acked(acker1.as_u32()));
        assert!(fresh.has_acked(acker2.as_u32()));
        assert_eq!(fresh.ack_count(), 3); // source + acker1 + acker2
    }

    #[test]
    fn test_type_constants() {
        assert_eq!(CannedMessageDocument::TYPE_ID, 0xC0);
        assert_eq!(CannedMessageDocument::TYPE_NAME, "CannedMessage");
    }

    #[test]
    fn test_from_conversions() {
        let event = CannedMessageAckEvent::new(
            CannedMessage::Moving,
            EcheLiteNodeId::new(0x999),
            None,
            5000,
        );

        let doc: CannedMessageDocument = event.clone().into();
        assert_eq!(doc.source_node(), 0x999);

        let recovered: CannedMessageAckEvent = doc.into();
        assert_eq!(recovered.source_node, EcheLiteNodeId::new(0x999));
    }
}
