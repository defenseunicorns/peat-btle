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

//! BLE PHY Configuration
//!
//! Configures physical layer parameters for BLE connections, including
//! support for LE Coded PHY for extended range operations.
//!
//! ## Overview
//!
//! BLE 5.0 introduced multiple PHY options allowing applications to choose
//! between range and throughput:
//!
//! | PHY | Data Rate | Range | Use Case |
//! |-----|-----------|-------|----------|
//! | LE 1M | 1 Mbps | ~100m | Default, good balance |
//! | LE 2M | 2 Mbps | ~50m | High throughput |
//! | LE Coded S=2 | 500 kbps | ~200m | Extended range |
//! | LE Coded S=8 | 125 kbps | ~400m | Maximum range |
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────┐
//! │                    Application                          │
//! │            (range/throughput requirements)              │
//! └─────────────────────┬──────────────────────────────────┘
//!                       │
//!                       ▼
//! ┌────────────────────────────────────────────────────────┐
//! │                 PhyController                           │
//! │  ┌──────────────┐  ┌────────────┐  ┌────────────────┐  │
//! │  │  PhyStrategy │──│   RSSI     │──│    PHY         │  │
//! │  │   (Adaptive) │  │  Monitor   │  │   Switch       │  │
//! │  └──────────────┘  └────────────┘  └────────────────┘  │
//! └─────────────────────┬──────────────────────────────────┘
//!                       │
//!                       ▼
//! ┌────────────────────────────────────────────────────────┐
//! │              BLE Controller (HCI)                       │
//! │         LE Set PHY / PHY Update Procedure              │
//! └────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ### Fixed PHY
//!
//! ```ignore
//! use peat_btle::phy::{PhyController, PhyStrategy, PhyCapabilities, BlePhy};
//!
//! let caps = PhyCapabilities::ble5_full();
//! let mut ctrl = PhyController::with_defaults(caps);
//!
//! // Force maximum range mode
//! ctrl.set_strategy(PhyStrategy::MaxRange);
//! ```
//!
//! ### Adaptive PHY
//!
//! ```ignore
//! use peat_btle::phy::{PhyController, PhyStrategy, PhyCapabilities};
//!
//! let caps = PhyCapabilities::ble5_full();
//! let mut ctrl = PhyController::with_defaults(caps);
//!
//! // Adaptive selection based on RSSI
//! ctrl.set_strategy(PhyStrategy::adaptive(-50, -75, 5));
//!
//! // Record RSSI samples
//! if let Some(event) = ctrl.record_rssi(rssi, current_time) {
//!     // Handle switch recommendation
//! }
//! ```
//!
//! ## Range Estimation
//!
//! Approximate relationship between RSSI and range for each PHY:
//!
//! | RSSI | LE 1M | LE 2M | Coded S=2 | Coded S=8 |
//! |------|-------|-------|-----------|-----------|
//! | -40 dBm | 10m | 5m | 20m | 40m |
//! | -60 dBm | 30m | 15m | 60m | 120m |
//! | -80 dBm | 80m | 40m | 160m | 320m |
//! | -90 dBm | -- | -- | 200m | 400m |
//!
//! ## Hardware Requirements
//!
//! - LE 2M: BLE 5.0 hardware
//! - LE Coded: BLE 5.0 hardware with Long Range support
//! - Not all BLE 5.0 devices support Coded PHY

pub mod controller;
pub mod strategy;
pub mod types;

pub use controller::{
    PhyController, PhyControllerConfig, PhyControllerEvent, PhyControllerState, PhyStats,
    PhyUpdateResult,
};
pub use strategy::{evaluate_phy_switch, PhyStrategy, PhySwitchDecision};
pub use types::{BlePhy, PhyCapabilities, PhyPreference};
