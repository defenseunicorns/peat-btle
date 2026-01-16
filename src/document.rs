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

//! HIVE Document wire format for BLE mesh sync
//!
//! This module provides the unified document format used across all platforms
//! (iOS, Android, ESP32) for mesh synchronization. The format is designed for
//! efficient BLE transmission while supporting CRDT semantics.
//!
//! ## Wire Format
//!
//! ```text
//! Header (8 bytes):
//!   version:  4 bytes (LE) - document version number
//!   node_id:  4 bytes (LE) - source node identifier
//!
//! GCounter (4 + N*12 bytes):
//!   num_entries: 4 bytes (LE)
//!   entries[N]:
//!     node_id: 4 bytes (LE)
//!     count:   8 bytes (LE)
//!
//! Extended Section (optional, when peripheral data present):
//!   marker:         1 byte (0xAB)
//!   reserved:       1 byte
//!   peripheral_len: 2 bytes (LE)
//!   peripheral:     variable (34-43 bytes)
//! ```

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use crate::sync::crdt::{
    ChatCRDT, EmergencyEvent, EventType, GCounter, Peripheral, PeripheralEvent,
};
use crate::NodeId;

/// Marker byte indicating extended section with peripheral data
pub const EXTENDED_MARKER: u8 = 0xAB;

/// Marker byte indicating emergency event section
pub const EMERGENCY_MARKER: u8 = 0xAC;

/// Marker byte indicating chat CRDT section
///
/// Used to include persisted chat messages in the document for CRDT sync.
///
/// ```text
/// marker:   1 byte (0xAD)
/// reserved: 1 byte (0x00)
/// len:      2 bytes (LE) - length of chat CRDT data
/// chat:     variable - ChatCRDT encoded data
/// ```
pub const CHAT_MARKER: u8 = 0xAD;

/// Marker byte indicating encrypted document (mesh-wide)
///
/// When present, the entire document payload following the marker is encrypted
/// using ChaCha20-Poly1305. The marker format is:
///
/// ```text
/// marker:   1 byte (0xAE)
/// reserved: 1 byte (0x00)
/// payload:  12 bytes nonce + variable ciphertext (includes 16-byte auth tag)
/// ```
///
/// Encryption happens at the HiveMesh layer before transmission, and decryption
/// happens upon receipt before document parsing.
pub const ENCRYPTED_MARKER: u8 = 0xAE;

/// Marker byte indicating per-peer E2EE message
///
/// Used for end-to-end encrypted messages between specific peer pairs.
/// Only the sender and recipient (who share a session key) can decrypt.
///
/// ```text
/// marker:     1 byte (0xAF)
/// flags:      1 byte (bit 0: key_exchange, bit 1: forward_secrecy)
/// recipient:  4 bytes (LE) - recipient node ID
/// sender:     4 bytes (LE) - sender node ID
/// counter:    8 bytes (LE) - message counter for replay protection
/// nonce:      12 bytes
/// ciphertext: variable (includes 16-byte auth tag)
/// ```
pub const PEER_E2EE_MARKER: u8 = 0xAF;

/// Marker byte indicating key exchange message for per-peer E2EE
///
/// Used to establish E2EE sessions between peers via X25519 key exchange.
///
/// ```text
/// marker:     1 byte (0xB0)
/// sender:     4 bytes (LE) - sender node ID
/// flags:      1 byte (bit 0: is_ephemeral)
/// public_key: 32 bytes - X25519 public key
/// ```
pub const KEY_EXCHANGE_MARKER: u8 = 0xB0;

/// Marker byte indicating relay envelope for multi-hop transmission
///
/// Used to wrap documents for multi-hop relay with deduplication and TTL.
/// See [`crate::relay`] module for details.
///
/// ```text
/// marker:        1 byte (0xB1)
/// flags:         1 byte (bit 0: requires_ack, bit 1: is_broadcast)
/// message_id:    16 bytes (UUID for deduplication)
/// hop_count:     1 byte (current hop count)
/// max_hops:      1 byte (TTL)
/// origin_node:   4 bytes (LE) - original sender node ID
/// payload_len:   4 bytes (LE)
/// payload:       variable (encrypted document)
/// ```
pub const RELAY_ENVELOPE_MARKER: u8 = 0xB1;

/// Marker byte indicating delta document for bandwidth-efficient sync
///
/// Used to send only changed operations instead of full state snapshots.
/// See [`crate::sync::delta_document`] module for details.
///
/// ```text
/// marker:        1 byte (0xB2)
/// flags:         1 byte (bit 0: has_vector_clock, bit 1: is_response)
/// origin_node:   4 bytes (LE) - origin node ID
/// timestamp_ms:  8 bytes (LE) - creation timestamp
/// vector_clock:  variable (if flag set)
/// op_count:      2 bytes (LE) - number of operations
/// operations:    variable
/// ```
pub const DELTA_DOCUMENT_MARKER: u8 = 0xB2;

/// Minimum document size (header only, no counter entries)
pub const MIN_DOCUMENT_SIZE: usize = 8;

/// Maximum recommended mesh size for reliable single-packet sync
///
/// Beyond this, documents may exceed typical BLE MTU (244 bytes).
/// Size calculation: 8 (header) + 4 + (N × 12) (GCounter) + 42 (Peripheral)
///   20 nodes = 8 + 244 + 42 = 294 bytes
pub const MAX_MESH_SIZE: usize = 20;

/// Target document size for single-packet transmission
///
/// Based on typical negotiated BLE MTU (247 bytes - 3 ATT overhead).
pub const TARGET_DOCUMENT_SIZE: usize = 244;

/// Hard limit before fragmentation would be required
///
/// BLE 5.0+ supports up to 517 byte MTU, but 512 is practical max payload.
pub const MAX_DOCUMENT_SIZE: usize = 512;

/// A HIVE document for mesh synchronization
///
/// Contains header information, a CRDT G-Counter for tracking mesh activity,
/// optional peripheral data for events, optional emergency event with ACK tracking,
/// and optional chat CRDT for mesh-wide messaging.
#[derive(Debug, Clone)]
pub struct HiveDocument {
    /// Document version (incremented on each change)
    pub version: u32,

    /// Source node ID that created/last modified this document
    pub node_id: NodeId,

    /// CRDT G-Counter tracking activity across the mesh
    pub counter: GCounter,

    /// Optional peripheral data (sensor info, events)
    pub peripheral: Option<Peripheral>,

    /// Optional active emergency event with distributed ACK tracking
    pub emergency: Option<EmergencyEvent>,

    /// Optional chat CRDT for mesh-wide messaging
    ///
    /// Contains persisted chat messages that sync across the mesh using
    /// add-only set semantics. Messages are identified by (origin_node, timestamp)
    /// and automatically deduplicated during merge.
    pub chat: Option<ChatCRDT>,
}

impl Default for HiveDocument {
    fn default() -> Self {
        Self {
            version: 1,
            node_id: NodeId::default(),
            counter: GCounter::new(),
            peripheral: None,
            emergency: None,
            chat: None,
        }
    }
}

impl HiveDocument {
    /// Create a new document for the given node
    pub fn new(node_id: NodeId) -> Self {
        Self {
            version: 1,
            node_id,
            counter: GCounter::new(),
            peripheral: None,
            emergency: None,
            chat: None,
        }
    }

    /// Create with an initial peripheral
    pub fn with_peripheral(mut self, peripheral: Peripheral) -> Self {
        self.peripheral = Some(peripheral);
        self
    }

    /// Create with an initial emergency event
    pub fn with_emergency(mut self, emergency: EmergencyEvent) -> Self {
        self.emergency = Some(emergency);
        self
    }

    /// Create with an initial chat CRDT
    pub fn with_chat(mut self, chat: ChatCRDT) -> Self {
        self.chat = Some(chat);
        self
    }

    /// Increment the document version
    pub fn increment_version(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    /// Increment the counter for this node
    pub fn increment_counter(&mut self) {
        self.counter.increment(&self.node_id, 1);
        self.increment_version();
    }

    /// Set an event on the peripheral
    pub fn set_event(&mut self, event_type: EventType, timestamp: u64) {
        if let Some(ref mut peripheral) = self.peripheral {
            peripheral.set_event(event_type, timestamp);
            self.increment_counter();
        }
    }

    /// Clear the event from the peripheral
    pub fn clear_event(&mut self) {
        if let Some(ref mut peripheral) = self.peripheral {
            peripheral.clear_event();
            self.increment_version();
        }
    }

    /// Set an emergency event
    ///
    /// Creates a new emergency event with the given source node, timestamp,
    /// and list of known peers to track for ACKs.
    pub fn set_emergency(&mut self, source_node: u32, timestamp: u64, known_peers: &[u32]) {
        self.emergency = Some(EmergencyEvent::new(source_node, timestamp, known_peers));
        self.increment_counter();
    }

    /// Record an ACK for the current emergency
    ///
    /// Returns true if the ACK was new (state changed)
    pub fn ack_emergency(&mut self, node_id: u32) -> bool {
        if let Some(ref mut emergency) = self.emergency {
            if emergency.ack(node_id) {
                self.increment_version();
                return true;
            }
        }
        false
    }

    /// Clear the emergency event
    pub fn clear_emergency(&mut self) {
        if self.emergency.is_some() {
            self.emergency = None;
            self.increment_version();
        }
    }

    /// Get the current emergency event (if any)
    pub fn get_emergency(&self) -> Option<&EmergencyEvent> {
        self.emergency.as_ref()
    }

    /// Check if there's an active emergency
    pub fn has_emergency(&self) -> bool {
        self.emergency.is_some()
    }

    // --- Chat CRDT methods ---

    /// Get the chat CRDT (if any)
    pub fn get_chat(&self) -> Option<&ChatCRDT> {
        self.chat.as_ref()
    }

    /// Get mutable reference to the chat CRDT, creating it if needed
    pub fn get_or_create_chat(&mut self) -> &mut ChatCRDT {
        if self.chat.is_none() {
            self.chat = Some(ChatCRDT::new());
        }
        self.chat.as_mut().unwrap()
    }

    /// Add a chat message to the document
    ///
    /// Returns true if the message was new (not a duplicate)
    pub fn add_chat_message(
        &mut self,
        origin_node: u32,
        timestamp: u64,
        sender: &str,
        text: &str,
    ) -> bool {
        use crate::sync::crdt::ChatMessage;

        let mut msg = ChatMessage::new(origin_node, timestamp, sender, text);
        msg.is_broadcast = true;

        let chat = self.get_or_create_chat();
        if chat.add_message(msg) {
            self.increment_counter();
            true
        } else {
            false
        }
    }

    /// Add a chat message with reply-to information
    pub fn add_chat_reply(
        &mut self,
        origin_node: u32,
        timestamp: u64,
        sender: &str,
        text: &str,
        reply_to_node: u32,
        reply_to_timestamp: u64,
    ) -> bool {
        use crate::sync::crdt::ChatMessage;

        let mut msg = ChatMessage::new(origin_node, timestamp, sender, text);
        msg.is_broadcast = true;
        msg.set_reply_to(reply_to_node, reply_to_timestamp);

        let chat = self.get_or_create_chat();
        if chat.add_message(msg) {
            self.increment_counter();
            true
        } else {
            false
        }
    }

    /// Check if the document has any chat messages
    pub fn has_chat(&self) -> bool {
        self.chat.as_ref().is_some_and(|c| !c.is_empty())
    }

    /// Get the number of chat messages
    pub fn chat_count(&self) -> usize {
        self.chat.as_ref().map_or(0, |c| c.len())
    }

    /// Merge with another document using CRDT semantics
    ///
    /// Returns true if our state changed (useful for triggering re-broadcast)
    pub fn merge(&mut self, other: &HiveDocument) -> bool {
        let mut changed = false;

        // Merge counter
        let old_value = self.counter.value();
        self.counter.merge(&other.counter);
        if self.counter.value() != old_value {
            changed = true;
        }

        // Merge emergency event
        if let Some(ref other_emergency) = other.emergency {
            match &mut self.emergency {
                Some(ref mut our_emergency) => {
                    if our_emergency.merge(other_emergency) {
                        changed = true;
                    }
                }
                None => {
                    self.emergency = Some(other_emergency.clone());
                    changed = true;
                }
            }
        }

        // Merge chat CRDT
        if let Some(ref other_chat) = other.chat {
            match &mut self.chat {
                Some(ref mut our_chat) => {
                    if our_chat.merge(other_chat) {
                        changed = true;
                    }
                }
                None => {
                    if !other_chat.is_empty() {
                        self.chat = Some(other_chat.clone());
                        changed = true;
                    }
                }
            }
        }

        if changed {
            self.increment_version();
        }
        changed
    }

    /// Get the current event type (if any)
    pub fn current_event(&self) -> Option<EventType> {
        self.peripheral
            .as_ref()
            .and_then(|p| p.last_event.as_ref())
            .map(|e| e.event_type)
    }

    /// Encode to bytes for BLE transmission
    ///
    /// Alias: [`Self::to_bytes()`]
    pub fn encode(&self) -> Vec<u8> {
        let counter_data = self.counter.encode();
        let peripheral_data = self.peripheral.as_ref().map(|p| p.encode());
        let emergency_data = self.emergency.as_ref().map(|e| e.encode());
        let chat_data = self
            .chat
            .as_ref()
            .filter(|c| !c.is_empty())
            .map(|c| c.encode());

        // Calculate total size
        let mut size = 8 + counter_data.len(); // header + counter
        if let Some(ref pdata) = peripheral_data {
            size += 4 + pdata.len(); // marker + reserved + len + peripheral
        }
        if let Some(ref edata) = emergency_data {
            size += 4 + edata.len(); // marker + reserved + len + emergency
        }
        if let Some(ref cdata) = chat_data {
            size += 4 + cdata.len(); // marker + reserved + len + chat
        }

        let mut buf = Vec::with_capacity(size);

        // Header
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.node_id.as_u32().to_le_bytes());

        // GCounter
        buf.extend_from_slice(&counter_data);

        // Extended section (if peripheral present)
        if let Some(pdata) = peripheral_data {
            buf.push(EXTENDED_MARKER);
            buf.push(0); // reserved
            buf.extend_from_slice(&(pdata.len() as u16).to_le_bytes());
            buf.extend_from_slice(&pdata);
        }

        // Emergency section (if emergency present)
        if let Some(edata) = emergency_data {
            buf.push(EMERGENCY_MARKER);
            buf.push(0); // reserved
            buf.extend_from_slice(&(edata.len() as u16).to_le_bytes());
            buf.extend_from_slice(&edata);
        }

        // Chat section (if chat has messages)
        if let Some(cdata) = chat_data {
            buf.push(CHAT_MARKER);
            buf.push(0); // reserved
            buf.extend_from_slice(&(cdata.len() as u16).to_le_bytes());
            buf.extend_from_slice(&cdata);
        }

        buf
    }

    /// Encode to bytes for transmission (alias for [`Self::encode()`])
    ///
    /// This is the conventional name used by external crates like hive-ffi
    /// for transport-agnostic document serialization.
    #[inline]
    pub fn to_bytes(&self) -> Vec<u8> {
        self.encode()
    }

    /// Decode from bytes received over BLE
    ///
    /// Alias: [`Self::from_bytes()`]
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < MIN_DOCUMENT_SIZE {
            return None;
        }

        // Header
        let version = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let node_id = NodeId::new(u32::from_le_bytes([data[4], data[5], data[6], data[7]]));

        // GCounter (starts at offset 8)
        let counter = GCounter::decode(&data[8..])?;

        // Calculate where counter ends
        let num_entries = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        let mut offset = 8 + 4 + num_entries * 12;

        let mut peripheral = None;
        let mut emergency = None;
        let mut chat = None;

        // Parse extended sections (can have peripheral, emergency, and/or chat)
        while offset < data.len() {
            let marker = data[offset];

            if marker == EXTENDED_MARKER {
                // Parse peripheral section
                if data.len() < offset + 4 {
                    break;
                }
                let _reserved = data[offset + 1];
                let section_len = u16::from_le_bytes([data[offset + 2], data[offset + 3]]) as usize;

                let section_start = offset + 4;
                if data.len() < section_start + section_len {
                    break;
                }

                peripheral = Peripheral::decode(&data[section_start..section_start + section_len]);
                offset = section_start + section_len;
            } else if marker == EMERGENCY_MARKER {
                // Parse emergency section
                if data.len() < offset + 4 {
                    break;
                }
                let _reserved = data[offset + 1];
                let section_len = u16::from_le_bytes([data[offset + 2], data[offset + 3]]) as usize;

                let section_start = offset + 4;
                if data.len() < section_start + section_len {
                    break;
                }

                emergency =
                    EmergencyEvent::decode(&data[section_start..section_start + section_len]);
                offset = section_start + section_len;
            } else if marker == CHAT_MARKER {
                // Parse chat section
                if data.len() < offset + 4 {
                    break;
                }
                let _reserved = data[offset + 1];
                let section_len = u16::from_le_bytes([data[offset + 2], data[offset + 3]]) as usize;

                let section_start = offset + 4;
                if data.len() < section_start + section_len {
                    break;
                }

                chat = ChatCRDT::decode(&data[section_start..section_start + section_len]);
                offset = section_start + section_len;
            } else {
                // Unknown marker, stop parsing
                break;
            }
        }

        Some(Self {
            version,
            node_id,
            counter,
            peripheral,
            emergency,
            chat,
        })
    }

    /// Decode from bytes (alias for [`Self::decode()`])
    ///
    /// This is the conventional name used by external crates like hive-ffi
    /// for transport-agnostic document deserialization.
    #[inline]
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        Self::decode(data)
    }

    /// Get the total counter value
    pub fn total_count(&self) -> u64 {
        self.counter.value()
    }

    /// Get the encoded size of this document
    ///
    /// Use this to check if the document fits within BLE MTU constraints.
    pub fn encoded_size(&self) -> usize {
        let counter_size = 4 + self.counter.node_count_total() * 12;
        let peripheral_size = self.peripheral.as_ref().map_or(0, |p| 4 + p.encode().len());
        let emergency_size = self.emergency.as_ref().map_or(0, |e| 4 + e.encode().len());
        let chat_size = self
            .chat
            .as_ref()
            .filter(|c| !c.is_empty())
            .map_or(0, |c| 4 + c.encoded_size());
        8 + counter_size + peripheral_size + emergency_size + chat_size
    }

    /// Check if the document exceeds the target size for single-packet transmission
    ///
    /// Returns `true` if the document is larger than [`TARGET_DOCUMENT_SIZE`].
    pub fn exceeds_target_size(&self) -> bool {
        self.encoded_size() > TARGET_DOCUMENT_SIZE
    }

    /// Check if the document exceeds the maximum size
    ///
    /// Returns `true` if the document is larger than [`MAX_DOCUMENT_SIZE`].
    pub fn exceeds_max_size(&self) -> bool {
        self.encoded_size() > MAX_DOCUMENT_SIZE
    }
}

/// Result from merging a received document
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// Node ID that sent this document
    pub source_node: NodeId,

    /// Event contained in the document (if any)
    pub event: Option<PeripheralEvent>,

    /// Whether the counter changed (indicates new data)
    pub counter_changed: bool,

    /// Whether the emergency state changed (new emergency or ACK updates)
    pub emergency_changed: bool,

    /// Whether chat messages changed (new messages received)
    pub chat_changed: bool,

    /// Updated total count after merge
    pub total_count: u64,
}

impl MergeResult {
    /// Check if this result contains an emergency event
    pub fn is_emergency(&self) -> bool {
        self.event
            .as_ref()
            .is_some_and(|e| e.event_type == EventType::Emergency)
    }

    /// Check if this result contains an ACK event
    pub fn is_ack(&self) -> bool {
        self.event
            .as_ref()
            .is_some_and(|e| e.event_type == EventType::Ack)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::crdt::PeripheralType;

    #[test]
    fn test_document_encode_decode_minimal() {
        let node_id = NodeId::new(0x12345678);
        let doc = HiveDocument::new(node_id);

        let encoded = doc.encode();
        assert_eq!(encoded.len(), 12); // 8 header + 4 counter (0 entries)

        let decoded = HiveDocument::decode(&encoded).unwrap();
        assert_eq!(decoded.version, 1);
        assert_eq!(decoded.node_id.as_u32(), 0x12345678);
        assert_eq!(decoded.counter.value(), 0);
        assert!(decoded.peripheral.is_none());
    }

    #[test]
    fn test_document_encode_decode_with_counter() {
        let node_id = NodeId::new(0x12345678);
        let mut doc = HiveDocument::new(node_id);
        doc.increment_counter();
        doc.increment_counter();

        let encoded = doc.encode();
        // 8 header + 4 num_entries + 1 entry (12 bytes) = 24
        assert_eq!(encoded.len(), 24);

        let decoded = HiveDocument::decode(&encoded).unwrap();
        assert_eq!(decoded.counter.value(), 2);
    }

    #[test]
    fn test_document_encode_decode_with_peripheral() {
        let node_id = NodeId::new(0x12345678);
        let peripheral =
            Peripheral::new(0xAABBCCDD, PeripheralType::SoldierSensor).with_callsign("ALPHA-1");

        let doc = HiveDocument::new(node_id).with_peripheral(peripheral);

        let encoded = doc.encode();
        let decoded = HiveDocument::decode(&encoded).unwrap();

        assert!(decoded.peripheral.is_some());
        let p = decoded.peripheral.unwrap();
        assert_eq!(p.id, 0xAABBCCDD);
        assert_eq!(p.callsign_str(), "ALPHA-1");
    }

    #[test]
    fn test_document_encode_decode_with_event() {
        let node_id = NodeId::new(0x12345678);
        let mut peripheral = Peripheral::new(0xAABBCCDD, PeripheralType::SoldierSensor);
        peripheral.set_event(EventType::Emergency, 1234567890);

        let doc = HiveDocument::new(node_id).with_peripheral(peripheral);

        let encoded = doc.encode();
        let decoded = HiveDocument::decode(&encoded).unwrap();

        assert!(decoded.peripheral.is_some());
        let p = decoded.peripheral.unwrap();
        assert!(p.last_event.is_some());
        let event = p.last_event.unwrap();
        assert_eq!(event.event_type, EventType::Emergency);
        assert_eq!(event.timestamp, 1234567890);
    }

    #[test]
    fn test_document_merge() {
        let node1 = NodeId::new(0x11111111);
        let node2 = NodeId::new(0x22222222);

        let mut doc1 = HiveDocument::new(node1);
        doc1.increment_counter();

        let mut doc2 = HiveDocument::new(node2);
        doc2.counter.increment(&node2, 3);

        // Merge doc2 into doc1
        let changed = doc1.merge(&doc2);
        assert!(changed);
        assert_eq!(doc1.counter.value(), 4); // 1 + 3
    }

    #[test]
    fn test_merge_result_helpers() {
        let emergency_event = PeripheralEvent::new(EventType::Emergency, 123);
        let result = MergeResult {
            source_node: NodeId::new(0x12345678),
            event: Some(emergency_event),
            counter_changed: true,
            emergency_changed: false,
            chat_changed: false,
            total_count: 10,
        };

        assert!(result.is_emergency());
        assert!(!result.is_ack());

        let ack_event = PeripheralEvent::new(EventType::Ack, 456);
        let result = MergeResult {
            source_node: NodeId::new(0x12345678),
            event: Some(ack_event),
            counter_changed: false,
            emergency_changed: false,
            chat_changed: false,
            total_count: 10,
        };

        assert!(!result.is_emergency());
        assert!(result.is_ack());
    }

    #[test]
    fn test_document_size_calculation() {
        use crate::sync::crdt::PeripheralType;

        let node_id = NodeId::new(0x12345678);

        // Minimal document: 8 header + 4 counter (0 entries) = 12 bytes
        let doc = HiveDocument::new(node_id);
        assert_eq!(doc.encoded_size(), 12);
        assert!(!doc.exceeds_target_size());

        // With one counter entry: 8 + (4 + 12) = 24 bytes
        let mut doc = HiveDocument::new(node_id);
        doc.increment_counter();
        assert_eq!(doc.encoded_size(), 24);

        // With peripheral: adds ~42 bytes (4 marker/len + 38 data)
        let peripheral = Peripheral::new(0xAABBCCDD, PeripheralType::SoldierSensor);
        let doc = HiveDocument::new(node_id).with_peripheral(peripheral);
        let encoded = doc.encode();
        assert_eq!(doc.encoded_size(), encoded.len());

        // Verify size stays under target for reasonable mesh
        let mut doc = HiveDocument::new(node_id);
        for i in 0..10 {
            doc.counter.increment(&NodeId::new(i), 1);
        }
        assert!(doc.encoded_size() < TARGET_DOCUMENT_SIZE);
        assert!(!doc.exceeds_max_size());
    }

    // ============================================================================
    // Chat CRDT Document Tests
    // ============================================================================

    #[test]
    fn test_document_add_chat_message() {
        let node_id = NodeId::new(0x12345678);
        let mut doc = HiveDocument::new(node_id);

        assert!(!doc.has_chat());
        assert_eq!(doc.chat_count(), 0);

        // Add a message
        assert!(doc.add_chat_message(0x12345678, 1000, "ALPHA", "Hello mesh!"));
        assert!(doc.has_chat());
        assert_eq!(doc.chat_count(), 1);

        // Duplicate should be rejected
        assert!(!doc.add_chat_message(0x12345678, 1000, "ALPHA", "Hello mesh!"));
        assert_eq!(doc.chat_count(), 1);

        // Different message should be accepted
        assert!(doc.add_chat_message(0x12345678, 2000, "ALPHA", "Second message"));
        assert_eq!(doc.chat_count(), 2);
    }

    #[test]
    fn test_document_add_chat_reply() {
        let node_id = NodeId::new(0x12345678);
        let mut doc = HiveDocument::new(node_id);

        // Add original message
        doc.add_chat_message(0xAABBCCDD, 1000, "BRAVO", "Need assistance");

        // Add reply
        assert!(doc.add_chat_reply(
            0x12345678,
            2000,
            "ALPHA",
            "Copy that",
            0xAABBCCDD, // reply to node
            1000        // reply to timestamp
        ));

        assert_eq!(doc.chat_count(), 2);

        // Verify reply-to info
        let chat = doc.get_chat().unwrap();
        let reply = chat.get_message(0x12345678, 2000).unwrap();
        assert!(reply.is_reply());
        assert_eq!(reply.reply_to_node, 0xAABBCCDD);
        assert_eq!(reply.reply_to_timestamp, 1000);
    }

    #[test]
    fn test_document_encode_decode_with_chat() {
        let node_id = NodeId::new(0x12345678);
        let mut doc = HiveDocument::new(node_id);

        doc.add_chat_message(0x12345678, 1000, "ALPHA", "First message");
        doc.add_chat_message(0xAABBCCDD, 2000, "BRAVO", "Second message");

        let encoded = doc.encode();
        let decoded = HiveDocument::decode(&encoded).unwrap();

        assert!(decoded.has_chat());
        assert_eq!(decoded.chat_count(), 2);

        let chat = decoded.get_chat().unwrap();
        let msg1 = chat.get_message(0x12345678, 1000).unwrap();
        assert_eq!(msg1.sender(), "ALPHA");
        assert_eq!(msg1.text(), "First message");

        let msg2 = chat.get_message(0xAABBCCDD, 2000).unwrap();
        assert_eq!(msg2.sender(), "BRAVO");
        assert_eq!(msg2.text(), "Second message");
    }

    #[test]
    fn test_document_merge_with_chat() {
        let node1 = NodeId::new(0x11111111);
        let node2 = NodeId::new(0x22222222);

        let mut doc1 = HiveDocument::new(node1);
        doc1.add_chat_message(0x11111111, 1000, "ALPHA", "From node 1");

        let mut doc2 = HiveDocument::new(node2);
        doc2.add_chat_message(0x22222222, 2000, "BRAVO", "From node 2");

        // Merge doc2 into doc1
        let changed = doc1.merge(&doc2);
        assert!(changed);
        assert_eq!(doc1.chat_count(), 2);

        // Merge again - no changes
        let changed = doc1.merge(&doc2);
        assert!(!changed);

        // Verify both messages present
        let chat = doc1.get_chat().unwrap();
        assert!(chat.get_message(0x11111111, 1000).is_some());
        assert!(chat.get_message(0x22222222, 2000).is_some());
    }

    #[test]
    fn test_document_chat_encoded_size() {
        let node_id = NodeId::new(0x12345678);
        let mut doc = HiveDocument::new(node_id);

        let base_size = doc.encoded_size();

        // Add a message
        doc.add_chat_message(0x12345678, 1000, "ALPHA", "Test");

        // Size should increase
        let with_chat_size = doc.encoded_size();
        assert!(with_chat_size > base_size);

        // Encoded size should match actual encoded length
        let encoded = doc.encode();
        assert_eq!(doc.encoded_size(), encoded.len());
    }
}
