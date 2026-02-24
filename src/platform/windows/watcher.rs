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

//! BLE advertisement watcher for Windows
//!
//! Wraps `BluetoothLEAdvertisementWatcher` for scanning.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use windows::Devices::Bluetooth::Advertisement::{
    BluetoothLEAdvertisementReceivedEventArgs, BluetoothLEAdvertisementWatcher,
    BluetoothLEAdvertisementWatcherStatus, BluetoothLEScanningMode,
};
use windows::Foundation::{EventRegistrationToken, TypedEventHandler};

use crate::config::DiscoveryConfig;
use crate::discovery::EcheBeacon;
use crate::error::{BleError, Result};
use crate::platform::DiscoveredDevice;
use crate::NodeId;

/// Discovered peripheral with parsed Eche data
#[derive(Debug, Clone)]
pub struct DiscoveredPeripheral {
    /// Bluetooth address (as u64)
    pub address: u64,
    /// Address as string (XX:XX:XX:XX:XX:XX format)
    pub address_string: String,
    /// Device name (if available)
    pub name: Option<String>,
    /// RSSI in dBm
    pub rssi: i16,
    /// Is this a Eche node?
    pub is_eche_node: bool,
    /// Parsed Eche node ID (if Eche node)
    pub node_id: Option<NodeId>,
    /// Raw advertisement data
    pub adv_data: Vec<u8>,
    /// Timestamp of discovery (Windows FILETIME)
    pub timestamp: i64,
}

/// Internal state for the watcher
struct WatcherState {
    /// Known peripherals by address
    peripherals: HashMap<u64, DiscoveredPeripheral>,
    /// Eche peripherals (subset of peripherals)
    eche_peripherals: Vec<DiscoveredPeripheral>,
}

impl Default for WatcherState {
    fn default() -> Self {
        Self {
            peripherals: HashMap::new(),
            eche_peripherals: Vec::new(),
        }
    }
}

/// BLE scanner using Windows Advertisement Watcher
pub struct BleWatcher {
    /// The WinRT watcher
    watcher: BluetoothLEAdvertisementWatcher,
    /// Event registration token for Received events
    received_token: Option<EventRegistrationToken>,
    /// Internal state
    state: Arc<Mutex<WatcherState>>,
    /// Whether currently scanning
    is_scanning: bool,
}

impl BleWatcher {
    /// Create a new BLE watcher
    pub fn new() -> Result<Self> {
        let watcher = BluetoothLEAdvertisementWatcher::new()
            .map_err(|e| BleError::PlatformError(format!("Failed to create watcher: {}", e)))?;

        Ok(Self {
            watcher,
            received_token: None,
            state: Arc::new(Mutex::new(WatcherState::default())),
            is_scanning: false,
        })
    }

    /// Start scanning for BLE devices
    pub fn start_scan(&mut self, config: &DiscoveryConfig) -> Result<()> {
        if self.is_scanning {
            return Ok(());
        }

        // Configure scanning mode (active gets scan responses with device names)
        let mode = if config.active_scan {
            BluetoothLEScanningMode::Active
        } else {
            BluetoothLEScanningMode::Passive
        };

        self.watcher
            .SetScanningMode(mode)
            .map_err(|e| BleError::PlatformError(format!("Failed to set scanning mode: {}", e)))?;

        // Set up the Received event handler
        let state = self.state.clone();
        let handler = TypedEventHandler::new(
            move |_watcher: &Option<BluetoothLEAdvertisementWatcher>,
                  args: &Option<BluetoothLEAdvertisementReceivedEventArgs>| {
                if let Some(args) = args {
                    if let Err(e) = Self::handle_advertisement(&state, args) {
                        log::warn!("Error handling advertisement: {}", e);
                    }
                }
                Ok(())
            },
        );

        let token = self
            .watcher
            .Received(&handler)
            .map_err(|e| BleError::PlatformError(format!("Failed to register handler: {}", e)))?;
        self.received_token = Some(token);

        // Start the watcher
        self.watcher
            .Start()
            .map_err(|e| BleError::PlatformError(format!("Failed to start watcher: {}", e)))?;

        self.is_scanning = true;
        log::info!("BLE scanning started");

        Ok(())
    }

    /// Stop scanning
    pub fn stop_scan(&mut self) -> Result<()> {
        if !self.is_scanning {
            return Ok(());
        }

        // Stop the watcher
        self.watcher
            .Stop()
            .map_err(|e| BleError::PlatformError(format!("Failed to stop watcher: {}", e)))?;

        // Remove the event handler
        if let Some(token) = self.received_token.take() {
            let _ = self.watcher.RemoveReceived(token);
        }

        self.is_scanning = false;
        log::info!("BLE scanning stopped");

        Ok(())
    }

    /// Check if currently scanning
    pub fn is_scanning(&self) -> bool {
        self.is_scanning
    }

    /// Get the current watcher status
    pub fn status(&self) -> Result<BluetoothLEAdvertisementWatcherStatus> {
        self.watcher
            .Status()
            .map_err(|e| BleError::PlatformError(format!("Failed to get status: {}", e)))
    }

    /// Get discovered Eche peripherals
    pub fn get_eche_peripherals(&self) -> Vec<DiscoveredPeripheral> {
        if let Ok(state) = self.state.lock() {
            state.eche_peripherals.clone()
        } else {
            Vec::new()
        }
    }

    /// Get all discovered peripherals
    pub fn get_all_peripherals(&self) -> Vec<DiscoveredPeripheral> {
        if let Ok(state) = self.state.lock() {
            state.peripherals.values().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Clear discovered peripherals
    pub fn clear_peripherals(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.peripherals.clear();
            state.eche_peripherals.clear();
        }
    }

    /// Handle an advertisement event
    fn handle_advertisement(
        state: &Arc<Mutex<WatcherState>>,
        args: &BluetoothLEAdvertisementReceivedEventArgs,
    ) -> Result<()> {
        // Get device address
        let address = args
            .BluetoothAddress()
            .map_err(|e| BleError::PlatformError(format!("Failed to get address: {}", e)))?;

        // Format address as string
        let address_string = format!(
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            (address >> 40) & 0xFF,
            (address >> 32) & 0xFF,
            (address >> 24) & 0xFF,
            (address >> 16) & 0xFF,
            (address >> 8) & 0xFF,
            address & 0xFF
        );

        // Get RSSI
        let rssi = args
            .RawSignalStrengthInDBm()
            .map_err(|e| BleError::PlatformError(format!("Failed to get RSSI: {}", e)))?;

        // Get timestamp
        let timestamp = args
            .Timestamp()
            .map_err(|e| BleError::PlatformError(format!("Failed to get timestamp: {}", e)))?
            .UniversalTime;

        // Get advertisement data
        let advertisement = args
            .Advertisement()
            .map_err(|e| BleError::PlatformError(format!("Failed to get advertisement: {}", e)))?;

        // Try to get local name
        let name = advertisement.LocalName().ok().and_then(|s| {
            let s = s.to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        });

        // Get manufacturer data to check for Eche beacon
        let mut adv_data = Vec::new();
        let mut is_eche_node = false;
        let mut node_id = None;

        if let Ok(manufacturer_data) = advertisement.ManufacturerData() {
            if let Ok(size) = manufacturer_data.Size() {
                for i in 0..size {
                    if let Ok(data) = manufacturer_data.GetAt(i) {
                        if let Ok(company_id) = data.CompanyId() {
                            // Check for Eche company ID (we use 0xFFFF for development)
                            if company_id == 0xFFFF {
                                if let Ok(buffer) = data.Data() {
                                    if let Ok(reader) =
                                        windows::Storage::Streams::DataReader::FromBuffer(&buffer)
                                    {
                                        if let Ok(len) = reader.UnconsumedBufferLength() {
                                            let mut bytes = vec![0u8; len as usize];
                                            if reader.ReadBytes(&mut bytes).is_ok() {
                                                adv_data = bytes.clone();

                                                // Try to parse as Eche beacon
                                                if let Some(beacon) = EcheBeacon::decode(&bytes) {
                                                    is_eche_node = true;
                                                    node_id = Some(beacon.node_id);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Also check service UUIDs for Eche service
        if let Ok(service_uuids) = advertisement.ServiceUuids() {
            if let Ok(size) = service_uuids.Size() {
                for i in 0..size {
                    if let Ok(uuid) = service_uuids.GetAt(i) {
                        let uuid_str = format!("{:?}", uuid);
                        if uuid_str.contains("f47ac10b-58cc-4372-a567-0e02b2c3d479") {
                            is_eche_node = true;
                            break;
                        }
                    }
                }
            }
        }

        // Create peripheral record
        let peripheral = DiscoveredPeripheral {
            address,
            address_string,
            name,
            rssi,
            is_eche_node,
            node_id,
            adv_data,
            timestamp,
        };

        // Update state
        if let Ok(mut state) = state.lock() {
            state.peripherals.insert(address, peripheral.clone());

            if is_eche_node {
                // Update or add to Eche peripherals list
                if let Some(existing) = state
                    .eche_peripherals
                    .iter_mut()
                    .find(|p| p.address == address)
                {
                    *existing = peripheral;
                } else {
                    state.eche_peripherals.push(peripheral);
                }
            }
        }

        Ok(())
    }
}

impl Drop for BleWatcher {
    fn drop(&mut self) {
        let _ = self.stop_scan();
    }
}

/// Convert a DiscoveredPeripheral to the platform-agnostic DiscoveredDevice
impl From<DiscoveredPeripheral> for DiscoveredDevice {
    fn from(peripheral: DiscoveredPeripheral) -> Self {
        DiscoveredDevice {
            address: peripheral.address_string,
            name: peripheral.name,
            rssi: peripheral.rssi as i8,
            is_eche_node: peripheral.is_eche_node,
            node_id: peripheral.node_id,
            adv_data: peripheral.adv_data,
        }
    }
}
