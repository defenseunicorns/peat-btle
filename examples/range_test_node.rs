//! Range Test Node - Linux BLE node for field testing WearTAK
//!
//! This binary runs on a Linux machine and acts as a HIVE mesh node that:
//! 1. Uses the same encrypted genesis as WearTAK watches
//! 2. Logs all received documents with RSSI and timestamps
//! 3. Detects SOS/emergency events as test markers
//! 4. Writes structured test data for analysis
//!
//! Usage:
//!   sudo ./range_test_node [--callsign BASESTATION] [--output test.log]
//!
//! Build:
//!   cargo build --release --features linux --example range_test_node

use hive_btle::{
    config::BleConfig,
    gatt::HiveCharacteristicUuids,
    platform::{linux::BluerAdapter, BleAdapter, DiscoveredDevice},
    security::MeshGenesis,
    HiveMesh, HiveMeshConfig, HIVE_SERVICE_UUID,
};
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{mpsc, RwLock};

/// WEARTAK shared genesis - same as in HiveBtleService.kt
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
    /// Addresses we're currently trying to connect to (to avoid duplicate attempts)
    connecting: HashSet<String>,
}

struct PeerInfo {
    callsign: Option<String>,
    last_seen: u64,
    last_rssi: i16,
    emergency: bool,
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
            connecting: HashSet::new(),
        }
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

    fn log_structured(&mut self, event_type: &str, node_id: u32, rssi: i16, callsign: Option<&str>, lat: Option<f32>, lon: Option<f32>) {
        let timestamp = now_ms();
        // Format: RANGE_TEST|timestamp|event|node_id|rssi|callsign|lat|lon|sos_active
        let line = format!(
            "RANGE_TEST|{}|{}|{:08X}|{}|{}|{}|{}|{}",
            timestamp,
            event_type,
            node_id,
            rssi,
            callsign.unwrap_or("?"),
            lat.map(|v| format!("{:.6}", v)).unwrap_or_else(|| "0".to_string()),
            lon.map(|v| format!("{:.6}", v)).unwrap_or_else(|| "0".to_string()),
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

    // Decode WEARTAK genesis
    let genesis = MeshGenesis::decode(WEARTAK_GENESIS_BYTES)
        .ok_or("Failed to decode WEARTAK genesis")?;

    let mesh_id = genesis.mesh_id();

    // Use a fixed node ID for stable testing (derived from "BASESTATION")
    // This ensures the advertisement name stays consistent across restarts
    let node_id = hive_btle::NodeId::new(0xBA5E0001); // "BASE-0001"

    log::info!("================================================");
    log::info!("WearTAK Range Test Node");
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
        state.log("=== Range Test Started ===");
        state.log(&format!("Node: {:08X} ({})", node_id.as_u32(), callsign));
        state.log(&format!("Mesh: {} ({})", mesh_id, genesis.mesh_name));
    }

    // Create mesh with encryption credentials from genesis
    let mesh_config = HiveMeshConfig::new(node_id, callsign, &mesh_id)
        .with_encryption(genesis.encryption_secret());
    let mesh = Arc::new(RwLock::new(HiveMesh::new(mesh_config)));

    log::info!("Mesh initialized with encryption, starting BLE adapter...");

    // Create BLE adapter
    let adapter = BluerAdapter::new().await?;

    log::info!(
        "Bluetooth adapter: {} ({})",
        adapter.adapter_name(),
        adapter.address().unwrap_or_else(|| "unknown".to_string())
    );

    // Configure BLE with the WEARTAK mesh ID
    let mut ble_config = BleConfig::new(node_id);
    ble_config.mesh.mesh_id = mesh_id.to_string();
    let mut adapter = adapter;
    adapter.init(&ble_config).await?;

    // Set adapter alias for scan response (matches Android's approach)
    // This is the name that will appear when devices scan for us
    let device_name = format!("HIVE-{:08X}", node_id.as_u32());
    adapter.set_adapter_alias(&device_name).await?;
    log::info!("Adapter alias set to: {}", device_name);

    // Create channel for discovered devices (used for active connections)
    let (discovery_tx, mut discovery_rx) = mpsc::channel::<DiscoveredDevice>(32);

    // Discovery callback - log and send to channel for connection attempts
    let test_state_discovery = test_state.clone();
    let discovery_tx_clone = discovery_tx.clone();
    adapter.set_discovery_callback(Some(Arc::new(move |device: DiscoveredDevice| {
        if device.is_hive_node {
            let state = test_state_discovery.clone();
            let device_clone = device.clone();
            let tx = discovery_tx_clone.clone();

            tokio::spawn(async move {
                let address = device_clone.address.clone();
                let node_id_opt = device_clone.node_id;

                // Log discovery
                {
                    let mut state = state.write().await;
                    state.log(&format!(
                        "DISCOVERED: {} ({}) RSSI={} NodeID={:?}",
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
                        });
                        peer.last_seen = now_ms();
                        peer.last_rssi = device_clone.rssi as i16;
                        if let Some(name) = &device_clone.name {
                            if let Some(cs) = name.strip_prefix("HIVE_").and_then(|s| s.split('-').next()) {
                                peer.callsign = Some(cs.to_string());
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
                }

                // Send to channel for connection attempt
                if node_id_opt.is_some() {
                    let _ = tx.send(device_clone).await;
                }
            });
        }
    })));

    // Register GATT service
    adapter.register_gatt_service().await?;
    log::info!("GATT service registered");

    // Wrap in Arc for shared ownership
    let adapter = Arc::new(adapter);

    // Sync data callback - process incoming documents
    let mesh_for_callback = mesh.clone();
    let test_state_sync = test_state.clone();
    adapter
        .set_sync_data_callback(move |data| {
            let mesh = mesh_for_callback.clone();
            let test_state = test_state_sync.clone();

            tokio::spawn(async move {
                let now = now_ms();
                let mesh_guard = mesh.read().await;

                if let Some(result) = mesh_guard.on_ble_data_received_anonymous("gatt-peer", &data, now) {
                    let mut state = test_state.write().await;

                    let node_id = result.source_node.as_u32();
                    let callsign = result.callsign.as_deref();
                    let is_emergency = result.is_emergency;

                    // Update peer info
                    let peer = state.peers_seen.entry(node_id).or_insert(PeerInfo {
                        callsign: None,
                        last_seen: 0,
                        last_rssi: -999,
                        emergency: false,
                    });
                    peer.last_seen = now;
                    if let Some(cs) = callsign {
                        peer.callsign = Some(cs.to_string());
                    }
                    peer.emergency = is_emergency;

                    // Check for SOS state change
                    if is_emergency && !state.sos_active {
                        state.sos_active = true;
                        state.sos_start_time = Some(now);
                        state.log("!!! SOS DETECTED - TEST MARKER START !!!");
                    } else if !is_emergency && state.sos_active {
                        if let Some(start) = state.sos_start_time {
                            let duration = now - start;
                            state.log(&format!("!!! SOS CLEARED - Duration: {}ms !!!", duration));
                        }
                        state.sos_active = false;
                        state.sos_start_time = None;
                    }

                    // Log the sync event
                    let event_type = if is_emergency { "SOS_SYNC" } else { "SYNC" };
                    let rssi = state.last_rssi;
                    state.log_structured(
                        event_type,
                        node_id,
                        rssi,
                        callsign,
                        result.latitude,
                        result.longitude,
                    );

                    log::info!(
                        "SYNC from {:08X} ({}): emergency={}, counter_changed={}",
                        node_id,
                        callsign.unwrap_or("?"),
                        is_emergency,
                        result.counter_changed
                    );
                }
            });
        })
        .await;

    // CRITICAL: Set initial sync_state with encrypted document BEFORE advertising
    // Without this, the GATT sync_state characteristic is empty and watches won't sync
    {
        let mesh_guard = mesh.read().await;
        let initial_doc = mesh_guard.build_document();
        adapter.update_sync_state(&initial_doc).await;
        log::info!("Initial sync_state set ({} bytes, encrypted)", initial_doc.len());
    }

    // Start advertising and scanning
    adapter.start().await?;

    log::info!("================================================");
    log::info!("Range Test Node RUNNING (ACTIVE MODE)");
    log::info!("Advertising as: HIVE-{:08X}", node_id.as_u32());
    log::info!("GATT service ready for incoming connections");
    log::info!("");
    log::info!("MODE: Active - will try to connect to discovered watches");
    log::info!("");
    log::info!("TEST PROTOCOL:");
    log::info!("  1. Ensure WearTAK is active on watches");
    log::info!("  2. We will discover and connect to watches");
    log::info!("  3. Tap SOS on watch to mark test events");
    log::info!("");
    log::info!("Press Ctrl+C to stop");
    log::info!("================================================");

    // Main event loop
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut tick_count = 0u64;

    // Clone adapter and mesh for connection task
    let adapter_for_connect = adapter.clone();
    let mesh_for_connect = mesh.clone();
    let test_state_for_connect = test_state.clone();

    loop {
        tokio::select! {
            // Process discovered devices for connection attempts
            Some(device) = discovery_rx.recv() => {
                if let Some(nid) = device.node_id {
                    let address = device.address.clone();

                    // Check if we're already connecting to this address
                    {
                        let mut state = test_state_for_connect.write().await;
                        if state.connecting.contains(&address) {
                            continue;
                        }
                        state.connecting.insert(address.clone());
                    }

                    let adapter = adapter_for_connect.clone();
                    let mesh = mesh_for_connect.clone();
                    let test_state = test_state_for_connect.clone();

                    // Spawn connection task
                    tokio::spawn(async move {
                        log::info!("Attempting to connect to {} ({:08X})...", address, nid.as_u32());

                        // Stop scanning before connecting (BlueZ can't do both at once)
                        let _ = adapter.stop_discovery().await;
                        tokio::time::sleep(Duration::from_millis(100)).await;

                        // Parse address
                        if let Ok(addr) = address.parse::<bluer::Address>() {
                            // Get the device handle
                            match adapter.get_device(addr) {
                                Ok(device) => {
                                    // Trust the device to avoid pairing prompts
                                    let _ = device.set_trusted(true).await;

                                    // Try to connect with retry
                                    let mut connected = false;
                                    for attempt in 1..=3 {
                                        log::info!("Connection attempt {} to {}", attempt, address);
                                        match device.connect().await {
                                            Ok(()) => {
                                                connected = true;
                                                break;
                                            }
                                            Err(e) => {
                                                log::warn!("Connection attempt {} failed: {}", attempt, e);
                                                tokio::time::sleep(Duration::from_millis(500)).await;
                                            }
                                        }
                                    }

                                    if connected {
                                        log::info!("Connected to {} ({:08X})", address, nid.as_u32());

                                            // Wait for services to be resolved
                                            tokio::time::sleep(Duration::from_millis(500)).await;

                                            // Find HIVE service
                                            match device.services().await {
                                                Ok(services) => {
                                                    // Find HIVE service by UUID
                                                    let mut hive_service = None;
                                                    for s in &services {
                                                        if let Ok(uuid) = s.uuid().await {
                                                            if uuid == HIVE_SERVICE_UUID {
                                                                hive_service = Some(s);
                                                                break;
                                                            }
                                                        }
                                                    }

                                                    if let Some(service) = hive_service {
                                                        log::info!("Found HIVE service on {}", address);

                                                        // Find sync_state characteristic
                                                        match service.characteristics().await {
                                                            Ok(chars) => {
                                                                let sync_state_uuid = HiveCharacteristicUuids::sync_state();
                                                                let sync_data_uuid = HiveCharacteristicUuids::sync_data();

                                                                // Find characteristics by UUID
                                                                let mut sync_state_char = None;
                                                                let mut sync_data_char = None;
                                                                for c in &chars {
                                                                    if let Ok(uuid) = c.uuid().await {
                                                                        if uuid == sync_state_uuid {
                                                                            sync_state_char = Some(c);
                                                                        } else if uuid == sync_data_uuid {
                                                                            sync_data_char = Some(c);
                                                                        }
                                                                    }
                                                                }

                                                                // Build our document
                                                                let mesh_guard = mesh.read().await;
                                                                let doc = mesh_guard.build_document();
                                                                drop(mesh_guard);

                                                                // Write our document to sync_data
                                                                if let Some(char) = sync_data_char {
                                                                    if let Err(e) = char.write(&doc).await {
                                                                        log::warn!("Failed to write sync_data: {}", e);
                                                                    } else {
                                                                        log::info!("Wrote {} bytes to sync_data", doc.len());
                                                                    }
                                                                }

                                                                // Read peer's sync_state
                                                                if let Some(char) = sync_state_char {
                                                                    match char.read().await {
                                                                        Ok(peer_doc) => {
                                                                            log::info!("Read {} bytes from peer sync_state", peer_doc.len());
                                                                            let mut state = test_state.write().await;
                                                                            state.log(&format!("SYNC_READ: {} bytes from {:08X}", peer_doc.len(), nid.as_u32()));

                                                                            // Process the document
                                                                            let now = now_ms();
                                                                            let mesh_guard = mesh.read().await;
                                                                            if let Some(result) = mesh_guard.on_ble_data_received_anonymous(&address, &peer_doc, now) {
                                                                                state.log(&format!(
                                                                                    "SYNC: {:08X} callsign={:?} emergency={} counter_changed={}",
                                                                                    result.source_node.as_u32(),
                                                                                    result.callsign,
                                                                                    result.is_emergency,
                                                                                    result.counter_changed
                                                                                ));
                                                                            }
                                                                        }
                                                                        Err(e) => {
                                                                            log::warn!("Failed to read sync_state: {}", e);
                                                                        }
                                                                    }
                                                                } else {
                                                                    log::warn!("sync_state characteristic not found");
                                                                }
                                                            }
                                                            Err(e) => {
                                                                log::warn!("Failed to get characteristics: {}", e);
                                                            }
                                                        }
                                                    } else {
                                                        log::warn!("HIVE service not found on {}", address);
                                                    }
                                                }
                                                Err(e) => {
                                                    log::warn!("Failed to get services: {}", e);
                                                }
                                            }

                                            // Disconnect
                                            let _ = device.disconnect().await;
                                        }
                                }
                                Err(e) => {
                                    log::warn!("Failed to get device {}: {}", address, e);
                                }
                            }
                        }

                        // Resume scanning after connection attempt
                        let _ = adapter.resume_discovery().await;

                        // Remove from connecting set
                        let mut state = test_state.write().await;
                        state.connecting.remove(&address);
                    });
                }
            }
            _ = interval.tick() => {
                tick_count += 1;
                let now = now_ms();

                // Run mesh tick (handles internal maintenance)
                let mesh_guard = mesh.read().await;
                let _ = mesh_guard.tick(now);

                // Always update sync_state with current encrypted document
                // This is critical - watches need valid encrypted data to sync
                let doc = mesh_guard.build_document();
                adapter.update_sync_state(&doc).await;

                // Status update every 10 seconds
                if tick_count % 10 == 0 {
                    let state = test_state.read().await;
                    let peer_count = state.peers_seen.len();
                    let active_peers = state.peers_seen.values()
                        .filter(|p| now - p.last_seen < 30000)
                        .count();

                    log::info!(
                        "Status [tick {}]: {} peers seen, {} active, SOS={}",
                        tick_count, peer_count, active_peers, state.sos_active
                    );
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
