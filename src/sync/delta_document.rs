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

//! Delta Document wire format for bandwidth-efficient sync
//!
//! This module implements the v2 document format that supports delta sync -
//! sending only changed operations instead of full state snapshots.
//!
//! ## Wire Format
//!
//! Delta documents are identified by the DELTA_DOCUMENT_MARKER (0xB2):
//!
//! ```text
//! [1 byte:  marker (0xB2)]
//! [1 byte:  flags]
//!   - bit 0: has_vector_clock
//!   - bit 1: is_response (sync response vs broadcast)
//!   - bits 2-7: reserved
//! [4 bytes: origin_node (LE)]
//! [8 bytes: timestamp_ms (LE)]
//! [variable: vector_clock (if flag set)]
//!   - [2 bytes: entry_count]
//!   - [entry_count × (4 bytes node_id + 8 bytes clock)]
//! [2 bytes: operation_count (LE)]
//! [operations...]
//! ```
//!
//! ## Operation Format
//!
//! Each operation is prefixed with a 1-byte type:
//!
//! - 0x01: IncrementCounter - counter increment
//! - 0x02: UpdatePeripheral - peripheral state update
//! - 0x03: SetEmergency - create emergency event
//! - 0x04: AckEmergency - acknowledge emergency
//! - 0x05: ClearEmergency - clear emergency
//!
//! ## Usage
//!
//! ```ignore
//! // Check if incoming data is a delta document
//! if DeltaDocument::is_delta_document(&data) {
//!     let delta = DeltaDocument::decode(&data)?;
//!     for op in &delta.operations {
//!         apply_operation(op);
//!     }
//! }
//!
//! // Build delta for a specific peer
//! let delta = mesh.build_delta_for_peer(&peer_id);
//! let data = delta.encode();
//! ```

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

use crate::registry::AppOperation;
use crate::sync::crdt::Peripheral;
use crate::sync::delta::VectorClock;
use crate::NodeId;

/// Marker byte for delta document format
pub const DELTA_DOCUMENT_MARKER: u8 = 0xB2;

/// Operation type constants
pub mod op_type {
    /// Counter increment operation
    pub const INCREMENT_COUNTER: u8 = 0x01;
    /// Peripheral state update operation
    pub const UPDATE_PERIPHERAL: u8 = 0x02;
    /// Set emergency event operation
    pub const SET_EMERGENCY: u8 = 0x03;
    /// Acknowledge emergency operation
    pub const ACK_EMERGENCY: u8 = 0x04;
    /// Clear emergency operation
    pub const CLEAR_EMERGENCY: u8 = 0x05;
}

/// Flags for delta document
#[derive(Debug, Clone, Copy, Default)]
pub struct DeltaFlags {
    /// Whether vector clock is included
    pub has_vector_clock: bool,
    /// Whether this is a sync response (vs broadcast)
    pub is_response: bool,
}

impl DeltaFlags {
    /// Encode flags to a single byte
    pub fn to_byte(&self) -> u8 {
        let mut flags = 0u8;
        if self.has_vector_clock {
            flags |= 0x01;
        }
        if self.is_response {
            flags |= 0x02;
        }
        flags
    }

    /// Decode flags from a single byte
    pub fn from_byte(byte: u8) -> Self {
        Self {
            has_vector_clock: byte & 0x01 != 0,
            is_response: byte & 0x02 != 0,
        }
    }
}

/// A CRDT operation for delta sync
#[derive(Debug, Clone)]
pub enum Operation {
    /// Increment a counter
    IncrementCounter {
        /// Counter ID (0 = default mesh counter)
        counter_id: u8,
        /// Node that incremented
        node_id: NodeId,
        /// Amount to increment
        amount: u64,
        /// Timestamp of increment
        timestamp: u64,
    },

    /// Update peripheral state
    UpdatePeripheral {
        /// The peripheral data
        peripheral: Peripheral,
        /// Timestamp of update
        timestamp: u64,
    },

    /// Set an emergency event
    SetEmergency {
        /// Source node declaring emergency
        source_node: NodeId,
        /// Timestamp of emergency
        timestamp: u64,
        /// Known peers at time of emergency
        known_peers: Vec<u32>,
    },

    /// Acknowledge an emergency
    AckEmergency {
        /// Node sending the ACK
        node_id: NodeId,
        /// Timestamp of emergency being ACKed
        emergency_timestamp: u64,
    },

    /// Clear an emergency
    ClearEmergency {
        /// Timestamp of emergency being cleared
        emergency_timestamp: u64,
    },

    /// App-layer document operation (0x10-0x1F range)
    ///
    /// Used for extensible document types registered via DocumentRegistry.
    /// The AppOperation contains type_id, op_code, source_node, timestamp, and payload.
    App(AppOperation),
}

impl Operation {
    /// Get the timestamp associated with this operation
    pub fn timestamp(&self) -> u64 {
        match self {
            Operation::IncrementCounter { timestamp, .. } => *timestamp,
            Operation::UpdatePeripheral { timestamp, .. } => *timestamp,
            Operation::SetEmergency { timestamp, .. } => *timestamp,
            Operation::AckEmergency {
                emergency_timestamp,
                ..
            } => *emergency_timestamp,
            Operation::ClearEmergency {
                emergency_timestamp,
            } => *emergency_timestamp,
            Operation::App(op) => op.timestamp,
        }
    }

    /// Get a unique key for this operation (for deduplication)
    pub fn key(&self) -> String {
        match self {
            Operation::IncrementCounter {
                counter_id,
                node_id,
                ..
            } => {
                #[cfg(feature = "std")]
                return format!("counter:{}:{}", counter_id, node_id.as_u32());
                #[cfg(not(feature = "std"))]
                return alloc::format!("counter:{}:{}", counter_id, node_id.as_u32());
            }
            Operation::UpdatePeripheral { peripheral, .. } => {
                #[cfg(feature = "std")]
                return format!("peripheral:{}", peripheral.id);
                #[cfg(not(feature = "std"))]
                return alloc::format!("peripheral:{}", peripheral.id);
            }
            Operation::SetEmergency { source_node, .. } => {
                #[cfg(feature = "std")]
                return format!("emergency:{}", source_node.as_u32());
                #[cfg(not(feature = "std"))]
                return alloc::format!("emergency:{}", source_node.as_u32());
            }
            Operation::AckEmergency { node_id, .. } => {
                #[cfg(feature = "std")]
                return format!("ack:{}", node_id.as_u32());
                #[cfg(not(feature = "std"))]
                return alloc::format!("ack:{}", node_id.as_u32());
            }
            Operation::ClearEmergency { .. } => "clear_emergency".into(),
            Operation::App(op) => {
                // Key includes type_id, source_node, and timestamp for document identity
                #[cfg(feature = "std")]
                return format!("app:{}:{}:{}", op.type_id, op.source_node, op.timestamp);
                #[cfg(not(feature = "std"))]
                return alloc::format!("app:{}:{}:{}", op.type_id, op.source_node, op.timestamp);
            }
        }
    }

    /// Encode to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        match self {
            Operation::IncrementCounter {
                counter_id,
                node_id,
                amount,
                timestamp,
            } => {
                buf.push(op_type::INCREMENT_COUNTER);
                buf.push(*counter_id);
                buf.extend_from_slice(&node_id.as_u32().to_le_bytes());
                buf.extend_from_slice(&amount.to_le_bytes());
                buf.extend_from_slice(&timestamp.to_le_bytes());
            }
            Operation::UpdatePeripheral {
                peripheral,
                timestamp,
            } => {
                buf.push(op_type::UPDATE_PERIPHERAL);
                buf.extend_from_slice(&timestamp.to_le_bytes());
                let pdata = peripheral.encode();
                buf.extend_from_slice(&(pdata.len() as u16).to_le_bytes());
                buf.extend_from_slice(&pdata);
            }
            Operation::SetEmergency {
                source_node,
                timestamp,
                known_peers,
            } => {
                buf.push(op_type::SET_EMERGENCY);
                buf.extend_from_slice(&source_node.as_u32().to_le_bytes());
                buf.extend_from_slice(&timestamp.to_le_bytes());
                buf.push(known_peers.len() as u8);
                for peer in known_peers {
                    buf.extend_from_slice(&peer.to_le_bytes());
                }
            }
            Operation::AckEmergency {
                node_id,
                emergency_timestamp,
            } => {
                buf.push(op_type::ACK_EMERGENCY);
                buf.extend_from_slice(&node_id.as_u32().to_le_bytes());
                buf.extend_from_slice(&emergency_timestamp.to_le_bytes());
            }
            Operation::ClearEmergency {
                emergency_timestamp,
            } => {
                buf.push(op_type::CLEAR_EMERGENCY);
                buf.extend_from_slice(&emergency_timestamp.to_le_bytes());
            }
            Operation::App(op) => {
                // AppOperation has its own encode that includes op_type byte (0x10-0x1F)
                buf.extend_from_slice(&op.encode());
            }
        }

        buf
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<(Self, usize)> {
        if data.is_empty() {
            return None;
        }

        let op_type = data[0];

        match op_type {
            op_type::INCREMENT_COUNTER => {
                if data.len() < 22 {
                    return None;
                }
                let counter_id = data[1];
                let node_id = NodeId::new(u32::from_le_bytes([data[2], data[3], data[4], data[5]]));
                let amount = u64::from_le_bytes([
                    data[6], data[7], data[8], data[9], data[10], data[11], data[12], data[13],
                ]);
                let timestamp = u64::from_le_bytes([
                    data[14], data[15], data[16], data[17], data[18], data[19], data[20], data[21],
                ]);
                Some((
                    Operation::IncrementCounter {
                        counter_id,
                        node_id,
                        amount,
                        timestamp,
                    },
                    22,
                ))
            }
            op_type::UPDATE_PERIPHERAL => {
                if data.len() < 11 {
                    return None;
                }
                let timestamp = u64::from_le_bytes([
                    data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8],
                ]);
                let plen = u16::from_le_bytes([data[9], data[10]]) as usize;
                if data.len() < 11 + plen {
                    return None;
                }
                let peripheral = Peripheral::decode(&data[11..11 + plen])?;
                Some((
                    Operation::UpdatePeripheral {
                        peripheral,
                        timestamp,
                    },
                    11 + plen,
                ))
            }
            op_type::SET_EMERGENCY => {
                if data.len() < 14 {
                    return None;
                }
                let source_node =
                    NodeId::new(u32::from_le_bytes([data[1], data[2], data[3], data[4]]));
                let timestamp = u64::from_le_bytes([
                    data[5], data[6], data[7], data[8], data[9], data[10], data[11], data[12],
                ]);
                let peer_count = data[13] as usize;
                if data.len() < 14 + peer_count * 4 {
                    return None;
                }
                let mut known_peers = Vec::with_capacity(peer_count);
                let mut offset = 14;
                for _ in 0..peer_count {
                    known_peers.push(u32::from_le_bytes([
                        data[offset],
                        data[offset + 1],
                        data[offset + 2],
                        data[offset + 3],
                    ]));
                    offset += 4;
                }
                Some((
                    Operation::SetEmergency {
                        source_node,
                        timestamp,
                        known_peers,
                    },
                    offset,
                ))
            }
            op_type::ACK_EMERGENCY => {
                if data.len() < 13 {
                    return None;
                }
                let node_id = NodeId::new(u32::from_le_bytes([data[1], data[2], data[3], data[4]]));
                let emergency_timestamp = u64::from_le_bytes([
                    data[5], data[6], data[7], data[8], data[9], data[10], data[11], data[12],
                ]);
                Some((
                    Operation::AckEmergency {
                        node_id,
                        emergency_timestamp,
                    },
                    13,
                ))
            }
            op_type::CLEAR_EMERGENCY => {
                if data.len() < 9 {
                    return None;
                }
                let emergency_timestamp = u64::from_le_bytes([
                    data[1], data[2], data[3], data[4], data[5], data[6], data[7], data[8],
                ]);
                Some((
                    Operation::ClearEmergency {
                        emergency_timestamp,
                    },
                    9,
                ))
            }
            // App-layer operations (0x10-0x1F range)
            op if AppOperation::is_app_op_type(op) => {
                let (app_op, consumed) = AppOperation::decode(data)?;
                Some((Operation::App(app_op), consumed))
            }
            _ => None,
        }
    }
}

/// A delta document containing only changed operations
#[derive(Debug, Clone)]
pub struct DeltaDocument {
    /// Origin node that created this delta
    pub origin_node: NodeId,

    /// Timestamp when delta was created
    pub timestamp_ms: u64,

    /// Flags
    pub flags: DeltaFlags,

    /// Vector clock (for sync negotiation)
    pub vector_clock: Option<VectorClock>,

    /// Operations in this delta
    pub operations: Vec<Operation>,
}

impl DeltaDocument {
    /// Create a new empty delta document
    pub fn new(origin_node: NodeId, timestamp_ms: u64) -> Self {
        Self {
            origin_node,
            timestamp_ms,
            flags: DeltaFlags::default(),
            vector_clock: None,
            operations: Vec::new(),
        }
    }

    /// Create with vector clock
    pub fn with_vector_clock(mut self, clock: VectorClock) -> Self {
        self.vector_clock = Some(clock);
        self.flags.has_vector_clock = true;
        self
    }

    /// Mark as sync response
    pub fn as_response(mut self) -> Self {
        self.flags.is_response = true;
        self
    }

    /// Add an operation
    pub fn add_operation(&mut self, op: Operation) {
        self.operations.push(op);
    }

    /// Check if empty (no operations)
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// Get operation count
    pub fn operation_count(&self) -> usize {
        self.operations.len()
    }

    /// Check if data starts with delta document marker
    pub fn is_delta_document(data: &[u8]) -> bool {
        !data.is_empty() && data[0] == DELTA_DOCUMENT_MARKER
    }

    /// Encode to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // Marker
        buf.push(DELTA_DOCUMENT_MARKER);

        // Flags
        buf.push(self.flags.to_byte());

        // Origin node
        buf.extend_from_slice(&self.origin_node.as_u32().to_le_bytes());

        // Timestamp
        buf.extend_from_slice(&self.timestamp_ms.to_le_bytes());

        // Vector clock (if present)
        if let Some(ref clock) = self.vector_clock {
            let clock_data = clock.encode();
            buf.extend_from_slice(&clock_data);
        }

        // Operation count
        buf.extend_from_slice(&(self.operations.len() as u16).to_le_bytes());

        // Operations
        for op in &self.operations {
            buf.extend_from_slice(&op.encode());
        }

        buf
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        // Minimum size: marker(1) + flags(1) + origin(4) + timestamp(8) + op_count(2) = 16
        if data.len() < 16 {
            return None;
        }

        if data[0] != DELTA_DOCUMENT_MARKER {
            return None;
        }

        let flags = DeltaFlags::from_byte(data[1]);
        let origin_node = NodeId::new(u32::from_le_bytes([data[2], data[3], data[4], data[5]]));
        let timestamp_ms = u64::from_le_bytes([
            data[6], data[7], data[8], data[9], data[10], data[11], data[12], data[13],
        ]);

        let mut offset = 14;

        // Vector clock (if present)
        let vector_clock = if flags.has_vector_clock {
            let clock = VectorClock::decode(&data[offset..])?;
            // Calculate clock size: 4 bytes count + count * 12 bytes
            let count = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as usize;
            offset += 4 + count * 12;
            Some(clock)
        } else {
            None
        };

        // Operation count
        if data.len() < offset + 2 {
            return None;
        }
        let op_count = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;

        // Operations
        let mut operations = Vec::with_capacity(op_count);
        for _ in 0..op_count {
            if offset >= data.len() {
                return None;
            }
            let (op, size) = Operation::decode(&data[offset..])?;
            operations.push(op);
            offset += size;
        }

        Some(Self {
            origin_node,
            timestamp_ms,
            flags,
            vector_clock,
            operations,
        })
    }

    /// Get estimated encoded size
    pub fn encoded_size(&self) -> usize {
        let base = 16; // marker + flags + origin + timestamp + op_count
        let clock_size = self
            .vector_clock
            .as_ref()
            .map(|c| c.encode().len())
            .unwrap_or(0);
        let ops_size: usize = self.operations.iter().map(|op| op.encode().len()).sum();
        base + clock_size + ops_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::crdt::PeripheralType;

    #[test]
    fn test_operation_increment_counter_encode_decode() {
        let op = Operation::IncrementCounter {
            counter_id: 0,
            node_id: NodeId::new(0x12345678),
            amount: 42,
            timestamp: 1000,
        };

        let encoded = op.encode();
        let (decoded, size) = Operation::decode(&encoded).unwrap();

        assert_eq!(size, encoded.len());
        if let Operation::IncrementCounter {
            counter_id,
            node_id,
            amount,
            timestamp,
        } = decoded
        {
            assert_eq!(counter_id, 0);
            assert_eq!(node_id.as_u32(), 0x12345678);
            assert_eq!(amount, 42);
            assert_eq!(timestamp, 1000);
        } else {
            panic!("Wrong operation type");
        }
    }

    #[test]
    fn test_operation_update_peripheral_encode_decode() {
        let peripheral =
            Peripheral::new(0xAABBCCDD, PeripheralType::SoldierSensor).with_callsign("ALPHA-1");

        let op = Operation::UpdatePeripheral {
            peripheral: peripheral.clone(),
            timestamp: 2000,
        };

        let encoded = op.encode();
        let (decoded, size) = Operation::decode(&encoded).unwrap();

        assert_eq!(size, encoded.len());
        if let Operation::UpdatePeripheral {
            peripheral: p,
            timestamp: t,
        } = decoded
        {
            assert_eq!(p.id, peripheral.id);
            assert_eq!(p.callsign_str(), "ALPHA-1");
            assert_eq!(t, 2000);
        } else {
            panic!("Wrong operation type");
        }
    }

    #[test]
    fn test_operation_set_emergency_encode_decode() {
        let op = Operation::SetEmergency {
            source_node: NodeId::new(0x11111111),
            timestamp: 3000,
            known_peers: vec![0x22222222, 0x33333333],
        };

        let encoded = op.encode();
        let (decoded, size) = Operation::decode(&encoded).unwrap();

        assert_eq!(size, encoded.len());
        if let Operation::SetEmergency {
            source_node,
            timestamp,
            known_peers,
        } = decoded
        {
            assert_eq!(source_node.as_u32(), 0x11111111);
            assert_eq!(timestamp, 3000);
            assert_eq!(known_peers, vec![0x22222222, 0x33333333]);
        } else {
            panic!("Wrong operation type");
        }
    }

    #[test]
    fn test_operation_ack_emergency_encode_decode() {
        let op = Operation::AckEmergency {
            node_id: NodeId::new(0x22222222),
            emergency_timestamp: 3000,
        };

        let encoded = op.encode();
        let (decoded, size) = Operation::decode(&encoded).unwrap();

        assert_eq!(size, encoded.len());
        if let Operation::AckEmergency {
            node_id,
            emergency_timestamp,
        } = decoded
        {
            assert_eq!(node_id.as_u32(), 0x22222222);
            assert_eq!(emergency_timestamp, 3000);
        } else {
            panic!("Wrong operation type");
        }
    }

    #[test]
    fn test_operation_clear_emergency_encode_decode() {
        let op = Operation::ClearEmergency {
            emergency_timestamp: 3000,
        };

        let encoded = op.encode();
        let (decoded, size) = Operation::decode(&encoded).unwrap();

        assert_eq!(size, encoded.len());
        if let Operation::ClearEmergency {
            emergency_timestamp,
        } = decoded
        {
            assert_eq!(emergency_timestamp, 3000);
        } else {
            panic!("Wrong operation type");
        }
    }

    #[test]
    fn test_delta_document_empty() {
        let delta = DeltaDocument::new(NodeId::new(0x12345678), 1000);

        assert!(delta.is_empty());
        assert_eq!(delta.operation_count(), 0);

        let encoded = delta.encode();
        assert!(DeltaDocument::is_delta_document(&encoded));

        let decoded = DeltaDocument::decode(&encoded).unwrap();
        assert_eq!(decoded.origin_node.as_u32(), 0x12345678);
        assert_eq!(decoded.timestamp_ms, 1000);
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_delta_document_with_operations() {
        let mut delta = DeltaDocument::new(NodeId::new(0x12345678), 1000);

        delta.add_operation(Operation::IncrementCounter {
            counter_id: 0,
            node_id: NodeId::new(0x12345678),
            amount: 1,
            timestamp: 1000,
        });

        delta.add_operation(Operation::AckEmergency {
            node_id: NodeId::new(0x12345678),
            emergency_timestamp: 500,
        });

        assert_eq!(delta.operation_count(), 2);

        let encoded = delta.encode();
        let decoded = DeltaDocument::decode(&encoded).unwrap();

        assert_eq!(decoded.operation_count(), 2);
    }

    #[test]
    fn test_delta_document_with_vector_clock() {
        let mut clock = VectorClock::new();
        clock.update(&NodeId::new(0x11111111), 5);
        clock.update(&NodeId::new(0x22222222), 3);

        let delta =
            DeltaDocument::new(NodeId::new(0x12345678), 1000).with_vector_clock(clock.clone());

        assert!(delta.flags.has_vector_clock);

        let encoded = delta.encode();
        let decoded = DeltaDocument::decode(&encoded).unwrap();

        assert!(decoded.flags.has_vector_clock);
        assert!(decoded.vector_clock.is_some());

        let decoded_clock = decoded.vector_clock.unwrap();
        assert_eq!(decoded_clock.get(&NodeId::new(0x11111111)), 5);
        assert_eq!(decoded_clock.get(&NodeId::new(0x22222222)), 3);
    }

    #[test]
    fn test_delta_document_is_delta_document() {
        let delta = DeltaDocument::new(NodeId::new(0x12345678), 1000);
        let encoded = delta.encode();

        assert!(DeltaDocument::is_delta_document(&encoded));

        // Non-delta data
        let non_delta = vec![0x00, 0x01, 0x02, 0x03];
        assert!(!DeltaDocument::is_delta_document(&non_delta));

        let empty: Vec<u8> = vec![];
        assert!(!DeltaDocument::is_delta_document(&empty));
    }

    #[test]
    fn test_operation_key() {
        let op1 = Operation::IncrementCounter {
            counter_id: 0,
            node_id: NodeId::new(0x11111111),
            amount: 1,
            timestamp: 1000,
        };
        let op2 = Operation::IncrementCounter {
            counter_id: 0,
            node_id: NodeId::new(0x11111111),
            amount: 2,
            timestamp: 2000,
        };
        let op3 = Operation::IncrementCounter {
            counter_id: 0,
            node_id: NodeId::new(0x22222222),
            amount: 1,
            timestamp: 1000,
        };

        // Same node, same counter = same key
        assert_eq!(op1.key(), op2.key());

        // Different node = different key
        assert_ne!(op1.key(), op3.key());
    }
}
