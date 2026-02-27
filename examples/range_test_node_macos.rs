//! Range Test Node - macOS BLE node for field testing WearTAK
//!
//! This binary runs on macOS and acts as an Peat mesh node that:
//! 1. Uses the same encrypted genesis as WearTAK watches
//! 2. Logs all discovered devices with RSSI and timestamps
//! 3. Can connect to discovered watches to sync data
//!
//! Usage:
//!   cargo run --features macos --example range_test_node_macos -- [--callsign BASESTATION] [--output test.log]
//!
//! Build:
//!   cargo build --release --features macos --example range_test_node_macos

use peat_btle::{
    config::BleConfig,
    platform::{apple::CoreBluetoothAdapter, BleAdapter, DiscoveredDevice},
    security::MeshGenesis,
    PeatMesh, PeatMeshConfig,
};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// WEARTAK shared genesis - same as in PeatBtleService.kt
/// MESH_ID: 29C916FA (decoded from base64)
const WEARTAK_GENESIS_BYTES: &[u8] = &[
    0x07, 0x00, 0x57, 0x45, 0x41, 0x52, 0x54, 0x41, 0x4B, 0xE0, 0xEE, 0xED, 0x84, 0x0D, 0x37, 0x75,
    0x75, 0xC1, 0x36, 0x44, 0xFE, 0x80, 0x6D, 0xB6, 0x69, 0x34, 0x46, 0x20, 0x21, 0x02, 0x71, 0x7E,
    0x51, 0x1E, 0xD3, 0xA0, 0x21, 0xD2, 0xC1, 0xAD, 0xBE, 0xED, 0x53, 0xB2, 0xD3, 0xC6, 0x41, 0x4B,
    0x08, 0xB3, 0xFE, 0x0D, 0xED, 0xB5, 0x20, 0x02, 0xD2, 0x5C, 0x06, 0xE2, 0xE9, 0x94, 0x7F, 0x73,
    0x75, 0x57, 0x5B, 0xD9, 0x4A, 0x59, 0xA7, 0x1B, 0x33, 0x46, 0x2A, 0x7C, 0xAF, 0x67, 0xE4, 0x95,
    0xEA, 0xA1, 0xBE, 0xFF, 0xB2, 0xD2, 0x0C, 0xEB, 0x79, 0xC1, 0x30, 0xBC, 0xC9, 0x88, 0x54, 0xC6,
    0x97, 0xD1, 0x3A, 0xC1, 0xC1, 0x7C, 0x1B, 0x3D, 0x20, 0x51, 0xEA, 0xD8, 0xD8, 0x9B, 0x01, 0x00,
    0x00, 0x00,
];

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

struct TestState {
    log_file: Option<std::fs::File>,
    sos_active: bool,
    sos_start_time: Option<u64>,
    last_rssi: i16,
    peers_seen: std::collections::HashMap<u32, PeerInfo>,
    /// Track Peat peripherals by their CoreBluetooth identifier (for devices without node ID in name)
    peat_peripherals: std::collections::HashMap<String, PeatPeripheral>,
}

struct PeerInfo {
    callsign: Option<String>,
    last_seen: u64,
    last_rssi: i16,
    emergency: bool,
    address: String,
}

/// Track a discovered Peat peripheral before we know its node ID
struct PeatPeripheral {
    identifier: String,
    name: Option<String>,
    last_seen: u64,
    last_rssi: i16,
    /// Connection state
    connection_state: ConnectionState,
    /// Discovered node ID (after reading from GATT)
    node_id: Option<u32>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum ConnectionState {
    Discovered,
    Connecting,
    Connected,
    DiscoveringServices,
    DiscoveringCharacteristics,
    ReadingNodeInfo,
    Ready, // Fully connected with node ID known
    Failed,
}

impl TestState {
    fn new(log_path: Option<&str>) -> Self {
        let log_file = log_path.map(|path| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .expect("Failed to open log file")
        });

        Self {
            log_file,
            sos_active: false,
            sos_start_time: None,
            last_rssi: -999,
            peers_seen: std::collections::HashMap::new(),
            peat_peripherals: std::collections::HashMap::new(),
        }
    }

    /// Get peripherals that are ready to connect (discovered but not yet connecting/connected)
    fn get_connectable_peripherals(&self) -> Vec<String> {
        self.peat_peripherals
            .iter()
            .filter(|(_, p)| p.connection_state == ConnectionState::Discovered)
            .map(|(id, _)| id.clone())
            .collect()
    }

    fn log(&mut self, msg: &str) {
        let timestamp = now_ms();
        let line = format!("[{}] {}", timestamp, msg);
        println!("{}", line);

        if let Some(ref mut file) = self.log_file {
            writeln!(file, "{}", line).ok();
            file.flush().ok();
        }
    }

    fn log_structured(
        &mut self,
        event_type: &str,
        node_id: u32,
        rssi: i16,
        callsign: Option<&str>,
        lat: Option<f32>,
        lon: Option<f32>,
    ) {
        let timestamp = now_ms();
        // Format: RANGE_TEST|timestamp|event|node_id|rssi|callsign|lat|lon|sos_active
        let line = format!(
            "RANGE_TEST|{}|{}|{:08X}|{}|{}|{}|{}|{}",
            timestamp,
            event_type,
            node_id,
            rssi,
            callsign.unwrap_or("?"),
            lat.map(|v| format!("{:.6}", v))
                .unwrap_or_else(|| "0".to_string()),
            lon.map(|v| format!("{:.6}", v))
                .unwrap_or_else(|| "0".to_string()),
            if self.sos_active { "SOS" } else { "NORMAL" }
        );
        println!("{}", line);

        if let Some(ref mut file) = self.log_file {
            writeln!(file, "{}", line).ok();
            file.flush().ok();
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Parse args
    let args: Vec<String> = std::env::args().collect();
    let callsign = args
        .iter()
        .position(|a| a == "--callsign")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("BASESTATION");
    let output_path = args
        .iter()
        .position(|a| a == "--output")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());
    let debug_scan = args.iter().any(|a| a == "--debug");

    // Decode WEARTAK genesis
    let genesis =
        MeshGenesis::decode(WEARTAK_GENESIS_BYTES).ok_or("Failed to decode WEARTAK genesis")?;

    let mesh_id = genesis.mesh_id();

    // Use a fixed node ID for stable testing
    let node_id = peat_btle::NodeId::new(0xBA5E0001); // "BASE-0001"

    log::info!("================================================");
    log::info!("WearTAK Range Test Node (macOS)");
    log::info!("================================================");
    log::info!("Node ID:    0x{:08X}", node_id.as_u32());
    log::info!("Callsign:   {}", callsign);
    log::info!("Mesh ID:    {} (WEARTAK)", mesh_id);
    log::info!("Mesh Name:  {}", genesis.mesh_name);
    log::info!("Output:     {}", output_path.unwrap_or("stdout only"));
    log::info!("================================================");

    // Create test state
    let test_state = Arc::new(RwLock::new(TestState::new(output_path)));

    // Log header
    {
        let mut state = test_state.write().await;
        state.log("=== Range Test Started (macOS) ===");
        state.log(&format!("Node: {:08X} ({})", node_id.as_u32(), callsign));
        state.log(&format!("Mesh: {} ({})", mesh_id, genesis.mesh_name));
    }

    // Create mesh with encryption credentials from genesis
    let mesh_config = PeatMeshConfig::new(node_id, callsign, &mesh_id)
        .with_encryption(genesis.encryption_secret());
    let mesh = Arc::new(RwLock::new(PeatMesh::new(mesh_config)));

    log::info!("Mesh initialized with encryption, starting BLE adapter...");

    // Create CoreBluetooth adapter
    let mut adapter = CoreBluetoothAdapter::new()?;

    log::info!("CoreBluetooth adapter created");

    // Configure BLE with the WEARTAK mesh ID
    let mut ble_config = BleConfig::new(node_id);
    ble_config.mesh.mesh_id = mesh_id.to_string();
    adapter.init(&ble_config).await?;

    // Our own node ID for filtering self-discovery
    let our_node_id = node_id;

    // Discovery callback - log discovered devices
    let test_state_discovery = test_state.clone();
    let mesh_clone = mesh.clone();
    let debug_mode = debug_scan;
    adapter.set_discovery_callback(Some(Arc::new(move |device: DiscoveredDevice| {
        // In debug mode, log ALL devices; otherwise only Peat nodes
        if device.is_peat_node || debug_mode {
            let state = test_state_discovery.clone();
            let device_clone = device.clone();
            let _mesh = mesh_clone.clone();

            tokio::spawn(async move {
                let address = device_clone.address.clone();
                let node_id_opt = device_clone.node_id;

                // Filter out self-discovery (our own advertisement)
                // CoreBluetooth truncates names, so "PEAT-BA5E0001" becomes "PEAT-BA5E0"
                // Check by name pattern (hex prefix match) or exact node ID
                let our_hex = format!("{:08X}", our_node_id.as_u32());
                let is_self = device_clone
                    .name
                    .as_ref()
                    .map(|n| {
                        // Check if name contains our node ID (or truncated prefix)
                        // Our ID: BA5E0001 -> name might be PEAT-BA5E0 (truncated)
                        if let Some(suffix) = n.strip_prefix("PEAT-") {
                            // Check if our hex starts with the advertised suffix
                            our_hex.starts_with(&suffix.to_uppercase())
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false)
                    || node_id_opt.map(|nid| nid == our_node_id).unwrap_or(false);

                if is_self {
                    log::trace!(
                        "Ignoring self-discovery: {}",
                        device_clone.name.as_deref().unwrap_or("?")
                    );
                    return;
                }

                // Log discovery
                {
                    let mut state = state.write().await;
                    let peat_marker = if device_clone.is_peat_node {
                        "[PEAT]"
                    } else {
                        "[other]"
                    };
                    state.log(&format!(
                        "DISCOVERED {}: {} ({}) RSSI={} NodeID={:?}",
                        peat_marker,
                        device_clone.name.as_deref().unwrap_or("?"),
                        address,
                        device_clone.rssi,
                        node_id_opt.map(|n| format!("{:08X}", n.as_u32()))
                    ));

                    if let Some(nid) = node_id_opt {
                        // Update peer info from advertisement
                        let peer = state.peers_seen.entry(nid.as_u32()).or_insert(PeerInfo {
                            callsign: None,
                            last_seen: 0,
                            last_rssi: -999,
                            emergency: false,
                            address: address.clone(),
                        });
                        peer.last_seen = now_ms();
                        peer.last_rssi = device_clone.rssi as i16;
                        peer.address = address.clone();

                        // Try to extract callsign from name
                        if let Some(name) = &device_clone.name {
                            // Format: PEAT_<MESH>-<CALLSIGN>-<SHORT_ID> or PEAT-<NODE_ID>
                            if let Some(rest) = name.strip_prefix("PEAT_") {
                                // PEAT_WEARTAK-RANGER-8DD4
                                if let Some(after_mesh) = rest.split('-').nth(1) {
                                    peer.callsign = Some(after_mesh.to_string());
                                }
                            }
                        }

                        state.log_structured(
                            "DISCOVERY",
                            nid.as_u32(),
                            device_clone.rssi as i16,
                            device_clone.name.as_deref(),
                            None,
                            None,
                        );
                    }

                    // Also track by CoreBluetooth identifier for Peat devices
                    // This is important for devices advertising F47A but without PEAT-style name
                    if device_clone.is_peat_node {
                        let peripheral = state.peat_peripherals.entry(address.clone()).or_insert(
                            PeatPeripheral {
                                identifier: address.clone(),
                                name: device_clone.name.clone(),
                                last_seen: 0,
                                last_rssi: -999,
                                connection_state: ConnectionState::Discovered,
                                node_id: node_id_opt.map(|n| n.as_u32()),
                            },
                        );
                        peripheral.last_seen = now_ms();
                        peripheral.last_rssi = device_clone.rssi as i16;
                        if peripheral.name.is_none() {
                            peripheral.name = device_clone.name.clone();
                        }
                    }
                }
            });
        }
    })));

    // Register GATT service
    adapter.register_gatt_service().await?;
    log::info!("GATT service registered");

    // Wrap in Arc for shared ownership
    let adapter = Arc::new(adapter);

    // Start advertising and scanning
    if debug_scan {
        // In debug mode, start without filtering to see ALL BLE devices
        log::info!("DEBUG MODE: Scanning for ALL BLE devices (no filter)");
        adapter.start().await?; // This starts advertising
        adapter.start_scan_unfiltered().await?; // Override with unfiltered scan
    } else {
        adapter.start().await?;
    }

    log::info!("================================================");
    log::info!("Range Test Node RUNNING (macOS)");
    log::info!("Advertising as: PEAT-{:08X}", node_id.as_u32());
    log::info!("GATT service ready for incoming connections");
    log::info!("");
    if debug_scan {
        log::info!("MODE: DEBUG - scanning ALL BLE devices");
    } else {
        log::info!("MODE: Passive - listening for Peat advertisements");
    }
    log::info!("      CoreBluetooth will handle connections automatically");
    log::info!("");
    log::info!("TEST PROTOCOL:");
    log::info!("  1. Ensure WearTAK is active on watches");
    log::info!("  2. Walk around to test range");
    log::info!("  3. Watch RSSI values in log");
    log::info!("");
    log::info!("Press Ctrl+C to stop");
    log::info!("================================================");

    // Main event loop
    let mut interval = tokio::time::interval(Duration::from_millis(100));
    let mut tick_count = 0u64;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                tick_count += 1;
                let now = now_ms();

                // CRITICAL: Poll the adapter to process CoreBluetooth events
                // This is what triggers discovery callbacks
                if let Err(e) = adapter.poll().await {
                    log::warn!("Adapter poll error: {}", e);
                }

                // Run mesh tick every second (100 * 10ms = 1s, but we're at 100ms)
                if tick_count % 10 == 0 {
                    let mesh_guard = mesh.read().await;
                    let _ = mesh_guard.tick(now);
                }

                // Status update every 10 seconds
                if tick_count % 100 == 0 {
                    let state = test_state.read().await;
                    let peer_count = state.peers_seen.len();
                    let active_peers = state.peers_seen.values()
                        .filter(|p| now.saturating_sub(p.last_seen) < 30000)
                        .count();

                    log::info!(
                        "Status: {} peers seen, {} active in last 30s",
                        peer_count, active_peers
                    );

                    // Log active peers with their last RSSI
                    for (nid, peer) in &state.peers_seen {
                        if now.saturating_sub(peer.last_seen) < 30000 {
                            let age = (now.saturating_sub(peer.last_seen)) / 1000;
                            log::info!(
                                "  - {:08X} ({}): RSSI={} ({}s ago)",
                                nid,
                                peer.callsign.as_deref().unwrap_or("?"),
                                peer.last_rssi,
                                age
                            );
                        }
                    }

                    // Log Peat peripherals by identifier (devices without node ID in name)
                    let peat_count = state.peat_peripherals.len();
                    let connectable: Vec<_> = state.peat_peripherals.values()
                        .filter(|p| p.connection_state == ConnectionState::Discovered && now.saturating_sub(p.last_seen) < 10000)
                        .collect();
                    if peat_count > 0 {
                        log::info!("  Peat peripherals: {} total, {} ready to connect", peat_count, connectable.len());
                        for p in &connectable {
                            log::info!(
                                "    - {} ({}): RSSI={}",
                                p.name.as_deref().unwrap_or("?"),
                                &p.identifier[..8],
                                p.last_rssi
                            );
                        }
                    }
                }

                // Attempt to connect to discovered Peat peripherals every 5 seconds
                if tick_count % 50 == 25 {
                    // Get a connectable peripheral
                    let connectable = {
                        let state = test_state.read().await;
                        state.get_connectable_peripherals().into_iter().next()
                    };

                    if let Some(identifier) = connectable {
                        log::info!("Attempting to connect to Peat peripheral: {}...", &identifier[..8]);

                        // Mark as connecting
                        {
                            let mut state = test_state.write().await;
                            if let Some(p) = state.peat_peripherals.get_mut(&identifier) {
                                p.connection_state = ConnectionState::Connecting;
                            }
                        }

                        // Attempt connection
                        match adapter.connect_by_identifier(&identifier).await {
                            Ok(()) => {
                                log::info!("Connection initiated to {}", &identifier[..8]);
                            }
                            Err(e) => {
                                log::warn!("Failed to connect to {}: {}", &identifier[..8], e);
                                // Mark as failed
                                let mut state = test_state.write().await;
                                if let Some(p) = state.peat_peripherals.get_mut(&identifier) {
                                    p.connection_state = ConnectionState::Failed;
                                }
                            }
                        }
                    }
                }

                // Check for peripherals in Connecting state and verify if connected
                // If connected, start service discovery
                if tick_count % 10 == 5 {
                    let connecting_peripherals: Vec<String> = {
                        let state = test_state.read().await;
                        state.peat_peripherals
                            .iter()
                            .filter(|(_, p)| p.connection_state == ConnectionState::Connecting)
                            .map(|(id, _)| id.clone())
                            .collect()
                    };

                    for identifier in connecting_peripherals {
                        // Check if the peripheral is now connected
                        if let Some(info) = adapter.get_peripheral_info(&identifier).await {
                            if info.connected {
                                log::info!("Peripheral {} now connected, discovering services...", &identifier[..8]);

                                // Update state and start service discovery
                                {
                                    let mut state = test_state.write().await;
                                    if let Some(p) = state.peat_peripherals.get_mut(&identifier) {
                                        p.connection_state = ConnectionState::DiscoveringServices;
                                    }
                                }

                                // Discover Peat GATT service
                                match adapter.discover_services(&identifier).await {
                                    Ok(()) => {
                                        log::info!("Service discovery initiated for {}", &identifier[..8]);
                                    }
                                    Err(e) => {
                                        log::warn!("Failed to discover services on {}: {}", &identifier[..8], e);
                                    }
                                }
                            }
                        }
                    }
                }

                // Process peripheral events (service discovery, characteristic updates, etc.)
                while let Some(event) = adapter.try_recv_peripheral_event().await {
                    use peat_btle::platform::apple::PeripheralEvent;
                    match event {
                        PeripheralEvent::ServicesDiscovered { identifier, error } => {
                            if let Some(err) = error {
                                log::warn!("Service discovery failed for {}: {}", &identifier[..8.min(identifier.len())], err);
                            } else {
                                log::info!("Services discovered on {}, discovering characteristics...", &identifier[..8.min(identifier.len())]);

                                // Update state
                                {
                                    let mut state = test_state.write().await;
                                    if let Some(p) = state.peat_peripherals.get_mut(&identifier) {
                                        p.connection_state = ConnectionState::DiscoveringCharacteristics;
                                    }
                                }

                                // Discover characteristics for Peat service
                                if let Err(e) = adapter.discover_characteristics(&identifier).await {
                                    log::warn!("Failed to discover characteristics: {}", e);
                                }
                            }
                        }
                        PeripheralEvent::CharacteristicsDiscovered { identifier, service_uuid, error } => {
                            if let Some(err) = error {
                                log::warn!("Characteristic discovery failed for {}: {}", &identifier[..8.min(identifier.len())], err);
                            } else {
                                log::info!("Characteristics discovered for service {} on {}, reading node_info...",
                                    service_uuid, &identifier[..8.min(identifier.len())]);

                                // Update state
                                {
                                    let mut state = test_state.write().await;
                                    if let Some(p) = state.peat_peripherals.get_mut(&identifier) {
                                        p.connection_state = ConnectionState::ReadingNodeInfo;
                                    }
                                }

                                // Try reading node_info with various UUID formats
                                // WearTAK uses format like F47A0001-58CC-4372-A567-0E02B2C3D479
                                let node_info_uuids = [
                                    "F47A0001-58CC-4372-A567-0E02B2C3D479",  // Full Peat format
                                    "0001",                                   // Short format
                                    "00000001-0000-1000-8000-00805F9B34FB",  // Bluetooth SIG base
                                ];

                                let mut read_success = false;
                                for uuid in &node_info_uuids {
                                    if adapter.read_characteristic(&identifier, uuid).await.is_ok() {
                                        log::info!("Reading node_info using UUID: {}", uuid);
                                        read_success = true;
                                        break;
                                    }
                                }

                                if !read_success {
                                    // If node_info not available, try reading sync_data (0003)
                                    // which might contain peer info
                                    log::warn!("node_info not found, trying sync_data...");
                                    if let Err(e) = adapter.read_characteristic(&identifier, "F47A0003-58CC-4372-A567-0E02B2C3D479").await {
                                        log::warn!("Failed to read sync_data: {}", e);
                                    }
                                }
                            }
                        }
                        PeripheralEvent::CharacteristicValueUpdated { identifier, characteristic_uuid, value, error } => {
                            if let Some(err) = error {
                                log::warn!("Characteristic read failed: {}", err);
                            } else {
                                log::info!("Received {} bytes from char {} on {}",
                                    value.len(), characteristic_uuid, &identifier[..8.min(identifier.len())]);

                                // Log raw data for debugging
                                if !value.is_empty() {
                                    log::debug!("Raw data: {:02X?}", &value[..value.len().min(32)]);
                                }

                                // If this is node_info (0001), extract the node ID
                                if characteristic_uuid.contains("0001") {
                                    if value.len() >= 4 {
                                        let node_id = u32::from_le_bytes([value[0], value[1], value[2], value[3]]);
                                        log::info!("*** DISCOVERED NODE ID: {:08X} for {} ***", node_id, &identifier[..8.min(identifier.len())]);

                                        // Update state with the discovered node ID
                                        let mut state = test_state.write().await;
                                        if let Some(p) = state.peat_peripherals.get_mut(&identifier) {
                                            p.node_id = Some(node_id);
                                            p.connection_state = ConnectionState::Ready;
                                        }

                                        // Also add to peers_seen
                                        let peer = state.peers_seen.entry(node_id).or_insert(PeerInfo {
                                            callsign: None,
                                            last_seen: 0,
                                            last_rssi: -999,
                                            emergency: false,
                                            address: identifier.clone(),
                                        });
                                        peer.last_seen = now_ms();
                                        peer.address = identifier.clone();

                                        state.log(&format!("NODE_ID_DISCOVERED: {:08X} at {}", node_id, &identifier[..8.min(identifier.len())]));
                                    }
                                }
                                // If this is sync_data (0003), it contains encrypted Peat documents
                                else if characteristic_uuid.contains("0003") {
                                    log::info!("Received sync_data ({} bytes) from {}", value.len(), &identifier[..8.min(identifier.len())]);

                                    // Process through PeatMesh to decrypt and extract data
                                    let result = {
                                        let mesh_guard = mesh.read().await;
                                        mesh_guard.on_ble_data_received_anonymous(&identifier, &value, now_ms())
                                    };

                                    if let Some(data_result) = result {
                                        let source_node = data_result.source_node.as_u32();
                                        log::info!("*** DECRYPTED SYNC FROM NODE {:08X} ***", source_node);

                                        // Log rich data from the document
                                        if let Some(ref callsign) = data_result.callsign {
                                            log::info!("  Callsign: {}", callsign);
                                        }
                                        if let Some(lat) = data_result.latitude {
                                            if let Some(lon) = data_result.longitude {
                                                log::info!("  Location: {:.6}, {:.6}", lat, lon);
                                            }
                                        }
                                        if let Some(battery) = data_result.battery_percent {
                                            log::info!("  Battery: {}%", battery);
                                        }
                                        if let Some(hr) = data_result.heart_rate {
                                            log::info!("  Heart Rate: {} bpm", hr);
                                        }
                                        if data_result.is_emergency {
                                            log::warn!("  *** EMERGENCY ACTIVE ***");
                                        }

                                        // Update state with the discovered node ID
                                        let mut state = test_state.write().await;
                                        if let Some(p) = state.peat_peripherals.get_mut(&identifier) {
                                            p.node_id = Some(source_node);
                                            p.connection_state = ConnectionState::Ready;
                                        }

                                        // Add/update peers_seen and capture rssi for logging
                                        let last_rssi = {
                                            let peer = state.peers_seen.entry(source_node).or_insert(PeerInfo {
                                                callsign: None,
                                                last_seen: 0,
                                                last_rssi: -999,
                                                emergency: false,
                                                address: identifier.clone(),
                                            });
                                            peer.last_seen = now_ms();
                                            peer.address = identifier.clone();
                                            peer.emergency = data_result.is_emergency;
                                            if data_result.callsign.is_some() {
                                                peer.callsign = data_result.callsign.clone();
                                            }
                                            peer.last_rssi
                                        };

                                        state.log_structured(
                                            "SYNC_RECEIVED",
                                            source_node,
                                            last_rssi,
                                            data_result.callsign.as_deref(),
                                            data_result.latitude,
                                            data_result.longitude,
                                        );

                                        // Bidirectional sync: write our document back to the watch
                                        let our_doc = {
                                            let mesh_guard = mesh.read().await;
                                            mesh_guard.build_document()
                                        };
                                        log::info!("Writing our sync document ({} bytes) to {}", our_doc.len(), &identifier[..8.min(identifier.len())]);
                                        if let Err(e) = adapter.write_characteristic(
                                            &identifier,
                                            "F47A0003-58CC-4372-A567-0E02B2C3D479",
                                            &our_doc,
                                            true,  // with response
                                        ).await {
                                            log::warn!("Failed to write sync_data to {}: {}", &identifier[..8.min(identifier.len())], e);
                                        }
                                    } else {
                                        log::warn!("Failed to decrypt sync_data from {} - wrong mesh key?", &identifier[..8.min(identifier.len())]);
                                        let mut state = test_state.write().await;
                                        state.log(&format!("SYNC_DECRYPT_FAILED: {} bytes from {}", value.len(), &identifier[..8.min(identifier.len())]));
                                    }
                                }
                            }
                        }
                        PeripheralEvent::CharacteristicWritten { identifier, characteristic_uuid, error } => {
                            if let Some(err) = error {
                                log::warn!("Write to {} failed: {}", characteristic_uuid, err);
                            } else {
                                log::debug!("Write to {} succeeded on {}", characteristic_uuid, &identifier[..8.min(identifier.len())]);
                            }
                        }
                        _ => {
                            log::trace!("Unhandled peripheral event");
                        }
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                log::info!("Shutting down...");
                break;
            }
        }
    }

    // Cleanup
    {
        let mut state = test_state.write().await;
        state.log("=== Range Test Ended ===");
    }

    adapter.stop().await?;
    log::info!("Range test node stopped");

    Ok(())
}
