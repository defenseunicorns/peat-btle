//! BLE Test Client - Connects to ble_responder for automated testing
//!
//! This binary runs alongside ble_responder to perform automated loopback tests.
//! It connects to a HIVE node, syncs mesh state, and verifies the exchange.
//!
//! Usage:
//!   ./ble_test_client [--adapter hci1] [--mesh-id TEST] [--timeout 30]
//!
//! Build:
//!   cargo build --release --features linux --example ble_test_client
//!
//! Run (requires root or bluetooth group):
//!   sudo ./target/release/examples/ble_test_client --adapter hci1
//!
//! Exit codes:
//!   0 = Test passed (connected, synced, verified)
//!   1 = Test failed (timeout, no sync, verification failed)

use hive_btle::{
    config::BleConfig,
    gatt::HiveCharacteristicUuids,
    platform::{linux::BluerAdapter, BleAdapter, DiscoveredDevice},
    HiveMesh, HiveMeshConfig, NodeId, HIVE_SERVICE_UUID,
};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Parse args
    let args: Vec<String> = std::env::args().collect();
    let adapter_name = args
        .iter()
        .position(|a| a == "--adapter")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("hci1"); // Default to secondary adapter
    let mesh_id = args
        .iter()
        .position(|a| a == "--mesh-id")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("TEST");
    let timeout_secs: u64 = args
        .iter()
        .position(|a| a == "--timeout")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);

    let callsign = "TEST-CLI";
    let node_id = NodeId::new(0xC11E_0001); // Test client node ID

    log::info!("===========================================");
    log::info!("HIVE BLE Test Client");
    log::info!("===========================================");
    log::info!("Adapter:  {}", adapter_name);
    log::info!("Node ID:  0x{:08X}", node_id.as_u32());
    log::info!("Callsign: {}", callsign);
    log::info!("Mesh ID:  {}", mesh_id);
    log::info!("Timeout:  {}s", timeout_secs);
    log::info!("===========================================");

    // Create mesh
    let mesh_config = HiveMeshConfig::new(node_id, callsign, mesh_id);
    let mesh = Arc::new(RwLock::new(HiveMesh::new(mesh_config)));

    // Create BLE adapter on specified interface
    let adapter = BluerAdapter::with_adapter_name(adapter_name).await?;
    log::info!(
        "Bluetooth adapter: {} ({})",
        adapter.adapter_name(),
        adapter.address().unwrap_or_else(|| "unknown".to_string())
    );

    // Configure BLE
    let ble_config = BleConfig::new(node_id);
    let mut adapter = adapter;
    adapter.init(&ble_config).await?;

    // Track test state
    let found_peer = Arc::new(AtomicBool::new(false));
    let sync_received = Arc::new(AtomicBool::new(false));
    let peer_node_id = Arc::new(AtomicU32::new(0));
    let peer_callsign = Arc::new(RwLock::new(String::new()));

    // Set up discovery callback
    // Filter by mesh ID prefix in device name (e.g., "HIVE_CITEST-...")
    let found_peer_cb = found_peer.clone();
    let peer_node_id_cb = peer_node_id.clone();
    let mesh_id_prefix = format!("HIVE_{}-", mesh_id);
    adapter.set_discovery_callback(Some(Arc::new(move |device: DiscoveredDevice| {
        if device.is_hive_node {
            let name = device.name.as_deref().unwrap_or("unknown");
            log::info!(
                "Found HIVE node: {} ({}) RSSI={}",
                name,
                device.address,
                device.rssi
            );
            // Accept both new format (HIVE_CITEST-...) and legacy (HIVE-...)
            let matches_mesh = name.starts_with(&mesh_id_prefix) || name.starts_with("HIVE-");
            if !matches_mesh {
                log::debug!("Skipping non-HIVE peer: {}", name);
                return;
            }
            if let Some(pid) = device.node_id {
                peer_node_id_cb.store(pid.as_u32(), Ordering::SeqCst);
                found_peer_cb.store(true, Ordering::SeqCst);
            }
        }
    })));

    // Register GATT service
    adapter.register_gatt_service().await?;

    // Wrap in Arc for shared ownership
    let adapter = Arc::new(adapter);

    // Set up sync data callback
    let mesh_for_callback = mesh.clone();
    let sync_received_cb = sync_received.clone();
    let peer_callsign_cb = peer_callsign.clone();
    adapter
        .set_sync_data_callback(move |data| {
            let mesh = mesh_for_callback.clone();
            let sync_flag = sync_received_cb.clone();
            let callsign_store = peer_callsign_cb.clone();
            tokio::spawn(async move {
                let now = now_ms();
                let mesh_guard = mesh.read().await;
                if let Some(result) =
                    mesh_guard.on_ble_data_received_anonymous("gatt-peer", &data, now)
                {
                    log::info!(
                        "SYNC RECEIVED from 0x{:08X}: counter_changed={}, total={}",
                        result.source_node.as_u32(),
                        result.counter_changed,
                        result.total_count
                    );
                    if let Some(cs) = &result.callsign {
                        log::info!("  Peer callsign: {}", cs);
                        *callsign_store.write().await = cs.clone();
                    }
                    sync_flag.store(true, Ordering::SeqCst);
                }
            });
        })
        .await;

    // Start scanning (no advertising - we're the client)
    adapter.start().await?;
    log::info!("Scanning for HIVE nodes...");

    // Test loop with timeout
    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    let mut tick_count = 0u64;
    let mut connected = false;

    loop {
        // Check timeout
        if start.elapsed() > timeout {
            log::error!("TEST FAILED: Timeout after {}s", timeout_secs);
            log::error!("  Found peer: {}", found_peer.load(Ordering::SeqCst));
            log::error!("  Connected: {}", connected);
            log::error!("  Sync received: {}", sync_received.load(Ordering::SeqCst));
            adapter.stop().await?;
            std::process::exit(1);
        }

        // State machine
        if !found_peer.load(Ordering::SeqCst) {
            // Still scanning
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        }

        let pid = NodeId::new(peer_node_id.load(Ordering::SeqCst));

        if !connected {
            // Try to connect
            log::info!("Connecting to peer 0x{:08X}...", pid.as_u32());
            match adapter.connect(&pid).await {
                Ok(_conn) => {
                    log::info!("Connected!");
                    connected = true;
                    // Notify mesh of connection
                    let mesh_guard = mesh.read().await;
                    mesh_guard.on_ble_connected(&format!("{:08X}", pid.as_u32()), now_ms());
                }
                Err(e) => {
                    log::warn!("Connection failed: {}, retrying...", e);
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            }
        }

        // Connected - run tick and actively push/pull GATT data
        tick_count += 1;
        // Get the stored BluerConnection for GATT operations
        if let Some(conn) = adapter.get_connection(&pid).await {
            // Build document and write to peer's sync_data characteristic
            // Use build_document() instead of tick() because tick() requires
            // connected_count > 0 which needs full mesh lifecycle integration.
            let mesh_guard = mesh.read().await;
            let doc = mesh_guard.build_document();
            adapter.update_sync_state(&doc).await;
            match conn
                .write_characteristic(
                    HIVE_SERVICE_UUID,
                    HiveCharacteristicUuids::sync_data(),
                    &doc,
                )
                .await
            {
                Ok(()) => {
                    log::debug!("Tick {} - wrote {} bytes to peer", tick_count, doc.len())
                }
                Err(e) => log::warn!("Tick {} - failed to write sync_data: {}", tick_count, e),
            }
            drop(mesh_guard);

            // Read peer's sync_state characteristic
            match conn
                .read_characteristic(HIVE_SERVICE_UUID, HiveCharacteristicUuids::sync_state())
                .await
            {
                Ok(data) if !data.is_empty() => {
                    log::debug!("Read {} bytes from peer sync_state", data.len());
                    let mesh_guard = mesh.read().await;
                    if let Some(result) =
                        mesh_guard.on_ble_data_received_anonymous("gatt-peer", &data, now_ms())
                    {
                        log::info!(
                            "SYNC from peer 0x{:08X}: counter_changed={}, total={}",
                            result.source_node.as_u32(),
                            result.counter_changed,
                            result.total_count
                        );
                        if let Some(cs) = &result.callsign {
                            log::info!("  Peer callsign: {}", cs);
                            *peer_callsign.write().await = cs.clone();
                        }
                        sync_received.store(true, Ordering::SeqCst);
                    }
                }
                Ok(_) => {}
                Err(e) => log::debug!("Failed to read peer sync_state: {}", e),
            }
        }

        // Check if we received sync (from GATT read above or write callback)
        if sync_received.load(Ordering::SeqCst) {
            let mesh_guard = mesh.read().await;
            let total = mesh_guard.total_count();
            let peer_cs = peer_callsign.read().await;

            log::info!("===========================================");
            log::info!("TEST PASSED!");
            log::info!("===========================================");
            log::info!("  Total mesh count: {}", total);
            log::info!("  Peer callsign: {}", *peer_cs);
            log::info!("  Time elapsed: {:?}", start.elapsed());
            log::info!("===========================================");

            adapter.stop().await?;
            std::process::exit(0);
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
