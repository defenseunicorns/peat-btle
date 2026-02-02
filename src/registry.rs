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

//! Extensible Document Registry for app-layer CRDT types.
//!
//! This module enables external crates to register custom document types
//! that sync through hive-btle's delta mechanism.
//!
//! ## Overview
//!
//! The registry uses marker bytes in the 0xC0-0xCF range for app-layer types.
//! Each registered type must implement the [`DocumentType`] trait, providing
//! encode/decode/merge methods.
//!
//! ## Example
//!
//! ```ignore
//! use hive_btle::registry::{DocumentType, DocumentRegistry, AppOperation};
//!
//! #[derive(Clone)]
//! struct MyMessage {
//!     source_node: u32,
//!     timestamp: u64,
//!     content: String,
//! }
//!
//! impl DocumentType for MyMessage {
//!     const TYPE_ID: u8 = 0xC0;
//!     const TYPE_NAME: &'static str = "MyMessage";
//!
//!     fn identity(&self) -> (u32, u64) {
//!         (self.source_node, self.timestamp)
//!     }
//!
//!     fn encode(&self) -> Vec<u8> {
//!         // ... encoding logic
//!         vec![]
//!     }
//!
//!     fn decode(data: &[u8]) -> Option<Self> {
//!         // ... decoding logic
//!         None
//!     }
//!
//!     fn merge(&mut self, other: &Self) -> bool {
//!         // ... CRDT merge logic
//!         false
//!     }
//! }
//!
//! // Register and use
//! let registry = DocumentRegistry::new();
//! registry.register::<MyMessage>();
//! ```

#[cfg(not(feature = "std"))]
use alloc::{boxed::Box, vec::Vec};

#[cfg(feature = "std")]
use std::sync::RwLock;

#[cfg(not(feature = "std"))]
use spin::RwLock;

use core::any::Any;

#[cfg(feature = "std")]
use std::collections::HashMap;

#[cfg(not(feature = "std"))]
use hashbrown::HashMap;

/// Minimum marker byte for app-layer document types.
pub const APP_TYPE_MIN: u8 = 0xC0;

/// Maximum marker byte for app-layer document types.
pub const APP_TYPE_MAX: u8 = 0xCF;

/// Base operation type for app-layer delta operations.
/// Operations are encoded as 0x10 + (type_id - 0xC0), giving range 0x10-0x1F.
pub const APP_OP_BASE: u8 = 0x10;

/// A registered document type that can be synced through the mesh.
///
/// Implementations must be deterministic - the same logical state
/// must encode consistently for merge operations to work correctly.
/// Document identity (source_node, timestamp) is used for deduplication
/// instead of content hash, since CRDT merge may change byte ordering.
pub trait DocumentType: Clone + Send + Sync + 'static {
    /// Unique type identifier (marker byte in 0xC0-0xCF range).
    const TYPE_ID: u8;

    /// Human-readable type name for debugging.
    const TYPE_NAME: &'static str;

    /// Document identity for deduplication.
    ///
    /// Returns (source_node, timestamp) tuple that uniquely identifies
    /// this document instance.
    fn identity(&self) -> (u32, u64);

    /// Encode to wire format (payload only, not including type header).
    fn encode(&self) -> Vec<u8>;

    /// Decode from wire format.
    ///
    /// Input is the payload after the type header.
    fn decode(data: &[u8]) -> Option<Self>
    where
        Self: Sized;

    /// Merge with another instance using CRDT semantics.
    ///
    /// Returns true if our state changed.
    fn merge(&mut self, other: &Self) -> bool;

    /// Convert to a delta operation for efficient sync.
    ///
    /// Returns None if this type doesn't support delta sync
    /// (will use full-state sync instead).
    fn to_delta_op(&self) -> Option<AppOperation> {
        None
    }

    /// Apply a delta operation to this document.
    ///
    /// Returns true if state changed.
    fn apply_delta_op(&mut self, _op: &AppOperation) -> bool {
        false
    }
}

/// App-layer delta operation.
///
/// Used for efficient sync of registered document types.
#[derive(Debug, Clone)]
pub struct AppOperation {
    /// Document type ID (0xC0-0xCF).
    pub type_id: u8,

    /// Operation code (type-specific, 0-255).
    pub op_code: u8,

    /// Source node that created this operation.
    pub source_node: u32,

    /// Timestamp of the operation.
    pub timestamp: u64,

    /// Operation payload (type-specific).
    pub payload: Vec<u8>,
}

impl AppOperation {
    /// Create a new app operation.
    pub fn new(type_id: u8, op_code: u8, source_node: u32, timestamp: u64) -> Self {
        Self {
            type_id,
            op_code,
            source_node,
            timestamp,
            payload: Vec::new(),
        }
    }

    /// Create with payload.
    pub fn with_payload(mut self, payload: Vec<u8>) -> Self {
        self.payload = payload;
        self
    }

    /// Check if this is a valid app-layer operation type.
    pub fn is_app_op_type(op_type: u8) -> bool {
        (APP_OP_BASE..APP_OP_BASE + 16).contains(&op_type)
    }

    /// Get the operation type byte for wire encoding.
    pub fn op_type_byte(&self) -> u8 {
        APP_OP_BASE + (self.type_id - APP_TYPE_MIN)
    }

    /// Encode to wire format.
    ///
    /// Format:
    /// ```text
    /// [op_type: 1B]      - 0x10 + (type_id - 0xC0)
    /// [op_code: 1B]      - type-specific operation code
    /// [source_node: 4B]  - LE
    /// [timestamp: 8B]    - LE
    /// [payload_len: 2B]  - LE
    /// [payload: var]
    /// ```
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(16 + self.payload.len());

        buf.push(self.op_type_byte());
        buf.push(self.op_code);
        buf.extend_from_slice(&self.source_node.to_le_bytes());
        buf.extend_from_slice(&self.timestamp.to_le_bytes());
        buf.extend_from_slice(&(self.payload.len() as u16).to_le_bytes());
        buf.extend_from_slice(&self.payload);

        buf
    }

    /// Decode from wire format.
    ///
    /// Returns (operation, bytes_consumed) on success.
    pub fn decode(data: &[u8]) -> Option<(Self, usize)> {
        // Minimum size: op_type(1) + op_code(1) + source(4) + timestamp(8) + len(2) = 16
        if data.len() < 16 {
            return None;
        }

        let op_type = data[0];
        if !Self::is_app_op_type(op_type) {
            return None;
        }

        let type_id = APP_TYPE_MIN + (op_type - APP_OP_BASE);
        let op_code = data[1];
        let source_node = u32::from_le_bytes([data[2], data[3], data[4], data[5]]);
        let timestamp = u64::from_le_bytes([
            data[6], data[7], data[8], data[9], data[10], data[11], data[12], data[13],
        ]);
        let payload_len = u16::from_le_bytes([data[14], data[15]]) as usize;

        if data.len() < 16 + payload_len {
            return None;
        }

        let payload = data[16..16 + payload_len].to_vec();

        Some((
            Self {
                type_id,
                op_code,
                source_node,
                timestamp,
                payload,
            },
            16 + payload_len,
        ))
    }
}

/// Type-erased handler for document operations.
///
/// This trait enables the registry to work with heterogeneous types.
trait DocumentHandler: Send + Sync {
    /// Get type name for debugging.
    fn type_name(&self) -> &'static str;

    /// Decode document from bytes.
    fn decode(&self, data: &[u8]) -> Option<Box<dyn Any + Send + Sync>>;

    /// Merge two documents.
    fn merge(&self, doc: &mut dyn Any, other: &dyn Any) -> bool;

    /// Encode document to bytes.
    fn encode(&self, doc: &dyn Any) -> Vec<u8>;

    /// Get document identity.
    fn identity(&self, doc: &dyn Any) -> (u32, u64);

    /// Convert to delta operation.
    fn to_delta_op(&self, doc: &dyn Any) -> Option<AppOperation>;

    /// Apply delta operation.
    fn apply_delta_op(&self, doc: &mut dyn Any, op: &AppOperation) -> bool;
}

/// Concrete handler for a specific DocumentType.
struct TypedHandler<T: DocumentType> {
    _marker: core::marker::PhantomData<T>,
}

impl<T: DocumentType> Default for TypedHandler<T> {
    fn default() -> Self {
        Self {
            _marker: core::marker::PhantomData,
        }
    }
}

impl<T: DocumentType> DocumentHandler for TypedHandler<T> {
    fn type_name(&self) -> &'static str {
        T::TYPE_NAME
    }

    fn decode(&self, data: &[u8]) -> Option<Box<dyn Any + Send + Sync>> {
        T::decode(data).map(|doc| Box::new(doc) as Box<dyn Any + Send + Sync>)
    }

    fn merge(&self, doc: &mut dyn Any, other: &dyn Any) -> bool {
        if let (Some(doc), Some(other)) = (doc.downcast_mut::<T>(), other.downcast_ref::<T>()) {
            doc.merge(other)
        } else {
            false
        }
    }

    fn encode(&self, doc: &dyn Any) -> Vec<u8> {
        doc.downcast_ref::<T>()
            .map(|d| d.encode())
            .unwrap_or_default()
    }

    fn identity(&self, doc: &dyn Any) -> (u32, u64) {
        doc.downcast_ref::<T>()
            .map(|d| d.identity())
            .unwrap_or((0, 0))
    }

    fn to_delta_op(&self, doc: &dyn Any) -> Option<AppOperation> {
        doc.downcast_ref::<T>().and_then(|d| d.to_delta_op())
    }

    fn apply_delta_op(&self, doc: &mut dyn Any, op: &AppOperation) -> bool {
        doc.downcast_mut::<T>()
            .map(|d| d.apply_delta_op(op))
            .unwrap_or(false)
    }
}

/// Registry for document type handlers.
///
/// Thread-safe, supports dynamic registration at runtime.
pub struct DocumentRegistry {
    handlers: RwLock<HashMap<u8, Box<dyn DocumentHandler>>>,
}

impl Default for DocumentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
        }
    }

    /// Register a document type handler.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - TYPE_ID is outside 0xC0-0xCF range
    /// - TYPE_ID is already registered
    pub fn register<T: DocumentType>(&self) {
        let type_id = T::TYPE_ID;

        assert!(
            (APP_TYPE_MIN..=APP_TYPE_MAX).contains(&type_id),
            "TYPE_ID 0x{:02X} is outside valid range 0xC0-0xCF",
            type_id
        );

        let handlers = self.handlers.write();
        #[cfg(feature = "std")]
        let mut handlers = handlers.unwrap();
        #[cfg(not(feature = "std"))]
        let mut handlers = handlers;

        assert!(
            !handlers.contains_key(&type_id),
            "TYPE_ID 0x{:02X} is already registered",
            type_id
        );

        handlers.insert(type_id, Box::new(TypedHandler::<T>::default()));
    }

    /// Try to register a document type, returning false if already registered.
    pub fn try_register<T: DocumentType>(&self) -> bool {
        let type_id = T::TYPE_ID;

        if !(APP_TYPE_MIN..=APP_TYPE_MAX).contains(&type_id) {
            return false;
        }

        let handlers = self.handlers.write();
        #[cfg(feature = "std")]
        let mut handlers = handlers.unwrap();
        #[cfg(not(feature = "std"))]
        let mut handlers = handlers;

        if handlers.contains_key(&type_id) {
            return false;
        }

        handlers.insert(type_id, Box::new(TypedHandler::<T>::default()));
        true
    }

    /// Check if a type is registered.
    pub fn is_registered(&self, type_id: u8) -> bool {
        let handlers = self.handlers.read();
        #[cfg(feature = "std")]
        let handlers = handlers.unwrap();

        handlers.contains_key(&type_id)
    }

    /// Check if a marker byte is an app-layer type.
    pub fn is_app_type(type_id: u8) -> bool {
        (APP_TYPE_MIN..=APP_TYPE_MAX).contains(&type_id)
    }

    /// Get type name for debugging.
    pub fn type_name(&self, type_id: u8) -> Option<&'static str> {
        let handlers = self.handlers.read();
        #[cfg(feature = "std")]
        let handlers = handlers.unwrap();

        handlers.get(&type_id).map(|h| h.type_name())
    }

    /// Get all registered type IDs.
    pub fn registered_types(&self) -> Vec<u8> {
        let handlers = self.handlers.read();
        #[cfg(feature = "std")]
        let handlers = handlers.unwrap();

        handlers.keys().copied().collect()
    }

    /// Decode a document from bytes.
    pub fn decode(&self, type_id: u8, data: &[u8]) -> Option<Box<dyn Any + Send + Sync>> {
        let handlers = self.handlers.read();
        #[cfg(feature = "std")]
        let handlers = handlers.unwrap();

        handlers.get(&type_id).and_then(|h| h.decode(data))
    }

    /// Merge two documents.
    pub fn merge(&self, type_id: u8, doc: &mut dyn Any, other: &dyn Any) -> bool {
        let handlers = self.handlers.read();
        #[cfg(feature = "std")]
        let handlers = handlers.unwrap();

        handlers
            .get(&type_id)
            .map(|h| h.merge(doc, other))
            .unwrap_or(false)
    }

    /// Encode a document to bytes.
    pub fn encode(&self, type_id: u8, doc: &dyn Any) -> Vec<u8> {
        let handlers = self.handlers.read();
        #[cfg(feature = "std")]
        let handlers = handlers.unwrap();

        handlers
            .get(&type_id)
            .map(|h| h.encode(doc))
            .unwrap_or_default()
    }

    /// Get document identity.
    pub fn identity(&self, type_id: u8, doc: &dyn Any) -> Option<(u32, u64)> {
        let handlers = self.handlers.read();
        #[cfg(feature = "std")]
        let handlers = handlers.unwrap();

        handlers.get(&type_id).map(|h| h.identity(doc))
    }

    /// Convert document to delta operation.
    pub fn to_delta_op(&self, type_id: u8, doc: &dyn Any) -> Option<AppOperation> {
        let handlers = self.handlers.read();
        #[cfg(feature = "std")]
        let handlers = handlers.unwrap();

        handlers.get(&type_id).and_then(|h| h.to_delta_op(doc))
    }

    /// Apply delta operation to document.
    pub fn apply_delta_op(&self, type_id: u8, doc: &mut dyn Any, op: &AppOperation) -> bool {
        let handlers = self.handlers.read();
        #[cfg(feature = "std")]
        let handlers = handlers.unwrap();

        handlers
            .get(&type_id)
            .map(|h| h.apply_delta_op(doc, op))
            .unwrap_or(false)
    }
}

/// Decode a typed document directly (when the type is known at compile time).
pub fn decode_typed<T: DocumentType>(data: &[u8]) -> Option<T> {
    T::decode(data)
}

/// Encode a document with its type header.
///
/// Format:
/// ```text
/// [type_id: 1B]   - 0xC0-0xCF
/// [flags: 1B]     - reserved (0x00)
/// [length: 2B]    - LE, payload length
/// [payload: var]  - type-specific encoding
/// ```
pub fn encode_with_header<T: DocumentType>(doc: &T) -> Vec<u8> {
    let payload = doc.encode();
    let mut buf = Vec::with_capacity(4 + payload.len());

    buf.push(T::TYPE_ID);
    buf.push(0x00); // flags (reserved)
    buf.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    buf.extend_from_slice(&payload);

    buf
}

/// Decode header and extract type_id and payload.
///
/// Returns (type_id, payload_slice) on success.
pub fn decode_header(data: &[u8]) -> Option<(u8, &[u8])> {
    if data.len() < 4 {
        return None;
    }

    let type_id = data[0];
    if !DocumentRegistry::is_app_type(type_id) {
        return None;
    }

    let _flags = data[1];
    let length = u16::from_le_bytes([data[2], data[3]]) as usize;

    if data.len() < 4 + length {
        return None;
    }

    Some((type_id, &data[4..4 + length]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct TestMessage {
        source_node: u32,
        timestamp: u64,
        content: String,
        ack_count: u32,
    }

    impl DocumentType for TestMessage {
        const TYPE_ID: u8 = 0xC0;
        const TYPE_NAME: &'static str = "TestMessage";

        fn identity(&self) -> (u32, u64) {
            (self.source_node, self.timestamp)
        }

        fn encode(&self) -> Vec<u8> {
            let mut buf = Vec::new();
            buf.extend_from_slice(&self.source_node.to_le_bytes());
            buf.extend_from_slice(&self.timestamp.to_le_bytes());
            buf.extend_from_slice(&self.ack_count.to_le_bytes());
            buf.push(self.content.len() as u8);
            buf.extend_from_slice(self.content.as_bytes());
            buf
        }

        fn decode(data: &[u8]) -> Option<Self> {
            if data.len() < 17 {
                return None;
            }
            let source_node = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            let timestamp = u64::from_le_bytes([
                data[4], data[5], data[6], data[7], data[8], data[9], data[10], data[11],
            ]);
            let ack_count = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
            let content_len = data[16] as usize;
            if data.len() < 17 + content_len {
                return None;
            }
            let content = String::from_utf8_lossy(&data[17..17 + content_len]).to_string();
            Some(Self {
                source_node,
                timestamp,
                content,
                ack_count,
            })
        }

        fn merge(&mut self, other: &Self) -> bool {
            if self.identity() != other.identity() {
                return false;
            }
            if other.ack_count > self.ack_count {
                self.ack_count = other.ack_count;
                return true;
            }
            false
        }

        fn to_delta_op(&self) -> Option<AppOperation> {
            Some(
                AppOperation::new(Self::TYPE_ID, 0x01, self.source_node, self.timestamp)
                    .with_payload(self.ack_count.to_le_bytes().to_vec()),
            )
        }
    }

    #[test]
    fn test_registry_register() {
        let registry = DocumentRegistry::new();
        registry.register::<TestMessage>();

        assert!(registry.is_registered(0xC0));
        assert!(!registry.is_registered(0xC1));
        assert_eq!(registry.type_name(0xC0), Some("TestMessage"));
    }

    #[test]
    fn test_registry_try_register() {
        let registry = DocumentRegistry::new();

        assert!(registry.try_register::<TestMessage>());
        assert!(!registry.try_register::<TestMessage>()); // Already registered
    }

    #[test]
    #[should_panic(expected = "outside valid range")]
    fn test_registry_invalid_type_id() {
        #[derive(Clone)]
        struct BadType;

        impl DocumentType for BadType {
            const TYPE_ID: u8 = 0xAB; // Invalid - not in 0xC0-0xCF
            const TYPE_NAME: &'static str = "BadType";

            fn identity(&self) -> (u32, u64) {
                (0, 0)
            }
            fn encode(&self) -> Vec<u8> {
                vec![]
            }
            fn decode(_: &[u8]) -> Option<Self> {
                None
            }
            fn merge(&mut self, _: &Self) -> bool {
                false
            }
        }

        let registry = DocumentRegistry::new();
        registry.register::<BadType>();
    }

    #[test]
    fn test_document_encode_decode() {
        let msg = TestMessage {
            source_node: 0x12345678,
            timestamp: 1000,
            content: "Hello".to_string(),
            ack_count: 5,
        };

        let encoded = msg.encode();
        let decoded = TestMessage::decode(&encoded).unwrap();

        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_document_merge() {
        let mut msg1 = TestMessage {
            source_node: 0x12345678,
            timestamp: 1000,
            content: "Hello".to_string(),
            ack_count: 5,
        };

        let msg2 = TestMessage {
            source_node: 0x12345678,
            timestamp: 1000,
            content: "Hello".to_string(),
            ack_count: 10,
        };

        assert!(msg1.merge(&msg2));
        assert_eq!(msg1.ack_count, 10);

        // Merging with lower count should not change
        let msg3 = TestMessage {
            source_node: 0x12345678,
            timestamp: 1000,
            content: "Hello".to_string(),
            ack_count: 3,
        };
        assert!(!msg1.merge(&msg3));
        assert_eq!(msg1.ack_count, 10);
    }

    #[test]
    fn test_registry_decode() {
        let registry = DocumentRegistry::new();
        registry.register::<TestMessage>();

        let msg = TestMessage {
            source_node: 0xAABBCCDD,
            timestamp: 2000,
            content: "Test".to_string(),
            ack_count: 7,
        };

        let encoded = msg.encode();
        let decoded = registry.decode(0xC0, &encoded).unwrap();
        let decoded_msg = decoded.downcast_ref::<TestMessage>().unwrap();

        assert_eq!(decoded_msg, &msg);
    }

    #[test]
    fn test_registry_merge() {
        let registry = DocumentRegistry::new();
        registry.register::<TestMessage>();

        let mut msg1 = TestMessage {
            source_node: 0x12345678,
            timestamp: 1000,
            content: "Hello".to_string(),
            ack_count: 5,
        };

        let msg2 = TestMessage {
            source_node: 0x12345678,
            timestamp: 1000,
            content: "Hello".to_string(),
            ack_count: 15,
        };

        let changed = registry.merge(0xC0, &mut msg1, &msg2);
        assert!(changed);
        assert_eq!(msg1.ack_count, 15);
    }

    #[test]
    fn test_app_operation_encode_decode() {
        let op = AppOperation::new(0xC0, 0x01, 0x12345678, 1000).with_payload(vec![1, 2, 3, 4]);

        let encoded = op.encode();
        let (decoded, size) = AppOperation::decode(&encoded).unwrap();

        assert_eq!(size, encoded.len());
        assert_eq!(decoded.type_id, 0xC0);
        assert_eq!(decoded.op_code, 0x01);
        assert_eq!(decoded.source_node, 0x12345678);
        assert_eq!(decoded.timestamp, 1000);
        assert_eq!(decoded.payload, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_encode_with_header() {
        let msg = TestMessage {
            source_node: 0x12345678,
            timestamp: 1000,
            content: "Hi".to_string(),
            ack_count: 3,
        };

        let encoded = encode_with_header(&msg);

        assert_eq!(encoded[0], 0xC0); // type_id
        assert_eq!(encoded[1], 0x00); // flags

        let (type_id, payload) = decode_header(&encoded).unwrap();
        assert_eq!(type_id, 0xC0);

        let decoded = TestMessage::decode(payload).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_is_app_type() {
        assert!(DocumentRegistry::is_app_type(0xC0));
        assert!(DocumentRegistry::is_app_type(0xCF));
        assert!(!DocumentRegistry::is_app_type(0xAB));
        assert!(!DocumentRegistry::is_app_type(0xD0));
    }

    #[test]
    fn test_is_app_op_type() {
        assert!(AppOperation::is_app_op_type(0x10));
        assert!(AppOperation::is_app_op_type(0x1F));
        assert!(!AppOperation::is_app_op_type(0x01));
        assert!(!AppOperation::is_app_op_type(0x20));
    }
}
