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

//! Linux/BlueZ platform implementation
//!
//! This module provides the BLE adapter implementation for Linux using
//! the `bluer` crate (BlueZ D-Bus bindings).
//!
//! ## Requirements
//!
//! - Linux with BlueZ 5.48+
//! - D-Bus system bus access
//! - Bluetooth adapter (built-in, USB dongle, etc.)
//!
//! ## Usage
//!
//! ```ignore
//! use peat_btle::platform::linux::BluerAdapter;
//! use peat_btle::{BleConfig, NodeId};
//!
//! let config = BleConfig::new(NodeId::new(0x12345678));
//! let mut adapter = BluerAdapter::new().await?;
//! adapter.init(&config).await?;
//! adapter.start().await?;
//! ```

mod adapter;
mod connection;

pub use adapter::BluerAdapter;
pub use connection::BluerConnection;
