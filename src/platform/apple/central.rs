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

//! CBCentralManager wrapper
//!
//! This module provides a Rust wrapper around CoreBluetooth's CBCentralManager,
//! which is used for scanning and connecting to BLE peripherals (GATT client role).

use std::collections::HashMap;
use std::sync::Arc;

use objc2::msg_send;
use objc2::rc::Retained;
use objc2_core_bluetooth::{CBCentralManager, CBPeripheral, CBUUID};
use objc2_foundation::{NSArray, NSString};
use tokio::sync::{mpsc, RwLock};

use crate::config::DiscoveryConfig;
use crate::error::{BleError, Result};
use crate::NodeId;

use super::delegates::{
    CentralEvent, CentralState, PeripheralEvent, RustCentralManagerDelegate, RustPeripheralDelegate,
};

/// Wrapper around CBCentralManager for BLE scanning and connecting
///
/// CBCentralManager is the central role in CoreBluetooth, used to:
/// - Scan for BLE peripherals
/// - Connect to peripherals
/// - Discover services and characteristics
/// - Read/write characteristic values
///
/// # Safety
/// This type is marked `Send + Sync` because CoreBluetooth callbacks are
/// dispatched on the main queue and the manager is only accessed from async
/// tasks that ensure proper synchronization. The underlying CBCentralManager
/// and CBPeripheralManager are not inherently thread-safe, but our usage pattern
/// (single async context + main queue dispatch) ensures safety.
pub struct CentralManager {
    /// The actual CBCentralManager instance
    manager: Retained<CBCentralManager>,
    /// The delegate that receives callbacks
    delegate: Retained<RustCentralManagerDelegate>,
    /// Current state of the central manager
    state: Arc<RwLock<CentralState>>,
    /// Channel receiver for delegate events
    event_rx: Arc<RwLock<mpsc::Receiver<CentralEvent>>>,
    /// Known peripherals by identifier
    peripherals: Arc<RwLock<HashMap<String, PeripheralInfo>>>,
    /// Whether scanning is active
    scanning: Arc<RwLock<bool>>,
    /// Peripheral delegates by identifier (one per connected peripheral)
    peripheral_delegates: Arc<RwLock<HashMap<String, Retained<RustPeripheralDelegate>>>>,
    /// Channel for peripheral events from all connected peripherals
    peripheral_event_tx: mpsc::Sender<PeripheralEvent>,
    /// Channel receiver for peripheral events
    peripheral_event_rx: Arc<RwLock<mpsc::Receiver<PeripheralEvent>>>,
}

/// Information about a discovered peripheral
#[derive(Debug, Clone)]
pub struct PeripheralInfo {
    /// Peripheral identifier (UUID)
    pub identifier: String,
    /// Advertised name
    pub name: Option<String>,
    /// Last seen RSSI
    pub rssi: i8,
    /// Is this a Peat node
    pub is_peat_node: bool,
    /// Node ID if Peat node
    pub node_id: Option<NodeId>,
    /// Whether currently connected
    pub connected: bool,
}

// SAFETY: CentralManager uses interior mutability via Arc<RwLock<_>> for all
// mutable state. The CBCentralManager is only accessed from the async context
// and its callbacks are dispatched on the main queue. We ensure that all
// access to the Objective-C objects goes through the proper synchronization.
unsafe impl Send for CentralManager {}
unsafe impl Sync for CentralManager {}

impl CentralManager {
    /// Create a new CentralManager
    ///
    /// This initializes the CBCentralManager with default options.
    /// The manager won't be ready until `state` becomes `PoweredOn`.
    pub fn new() -> Result<Self> {
        let (event_tx, event_rx) = mpsc::channel(100);
        let (peripheral_event_tx, peripheral_event_rx) = mpsc::channel(100);

        // Create delegate
        let delegate = RustCentralManagerDelegate::new(event_tx);

        // Create CBCentralManager and set delegate
        // Using main queue for callbacks
        let manager = unsafe { CBCentralManager::new() };
        unsafe {
            manager.setDelegate(Some(delegate.as_protocol()));
        }

        log::info!("CBCentralManager initialized");

        Ok(Self {
            manager,
            delegate,
            state: Arc::new(RwLock::new(CentralState::Unknown)),
            event_rx: Arc::new(RwLock::new(event_rx)),
            peripherals: Arc::new(RwLock::new(HashMap::new())),
            scanning: Arc::new(RwLock::new(false)),
            peripheral_delegates: Arc::new(RwLock::new(HashMap::new())),
            peripheral_event_tx,
            peripheral_event_rx: Arc::new(RwLock::new(peripheral_event_rx)),
        })
    }

    /// Get the current central manager state
    pub(super) async fn state(&self) -> CentralState {
        *self.state.read().await
    }

    /// Wait for the central manager to be ready (powered on)
    ///
    /// Returns an error if Bluetooth is unavailable or unauthorized.
    #[allow(dead_code)] // Useful for manual initialization flows
    pub(super) async fn wait_ready(&self) -> Result<()> {
        // Process events until state changes to a terminal state
        loop {
            self.process_events().await?;

            let state = self.state().await;
            match state {
                CentralState::PoweredOn => return Ok(()),
                CentralState::Unsupported => {
                    return Err(BleError::NotSupported(
                        "Bluetooth not supported".to_string(),
                    ))
                }
                CentralState::Unauthorized => {
                    return Err(BleError::PlatformError(
                        "Bluetooth not authorized".to_string(),
                    ))
                }
                CentralState::PoweredOff => {
                    return Err(BleError::PlatformError(
                        "Bluetooth is powered off".to_string(),
                    ))
                }
                CentralState::Unknown | CentralState::Resetting => {
                    // Wait a bit and try again
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// Start scanning for BLE peripherals
    ///
    /// # Arguments
    /// * `config` - Discovery configuration
    /// * `service_uuids` - Optional list of service UUIDs to filter by
    pub async fn start_scan(
        &self,
        _config: &DiscoveryConfig,
        service_uuids: Option<Vec<String>>,
    ) -> Result<()> {
        // Track filter count for logging (before we drop the non-Send types)
        let filter_count = service_uuids.as_ref().map(|v| v.len());

        // Build service UUID filter array if provided and start scanning
        // All ObjC work must complete before any await points (CBUUID is not Send)
        {
            let uuid_filter: Option<Retained<NSArray<CBUUID>>> =
                service_uuids.map(|uuids| unsafe {
                    let cb_uuids: Vec<Retained<CBUUID>> = uuids
                        .iter()
                        .map(|uuid_str| {
                            let ns_str = NSString::from_str(uuid_str);
                            CBUUID::UUIDWithString(&ns_str)
                        })
                        .collect();

                    // Convert to array with proper retain semantics
                    let uuid_refs: Vec<Retained<CBUUID>> = cb_uuids
                        .into_iter()
                        .map(|uuid| {
                            let ptr: *mut CBUUID = msg_send![Retained::as_ptr(&uuid), retain];
                            Retained::from_raw(ptr).unwrap()
                        })
                        .collect();
                    NSArray::from_vec(uuid_refs)
                });

            // Start scanning with service UUID filter (if provided)
            // TODO: Create proper options dictionary based on config for allow duplicates etc.
            unsafe {
                self.manager
                    .scanForPeripheralsWithServices_options(uuid_filter.as_deref(), None);
            }
        }
        // uuid_filter is dropped here, before the await

        let filter_desc = filter_count
            .map(|count| format!("filtering by {} service UUID(s)", count))
            .unwrap_or_else(|| "no filter".to_string());
        log::info!("Started BLE scanning ({})", filter_desc);
        *self.scanning.write().await = true;
        Ok(())
    }

    /// Stop scanning for peripherals
    pub async fn stop_scan(&self) -> Result<()> {
        unsafe {
            self.manager.stopScan();
        }

        log::info!("Stopped BLE scanning");
        *self.scanning.write().await = false;
        Ok(())
    }

    /// Check if currently scanning
    #[allow(dead_code)] // Useful for state inspection
    pub(super) async fn is_scanning(&self) -> bool {
        *self.scanning.read().await
    }

    /// Connect to a peripheral by identifier
    ///
    /// # Arguments
    /// * `identifier` - The peripheral's UUID identifier
    pub(super) async fn connect(&self, identifier: &str) -> Result<()> {
        // Get the CBPeripheral from delegate's storage
        let peripheral = self.delegate.get_peripheral(identifier).ok_or_else(|| {
            BleError::ConnectionFailed(format!("Unknown peripheral: {}", identifier))
        })?;

        // Connect without specific options
        unsafe {
            self.manager.connectPeripheral_options(&peripheral, None);
        }

        log::info!("Connecting to peripheral: {}", identifier);
        Ok(())
    }

    /// Disconnect from a peripheral
    pub(super) async fn disconnect(&self, identifier: &str) -> Result<()> {
        // Get the CBPeripheral from delegate's storage
        if let Some(peripheral) = self.delegate.get_peripheral(identifier) {
            unsafe {
                self.manager.cancelPeripheralConnection(&peripheral);
            }
            log::info!("Disconnecting from peripheral: {}", identifier);
        }

        Ok(())
    }

    /// Get the CBPeripheral for an identifier
    #[allow(dead_code)] // Useful for low-level CoreBluetooth access
    pub(super) fn get_cb_peripheral(&self, identifier: &str) -> Option<Retained<CBPeripheral>> {
        self.delegate.get_peripheral(identifier)
    }

    /// Discover services on a connected peripheral
    ///
    /// # Arguments
    /// * `identifier` - The peripheral's UUID identifier
    /// * `service_uuids` - Optional list of service UUIDs to discover (None = all)
    pub(super) async fn discover_services(
        &self,
        identifier: &str,
        service_uuids: Option<&[&str]>,
    ) -> Result<()> {
        let peripheral = self.delegate.get_peripheral(identifier).ok_or_else(|| {
            BleError::ConnectionFailed(format!("Unknown peripheral: {}", identifier))
        })?;

        unsafe {
            // Create UUID array if specific UUIDs requested
            let uuids: Option<Retained<NSArray<CBUUID>>> = service_uuids.map(|uuids| {
                let uuid_objects: Vec<_> = uuids
                    .iter()
                    .map(|uuid_str| CBUUID::UUIDWithString(&NSString::from_str(uuid_str)))
                    .collect();
                NSArray::from_vec(uuid_objects)
            });

            // Call discoverServices
            peripheral.discoverServices(uuids.as_deref());
        }

        log::info!("Discovering services on peripheral: {}", identifier);
        Ok(())
    }

    /// Discover characteristics for a service on a connected peripheral
    pub(super) async fn discover_characteristics(
        &self,
        identifier: &str,
        service_uuid: &str,
    ) -> Result<()> {
        let peripheral = self.delegate.get_peripheral(identifier).ok_or_else(|| {
            BleError::ConnectionFailed(format!("Unknown peripheral: {}", identifier))
        })?;

        let target_uuid_upper = service_uuid.to_uppercase();

        unsafe {
            let services = peripheral.services();

            if let Some(services) = services {
                for i in 0..services.len() {
                    let service = &services[i];
                    let service_uuid_str = service.UUID().UUIDString().to_string().to_uppercase();
                    if service_uuid_str == target_uuid_upper {
                        // Found the service, discover all characteristics
                        peripheral.discoverCharacteristics_forService(None, service);
                        log::info!(
                            "Discovering characteristics for service {} on {}",
                            service_uuid,
                            identifier
                        );
                        return Ok(());
                    }
                }
            }
        }

        Err(BleError::ConnectionFailed(format!(
            "Service {} not found on {}",
            service_uuid, identifier
        )))
    }

    /// Read a characteristic value from a connected peripheral
    pub(super) async fn read_characteristic(
        &self,
        identifier: &str,
        service_uuid: &str,
        characteristic_uuid: &str,
    ) -> Result<()> {
        let peripheral = self.delegate.get_peripheral(identifier).ok_or_else(|| {
            BleError::ConnectionFailed(format!("Unknown peripheral: {}", identifier))
        })?;

        let target_service_upper = service_uuid.to_uppercase();
        let target_char_upper = characteristic_uuid.to_uppercase();

        unsafe {
            if let Some(services) = peripheral.services() {
                for i in 0..services.len() {
                    let service = &services[i];
                    let svc_uuid = service.UUID().UUIDString().to_string().to_uppercase();
                    if svc_uuid == target_service_upper || svc_uuid.contains(&target_service_upper)
                    {
                        if let Some(characteristics) = service.characteristics() {
                            // Log available characteristics
                            let char_list: Vec<String> = (0..characteristics.len())
                                .map(|j| characteristics[j].UUID().UUIDString().to_string())
                                .collect();
                            log::debug!(
                                "Available characteristics on {}: {:?}",
                                identifier,
                                char_list
                            );

                            for j in 0..characteristics.len() {
                                let characteristic = &characteristics[j];
                                let char_uuid = characteristic
                                    .UUID()
                                    .UUIDString()
                                    .to_string()
                                    .to_uppercase();
                                // Match by exact UUID or if the characteristic UUID contains our target
                                if char_uuid == target_char_upper
                                    || char_uuid.contains(&target_char_upper)
                                {
                                    peripheral.readValueForCharacteristic(characteristic);
                                    log::debug!(
                                        "Reading characteristic {} (matched {}) from {}",
                                        char_uuid,
                                        characteristic_uuid,
                                        identifier
                                    );
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(BleError::ConnectionFailed(format!(
            "Characteristic {} not found",
            characteristic_uuid
        )))
    }

    /// Write a value to a characteristic on a connected peripheral
    pub(super) async fn write_characteristic(
        &self,
        identifier: &str,
        service_uuid: &str,
        characteristic_uuid: &str,
        data: &[u8],
        with_response: bool,
    ) -> Result<()> {
        use objc2_foundation::NSData;

        let peripheral = self.delegate.get_peripheral(identifier).ok_or_else(|| {
            BleError::ConnectionFailed(format!("Unknown peripheral: {}", identifier))
        })?;

        let target_service_upper = service_uuid.to_uppercase();
        let target_char_upper = characteristic_uuid.to_uppercase();

        unsafe {
            if let Some(services) = peripheral.services() {
                for i in 0..services.len() {
                    let service = &services[i];
                    let svc_uuid = service.UUID().UUIDString().to_string().to_uppercase();
                    if svc_uuid == target_service_upper {
                        if let Some(characteristics) = service.characteristics() {
                            for j in 0..characteristics.len() {
                                let characteristic = &characteristics[j];
                                let char_uuid = characteristic
                                    .UUID()
                                    .UUIDString()
                                    .to_string()
                                    .to_uppercase();
                                if char_uuid == target_char_upper {
                                    let ns_data = NSData::with_bytes(data);
                                    // CBCharacteristicWriteType: 0 = WithResponse, 1 = WithoutResponse
                                    let write_type: isize = if with_response { 0 } else { 1 };
                                    let _: () = msg_send![&*peripheral, writeValue: &*ns_data forCharacteristic: &**characteristic type: write_type];
                                    log::debug!(
                                        "Writing {} bytes to characteristic {} on {}",
                                        data.len(),
                                        characteristic_uuid,
                                        identifier
                                    );
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(BleError::ConnectionFailed(format!(
            "Characteristic {} not found",
            characteristic_uuid
        )))
    }

    /// Get the next peripheral event if available
    pub(super) async fn try_recv_peripheral_event(&self) -> Option<PeripheralEvent> {
        let mut rx = self.peripheral_event_rx.write().await;
        rx.try_recv().ok()
    }

    /// Set up a peripheral delegate for a connected peripheral
    ///
    /// Note: This uses try_write to avoid holding the delegate across await points
    /// (RustPeripheralDelegate is not Send)
    pub(super) fn setup_peripheral_delegate(&self, identifier: &str) -> Result<()> {
        let peripheral = self.delegate.get_peripheral(identifier).ok_or_else(|| {
            BleError::ConnectionFailed(format!("Unknown peripheral: {}", identifier))
        })?;

        // Create a new delegate for this peripheral
        let delegate = RustPeripheralDelegate::new(self.peripheral_event_tx.clone());

        // Set the delegate on the peripheral
        unsafe {
            peripheral.setDelegate(Some(delegate.as_protocol()));
        }

        // Store the delegate to keep it alive (use try_write to avoid blocking)
        if let Ok(mut delegates) = self.peripheral_delegates.try_write() {
            delegates.insert(identifier.to_string(), delegate);
            log::debug!("Set up peripheral delegate for {}", identifier);
            Ok(())
        } else {
            // If we can't get the lock, the delegate won't be stored but
            // it's still set on the peripheral - this is a minor issue
            log::warn!(
                "Could not store peripheral delegate for {} (lock contention)",
                identifier
            );
            Ok(())
        }
    }

    /// Get information about a discovered peripheral
    #[allow(dead_code)] // Useful for debugging discovery
    pub(super) async fn get_peripheral(&self, identifier: &str) -> Option<PeripheralInfo> {
        let peripherals = self.peripherals.read().await;
        peripherals.get(identifier).cloned()
    }

    /// Get all discovered peripherals
    #[allow(dead_code)] // Useful for debugging discovery
    pub(super) async fn get_discovered_peripherals(&self) -> Vec<PeripheralInfo> {
        let peripherals = self.peripherals.read().await;
        peripherals.values().cloned().collect()
    }

    /// Get all Peat node peripherals
    pub(super) async fn get_peat_peripherals(&self) -> Vec<PeripheralInfo> {
        let peripherals = self.peripherals.read().await;
        peripherals
            .values()
            .filter(|p| p.is_peat_node)
            .cloned()
            .collect()
    }

    /// Process pending delegate events
    ///
    /// Call this periodically to update internal state from delegate callbacks.
    /// This also pumps the Objective-C run loop to ensure CoreBluetooth callbacks
    /// are delivered.
    pub(super) async fn process_events(&self) -> Result<()> {
        // Pump the Objective-C run loop to deliver pending CoreBluetooth callbacks
        // CoreBluetooth callbacks are dispatched on the main queue, which requires
        // the run loop to be running for delivery.
        unsafe {
            use objc2_foundation::NSRunLoop;
            let run_loop = NSRunLoop::mainRunLoop();
            // Run for a brief moment to process pending events
            // NSDefaultRunLoopMode is the standard mode
            let mode = objc2_foundation::NSDefaultRunLoopMode;
            let date = objc2_foundation::NSDate::dateWithTimeIntervalSinceNow(0.001);
            run_loop.runMode_beforeDate(mode, &date);
        }

        let mut event_rx = self.event_rx.write().await;

        while let Ok(event) = event_rx.try_recv() {
            match event {
                CentralEvent::StateChanged(state) => {
                    log::debug!("Central state changed: {:?}", state);
                    *self.state.write().await = state;
                }
                CentralEvent::DiscoveredPeripheral {
                    identifier,
                    name,
                    rssi,
                    is_peat_node,
                    node_id,
                    ..
                } => {
                    let mut peripherals = self.peripherals.write().await;
                    peripherals.insert(
                        identifier.clone(),
                        PeripheralInfo {
                            identifier,
                            name,
                            rssi,
                            is_peat_node,
                            node_id,
                            connected: false,
                        },
                    );
                }
                CentralEvent::Connected { identifier } => {
                    log::info!("Connected to peripheral: {}", identifier);

                    // Set up peripheral delegate for GATT operations
                    if let Err(e) = self.setup_peripheral_delegate(&identifier) {
                        log::warn!("Failed to set up peripheral delegate: {}", e);
                    }

                    let mut peripherals = self.peripherals.write().await;
                    if let Some(peripheral) = peripherals.get_mut(&identifier) {
                        peripheral.connected = true;
                    }
                }
                CentralEvent::Disconnected { identifier, .. } => {
                    log::info!("Disconnected from peripheral: {}", identifier);
                    let mut peripherals = self.peripherals.write().await;
                    if let Some(peripheral) = peripherals.get_mut(&identifier) {
                        peripheral.connected = false;
                    }
                }
                CentralEvent::ConnectionFailed { identifier, error } => {
                    log::warn!("Connection to {} failed: {}", identifier, error);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peripheral_info() {
        let info = PeripheralInfo {
            identifier: "12345678-1234-1234-1234-123456789ABC".to_string(),
            name: Some("PEAT-DEADBEEF".to_string()),
            rssi: -65,
            is_peat_node: true,
            node_id: Some(NodeId::new(0xDEADBEEF)),
            connected: false,
        };

        assert!(info.is_peat_node);
        assert!(!info.connected);
        assert_eq!(info.rssi, -65);
    }
}
