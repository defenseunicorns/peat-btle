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

//! CoreBluetooth connection wrapper
//!
//! This module provides a connection wrapper for CoreBluetooth peripherals,
//! implementing the `BleConnection` trait.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

use objc2::msg_send;
use objc2::rc::Retained;
use objc2::ClassType;
use objc2_core_bluetooth::{CBPeripheral, CBUUID};
use objc2_foundation::{NSArray, NSData, NSString};

use crate::config::BlePhy;
use crate::error::{BleError, Result};
use crate::transport::BleConnection;
use crate::NodeId;

use super::delegates::{PeripheralDelegate, PeripheralEvent};

/// Internal connection state
struct ConnectionState {
    /// Whether the connection is alive
    alive: bool,
    /// Negotiated MTU
    mtu: u16,
    /// Current PHY (CoreBluetooth doesn't expose PHY, assume 1M)
    phy: BlePhy,
    /// Last RSSI reading
    rssi: Option<i8>,
    /// Whether services have been discovered
    services_discovered: bool,
}

/// CoreBluetooth connection wrapper
///
/// Wraps a CBPeripheral connection with state tracking and
/// implements the `BleConnection` trait.
#[derive(Clone)]
pub struct CoreBluetoothConnection {
    /// Remote peer ID
    peer_id: NodeId,
    /// Peripheral identifier (UUID string)
    identifier: String,
    /// Connection state
    state: Arc<RwLock<ConnectionState>>,
    /// When the connection was established
    connected_at: Instant,
    /// Channel receiver for peripheral events
    event_rx: Arc<RwLock<mpsc::Receiver<PeripheralEvent>>>,
    /// Peripheral delegate (must be kept alive)
    delegate: Arc<PeripheralDelegate>,
    /// The CBPeripheral reference (stored via std RwLock since Retained is not Send)
    cb_peripheral: Arc<std::sync::RwLock<Option<Retained<CBPeripheral>>>>,
}

// SAFETY: CoreBluetoothConnection uses interior mutability via Arc<RwLock<_>> for all
// mutable state. The CBPeripheral is protected behind std::sync::RwLock and is only
// accessed synchronously before await points.
unsafe impl Send for CoreBluetoothConnection {}
unsafe impl Sync for CoreBluetoothConnection {}

impl CoreBluetoothConnection {
    /// Create a new connection wrapper
    ///
    /// # Arguments
    /// * `peer_id` - Peat node ID of the remote peer
    /// * `identifier` - CoreBluetooth peripheral identifier (UUID)
    pub(super) fn new(peer_id: NodeId, identifier: String) -> Self {
        let (event_tx, event_rx) = mpsc::channel(100);
        let delegate = Arc::new(PeripheralDelegate::new(event_tx));

        let state = ConnectionState {
            alive: true,
            mtu: 23,           // Default BLE MTU, will be updated after connection
            phy: BlePhy::Le1M, // CoreBluetooth doesn't expose PHY selection
            rssi: None,
            services_discovered: false,
        };

        Self {
            peer_id,
            identifier,
            state: Arc::new(RwLock::new(state)),
            connected_at: Instant::now(),
            event_rx: Arc::new(RwLock::new(event_rx)),
            delegate,
            cb_peripheral: Arc::new(std::sync::RwLock::new(None)),
        }
    }

    /// Set the CBPeripheral reference for this connection.
    ///
    /// Must be called after construction to enable GATT operations.
    /// The peripheral's delegate is set to receive callbacks through
    /// our event channel.
    pub(super) fn set_cb_peripheral(&self, peripheral: Retained<CBPeripheral>) {
        let mut lock = self.cb_peripheral.write().unwrap();
        *lock = Some(peripheral);
    }

    /// Get the CBPeripheral reference, if set
    fn get_cb_peripheral(&self) -> Result<Retained<CBPeripheral>> {
        let lock = self.cb_peripheral.read().unwrap();
        lock.clone()
            .ok_or_else(|| BleError::InvalidState("CBPeripheral not set on connection".to_string()))
    }

    /// Get the peripheral identifier
    #[allow(dead_code)] // Useful for debugging
    pub(super) fn identifier(&self) -> &str {
        &self.identifier
    }

    /// Update connection state from delegate callback
    #[allow(dead_code)] // Will be called from delegate events
    pub(super) async fn update_connection_state(&self, connected: bool) {
        let mut state = self.state.write().await;
        state.alive = connected;
    }

    /// Update MTU (called after MTU exchange completes)
    #[allow(dead_code)] // Will be called from delegate events
    pub(super) async fn update_mtu(&self, mtu: u16) {
        let mut state = self.state.write().await;
        state.mtu = mtu;
        log::debug!("MTU updated to {} for peer {}", mtu, self.peer_id);
    }

    /// Update RSSI
    #[allow(dead_code)] // Will be called from delegate events
    pub(super) async fn update_rssi(&self, rssi: i8) {
        let mut state = self.state.write().await;
        state.rssi = Some(rssi);
    }

    /// Mark services as discovered
    #[allow(dead_code)] // Will be called from delegate events
    pub(super) async fn mark_services_discovered(&self) {
        let mut state = self.state.write().await;
        state.services_discovered = true;
        log::debug!("Services discovered for peer {}", self.peer_id);
    }

    /// Check if services have been discovered
    #[allow(dead_code)] // Useful for checking discovery state
    pub(super) async fn are_services_discovered(&self) -> bool {
        let state = self.state.read().await;
        state.services_discovered
    }

    /// Mark connection as dead
    pub(super) async fn mark_dead(&self) {
        let mut state = self.state.write().await;
        state.alive = false;
    }

    /// Disconnect from the peripheral
    ///
    /// Note: On CoreBluetooth, disconnection is handled by the CentralManager,
    /// not the peripheral itself. The caller (adapter) is responsible for
    /// calling centralManager.cancelPeripheralConnection() separately.
    pub(super) async fn disconnect(&self) -> Result<()> {
        self.mark_dead().await;
        log::info!(
            "CoreBluetoothConnection::disconnect({}) - Connection marked dead, CentralManager must cancel",
            self.identifier
        );
        Ok(())
    }

    /// Discover services on the peripheral
    #[allow(dead_code)] // GATT client operation - called from adapter
    pub(super) async fn discover_services(&self, service_uuids: Option<Vec<String>>) -> Result<()> {
        let peripheral = self.get_cb_peripheral()?;

        // Build UUID filter and call discoverServices - all ObjC work before await
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
                    NSArray::from_vec(cb_uuids)
                });

            unsafe {
                peripheral.discoverServices(uuid_filter.as_deref());
            }
        }

        log::debug!(
            "Initiated service discovery on peripheral {}",
            self.identifier
        );

        // Wait for the delegate callback
        let timeout = tokio::time::Duration::from_secs(10);
        let result = tokio::time::timeout(timeout, self.wait_for_services_discovered()).await;

        match result {
            Ok(Ok(())) => {
                self.mark_services_discovered().await;
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(BleError::Timeout),
        }
    }

    /// Wait for the ServicesDiscovered event from the delegate
    async fn wait_for_services_discovered(&self) -> Result<()> {
        loop {
            let mut event_rx = self.event_rx.write().await;
            match tokio::time::timeout(tokio::time::Duration::from_millis(100), event_rx.recv())
                .await
            {
                Ok(Some(PeripheralEvent::ServicesDiscovered { error, .. })) => {
                    if let Some(e) = error {
                        return Err(BleError::GattError(format!(
                            "Service discovery failed: {}",
                            e
                        )));
                    }
                    return Ok(());
                }
                Ok(Some(other)) => {
                    // Process other events while waiting
                    self.handle_event(other).await;
                }
                Ok(None) => {
                    return Err(BleError::ConnectionFailed(
                        "Event channel closed".to_string(),
                    ));
                }
                Err(_) => {
                    // Timeout on this iteration, keep waiting (outer timeout handles overall)
                    continue;
                }
            }
        }
    }

    /// Handle a peripheral event by updating internal state
    async fn handle_event(&self, event: PeripheralEvent) {
        match event {
            PeripheralEvent::MtuUpdated { mtu, .. } => {
                self.update_mtu(mtu).await;
            }
            PeripheralEvent::RssiRead { rssi, error, .. } => {
                if error.is_none() {
                    self.update_rssi(rssi).await;
                }
            }
            _ => {
                log::trace!("Unhandled peripheral event in wait: {:?}", event);
            }
        }
    }

    /// Discover characteristics for a service
    #[allow(dead_code)] // GATT client operation - called from adapter
    pub(super) async fn discover_characteristics(
        &self,
        service_uuid: &str,
        characteristic_uuids: Option<Vec<String>>,
    ) -> Result<()> {
        let peripheral = self.get_cb_peripheral()?;
        let target_uuid_upper = service_uuid.to_uppercase();

        // Find the service and initiate characteristic discovery - ObjC work before await
        {
            let char_filter: Option<Retained<NSArray<CBUUID>>> =
                characteristic_uuids.map(|uuids| unsafe {
                    let cb_uuids: Vec<Retained<CBUUID>> = uuids
                        .iter()
                        .map(|uuid_str| {
                            let ns_str = NSString::from_str(uuid_str);
                            CBUUID::UUIDWithString(&ns_str)
                        })
                        .collect();
                    NSArray::from_vec(cb_uuids)
                });

            unsafe {
                let services = peripheral.services();
                if let Some(services) = services {
                    let mut found = false;
                    for i in 0..services.len() {
                        let service = &services[i];
                        let svc_uuid = service.UUID().UUIDString().to_string().to_uppercase();
                        if svc_uuid == target_uuid_upper {
                            peripheral.discoverCharacteristics_forService(
                                char_filter.as_deref(),
                                service,
                            );
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        return Err(BleError::ServiceNotFound(service_uuid.to_string()));
                    }
                } else {
                    return Err(BleError::InvalidState(
                        "No services discovered yet".to_string(),
                    ));
                }
            }
        }

        log::debug!(
            "Initiated characteristic discovery for service {} on {}",
            service_uuid,
            self.identifier
        );

        // Wait for the delegate callback
        let timeout = tokio::time::Duration::from_secs(10);
        let target_svc = service_uuid.to_uppercase();
        let result = tokio::time::timeout(timeout, async {
            loop {
                let mut event_rx = self.event_rx.write().await;
                match tokio::time::timeout(tokio::time::Duration::from_millis(100), event_rx.recv())
                    .await
                {
                    Ok(Some(PeripheralEvent::CharacteristicsDiscovered {
                        service_uuid: svc,
                        error,
                        ..
                    })) if svc.to_uppercase() == target_svc => {
                        if let Some(e) = error {
                            return Err(BleError::GattError(format!(
                                "Characteristic discovery failed: {}",
                                e
                            )));
                        }
                        return Ok(());
                    }
                    Ok(Some(other)) => {
                        self.handle_event(other).await;
                    }
                    Ok(None) => {
                        return Err(BleError::ConnectionFailed(
                            "Event channel closed".to_string(),
                        ));
                    }
                    Err(_) => continue,
                }
            }
        })
        .await;

        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(BleError::Timeout),
        }
    }

    /// Read a characteristic value
    #[allow(dead_code)] // GATT client operation - called from adapter
    pub(super) async fn read_characteristic(
        &self,
        service_uuid: &str,
        characteristic_uuid: &str,
    ) -> Result<Vec<u8>> {
        let peripheral = self.get_cb_peripheral()?;
        let target_service_upper = service_uuid.to_uppercase();
        let target_char_upper = characteristic_uuid.to_uppercase();

        // Find the characteristic and initiate read - ObjC work before await
        {
            unsafe {
                let characteristic = self.find_cb_characteristic(
                    &peripheral,
                    &target_service_upper,
                    &target_char_upper,
                )?;
                peripheral.readValueForCharacteristic(&characteristic);
            }
        }

        log::debug!(
            "Initiated read of characteristic {} on {}",
            characteristic_uuid,
            self.identifier
        );

        // Wait for the delegate callback with the value
        let timeout = tokio::time::Duration::from_secs(10);
        let target_char = target_char_upper.clone();
        let result = tokio::time::timeout(timeout, async {
            loop {
                let mut event_rx = self.event_rx.write().await;
                match tokio::time::timeout(tokio::time::Duration::from_millis(100), event_rx.recv())
                    .await
                {
                    Ok(Some(PeripheralEvent::CharacteristicValueUpdated {
                        characteristic_uuid: char_uuid,
                        value,
                        error,
                        ..
                    })) if char_uuid.to_uppercase() == target_char
                        || char_uuid.to_uppercase().contains(&target_char) =>
                    {
                        if let Some(e) = error {
                            return Err(BleError::GattError(format!("Read failed: {}", e)));
                        }
                        return Ok(value);
                    }
                    Ok(Some(other)) => {
                        self.handle_event(other).await;
                    }
                    Ok(None) => {
                        return Err(BleError::ConnectionFailed(
                            "Event channel closed".to_string(),
                        ));
                    }
                    Err(_) => continue,
                }
            }
        })
        .await;

        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(BleError::Timeout),
        }
    }

    /// Find a CBCharacteristic by service UUID and characteristic UUID
    ///
    /// # Safety
    /// Caller must ensure peripheral is a valid CBPeripheral with discovered services.
    unsafe fn find_cb_characteristic(
        &self,
        peripheral: &CBPeripheral,
        service_uuid_upper: &str,
        char_uuid_upper: &str,
    ) -> Result<Retained<objc2_core_bluetooth::CBCharacteristic>> {
        let services = peripheral
            .services()
            .ok_or_else(|| BleError::InvalidState("No services discovered".to_string()))?;

        for i in 0..services.len() {
            let service = &services[i];
            let svc_uuid = service.UUID().UUIDString().to_string().to_uppercase();
            if svc_uuid == service_uuid_upper || svc_uuid.contains(service_uuid_upper) {
                if let Some(characteristics) = service.characteristics() {
                    for j in 0..characteristics.len() {
                        let characteristic = &characteristics[j];
                        let c_uuid = characteristic
                            .UUID()
                            .UUIDString()
                            .to_string()
                            .to_uppercase();
                        if c_uuid == char_uuid_upper || c_uuid.contains(char_uuid_upper) {
                            return Ok(characteristic.retain());
                        }
                    }
                }
            }
        }

        Err(BleError::CharacteristicNotFound(format!(
            "{}:{}",
            service_uuid_upper, char_uuid_upper
        )))
    }

    /// Write a characteristic value
    #[allow(dead_code)] // GATT client operation - called from adapter
    pub(super) async fn write_characteristic(
        &self,
        service_uuid: &str,
        characteristic_uuid: &str,
        value: &[u8],
        with_response: bool,
    ) -> Result<()> {
        let peripheral = self.get_cb_peripheral()?;
        let target_service_upper = service_uuid.to_uppercase();
        let target_char_upper = characteristic_uuid.to_uppercase();

        // Find the characteristic and initiate write - ObjC work before await
        {
            unsafe {
                let characteristic = self.find_cb_characteristic(
                    &peripheral,
                    &target_service_upper,
                    &target_char_upper,
                )?;

                let ns_data = NSData::with_bytes(value);
                // CBCharacteristicWriteType: 0 = WithResponse, 1 = WithoutResponse
                let write_type: isize = if with_response { 0 } else { 1 };
                let _: () = msg_send![
                    &*peripheral,
                    writeValue: &*ns_data
                    forCharacteristic: &*characteristic
                    type: write_type
                ];
            }
        }

        log::debug!(
            "Initiated write of {} bytes to characteristic {} on {} (response={})",
            value.len(),
            characteristic_uuid,
            self.identifier,
            with_response
        );

        // For write-without-response, return immediately
        if !with_response {
            return Ok(());
        }

        // Wait for the delegate callback confirming the write
        let timeout = tokio::time::Duration::from_secs(10);
        let target_char = target_char_upper.clone();
        let result = tokio::time::timeout(timeout, async {
            loop {
                let mut event_rx = self.event_rx.write().await;
                match tokio::time::timeout(tokio::time::Duration::from_millis(100), event_rx.recv())
                    .await
                {
                    Ok(Some(PeripheralEvent::CharacteristicWritten {
                        characteristic_uuid: char_uuid,
                        error,
                        ..
                    })) if char_uuid.to_uppercase() == target_char
                        || char_uuid.to_uppercase().contains(&target_char) =>
                    {
                        if let Some(e) = error {
                            return Err(BleError::GattError(format!("Write failed: {}", e)));
                        }
                        return Ok(());
                    }
                    Ok(Some(other)) => {
                        self.handle_event(other).await;
                    }
                    Ok(None) => {
                        return Err(BleError::ConnectionFailed(
                            "Event channel closed".to_string(),
                        ));
                    }
                    Err(_) => continue,
                }
            }
        })
        .await;

        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(BleError::Timeout),
        }
    }

    /// Enable notifications for a characteristic
    #[allow(dead_code)] // GATT client operation - called from adapter
    pub(super) async fn enable_notifications(
        &self,
        service_uuid: &str,
        characteristic_uuid: &str,
    ) -> Result<()> {
        self.set_notify_value(service_uuid, characteristic_uuid, true)
            .await
    }

    /// Disable notifications for a characteristic
    #[allow(dead_code)] // GATT client operation - called from adapter
    pub(super) async fn disable_notifications(
        &self,
        service_uuid: &str,
        characteristic_uuid: &str,
    ) -> Result<()> {
        self.set_notify_value(service_uuid, characteristic_uuid, false)
            .await
    }

    /// Set notification state for a characteristic
    async fn set_notify_value(
        &self,
        service_uuid: &str,
        characteristic_uuid: &str,
        enabled: bool,
    ) -> Result<()> {
        let peripheral = self.get_cb_peripheral()?;
        let target_service_upper = service_uuid.to_uppercase();
        let target_char_upper = characteristic_uuid.to_uppercase();

        // Find characteristic and set notify value - ObjC work before await
        {
            unsafe {
                let characteristic = self.find_cb_characteristic(
                    &peripheral,
                    &target_service_upper,
                    &target_char_upper,
                )?;

                let _: () = msg_send![
                    &*peripheral,
                    setNotifyValue: enabled
                    forCharacteristic: &*characteristic
                ];
            }
        }

        log::debug!(
            "Set notify={} for characteristic {} on {}",
            enabled,
            characteristic_uuid,
            self.identifier
        );

        // Wait for the delegate callback confirming the notification state change
        let timeout = tokio::time::Duration::from_secs(10);
        let target_char = target_char_upper.clone();
        let result = tokio::time::timeout(timeout, async {
            loop {
                let mut event_rx = self.event_rx.write().await;
                match tokio::time::timeout(tokio::time::Duration::from_millis(100), event_rx.recv())
                    .await
                {
                    Ok(Some(PeripheralEvent::NotificationStateChanged {
                        characteristic_uuid: char_uuid,
                        error,
                        ..
                    })) if char_uuid.to_uppercase() == target_char
                        || char_uuid.to_uppercase().contains(&target_char) =>
                    {
                        if let Some(e) = error {
                            return Err(BleError::GattError(format!(
                                "Notification state change failed: {}",
                                e
                            )));
                        }
                        return Ok(());
                    }
                    Ok(Some(other)) => {
                        self.handle_event(other).await;
                    }
                    Ok(None) => {
                        return Err(BleError::ConnectionFailed(
                            "Event channel closed".to_string(),
                        ));
                    }
                    Err(_) => continue,
                }
            }
        })
        .await;

        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(BleError::Timeout),
        }
    }

    /// Read RSSI
    #[allow(dead_code)] // GATT client operation - called from adapter
    pub(super) async fn read_rssi(&self) -> Result<()> {
        let peripheral = self.get_cb_peripheral()?;

        // Call readRSSI - ObjC work before await
        {
            unsafe {
                peripheral.readRSSI();
            }
        }

        log::debug!("Initiated RSSI read on {}", self.identifier);

        // Wait for the delegate callback with the RSSI value
        let timeout = tokio::time::Duration::from_secs(5);
        let result = tokio::time::timeout(timeout, async {
            loop {
                let mut event_rx = self.event_rx.write().await;
                match tokio::time::timeout(tokio::time::Duration::from_millis(100), event_rx.recv())
                    .await
                {
                    Ok(Some(PeripheralEvent::RssiRead { rssi, error, .. })) => {
                        if let Some(e) = error {
                            return Err(BleError::GattError(format!("RSSI read failed: {}", e)));
                        }
                        self.update_rssi(rssi).await;
                        return Ok(());
                    }
                    Ok(Some(other)) => {
                        self.handle_event(other).await;
                    }
                    Ok(None) => {
                        return Err(BleError::ConnectionFailed(
                            "Event channel closed".to_string(),
                        ));
                    }
                    Err(_) => continue,
                }
            }
        })
        .await;

        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(BleError::Timeout),
        }
    }

    /// Process pending delegate events
    #[allow(dead_code)] // Event processing - called from adapter poll loop
    pub(super) async fn process_events(&self) -> Result<()> {
        let mut event_rx = self.event_rx.write().await;

        while let Ok(event) = event_rx.try_recv() {
            match event {
                PeripheralEvent::ServicesDiscovered { error, .. } => {
                    if error.is_none() {
                        self.mark_services_discovered().await;
                    } else {
                        log::warn!("Service discovery error: {:?}", error);
                    }
                }
                PeripheralEvent::CharacteristicsDiscovered {
                    service_uuid,
                    error,
                    ..
                } => {
                    if let Some(ref e) = error {
                        log::warn!("Characteristic discovery error for {}: {}", service_uuid, e);
                    } else {
                        log::debug!("Characteristics discovered for service {}", service_uuid);
                    }
                }
                PeripheralEvent::CharacteristicValueUpdated {
                    characteristic_uuid,
                    value,
                    error,
                    ..
                } => {
                    if let Some(ref e) = error {
                        log::warn!(
                            "Characteristic {} value update error: {}",
                            characteristic_uuid,
                            e
                        );
                    } else {
                        log::debug!(
                            "Characteristic {} value updated ({} bytes)",
                            characteristic_uuid,
                            value.len()
                        );
                    }
                }
                PeripheralEvent::CharacteristicWritten {
                    characteristic_uuid,
                    error,
                    ..
                } => {
                    if let Some(ref e) = error {
                        log::warn!("Characteristic {} write error: {}", characteristic_uuid, e);
                    } else {
                        log::debug!("Characteristic {} write confirmed", characteristic_uuid);
                    }
                }
                PeripheralEvent::NotificationStateChanged {
                    characteristic_uuid,
                    enabled,
                    error,
                    ..
                } => {
                    if let Some(ref e) = error {
                        log::warn!(
                            "Notification state change error for {}: {}",
                            characteristic_uuid,
                            e
                        );
                    } else {
                        log::debug!(
                            "Notifications {} for {}",
                            if enabled { "enabled" } else { "disabled" },
                            characteristic_uuid
                        );
                    }
                }
                PeripheralEvent::MtuUpdated { mtu, .. } => {
                    self.update_mtu(mtu).await;
                }
                PeripheralEvent::RssiRead { rssi, error, .. } => {
                    if error.is_none() {
                        self.update_rssi(rssi).await;
                    }
                }
            }
        }

        Ok(())
    }
}

impl BleConnection for CoreBluetoothConnection {
    fn peer_id(&self) -> &NodeId {
        &self.peer_id
    }

    fn is_alive(&self) -> bool {
        if let Ok(state) = self.state.try_read() {
            state.alive
        } else {
            // If we can't get the lock, assume alive
            true
        }
    }

    fn mtu(&self) -> u16 {
        if let Ok(state) = self.state.try_read() {
            state.mtu
        } else {
            23 // Default BLE MTU
        }
    }

    fn phy(&self) -> BlePhy {
        if let Ok(state) = self.state.try_read() {
            state.phy
        } else {
            BlePhy::Le1M
        }
    }

    fn rssi(&self) -> Option<i8> {
        if let Ok(state) = self.state.try_read() {
            state.rssi
        } else {
            None
        }
    }

    fn connected_duration(&self) -> Duration {
        self.connected_at.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_creation() {
        let node_id = NodeId::new(0xDEADBEEF);
        let identifier = "12345678-1234-1234-1234-123456789ABC".to_string();
        let conn = CoreBluetoothConnection::new(node_id.clone(), identifier.clone());

        assert_eq!(conn.peer_id(), &node_id);
        assert_eq!(conn.identifier(), identifier);
        assert!(conn.is_alive());
        assert_eq!(conn.mtu(), 23);
        assert_eq!(conn.phy(), BlePhy::Le1M);
    }

    #[tokio::test]
    async fn test_connection_state_updates() {
        let node_id = NodeId::new(0xDEADBEEF);
        let conn = CoreBluetoothConnection::new(
            node_id,
            "12345678-1234-1234-1234-123456789ABC".to_string(),
        );

        // Update MTU
        conn.update_mtu(247).await;
        assert_eq!(conn.mtu(), 247);

        // Update RSSI
        conn.update_rssi(-65).await;
        assert_eq!(conn.rssi(), Some(-65));

        // Mark dead
        conn.mark_dead().await;
        assert!(!conn.is_alive());
    }
}
