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

//! Android BLE connection stub
//!
//! This module provides a stub `AndroidConnection`. The actual BLE connections
//! are managed by the Kotlin EcheBtle class using Android BluetoothGatt APIs.
//!
//! On Android, connection lifecycle is:
//! 1. Kotlin EcheBtle discovers devices via BluetoothLeScanner
//! 2. Kotlin connects via BluetoothDevice.connectGatt()
//! 3. Kotlin calls EcheMesh.onBleConnected() via UniFFI
//! 4. Kotlin reads/writes GATT characteristics
//! 5. Kotlin calls EcheMesh.onBleDataReceived() via UniFFI
//! 6. On disconnect, Kotlin calls EcheMesh.onBleDisconnected() via UniFFI

use std::time::Duration;

use crate::config::BlePhy;
use crate::transport::BleConnection;
use crate::NodeId;

/// Android BLE connection stub
///
/// This is a placeholder that implements `BleConnection` but is never
/// instantiated in practice. All connection management happens in Kotlin.
#[derive(Clone)]
pub struct AndroidConnection {
    peer_id: NodeId,
}

impl AndroidConnection {
    /// Create a stub connection (for type compatibility only)
    #[allow(dead_code)]
    pub fn new_stub(peer_id: NodeId) -> Self {
        Self { peer_id }
    }
}

impl BleConnection for AndroidConnection {
    fn peer_id(&self) -> &NodeId {
        &self.peer_id
    }

    fn is_alive(&self) -> bool {
        false
    }

    fn mtu(&self) -> u16 {
        23 // Default BLE MTU
    }

    fn phy(&self) -> BlePhy {
        BlePhy::Le1M
    }

    fn rssi(&self) -> Option<i8> {
        None
    }

    fn connected_duration(&self) -> Duration {
        Duration::ZERO
    }
}
