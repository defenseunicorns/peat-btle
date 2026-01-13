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


//! BlueZ adapter implementation using the `bluer` crate

use async_trait::async_trait;
use bluer::{
    adv::{Advertisement, AdvertisementHandle},
    gatt::local::{
        Application, ApplicationHandle, Characteristic, CharacteristicNotify,
        CharacteristicNotifyMethod, CharacteristicRead, CharacteristicWrite,
        CharacteristicWriteMethod, Service,
    },
    Adapter, Address, Session,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock};

use crate::config::{BleConfig, DiscoveryConfig};
use crate::error::{BleError, Result};
use crate::gatt::HiveCharacteristicUuids;
use crate::platform::{
    BleAdapter, ConnectionCallback, ConnectionEvent, DisconnectReason, DiscoveredDevice,
    DiscoveryCallback,
};
use crate::transport::BleConnection;
use crate::{NodeId, HIVE_SERVICE_UUID};

use super::BluerConnection;

/// Internal state for the adapter
struct AdapterState {
    /// Active connections by node ID
    connections: HashMap<NodeId, BluerConnection>,
    /// Device address to node ID mapping
    address_to_node: HashMap<Address, NodeId>,
    /// Node ID to device address mapping
    node_to_address: HashMap<NodeId, Address>,
    /// Discovered devices (address -> device info)
    /// TODO: Wire up device tracking for connection management
    #[allow(dead_code)]
    discovered: HashMap<Address, DiscoveredDevice>,
}

impl Default for AdapterState {
    fn default() -> Self {
        Self {
            connections: HashMap::new(),
            address_to_node: HashMap::new(),
            node_to_address: HashMap::new(),
            discovered: HashMap::new(),
        }
    }
}

/// State shared between GATT characteristic callbacks
struct GattState {
    /// Node ID for this adapter
    node_id: Mutex<Option<NodeId>>,
    /// Node info data (readable)
    node_info: Mutex<Vec<u8>>,
    /// Sync state data (readable, notifiable)
    sync_state: Mutex<Vec<u8>>,
    /// Status data (readable, notifiable)
    status: Mutex<Vec<u8>>,
    /// Received sync data callback
    sync_data_callback: Mutex<Option<Box<dyn Fn(Vec<u8>) + Send + Sync>>>,
    /// Received command callback
    command_callback: Mutex<Option<Box<dyn Fn(Vec<u8>) + Send + Sync>>>,
}

impl GattState {
    fn new() -> Self {
        Self {
            node_id: Mutex::new(None),
            node_info: Mutex::new(Vec::new()),
            sync_state: Mutex::new(Vec::new()),
            status: Mutex::new(Vec::new()),
            sync_data_callback: Mutex::new(None),
            command_callback: Mutex::new(None),
        }
    }

    /// Initialize state with node information
    async fn init(&self, node_id: NodeId) {
        *self.node_id.lock().await = Some(node_id);
        // Initialize node_info with basic data (node_id as 4 bytes LE)
        *self.node_info.lock().await = node_id.as_u32().to_le_bytes().to_vec();
        // Initialize sync_state as idle (0x00)
        *self.sync_state.lock().await = vec![0x00];
        // Initialize status as empty
        *self.status.lock().await = vec![0x00];
    }
}

/// Linux/BlueZ BLE adapter
///
/// Implements the `BleAdapter` trait using the `bluer` crate for
/// BlueZ D-Bus communication.
pub struct BluerAdapter {
    /// BlueZ session (kept alive for adapter lifetime)
    #[allow(dead_code)]
    session: Session,
    /// BlueZ adapter
    adapter: Adapter,
    /// Cached adapter address (queried once at creation)
    cached_address: Option<String>,
    /// Cached power state (updated on power changes)
    cached_powered: std::sync::atomic::AtomicBool,
    /// Configuration
    config: RwLock<Option<BleConfig>>,
    /// Internal state
    state: RwLock<AdapterState>,
    /// Advertisement handle (keeps advertisement alive)
    adv_handle: RwLock<Option<AdvertisementHandle>>,
    /// GATT application handle (keeps service registered)
    gatt_handle: RwLock<Option<ApplicationHandle>>,
    /// GATT service state for read/write callbacks
    gatt_state: Arc<GattState>,
    /// Discovery callback
    discovery_callback: RwLock<Option<DiscoveryCallback>>,
    /// Connection callback
    connection_callback: RwLock<Option<ConnectionCallback>>,
    /// Shutdown signal
    shutdown_tx: broadcast::Sender<()>,
}

impl BluerAdapter {
    /// Create a new BlueZ adapter
    ///
    /// This connects to the system D-Bus and gets the default Bluetooth adapter.
    pub async fn new() -> Result<Self> {
        let session = Session::new().await.map_err(|e| {
            BleError::PlatformError(format!("Failed to create BlueZ session: {}", e))
        })?;

        let adapter = session
            .default_adapter()
            .await
            .map_err(|_| BleError::AdapterNotAvailable)?;

        // Check if adapter is powered
        let powered = adapter.is_powered().await.map_err(|e| {
            BleError::PlatformError(format!("Failed to check adapter power: {}", e))
        })?;

        if !powered {
            // Try to power on the adapter
            adapter.set_powered(true).await.map_err(|e| {
                BleError::PlatformError(format!("Failed to power on adapter: {}", e))
            })?;
        }

        // Cache the adapter address
        let cached_address = adapter.address().await.ok().map(|a| a.to_string());

        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            session,
            adapter,
            cached_address,
            cached_powered: std::sync::atomic::AtomicBool::new(true), // We ensured it's powered above
            config: RwLock::new(None),
            state: RwLock::new(AdapterState::default()),
            adv_handle: RwLock::new(None),
            gatt_handle: RwLock::new(None),
            gatt_state: Arc::new(GattState::new()),
            discovery_callback: RwLock::new(None),
            connection_callback: RwLock::new(None),
            shutdown_tx,
        })
    }

    /// Create adapter with a specific adapter name (e.g., "hci0")
    pub async fn with_adapter_name(name: &str) -> Result<Self> {
        let session = Session::new().await.map_err(|e| {
            BleError::PlatformError(format!("Failed to create BlueZ session: {}", e))
        })?;

        let adapter = session.adapter(name).map_err(|e| {
            BleError::PlatformError(format!("Failed to get adapter '{}': {}", name, e))
        })?;

        let powered = adapter.is_powered().await.map_err(|e| {
            BleError::PlatformError(format!("Failed to check adapter power: {}", e))
        })?;

        if !powered {
            adapter.set_powered(true).await.map_err(|e| {
                BleError::PlatformError(format!("Failed to power on adapter: {}", e))
            })?;
        }

        // Cache the adapter address
        let cached_address = adapter.address().await.ok().map(|a| a.to_string());

        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            session,
            adapter,
            cached_address,
            cached_powered: std::sync::atomic::AtomicBool::new(true),
            config: RwLock::new(None),
            state: RwLock::new(AdapterState::default()),
            adv_handle: RwLock::new(None),
            gatt_handle: RwLock::new(None),
            gatt_state: Arc::new(GattState::new()),
            discovery_callback: RwLock::new(None),
            connection_callback: RwLock::new(None),
            shutdown_tx,
        })
    }

    /// Get the adapter name (e.g., "hci0")
    pub fn adapter_name(&self) -> &str {
        self.adapter.name()
    }

    /// Build HIVE advertisement
    fn build_advertisement(&self, config: &BleConfig) -> Advertisement {
        let mut adv = Advertisement {
            advertisement_type: bluer::adv::Type::Peripheral,
            service_uuids: vec![HIVE_SERVICE_UUID].into_iter().collect(),
            local_name: Some(format!("HIVE-{:08X}", config.node_id.as_u32())),
            discoverable: Some(true),
            ..Default::default()
        };

        // Set TX power if supported
        adv.tx_power = Some(config.discovery.tx_power_dbm as i16);

        adv
    }

    /// Parse HIVE beacon from advertising data
    /// TODO: Use this method instead of inline parsing in discovery loop
    #[allow(dead_code)]
    fn parse_hive_beacon(
        &self,
        address: Address,
        name: Option<String>,
        rssi: i16,
        service_data: &HashMap<bluer::Uuid, Vec<u8>>,
        _manufacturer_data: &HashMap<u16, Vec<u8>>,
    ) -> Option<DiscoveredDevice> {
        // Check if this is a HIVE node by looking for our service UUID
        let is_hive = service_data.contains_key(&HIVE_SERVICE_UUID);

        // Try to extract node ID from name (HIVE-XXXXXXXX format)
        let node_id = name.as_ref().and_then(|n| {
            if n.starts_with("HIVE-") {
                NodeId::parse(&n[5..])
            } else {
                None
            }
        });

        Some(DiscoveredDevice {
            address: address.to_string(),
            name,
            rssi: rssi as i8,
            is_hive_node: is_hive || node_id.is_some(),
            node_id,
            adv_data: Vec::new(), // TODO: serialize full adv data
        })
    }

    /// Register node ID to address mapping
    pub async fn register_node_address(&self, node_id: NodeId, address: Address) {
        let mut state = self.state.write().await;
        state.address_to_node.insert(address, node_id.clone());
        state.node_to_address.insert(node_id, address);
    }

    /// Get address for a node ID
    pub async fn get_node_address(&self, node_id: &NodeId) -> Option<Address> {
        let state = self.state.read().await;
        state.node_to_address.get(node_id).copied()
    }
}

#[async_trait]
impl BleAdapter for BluerAdapter {
    async fn init(&mut self, config: &BleConfig) -> Result<()> {
        *self.config.write().await = Some(config.clone());
        log::info!(
            "BluerAdapter initialized for node {:08X}",
            config.node_id.as_u32()
        );
        Ok(())
    }

    async fn start(&self) -> Result<()> {
        let config = self.config.read().await;
        let config = config
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Adapter not initialized".to_string()))?;

        // Start advertising
        self.start_advertising(&config.discovery).await?;

        // Start scanning
        self.start_scan(&config.discovery).await?;

        log::info!("BluerAdapter started");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        // Stop advertising
        self.stop_advertising().await?;

        // Stop scanning
        self.stop_scan().await?;

        // Signal shutdown to background tasks
        let _ = self.shutdown_tx.send(());

        log::info!("BluerAdapter stopped");
        Ok(())
    }

    fn is_powered(&self) -> bool {
        self.cached_powered
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn address(&self) -> Option<String> {
        self.cached_address.clone()
    }

    async fn start_scan(&self, config: &DiscoveryConfig) -> Result<()> {
        use bluer::DiscoveryFilter;
        use bluer::DiscoveryTransport;

        let filter = DiscoveryFilter {
            transport: DiscoveryTransport::Le,
            duplicate_data: !config.filter_duplicates,
            ..Default::default()
        };

        self.adapter
            .set_discovery_filter(filter)
            .await
            .map_err(|e| {
                BleError::DiscoveryFailed(format!("Failed to set discovery filter: {}", e))
            })?;

        // Start discovery
        let discover =
            self.adapter.discover_devices().await.map_err(|e| {
                BleError::DiscoveryFailed(format!("Failed to start discovery: {}", e))
            })?;

        // Spawn task to handle discovered devices
        let callback = self.discovery_callback.read().await.clone();
        let adapter = self.adapter.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            use tokio_stream::StreamExt;
            let mut discover = std::pin::pin!(discover);

            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        log::debug!("Discovery task shutting down");
                        break;
                    }
                    event = discover.next() => {
                        match event {
                            Some(bluer::AdapterEvent::DeviceAdded(addr)) => {
                                if let Ok(device) = adapter.device(addr) {
                                    // Get device properties
                                    let name = device.name().await.ok().flatten();
                                    let rssi = device.rssi().await.ok().flatten().unwrap_or(0);

                                    // Get service UUIDs from the device
                                    let service_uuids = device.uuids().await.ok().flatten().unwrap_or_default();

                                    // Check if HIVE service UUID is present
                                    let has_hive_service = service_uuids.contains(&HIVE_SERVICE_UUID);

                                    // Check if name indicates HIVE node (fallback)
                                    let name_indicates_hive = name.as_ref().map(|n| n.starts_with("HIVE-")).unwrap_or(false);

                                    // HIVE node detection: prefer service UUID, fallback to name
                                    let is_hive_node = has_hive_service || name_indicates_hive;

                                    let discovered = DiscoveredDevice {
                                        address: addr.to_string(),
                                        name: name.clone(),
                                        rssi: rssi as i8,
                                        is_hive_node,
                                        node_id: name.and_then(|n| {
                                            n.strip_prefix("HIVE-").and_then(NodeId::parse)
                                        }),
                                        adv_data: Vec::new(),
                                    };

                                    log::debug!(
                                        "Discovered device: {} (HIVE: {}, service_uuid: {}, name: {})",
                                        discovered.address, is_hive_node, has_hive_service, name_indicates_hive
                                    );

                                    if let Some(ref cb) = callback {
                                        cb(discovered);
                                    }
                                }
                            }
                            Some(bluer::AdapterEvent::DeviceRemoved(addr)) => {
                                log::debug!("Device removed: {}", addr);
                            }
                            None => break,
                            _ => {}
                        }
                    }
                }
            }
        });

        log::info!("BLE scanning started");
        Ok(())
    }

    async fn stop_scan(&self) -> Result<()> {
        // Discovery is stopped when the stream is dropped
        // The shutdown signal will cause the task to exit
        log::info!("BLE scanning stopped");
        Ok(())
    }

    async fn start_advertising(&self, _config: &DiscoveryConfig) -> Result<()> {
        let ble_config = self.config.read().await;
        let ble_config = ble_config
            .as_ref()
            .ok_or_else(|| BleError::InvalidState("Adapter not initialized".to_string()))?;

        let adv = self.build_advertisement(ble_config);

        let handle =
            self.adapter.advertise(adv).await.map_err(|e| {
                BleError::PlatformError(format!("Failed to start advertising: {}", e))
            })?;

        *self.adv_handle.write().await = Some(handle);

        log::info!(
            "BLE advertising started for HIVE-{:08X}",
            ble_config.node_id.as_u32()
        );
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<()> {
        // Drop the advertisement handle to stop advertising
        *self.adv_handle.write().await = None;
        log::info!("BLE advertising stopped");
        Ok(())
    }

    fn set_discovery_callback(&mut self, callback: Option<DiscoveryCallback>) {
        // Use blocking write since this is a sync method
        // In practice, this should be called before start()
        if let Ok(mut cb) = self.discovery_callback.try_write() {
            *cb = callback;
        }
    }

    async fn connect(&self, peer_id: &NodeId) -> Result<Box<dyn BleConnection>> {
        // Look up the address for this node ID
        let address = self
            .get_node_address(peer_id)
            .await
            .ok_or_else(|| BleError::ConnectionFailed(format!("Unknown node ID: {}", peer_id)))?;

        let device = self
            .adapter
            .device(address)
            .map_err(|e| BleError::ConnectionFailed(format!("Failed to get device: {}", e)))?;

        // Connect to the device
        device
            .connect()
            .await
            .map_err(|e| BleError::ConnectionFailed(format!("Failed to connect: {}", e)))?;

        // Create connection wrapper
        let connection = BluerConnection::new(peer_id.clone(), device).await?;

        // Store connection
        {
            let mut state = self.state.write().await;
            state
                .connections
                .insert(peer_id.clone(), connection.clone());
        }

        // Notify callback
        if let Some(ref cb) = *self.connection_callback.read().await {
            cb(
                peer_id.clone(),
                ConnectionEvent::Connected {
                    mtu: connection.mtu(),
                    phy: connection.phy(),
                },
            );
        }

        log::info!("Connected to peer {}", peer_id);
        Ok(Box::new(connection))
    }

    async fn disconnect(&self, peer_id: &NodeId) -> Result<()> {
        let connection = {
            let mut state = self.state.write().await;
            state.connections.remove(peer_id)
        };

        if let Some(conn) = connection {
            conn.disconnect().await?;

            // Notify callback
            if let Some(ref cb) = *self.connection_callback.read().await {
                cb(
                    peer_id.clone(),
                    ConnectionEvent::Disconnected {
                        reason: DisconnectReason::LocalRequest,
                    },
                );
            }

            log::info!("Disconnected from peer {}", peer_id);
        }

        Ok(())
    }

    fn get_connection(&self, peer_id: &NodeId) -> Option<Box<dyn BleConnection>> {
        // Use try_read to avoid blocking
        if let Ok(state) = self.state.try_read() {
            state
                .connections
                .get(peer_id)
                .map(|c| Box::new(c.clone()) as Box<dyn BleConnection>)
        } else {
            None
        }
    }

    fn peer_count(&self) -> usize {
        if let Ok(state) = self.state.try_read() {
            state.connections.len()
        } else {
            0
        }
    }

    fn connected_peers(&self) -> Vec<NodeId> {
        if let Ok(state) = self.state.try_read() {
            state.connections.keys().cloned().collect()
        } else {
            Vec::new()
        }
    }

    fn set_connection_callback(&mut self, callback: Option<ConnectionCallback>) {
        if let Ok(mut cb) = self.connection_callback.try_write() {
            *cb = callback;
        }
    }

    async fn register_gatt_service(&self) -> Result<()> {
        // Get config to access node_id
        let config = self.config.read().await;
        let node_id = config
            .as_ref()
            .map(|c| c.node_id)
            .ok_or_else(|| BleError::InvalidState("Adapter not initialized".to_string()))?;

        // Initialize GATT state with node info
        self.gatt_state.init(node_id).await;

        // Clone Arc for use in callbacks
        let state = self.gatt_state.clone();
        let state_read_node = state.clone();
        let state_read_sync = state.clone();
        let state_read_status = state.clone();
        let state_write_sync = state.clone();
        let state_write_cmd = state.clone();

        // Build GATT application with HIVE service
        let app = Application {
            services: vec![Service {
                uuid: HIVE_SERVICE_UUID,
                primary: true,
                characteristics: vec![
                    // Node Info characteristic (0001) - READ
                    Characteristic {
                        uuid: HiveCharacteristicUuids::node_info(),
                        read: Some(CharacteristicRead {
                            read: true,
                            fun: Box::new(move |req| {
                                let state = state_read_node.clone();
                                Box::pin(async move {
                                    let data = state.node_info.lock().await;
                                    log::debug!(
                                        "GATT read node_info from {:?}: {} bytes",
                                        req.device_address,
                                        data.len()
                                    );
                                    Ok(data.clone())
                                })
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    // Sync State characteristic (0002) - READ, NOTIFY
                    Characteristic {
                        uuid: HiveCharacteristicUuids::sync_state(),
                        read: Some(CharacteristicRead {
                            read: true,
                            fun: Box::new(move |req| {
                                let state = state_read_sync.clone();
                                Box::pin(async move {
                                    let data = state.sync_state.lock().await;
                                    log::debug!(
                                        "GATT read sync_state from {:?}: {} bytes",
                                        req.device_address,
                                        data.len()
                                    );
                                    Ok(data.clone())
                                })
                            }),
                            ..Default::default()
                        }),
                        notify: Some(CharacteristicNotify {
                            notify: true,
                            method: CharacteristicNotifyMethod::Io,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    // Sync Data characteristic (0003) - WRITE, INDICATE
                    Characteristic {
                        uuid: HiveCharacteristicUuids::sync_data(),
                        write: Some(CharacteristicWrite {
                            write: true,
                            method: CharacteristicWriteMethod::Fun(Box::new(move |data, req| {
                                let state = state_write_sync.clone();
                                Box::pin(async move {
                                    log::debug!(
                                        "GATT write sync_data from {:?}: {} bytes",
                                        req.device_address,
                                        data.len()
                                    );
                                    // Invoke callback if set
                                    if let Some(ref cb) = *state.sync_data_callback.lock().await {
                                        cb(data);
                                    }
                                    Ok(())
                                })
                            })),
                            ..Default::default()
                        }),
                        notify: Some(CharacteristicNotify {
                            indicate: true,
                            method: CharacteristicNotifyMethod::Io,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    // Command characteristic (0004) - WRITE
                    Characteristic {
                        uuid: HiveCharacteristicUuids::command(),
                        write: Some(CharacteristicWrite {
                            write: true,
                            write_without_response: true,
                            method: CharacteristicWriteMethod::Fun(Box::new(move |data, req| {
                                let state = state_write_cmd.clone();
                                Box::pin(async move {
                                    log::debug!(
                                        "GATT write command from {:?}: {} bytes",
                                        req.device_address,
                                        data.len()
                                    );
                                    // Invoke callback if set
                                    if let Some(ref cb) = *state.command_callback.lock().await {
                                        cb(data);
                                    }
                                    Ok(())
                                })
                            })),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    // Status characteristic (0005) - READ, NOTIFY
                    Characteristic {
                        uuid: HiveCharacteristicUuids::status(),
                        read: Some(CharacteristicRead {
                            read: true,
                            fun: Box::new(move |req| {
                                let state = state_read_status.clone();
                                Box::pin(async move {
                                    let data = state.status.lock().await;
                                    log::debug!(
                                        "GATT read status from {:?}: {} bytes",
                                        req.device_address,
                                        data.len()
                                    );
                                    Ok(data.clone())
                                })
                            }),
                            ..Default::default()
                        }),
                        notify: Some(CharacteristicNotify {
                            notify: true,
                            method: CharacteristicNotifyMethod::Io,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        };

        // Register the GATT application with BlueZ
        let handle = self
            .adapter
            .serve_gatt_application(app)
            .await
            .map_err(|e| BleError::GattError(format!("Failed to register GATT service: {}", e)))?;

        // Store the handle to keep the service alive
        *self.gatt_handle.write().await = Some(handle);

        log::info!(
            "GATT service registered for node {:08X} with 5 characteristics",
            node_id.as_u32()
        );
        Ok(())
    }

    async fn unregister_gatt_service(&self) -> Result<()> {
        // Drop the handle to unregister the GATT application
        let handle = self.gatt_handle.write().await.take();
        if handle.is_some() {
            log::info!("GATT service unregistered");
        }
        Ok(())
    }

    fn supports_coded_phy(&self) -> bool {
        // Check if LE Coded PHY is supported
        // This would require checking adapter capabilities
        // For now, assume BLE 5.0+ adapters support it
        true
    }

    fn supports_extended_advertising(&self) -> bool {
        // Check if extended advertising is supported
        true
    }

    fn max_mtu(&self) -> u16 {
        // BlueZ typically supports up to 517 bytes MTU
        517
    }

    fn max_connections(&self) -> u8 {
        // BlueZ default is 7 connections
        7
    }
}

#[cfg(test)]
mod tests {
    // Integration tests require actual Bluetooth hardware
    // They should be run with --ignored flag on a Linux system

    #[tokio::test]
    #[ignore = "Requires BlueZ and Bluetooth hardware"]
    async fn test_adapter_creation() {
        use super::*;

        let adapter = BluerAdapter::new().await;
        assert!(
            adapter.is_ok(),
            "Failed to create adapter: {:?}",
            adapter.err()
        );
    }

    #[tokio::test]
    #[ignore = "Requires BlueZ and Bluetooth hardware"]
    async fn test_adapter_init() {
        use super::*;
        use crate::BleConfig;

        let mut adapter = BluerAdapter::new().await.unwrap();
        let config = BleConfig::new(NodeId::new(0x12345678));
        let result = adapter.init(&config).await;
        assert!(result.is_ok());
    }
}
