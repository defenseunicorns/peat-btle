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


//! Android platform implementation
//!
//! This module provides the BLE adapter implementation for Android using
//! JNI bindings to the Android Bluetooth API.
//!
//! ## Requirements
//!
//! - Android 6.0 (API 23) or later
//! - `BLUETOOTH`, `BLUETOOTH_ADMIN`, `ACCESS_FINE_LOCATION` permissions
//! - For BLE 5.0 features: Android 8.0 (API 26) or later
//!
//! ## Architecture
//!
//! The Android implementation uses JNI to call Android Bluetooth APIs:
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │           AndroidAdapter (Rust)          │
//! ├─────────────────────────────────────────┤
//! │                JNI Bridge                │
//! ├─────────────────────────────────────────┤
//! │  BluetoothAdapter / BluetoothLeScanner  │
//! │  BluetoothLeAdvertiser / BluetoothGatt  │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use hive_btle::platform::android::AndroidAdapter;
//! use hive_btle::{BleConfig, NodeId};
//!
//! // Must be called from Android app with JNI environment
//! let config = BleConfig::new(NodeId::new(0x12345678));
//! let mut adapter = AndroidAdapter::new(jni_env, context)?;
//! adapter.init(&config).await?;
//! adapter.start().await?;
//! ```
//!
//! ## JNI Callbacks
//!
//! The implementation registers JNI callbacks for:
//! - Scan results (`onScanResult`)
//! - Connection state changes (`onConnectionStateChange`)
//! - GATT service discovery (`onServicesDiscovered`)
//! - Characteristic reads/writes (`onCharacteristicRead`, `onCharacteristicWrite`)
//! - Notifications (`onCharacteristicChanged`)

mod adapter;
mod connection;
mod jni_bridge;

pub use adapter::AndroidAdapter;
pub use connection::AndroidConnection;
