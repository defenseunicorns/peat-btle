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

//! Multi-hop relay support for Eche BLE mesh
//!
//! This module provides message deduplication and hop tracking for multi-hop
//! relay scenarios. Without deduplication, messages would bounce infinitely
//! between mesh nodes.
//!
//! ## Wire Format
//!
//! Relay envelope wraps documents for multi-hop transmission:
//!
//! ```text
//! [1 byte:  marker (0xB1)]
//! [1 byte:  flags]
//!   - bit 0: requires_ack
//!   - bit 1: is_broadcast
//!   - bits 2-7: reserved
//! [16 bytes: message_id (UUID)]
//! [1 byte:  hop_count (current)]
//! [1 byte:  max_hops (TTL)]
//! [4 bytes: origin_node_id]
//! [4 bytes: payload_len]
//! [N bytes: payload (encrypted document)]
//! ```
//!
//! ## Deduplication
//!
//! The `SeenMessageCache` tracks message IDs with TTL expiration to prevent
//! infinite relay loops while allowing legitimate re-transmissions after
//! the TTL expires.

#[cfg(not(feature = "std"))]
use alloc::{collections::BTreeMap, vec::Vec};
#[cfg(feature = "std")]
use std::collections::HashMap;

use crate::NodeId;

/// Marker byte indicating relay envelope
pub const RELAY_ENVELOPE_MARKER: u8 = 0xB1;

/// Default max hops for relay messages
pub const DEFAULT_MAX_HOPS: u8 = 7;

/// Default TTL for seen messages (5 minutes in ms)
pub const DEFAULT_SEEN_TTL_MS: u64 = 300_000;

/// Maximum cache size before cleanup is forced
pub const MAX_CACHE_SIZE: usize = 1000;

/// A 128-bit message identifier for deduplication
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(not(feature = "std"), derive(Ord, PartialOrd))]
pub struct MessageId([u8; 16]);

impl MessageId {
    /// Create a new random message ID
    #[cfg(feature = "std")]
    pub fn new() -> Self {
        use std::time::SystemTime;

        // Generate pseudo-random ID from timestamp and random bits
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);

        let mut id = [0u8; 16];

        // Use timestamp for first 8 bytes
        id[0..8].copy_from_slice(&now.to_le_bytes()[0..8]);

        // Use LCG for remaining bytes (pseudo-random but fast)
        let mut seed = now as u64;
        for i in 0..8 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            id[8 + i] = (seed >> 32) as u8;
        }

        Self(id)
    }

    /// Create from raw bytes
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Create a deterministic ID from content hash
    ///
    /// Use this when you need consistent IDs for the same content,
    /// e.g., for idempotent operations.
    pub fn from_content(origin: NodeId, timestamp_ms: u64, payload_hash: u32) -> Self {
        let mut id = [0u8; 16];

        // Origin node (4 bytes)
        id[0..4].copy_from_slice(&origin.as_u32().to_le_bytes());

        // Timestamp (8 bytes)
        id[4..12].copy_from_slice(&timestamp_ms.to_le_bytes());

        // Payload hash (4 bytes)
        id[12..16].copy_from_slice(&payload_hash.to_le_bytes());

        Self(id)
    }
}

#[cfg(feature = "std")]
impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Display for MessageId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Display as hex (first 8 bytes for brevity)
        write!(
            f,
            "{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5], self.0[6], self.0[7]
        )
    }
}

/// Relay envelope flags
#[derive(Debug, Clone, Copy, Default)]
pub struct RelayFlags {
    /// Whether this message requires acknowledgment
    pub requires_ack: bool,
    /// Whether this is a broadcast (vs targeted)
    pub is_broadcast: bool,
}

impl RelayFlags {
    /// Encode flags to a byte
    pub fn to_byte(&self) -> u8 {
        let mut flags = 0u8;
        if self.requires_ack {
            flags |= 0x01;
        }
        if self.is_broadcast {
            flags |= 0x02;
        }
        flags
    }

    /// Decode flags from a byte
    pub fn from_byte(byte: u8) -> Self {
        Self {
            requires_ack: byte & 0x01 != 0,
            is_broadcast: byte & 0x02 != 0,
        }
    }
}

/// A relay envelope wrapping a document for multi-hop transmission
#[derive(Debug, Clone)]
pub struct RelayEnvelope {
    /// Unique message identifier for deduplication
    pub message_id: MessageId,

    /// Current hop count (increments with each relay)
    pub hop_count: u8,

    /// Maximum allowed hops (TTL)
    pub max_hops: u8,

    /// Original sender node ID
    pub origin_node: NodeId,

    /// Envelope flags
    pub flags: RelayFlags,

    /// The wrapped payload (typically an encrypted document)
    pub payload: Vec<u8>,
}

impl RelayEnvelope {
    /// Create a new relay envelope for a payload
    #[cfg(feature = "std")]
    pub fn new(origin_node: NodeId, payload: Vec<u8>) -> Self {
        Self {
            message_id: MessageId::new(),
            hop_count: 0,
            max_hops: DEFAULT_MAX_HOPS,
            origin_node,
            flags: RelayFlags::default(),
            payload,
        }
    }

    /// Create with broadcast flag
    #[cfg(feature = "std")]
    pub fn broadcast(origin_node: NodeId, payload: Vec<u8>) -> Self {
        Self {
            message_id: MessageId::new(),
            hop_count: 0,
            max_hops: DEFAULT_MAX_HOPS,
            origin_node,
            flags: RelayFlags {
                requires_ack: false,
                is_broadcast: true,
            },
            payload,
        }
    }

    /// Create with custom max hops
    pub fn with_max_hops(mut self, max_hops: u8) -> Self {
        self.max_hops = max_hops;
        self
    }

    /// Check if this envelope can be relayed further
    pub fn can_relay(&self) -> bool {
        self.hop_count < self.max_hops
    }

    /// Get remaining hops
    pub fn remaining_hops(&self) -> u8 {
        self.max_hops.saturating_sub(self.hop_count)
    }

    /// Create a relay copy with incremented hop count
    ///
    /// Returns None if TTL expired.
    pub fn relay(&self) -> Option<Self> {
        if !self.can_relay() {
            return None;
        }

        Some(Self {
            message_id: self.message_id,
            hop_count: self.hop_count + 1,
            max_hops: self.max_hops,
            origin_node: self.origin_node,
            flags: self.flags,
            payload: self.payload.clone(),
        })
    }

    /// Encode to bytes for transmission
    pub fn encode(&self) -> Vec<u8> {
        let size = 28 + self.payload.len(); // marker(1) + flags(1) + id(16) + hops(2) + origin(4) + len(4) + payload
        let mut buf = Vec::with_capacity(size);

        buf.push(RELAY_ENVELOPE_MARKER);
        buf.push(self.flags.to_byte());
        buf.extend_from_slice(self.message_id.as_bytes());
        buf.push(self.hop_count);
        buf.push(self.max_hops);
        buf.extend_from_slice(&self.origin_node.as_u32().to_le_bytes());
        buf.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.payload);

        buf
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        // Minimum size: marker(1) + flags(1) + id(16) + hops(2) + origin(4) + len(4) = 28
        if data.len() < 28 {
            return None;
        }

        if data[0] != RELAY_ENVELOPE_MARKER {
            return None;
        }

        let flags = RelayFlags::from_byte(data[1]);

        let mut id_bytes = [0u8; 16];
        id_bytes.copy_from_slice(&data[2..18]);
        let message_id = MessageId::from_bytes(id_bytes);

        let hop_count = data[18];
        let max_hops = data[19];

        let origin_node = NodeId::new(u32::from_le_bytes([data[20], data[21], data[22], data[23]]));

        let payload_len = u32::from_le_bytes([data[24], data[25], data[26], data[27]]) as usize;

        if data.len() < 28 + payload_len {
            return None;
        }

        let payload = data[28..28 + payload_len].to_vec();

        Some(Self {
            message_id,
            hop_count,
            max_hops,
            origin_node,
            flags,
            payload,
        })
    }

    /// Check if data starts with relay envelope marker
    pub fn is_relay_envelope(data: &[u8]) -> bool {
        !data.is_empty() && data[0] == RELAY_ENVELOPE_MARKER
    }
}

/// Cache entry for a seen message
#[derive(Debug, Clone)]
struct SeenEntry {
    /// When this message was first seen (ms)
    first_seen_ms: u64,
    /// How many times we've seen this message
    count: u32,
    /// Origin node that sent this message
    origin: NodeId,
}

/// Cache of seen message IDs for deduplication
///
/// Tracks message IDs with TTL expiration to prevent infinite relay loops
/// while allowing legitimate re-transmissions.
#[cfg(feature = "std")]
#[derive(Debug)]
pub struct SeenMessageCache {
    /// Map of message ID to entry
    cache: HashMap<MessageId, SeenEntry>,
    /// TTL for entries in milliseconds
    ttl_ms: u64,
    /// Last cleanup time
    last_cleanup_ms: u64,
}

#[cfg(feature = "std")]
impl SeenMessageCache {
    /// Create a new cache with default TTL
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            ttl_ms: DEFAULT_SEEN_TTL_MS,
            last_cleanup_ms: 0,
        }
    }

    /// Create with custom TTL
    pub fn with_ttl(ttl_ms: u64) -> Self {
        Self {
            cache: HashMap::new(),
            ttl_ms,
            last_cleanup_ms: 0,
        }
    }

    /// Check if a message has been seen before
    ///
    /// Returns true if the message was already seen (should not process/relay).
    /// Returns false if this is a new message (should process).
    pub fn has_seen(&self, message_id: &MessageId) -> bool {
        self.cache.contains_key(message_id)
    }

    /// Mark a message as seen
    ///
    /// Returns true if this is a new message (first time seen).
    /// Returns false if we've seen this message before.
    pub fn mark_seen(&mut self, message_id: MessageId, origin: NodeId, now_ms: u64) -> bool {
        // Run cleanup periodically
        if now_ms.saturating_sub(self.last_cleanup_ms) > self.ttl_ms / 2 {
            self.cleanup(now_ms);
        }

        if let Some(entry) = self.cache.get_mut(&message_id) {
            entry.count += 1;
            false // Already seen
        } else {
            self.cache.insert(
                message_id,
                SeenEntry {
                    first_seen_ms: now_ms,
                    count: 1,
                    origin,
                },
            );
            true // New message
        }
    }

    /// Check and mark in one operation
    ///
    /// Returns true if this is a new message that should be processed.
    /// Returns false if the message was already seen (duplicate).
    pub fn check_and_mark(&mut self, message_id: MessageId, origin: NodeId, now_ms: u64) -> bool {
        self.mark_seen(message_id, origin, now_ms)
    }

    /// Remove expired entries
    pub fn cleanup(&mut self, now_ms: u64) {
        self.last_cleanup_ms = now_ms;

        self.cache
            .retain(|_, entry| now_ms.saturating_sub(entry.first_seen_ms) < self.ttl_ms);

        // Force cleanup if still too large
        if self.cache.len() > MAX_CACHE_SIZE {
            // Remove oldest entries
            let mut entries: Vec<_> = self.cache.iter().collect();
            entries.sort_by_key(|(_, e)| e.first_seen_ms);

            let to_remove: Vec<_> = entries
                .iter()
                .take(self.cache.len() - MAX_CACHE_SIZE / 2)
                .map(|(id, _)| **id)
                .collect();

            for id in to_remove {
                self.cache.remove(&id);
            }
        }
    }

    /// Get the number of entries in the cache
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Get statistics about a message
    pub fn get_stats(&self, message_id: &MessageId) -> Option<(u64, u32, NodeId)> {
        self.cache
            .get(message_id)
            .map(|e| (e.first_seen_ms, e.count, e.origin))
    }
}

#[cfg(feature = "std")]
impl Default for SeenMessageCache {
    fn default() -> Self {
        Self::new()
    }
}

/// No_std version using BTreeMap
#[cfg(not(feature = "std"))]
#[derive(Debug)]
pub struct SeenMessageCache {
    cache: BTreeMap<MessageId, SeenEntry>,
    ttl_ms: u64,
    last_cleanup_ms: u64,
}

#[cfg(not(feature = "std"))]
impl SeenMessageCache {
    pub fn new() -> Self {
        Self {
            cache: BTreeMap::new(),
            ttl_ms: DEFAULT_SEEN_TTL_MS,
            last_cleanup_ms: 0,
        }
    }

    pub fn with_ttl(ttl_ms: u64) -> Self {
        Self {
            cache: BTreeMap::new(),
            ttl_ms,
            last_cleanup_ms: 0,
        }
    }

    pub fn has_seen(&self, message_id: &MessageId) -> bool {
        self.cache.contains_key(message_id)
    }

    pub fn mark_seen(&mut self, message_id: MessageId, origin: NodeId, now_ms: u64) -> bool {
        if now_ms.saturating_sub(self.last_cleanup_ms) > self.ttl_ms / 2 {
            self.cleanup(now_ms);
        }

        if let Some(entry) = self.cache.get_mut(&message_id) {
            entry.count += 1;
            false
        } else {
            self.cache.insert(
                message_id,
                SeenEntry {
                    first_seen_ms: now_ms,
                    count: 1,
                    origin,
                },
            );
            true
        }
    }

    pub fn check_and_mark(&mut self, message_id: MessageId, origin: NodeId, now_ms: u64) -> bool {
        self.mark_seen(message_id, origin, now_ms)
    }

    pub fn cleanup(&mut self, now_ms: u64) {
        self.last_cleanup_ms = now_ms;

        let expired: Vec<_> = self
            .cache
            .iter()
            .filter(|(_, e)| now_ms.saturating_sub(e.first_seen_ms) >= self.ttl_ms)
            .map(|(id, _)| *id)
            .collect();

        for id in expired {
            self.cache.remove(&id);
        }
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

#[cfg(not(feature = "std"))]
impl Default for SeenMessageCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_id_from_content() {
        let origin = NodeId::new(0x12345678);
        let id1 = MessageId::from_content(origin, 1000, 0xDEADBEEF);
        let id2 = MessageId::from_content(origin, 1000, 0xDEADBEEF);
        let id3 = MessageId::from_content(origin, 1001, 0xDEADBEEF);

        assert_eq!(id1, id2); // Same content = same ID
        assert_ne!(id1, id3); // Different timestamp = different ID
    }

    #[test]
    fn test_relay_flags() {
        let flags = RelayFlags {
            requires_ack: true,
            is_broadcast: false,
        };
        let byte = flags.to_byte();
        let decoded = RelayFlags::from_byte(byte);
        assert!(decoded.requires_ack);
        assert!(!decoded.is_broadcast);

        let flags = RelayFlags {
            requires_ack: false,
            is_broadcast: true,
        };
        let byte = flags.to_byte();
        let decoded = RelayFlags::from_byte(byte);
        assert!(!decoded.requires_ack);
        assert!(decoded.is_broadcast);
    }

    #[test]
    fn test_relay_envelope_encode_decode() {
        let origin = NodeId::new(0x12345678);
        let payload = vec![1, 2, 3, 4, 5];
        let envelope = RelayEnvelope::new(origin, payload.clone());

        let encoded = envelope.encode();
        let decoded = RelayEnvelope::decode(&encoded).unwrap();

        assert_eq!(decoded.message_id, envelope.message_id);
        assert_eq!(decoded.hop_count, 0);
        assert_eq!(decoded.max_hops, DEFAULT_MAX_HOPS);
        assert_eq!(decoded.origin_node, origin);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_relay_envelope_hop_tracking() {
        let origin = NodeId::new(0x12345678);
        let envelope = RelayEnvelope::new(origin, vec![1, 2, 3]).with_max_hops(3);

        assert!(envelope.can_relay());
        assert_eq!(envelope.remaining_hops(), 3);

        let relayed = envelope.relay().unwrap();
        assert_eq!(relayed.hop_count, 1);
        assert!(relayed.can_relay());

        let relayed = relayed.relay().unwrap();
        assert_eq!(relayed.hop_count, 2);
        assert!(relayed.can_relay());

        let relayed = relayed.relay().unwrap();
        assert_eq!(relayed.hop_count, 3);
        assert!(!relayed.can_relay()); // TTL expired

        assert!(relayed.relay().is_none()); // Cannot relay further
    }

    #[test]
    fn test_is_relay_envelope() {
        let data = vec![RELAY_ENVELOPE_MARKER, 0, 0, 0];
        assert!(RelayEnvelope::is_relay_envelope(&data));

        let data = vec![0x00, 0, 0, 0];
        assert!(!RelayEnvelope::is_relay_envelope(&data));

        let data: Vec<u8> = vec![];
        assert!(!RelayEnvelope::is_relay_envelope(&data));
    }

    #[test]
    fn test_seen_cache_basic() {
        let mut cache = SeenMessageCache::new();
        let origin = NodeId::new(0x12345678);

        let id1 = MessageId::from_content(origin, 1000, 0xAABBCCDD);
        let id2 = MessageId::from_content(origin, 1001, 0xAABBCCDD);

        // First time seeing id1
        assert!(cache.check_and_mark(id1, origin, 1000));
        assert!(!cache.has_seen(&id2));

        // Second time seeing id1 - should be rejected
        assert!(!cache.check_and_mark(id1, origin, 1001));

        // First time seeing id2
        assert!(cache.check_and_mark(id2, origin, 1002));

        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_seen_cache_cleanup() {
        let mut cache = SeenMessageCache::with_ttl(1000); // 1 second TTL
        let origin = NodeId::new(0x12345678);

        let id1 = MessageId::from_content(origin, 1000, 0x11111111);
        let id2 = MessageId::from_content(origin, 2000, 0x22222222);

        // Add id1 at t=0
        cache.mark_seen(id1, origin, 0);
        assert_eq!(cache.len(), 1);

        // Add id2 at t=500
        cache.mark_seen(id2, origin, 500);
        assert_eq!(cache.len(), 2);

        // Cleanup at t=1001 - id1 should be expired (1001 - 0 = 1001 >= 1000)
        // id2 should still be valid (1001 - 500 = 501 < 1000)
        cache.cleanup(1001);
        assert_eq!(cache.len(), 1);
        assert!(!cache.has_seen(&id1));
        assert!(cache.has_seen(&id2));

        // Cleanup at t=1501 - id2 should be expired too (1501 - 500 = 1001 >= 1000)
        cache.cleanup(1501);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_seen_cache_stats() {
        let mut cache = SeenMessageCache::new();
        let origin = NodeId::new(0x12345678);
        let id = MessageId::from_content(origin, 1000, 0xDEADBEEF);

        // First mark
        cache.mark_seen(id, origin, 1000);
        let (first_seen, count, orig) = cache.get_stats(&id).unwrap();
        assert_eq!(first_seen, 1000);
        assert_eq!(count, 1);
        assert_eq!(orig, origin);

        // Second mark - count should increase
        cache.mark_seen(id, origin, 2000);
        let (first_seen, count, _) = cache.get_stats(&id).unwrap();
        assert_eq!(first_seen, 1000); // Still the first time
        assert_eq!(count, 2);
    }
}
