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

//! Peat Discovery Module
//!
//! This module implements BLE discovery for Peat mesh networks, including:
//! - Beacon format encoding/decoding
//! - Advertising for broadcasting presence
//! - Scanning for discovering peers
//!
//! ## Discovery Flow
//!
//! 1. **Advertising**: Nodes broadcast their presence using Peat beacons
//!    containing node ID, hierarchy level, capabilities, and battery status.
//!
//! 2. **Scanning**: Nodes scan for Peat beacons, filtering by hierarchy level
//!    and signal strength to find potential parents.
//!
//! 3. **Parent Selection**: The scanner tracks discovered devices and selects
//!    the best parent candidate based on hierarchy level and RSSI.
//!
//! ## Example
//!
//! ```ignore
//! use peat_btle::discovery::{Advertiser, Scanner, ScanFilter};
//! use peat_btle::{NodeId, HierarchyLevel};
//! use peat_btle::config::DiscoveryConfig;
//!
//! // Create advertiser
//! let config = DiscoveryConfig::default();
//! let mut advertiser = Advertiser::new(config.clone(), NodeId::new(0x12345678))
//!     .with_name("PEAT-Node".to_string());
//!
//! advertiser.set_hierarchy_level(HierarchyLevel::Squad);
//! advertiser.start();
//!
//! // Create scanner
//! let mut scanner = Scanner::new(config);
//! scanner.set_filter(ScanFilter::potential_parents(HierarchyLevel::Platform));
//! scanner.start();
//! ```

mod advertiser;
mod beacon;
mod encrypted_beacon;
mod scanner;

pub use advertiser::{Advertiser, AdvertiserState, AdvertisingMode, AdvertisingPacket};
pub use beacon::{
    ParsedAdvertisement, PeatBeacon, BEACON_COMPACT_SIZE, BEACON_SIZE, BEACON_VERSION,
};
pub use encrypted_beacon::{
    mesh_id_to_bytes, BeaconKey, EncryptedBeacon, ENCRYPTED_BEACON_SIZE, ENCRYPTED_BEACON_VERSION,
    ENCRYPTED_DEVICE_NAME,
};
#[cfg(feature = "std")]
pub use scanner::Scanner;
pub use scanner::{ScanFilter, ScannerState, TrackedDevice};
