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

//! Windows platform implementation (WinRT Bluetooth APIs)
//!
//! This module provides the BLE adapter implementation for Windows using
//! the WinRT Bluetooth APIs via the `windows` crate.
//!
//! ## Requirements
//!
//! - Windows 10 version 1703+ (Creators Update) for scanning/connecting
//! - Windows 10 version 1803+ (April 2018 Update) for GATT server
//! - BLE-capable Bluetooth adapter
//!
//! ## Architecture
//!
//! Windows BLE uses two separate APIs:
//! - **Advertisement API**: For scanning (`BluetoothLEAdvertisementWatcher`) and
//!   broadcasting (`BluetoothLEAdvertisementPublisher`)
//! - **GATT API**: For client/server operations (`GattDeviceService`, `GattServiceProvider`)
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │       WinRtBleAdapter (Rust)            │
//! ├─────────────────────────────────────────┤
//! │     Watcher      │      Publisher       │
//! │   (scanning)     │    (advertising)     │
//! ├─────────────────────────────────────────┤
//! │  GattClient      │    GattServer        │
//! │  (connecting,    │   (hosting Eche      │
//! │   reading)       │    service)          │
//! ├─────────────────────────────────────────┤
//! │           WinRT Bluetooth APIs          │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use eche_btle::platform::windows::WinRtBleAdapter;
//! use eche_btle::{BleConfig, NodeId};
//!
//! let config = BleConfig::new(NodeId::new(0x12345678));
//! let mut adapter = WinRtBleAdapter::new()?;
//! adapter.init(&config).await?;
//! adapter.start().await?;
//! ```
//!
//! ## Windows Version Notes
//!
//! | Feature | Minimum Version |
//! |---------|-----------------|
//! | BLE Scanning | Windows 10 1703 |
//! | BLE Advertising | Windows 10 1703 |
//! | GATT Client | Windows 10 1703 |
//! | GATT Server | Windows 10 1803 |
//! | Extended Advertising | Windows 10 1903 |

mod adapter;
mod advertiser;
mod connection;
mod gatt_server;
mod watcher;

pub use adapter::WinRtBleAdapter;
pub use connection::WinRtConnection;
