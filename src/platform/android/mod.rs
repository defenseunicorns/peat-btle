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
//! This module provides stubs for the Android BLE adapter. The actual BLE
//! operations are handled by the Kotlin PeatBtle class using Android APIs,
//! with mesh logic provided by UniFFI bindings to Rust PeatMesh.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │        Kotlin PeatBtle (Android BLE)    │
//! ├─────────────────────────────────────────┤
//! │   UniFFI Bindings (uniffi.peat_btle)   │
//! ├─────────────────────────────────────────┤
//! │           Rust PeatMesh Core            │
//! └─────────────────────────────────────────┘
//! ```
//!
//! The Kotlin layer handles:
//! - BLE scanning and advertising
//! - GATT client/server operations
//! - Android permission management
//!
//! The Rust layer (via UniFFI) handles:
//! - Mesh state management
//! - CRDT document sync
//! - Encryption/decryption
//! - Peer management

mod adapter;
mod connection;

pub use adapter::AndroidAdapter;
pub use connection::AndroidConnection;
