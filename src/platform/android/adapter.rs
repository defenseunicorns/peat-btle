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

//! Android BLE adapter stub
//!
//! This module provides a stub `AndroidAdapter`. The actual BLE operations
//! are handled by the Kotlin PeatBtle class using Android Bluetooth APIs.
//! Mesh logic is provided via UniFFI bindings to Rust PeatMesh.
//!
//! ## Architecture
//!
//! The Android implementation uses a "Kotlin-first" approach:
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │     Kotlin PeatBtle (Android BLE)       │
//! │   - BLE scanning and advertising        │
//! │   - GATT client/server operations       │
//! │   - Android permission management       │
//! ├─────────────────────────────────────────┤
//! │   UniFFI Bindings (uniffi.peat_btle)   │
//! ├─────────────────────────────────────────┤
//! │          Rust PeatMesh Core             │
//! │   - Mesh state management               │
//! │   - CRDT document sync                  │
//! │   - Encryption/decryption               │
//! │   - Peer management                     │
//! └─────────────────────────────────────────┘
//! ```
//!
//! This stub exists to satisfy the platform module structure but is not
//! used at runtime. All BLE operations go through Kotlin -> UniFFI -> PeatMesh.

use async_trait::async_trait;

#[allow(unused_imports)]
use crate::config::{BleConfig, BlePhy, DiscoveryConfig};
use crate::error::{BleError, Result};
use crate::platform::{BleAdapter, ConnectionCallback, DiscoveryCallback};
use crate::transport::BleConnection;
use crate::NodeId;

use super::connection::AndroidConnection;

/// Android BLE adapter stub
///
/// This is a placeholder implementation. On Android, BLE operations are
/// handled entirely by the Kotlin PeatBtle class. The Rust PeatMesh
/// is accessed via UniFFI bindings for mesh logic only.
///
/// See the Kotlin `PeatBtle` class for the actual Android BLE implementation.
pub struct AndroidAdapter {
    _private: (),
}

impl AndroidAdapter {
    /// This adapter is not meant to be instantiated from Rust.
    ///
    /// On Android, use the Kotlin PeatBtle class instead, which accesses
    /// PeatMesh via UniFFI bindings.
    pub fn new_stub() -> Self {
        Self { _private: () }
    }
}

#[async_trait]
impl BleAdapter for AndroidAdapter {
    async fn init(&mut self, _config: &BleConfig) -> Result<()> {
        // No-op: Kotlin handles actual BLE initialization
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        // No-op: Kotlin handles actual BLE start
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        // No-op: Kotlin handles actual BLE stop
        Ok(())
    }

    fn is_powered(&self) -> bool {
        // Assume powered — Kotlin manages actual power state
        true
    }

    fn address(&self) -> Option<String> {
        None
    }

    async fn start_scan(&self, _config: &DiscoveryConfig) -> Result<()> {
        Err(BleError::NotSupported(
            "Use Kotlin PeatBtle for Android BLE".to_string(),
        ))
    }

    async fn stop_scan(&self) -> Result<()> {
        Err(BleError::NotSupported(
            "Use Kotlin PeatBtle for Android BLE".to_string(),
        ))
    }

    async fn start_advertising(&self, _config: &DiscoveryConfig) -> Result<()> {
        Err(BleError::NotSupported(
            "Use Kotlin PeatBtle for Android BLE".to_string(),
        ))
    }

    async fn stop_advertising(&self) -> Result<()> {
        Err(BleError::NotSupported(
            "Use Kotlin PeatBtle for Android BLE".to_string(),
        ))
    }

    fn set_discovery_callback(&mut self, _callback: Option<DiscoveryCallback>) {
        // No-op: Discovery handled by Kotlin
    }

    async fn connect(&self, _peer_id: &NodeId) -> Result<Box<dyn BleConnection>> {
        Err(BleError::NotSupported(
            "Use Kotlin PeatBtle for Android BLE".to_string(),
        ))
    }

    async fn disconnect(&self, _peer_id: &NodeId) -> Result<()> {
        Err(BleError::NotSupported(
            "Use Kotlin PeatBtle for Android BLE".to_string(),
        ))
    }

    fn get_connection(&self, _peer_id: &NodeId) -> Option<Box<dyn BleConnection>> {
        None
    }

    fn peer_count(&self) -> usize {
        0
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        Vec::new()
    }

    fn set_connection_callback(&mut self, _callback: Option<ConnectionCallback>) {
        // No-op: Connection events handled by Kotlin
    }

    async fn register_gatt_service(&self) -> Result<()> {
        Err(BleError::NotSupported(
            "Use Kotlin PeatBtle for Android BLE".to_string(),
        ))
    }

    async fn unregister_gatt_service(&self) -> Result<()> {
        Err(BleError::NotSupported(
            "Use Kotlin PeatBtle for Android BLE".to_string(),
        ))
    }

    fn supports_coded_phy(&self) -> bool {
        false
    }

    fn supports_extended_advertising(&self) -> bool {
        false
    }

    fn max_mtu(&self) -> u16 {
        517
    }

    fn max_connections(&self) -> u8 {
        7
    }
}

// AndroidConnection is not used but we keep a type alias for compatibility
#[allow(dead_code)]
type Connection = AndroidConnection;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_android_adapter_lifecycle() {
        let mut adapter = AndroidAdapter::new_stub();
        let config = BleConfig::default();
        assert!(adapter.init(&config).await.is_ok());
        assert!(adapter.start().await.is_ok());
        assert!(adapter.is_powered());
        assert!(adapter.stop().await.is_ok());
    }
}
