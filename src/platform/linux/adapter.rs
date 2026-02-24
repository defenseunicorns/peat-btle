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
use crate::gatt::EcheCharacteristicUuids;
use crate::platform::{
    BleAdapter, ConnectionCallback, ConnectionEvent, DisconnectReason, DiscoveredDevice,
    DiscoveryCallback,
};
use crate::transport::BleConnection;
use crate::{NodeId, ECHE_SERVICE_UUID};

use super::BluerConnection;

/// Internal state for the adapter
#[derive(Default)]
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

/// Type alias for callback functions to reduce complexity
type SyncCallback = Box<dyn Fn(Vec<u8>) + Send + Sync>;

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
    sync_data_callback: Mutex<Option<SyncCallback>>,
    /// Received command callback
    command_callback: Mutex<Option<SyncCallback>>,
    /// Per-peer MTU from GATT operations (address -> mtu)
    /// Updated when peers perform GATT read/write operations
    peer_mtu: Mutex<HashMap<Address, u16>>,
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
            peer_mtu: Mutex::new(HashMap::new()),
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

    /// Update the MTU for a peer based on GATT request
    async fn update_peer_mtu(&self, address: Address, mtu: u16) {
        let mut peer_mtu = self.peer_mtu.lock().await;
        let old_mtu = peer_mtu.insert(address, mtu);
        if old_mtu != Some(mtu) {
            log::debug!("Peer {} MTU: {} (was {:?})", address, mtu, old_mtu);
        }
    }

    /// Get the MTU for a peer
    async fn get_peer_mtu(&self, address: &Address) -> Option<u16> {
        self.peer_mtu.lock().await.get(address).copied()
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
    state: Arc<RwLock<AdapterState>>,
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

        // Disable pairing to prevent pairing prompts on remote devices
        if let Err(e) = adapter.set_pairable(false).await {
            log::warn!("Failed to disable pairing: {}", e);
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
            state: Arc::new(RwLock::new(AdapterState::default())),
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

        // Disable pairing to prevent pairing prompts on remote devices
        if let Err(e) = adapter.set_pairable(false).await {
            log::warn!("Failed to disable pairing: {}", e);
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
            state: Arc::new(RwLock::new(AdapterState::default())),
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

    /// Build Eche advertisement
    ///
    /// Matches Android's advertisement format for maximum compatibility:
    /// - 16-bit Eche service UUID alias (0xF47A)
    /// - Service data with [nodeId:4 bytes BE][meshId:4 bytes BE]
    /// - Device name goes in scan response (handled by BlueZ via adapter alias)
    ///
    /// This keeps the advertisement packet under 31 bytes:
    /// - Flags: 3 bytes
    /// - Service UUID (16-bit): 4 bytes
    /// - Service data: 2 (header) + 2 (UUID) + 4 (nodeId) + 4 (meshId) = 12 bytes
    /// - Total: 19 bytes (name in scan response)
    fn build_advertisement(&self, config: &BleConfig) -> Advertisement {
        use std::collections::BTreeMap;

        // The 16-bit UUID 0xF47A in its 128-bit Bluetooth SIG form
        let service_uuid_16bit =
            uuid::Uuid::parse_str("0000F47A-0000-1000-8000-00805F9B34FB").unwrap();

        // Build service data: [nodeId:4 bytes BE][meshId:4 bytes BE]
        // meshId is an 8-char hex string like "29C916FA" -> 4 bytes
        let mut service_data_bytes = config.node_id.as_u32().to_be_bytes().to_vec();

        // Parse mesh_id as hex and append (4 bytes)
        if let Ok(mesh_id_int) = u32::from_str_radix(&config.mesh.mesh_id, 16) {
            service_data_bytes.extend_from_slice(&mesh_id_int.to_be_bytes());
            log::debug!(
                "Advertisement includes mesh_id: {} (0x{:08X})",
                config.mesh.mesh_id,
                mesh_id_int
            );
        } else {
            // Fallback: use first 4 bytes of mesh_id string as ASCII
            let mesh_bytes: Vec<u8> = config.mesh.mesh_id.bytes().take(4).collect();
            service_data_bytes.extend_from_slice(&mesh_bytes);
            log::debug!(
                "Advertisement includes mesh_id as ASCII: {}",
                config.mesh.mesh_id
            );
        }

        let mut service_data = BTreeMap::new();
        service_data.insert(service_uuid_16bit, service_data_bytes);

        // Device name - include in advertisement for maximum compatibility
        // (some scanners need the name in the main advertisement)
        let device_name = format!("ECHE-{:08X}", config.node_id.as_u32());

        Advertisement {
            advertisement_type: bluer::adv::Type::Peripheral,
            service_uuids: vec![service_uuid_16bit].into_iter().collect(),
            // Include local_name - BlueZ will put it in scan response if needed
            local_name: Some(device_name),
            service_data,
            // Set discoverable to allow other devices to find us
            discoverable: Some(true),
            ..Default::default()
        }
    }

    /// Set the adapter's alias (used for scan response device name)
    pub async fn set_adapter_alias(&self, alias: &str) -> Result<()> {
        self.adapter
            .set_alias(alias.to_string())
            .await
            .map_err(|e| BleError::PlatformError(format!("Failed to set adapter alias: {}", e)))
    }

    /// Parse Eche beacon from advertising data
    /// TODO: Use this method instead of inline parsing in discovery loop
    #[allow(dead_code)]
    fn parse_eche_beacon(
        &self,
        address: Address,
        name: Option<String>,
        rssi: i16,
        service_data: &HashMap<bluer::Uuid, Vec<u8>>,
        _manufacturer_data: &HashMap<u16, Vec<u8>>,
    ) -> Option<DiscoveredDevice> {
        // Check if this is a Eche node by looking for our service UUID
        let is_eche = service_data.contains_key(&ECHE_SERVICE_UUID);

        // Try to extract node ID from name (ECHE-XXXXXXXX format)
        let node_id = name
            .as_ref()
            .and_then(|n| n.strip_prefix("ECHE-"))
            .and_then(NodeId::parse);

        Some(DiscoveredDevice {
            address: address.to_string(),
            name,
            rssi: rssi as i8,
            is_eche_node: is_eche || node_id.is_some(),
            node_id,
            adv_data: Vec::new(), // TODO: serialize full adv data
        })
    }

    /// Register node ID to address mapping
    pub async fn register_node_address(&self, node_id: NodeId, address: Address) {
        let mut state = self.state.write().await;
        state.address_to_node.insert(address, node_id);
        state.node_to_address.insert(node_id, address);
    }

    /// Get address for a node ID
    pub async fn get_node_address(&self, node_id: &NodeId) -> Option<Address> {
        let state = self.state.read().await;
        state.node_to_address.get(node_id).copied()
    }

    /// Get a clone of a stored connection by node ID
    ///
    /// Returns the `BluerConnection` for a connected peer, allowing direct
    /// GATT operations (read/write characteristics) on the remote device.
    pub async fn get_connection(&self, node_id: &NodeId) -> Option<BluerConnection> {
        let state = self.state.read().await;
        state.connections.get(node_id).cloned()
    }

    /// Set callback for when sync data is received from a connected peer
    ///
    /// This is invoked when a remote device writes to the sync_data characteristic.
    /// Use this to feed received documents into `EcheMesh::on_ble_data_received_anonymous`.
    pub async fn set_sync_data_callback<F>(&self, callback: F)
    where
        F: Fn(Vec<u8>) + Send + Sync + 'static,
    {
        *self.gatt_state.sync_data_callback.lock().await = Some(Box::new(callback));
    }

    /// Clear the sync data callback
    pub async fn clear_sync_data_callback(&self) {
        *self.gatt_state.sync_data_callback.lock().await = None;
    }

    /// Update the sync state data that connected peers can read
    ///
    /// Call this with the output of `EcheMesh::tick()` or `EcheMesh::build_document()`
    /// to make the current mesh state available to connected peers.
    pub async fn update_sync_state(&self, data: &[u8]) {
        *self.gatt_state.sync_state.lock().await = data.to_vec();
    }

    /// Get current sync state data
    pub async fn get_sync_state(&self) -> Vec<u8> {
        self.gatt_state.sync_state.lock().await.clone()
    }

    /// Get the negotiated MTU for a connected peer (by BLE address)
    ///
    /// Returns the MTU captured from the peer's last GATT operation.
    /// This is populated when the peer performs read/write operations on our GATT server.
    pub async fn get_peer_mtu(&self, address: &Address) -> Option<u16> {
        self.gatt_state.get_peer_mtu(address).await
    }

    /// Get all known peer MTUs (for debugging/monitoring)
    pub async fn get_all_peer_mtus(&self) -> HashMap<Address, u16> {
        self.gatt_state.peer_mtu.lock().await.clone()
    }

    /// Get a device handle by address for direct GATT operations
    ///
    /// This is useful when you need to connect to a device directly.
    pub fn get_device(&self, address: Address) -> std::result::Result<bluer::Device, bluer::Error> {
        self.adapter.device(address)
    }

    /// Connect to a device with explicit address type
    ///
    /// This is needed for devices using random BLE addresses (common in BLE peripherals
    /// like WearOS watches). The address type can be determined from the address itself:
    /// - If first byte MSBs are 11 (0xC0+ range), it's typically a random static address
    /// - Use `AddressType::LeRandom` for random addresses
    /// - Use `AddressType::LePublic` for public addresses
    pub async fn connect_device(
        &self,
        address: Address,
        address_type: bluer::AddressType,
    ) -> std::result::Result<bluer::Device, bluer::Error> {
        self.adapter.connect_device(address, address_type).await
    }

    /// Determine if a BLE address is a random address based on its structure
    ///
    /// In BLE, random addresses have specific patterns in the two MSBs of the first byte:
    /// - 11: Random static address
    /// - 01: Resolvable private address (RPA)
    /// - 00: Non-resolvable private address
    ///
    /// Public addresses don't follow this pattern and are manufacturer-assigned.
    pub fn is_random_address(address: &Address) -> bool {
        let bytes = address.0;
        // The first byte of the address string (e.g., "D8" in "D8:3A:DD:F5:FD:53") is bytes[0]
        let first_byte = bytes[0];
        // Random static: top 2 bits = 11 (0xC0+)
        // RPA: top 2 bits = 01 (0x40-0x7F)
        // Non-resolvable: top 2 bits = 00 (0x00-0x3F)
        // A simple heuristic: if MSB bits are 11, it's almost certainly random static
        (first_byte & 0xC0) == 0xC0
    }

    /// Stop BLE discovery temporarily
    ///
    /// This is useful before connecting to avoid the "le-connection-abort-by-local" error
    /// that can happen when BlueZ tries to scan and connect simultaneously.
    pub async fn stop_discovery(&self) -> Result<()> {
        // Note: This doesn't stop our discovery stream task, just tells the adapter to pause
        self.adapter
            .set_discovery_filter(bluer::DiscoveryFilter::default())
            .await
            .ok();
        Ok(())
    }

    /// Resume BLE discovery
    pub async fn resume_discovery(&self) -> Result<()> {
        use bluer::DiscoveryFilter;
        use bluer::DiscoveryTransport;

        let filter = DiscoveryFilter {
            transport: DiscoveryTransport::Le,
            ..Default::default()
        };
        self.adapter.set_discovery_filter(filter).await.ok();
        Ok(())
    }

    /// Remove a device from BlueZ's cache
    ///
    /// This can help clear stale state that causes connection failures.
    /// Use this when repeated connection attempts fail.
    pub async fn remove_device(&self, address: Address) -> Result<()> {
        self.adapter
            .remove_device(address)
            .await
            .map_err(|e| BleError::ConnectionFailed(format!("Failed to remove device: {}", e)))?;
        log::debug!("Removed device {} from BlueZ cache", address);
        Ok(())
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
        let state = self.state.clone();
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
                                    // Get device properties from advertisement data only
                                    // IMPORTANT: Do NOT call device.uuids() as it may trigger
                                    // service discovery which causes unwanted pairing requests
                                    let name = device.name().await.ok().flatten();
                                    let rssi = device.rssi().await.ok().flatten().unwrap_or(0);

                                    // Get service data from advertisement (no connection needed)
                                    let service_data = device.service_data().await.ok().flatten().unwrap_or_default();

                                    // The 16-bit UUID 0xF47A expands to this 128-bit form
                                    let service_uuid_16bit =
                                        uuid::Uuid::parse_str("0000F47A-0000-1000-8000-00805F9B34FB").unwrap();

                                    // Check if Eche service UUID is present in advertisement
                                    // Check both the full UUID and the 16-bit alias
                                    let has_eche_service = service_data.contains_key(&ECHE_SERVICE_UUID)
                                        || service_data.contains_key(&service_uuid_16bit);

                                    // Check if name indicates Eche node (fallback)
                                    // Supports both formats:
                                    // - New: ECHE_<MESH>-<NODE_ID> (e.g., "ECHE_WEARTAK-8DD4")
                                    // - Legacy: ECHE-<NODE_ID> (e.g., "ECHE-12345678")
                                    let name_indicates_eche = name.as_ref().map(|n| {
                                        n.starts_with("ECHE_") || n.starts_with("ECHE-")
                                    }).unwrap_or(false);

                                    // Eche node detection: prefer service UUID, fallback to name
                                    let is_eche_node = has_eche_service || name_indicates_eche;

                                    // Parse node ID from name (supports both formats)
                                    let mut node_id = name.as_ref().and_then(|n| {
                                        crate::config::MeshConfig::parse_device_name(n)
                                            .map(|(_, node_id)| node_id)
                                    });

                                    // If not found in name, try to extract from service data
                                    // Service data format: [nodeId:4 bytes BE][meshId:UTF-8]
                                    if node_id.is_none() {
                                        if let Some(data) = service_data.get(&service_uuid_16bit)
                                            .or_else(|| service_data.get(&ECHE_SERVICE_UUID))
                                        {
                                            if data.len() >= 4 {
                                                let id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                                                node_id = Some(NodeId::new(id));
                                            }
                                        }
                                    }

                                    let discovered = DiscoveredDevice {
                                        address: addr.to_string(),
                                        name: name.clone(),
                                        rssi: rssi as i8,
                                        is_eche_node,
                                        node_id,
                                        adv_data: Vec::new(),
                                    };

                                    // Register node ID → address mapping so connect() can find the peer
                                    if let Some(nid) = node_id {
                                        let mut s = state.write().await;
                                        s.address_to_node.insert(addr, nid);
                                        s.node_to_address.insert(nid, addr);
                                    }

                                    log::debug!(
                                        "Discovered device: {} (Eche: {}, service_uuid: {}, name: {})",
                                        discovered.address, is_eche_node, has_eche_service, name_indicates_eche
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
            "BLE advertising started for Eche-{:08X}",
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

        // Trust the device to prevent pairing prompts on the remote side
        // This tells BlueZ we accept this device without requiring user confirmation
        if let Err(e) = device.set_trusted(true).await {
            log::warn!("Failed to set device as trusted: {}", e);
        }

        // Pause advertising during connection to avoid le-connection-abort-by-local.
        // Single-radio BLE adapters often can't advertise and initiate connections
        // simultaneously. We temporarily drop the advertisement handle then restart.
        let had_advertising = self.adv_handle.read().await.is_some();
        if had_advertising {
            log::debug!("Pausing advertising for connection to {}", address);
            *self.adv_handle.write().await = None;
            // Small delay for the adapter to finish stopping
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        // Connect to the device
        let connect_result = device.connect().await;

        // Restart advertising if it was active
        if had_advertising {
            log::debug!("Resuming advertising after connection attempt");
            let ble_config = self.config.read().await;
            if let Some(ref cfg) = *ble_config {
                let adv = self.build_advertisement(cfg);
                match self.adapter.advertise(adv).await {
                    Ok(handle) => *self.adv_handle.write().await = Some(handle),
                    Err(e) => log::warn!("Failed to restart advertising: {}", e),
                }
            }
        }

        connect_result
            .map_err(|e| BleError::ConnectionFailed(format!("Failed to connect: {}", e)))?;

        // Create connection wrapper
        let connection = BluerConnection::new(*peer_id, device).await?;

        // Store connection
        {
            let mut state = self.state.write().await;
            state.connections.insert(*peer_id, connection.clone());
        }

        // Notify callback
        if let Some(ref cb) = *self.connection_callback.read().await {
            cb(
                *peer_id,
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
                    *peer_id,
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

        // Build GATT application with Eche service
        let app = Application {
            services: vec![Service {
                uuid: ECHE_SERVICE_UUID,
                primary: true,
                characteristics: vec![
                    // Node Info characteristic (0001) - READ
                    Characteristic {
                        uuid: EcheCharacteristicUuids::node_info(),
                        read: Some(CharacteristicRead {
                            read: true,
                            fun: Box::new(move |req| {
                                let state = state_read_node.clone();
                                Box::pin(async move {
                                    // Track peer MTU from GATT request
                                    state.update_peer_mtu(req.device_address, req.mtu).await;
                                    let data = state.node_info.lock().await;
                                    log::debug!(
                                        "GATT read node_info from {:?}: {} bytes (MTU={})",
                                        req.device_address,
                                        data.len(),
                                        req.mtu
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
                        uuid: EcheCharacteristicUuids::sync_state(),
                        read: Some(CharacteristicRead {
                            read: true,
                            fun: Box::new(move |req| {
                                let state = state_read_sync.clone();
                                Box::pin(async move {
                                    // Track peer MTU from GATT request
                                    state.update_peer_mtu(req.device_address, req.mtu).await;
                                    let data = state.sync_state.lock().await;
                                    log::debug!(
                                        "GATT read sync_state from {:?}: {} bytes (MTU={})",
                                        req.device_address,
                                        data.len(),
                                        req.mtu
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
                        uuid: EcheCharacteristicUuids::sync_data(),
                        write: Some(CharacteristicWrite {
                            write: true,
                            method: CharacteristicWriteMethod::Fun(Box::new(move |data, req| {
                                let state = state_write_sync.clone();
                                Box::pin(async move {
                                    // Track peer MTU from GATT request
                                    state.update_peer_mtu(req.device_address, req.mtu).await;
                                    log::debug!(
                                        "GATT write sync_data from {:?}: {} bytes (MTU={})",
                                        req.device_address,
                                        data.len(),
                                        req.mtu
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
                        uuid: EcheCharacteristicUuids::command(),
                        write: Some(CharacteristicWrite {
                            write: true,
                            write_without_response: true,
                            method: CharacteristicWriteMethod::Fun(Box::new(move |data, req| {
                                let state = state_write_cmd.clone();
                                Box::pin(async move {
                                    // Track peer MTU from GATT request
                                    state.update_peer_mtu(req.device_address, req.mtu).await;
                                    log::debug!(
                                        "GATT write command from {:?}: {} bytes (MTU={})",
                                        req.device_address,
                                        data.len(),
                                        req.mtu
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
                        uuid: EcheCharacteristicUuids::status(),
                        read: Some(CharacteristicRead {
                            read: true,
                            fun: Box::new(move |req| {
                                let state = state_read_status.clone();
                                Box::pin(async move {
                                    // Track peer MTU from GATT request
                                    state.update_peer_mtu(req.device_address, req.mtu).await;
                                    let data = state.status.lock().await;
                                    log::debug!(
                                        "GATT read status from {:?}: {} bytes (MTU={})",
                                        req.device_address,
                                        data.len(),
                                        req.mtu
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
