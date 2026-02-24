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

//! Eche GATT Service Module
//!
//! Provides the GATT service implementation for Eche Protocol BLE communication.
//!
//! ## Service Structure
//!
//! ```text
//! Eche GATT Service (UUID: f47ac10b-58cc-4372-a567-0e02b2c3d479)
//! ├── Node Info (read)           - Basic node information
//! ├── Sync State (read/notify)   - Current sync status
//! ├── Sync Data (write/indicate) - Sync data transfer
//! ├── Command (write)            - Control commands
//! └── Status (read/notify)       - Node status updates
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use eche_btle::gatt::{EcheGattService, EcheCharacteristics};
//! use eche_btle::{NodeId, HierarchyLevel};
//!
//! // Create the GATT service
//! let service = EcheGattService::new(
//!     NodeId::new(0x12345678),
//!     HierarchyLevel::Platform,
//!     0, // capabilities
//! );
//!
//! // Get characteristic descriptors for registration
//! let chars = service.characteristics();
//!
//! // Handle reads
//! let node_info = service.read_node_info().await;
//!
//! // Handle writes
//! service.write_command(&command_data).await?;
//! ```
//!
//! ## Sync Protocol
//!
//! The sync protocol uses fragmentation for large messages:
//!
//! ```ignore
//! use eche_btle::gatt::SyncProtocol;
//!
//! let mut protocol = SyncProtocol::new();
//! protocol.set_mtu(251); // Use negotiated MTU
//!
//! // Start sync with local sync vector
//! protocol.start_sync(sync_vector);
//!
//! // Send outgoing messages
//! while let Some(msg) = protocol.next_outgoing() {
//!     // Write msg.encode() to Sync Data characteristic
//! }
//!
//! // Process incoming messages
//! if let Some((msg_type, payload)) = protocol.process_incoming(&data) {
//!     // Handle received message
//! }
//! ```

mod characteristics;
mod protocol;
#[cfg(feature = "std")]
mod service;

pub use characteristics::{
    CharacteristicProperties, Command, CommandType, EcheCharacteristicUuids, NodeInfo, StatusData,
    StatusFlags, SyncDataHeader, SyncDataOp, SyncState, SyncStateData,
};
pub use protocol::{
    fragment_payload, max_payload_size, FragmentReassembler, SyncMessage, SyncMessageType,
    SyncProtocol, SyncProtocolState, DEFAULT_MAX_PAYLOAD,
};
#[cfg(feature = "std")]
pub use service::{
    CharacteristicDescriptor, GattEvent, GattEventCallback, EcheCharacteristics, EcheGattService,
};
