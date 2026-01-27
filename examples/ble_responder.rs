//! BLE Responder - Functional test node for Raspberry Pi
//!
//! This binary runs on a Raspberry Pi and acts as a HIVE mesh node that:
//! 1. Advertises as a HIVE device with GATT service
//! 2. Accepts connections from other HIVE nodes (e.g., Android)
//! 3. Syncs mesh state with connected peers
//! 4. Logs all activity for test verification
//!
//! Usage:
//!   ./ble_responder [--mesh-id TEST] [--callsign PI-TEST]
//!
//! Build for Pi:
//!   cargo build --release --features linux --example ble_responder
//!
//! Run (requires root or bluetooth group):
//!   sudo ./target/release/examples/ble_responder
//!
//! Loopback Test:
//!   1. Run this on a Raspberry Pi
//!   2. Connect with Android device running ATAK + hive-btle plugin
//!   3. Verify mesh state syncs (counters, callsigns, etc.)

use hive_btle::{
    config::BleConfig,
    platform::{linux::BluerAdapter, BleAdapter, DiscoveredDevice},
    HiveMesh, HiveMeshConfig, NodeId,
};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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
    let mesh_id = args
        .iter()
        .position(|a| a == "--mesh-id")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("TEST");
    let callsign = args
        .iter()
        .position(|a| a == "--callsign")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("PI-RESP");

    // Generate node ID from hostname
    let hostname = std::fs::read_to_string("/etc/hostname")
        .unwrap_or_else(|_| "rpi".to_string())
        .trim()
        .to_string();
    let node_id = NodeId::new(
        hostname
            .bytes()
            .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32)),
    );

    log::info!("===========================================");
    log::info!("HIVE BLE Responder (Loopback Test Node)");
    log::info!("===========================================");
    log::info!("Node ID:  0x{:08X}", node_id.as_u32());
    log::info!("Callsign: {}", callsign);
    log::info!("Mesh ID:  {}", mesh_id);
    log::info!("===========================================");

    // Create mesh
    let mesh_config = HiveMeshConfig::new(node_id, callsign, mesh_id);
    let mesh = Arc::new(RwLock::new(HiveMesh::new(mesh_config)));

    log::info!("Mesh initialized, starting BLE adapter...");

    // Create BLE adapter
    let adapter = BluerAdapter::new().await?;

    log::info!(
        "Bluetooth adapter: {} ({})",
        adapter.adapter_name(),
        adapter.address().unwrap_or_else(|| "unknown".to_string())
    );

    // Configure BLE and set up callbacks before wrapping in Arc
    let ble_config = BleConfig::new(node_id);
    let mut adapter = adapter;
    adapter.init(&ble_config).await?;

    // Set up discovery callback (requires &mut self, so do before Arc)
    adapter.set_discovery_callback(Some(Arc::new(move |device: DiscoveredDevice| {
        if device.is_hive_node {
            log::info!(
                "Discovered HIVE node: {} ({})",
                device.name.as_deref().unwrap_or("unknown"),
                device.address
            );
            if let Some(peer_id) = device.node_id {
                log::info!(
                    "  Node ID: 0x{:08X}, RSSI: {}",
                    peer_id.as_u32(),
                    device.rssi
                );
            }
        }
    })));

    // Register GATT service (requires &self)
    adapter.register_gatt_service().await?;
    log::info!("GATT service registered");

    // Now wrap in Arc for shared ownership
    let adapter = Arc::new(adapter);

    // Set up sync data callback - process incoming documents from peers
    let mesh_for_callback = mesh.clone();
    adapter
        .set_sync_data_callback(move |data| {
            let mesh = mesh_for_callback.clone();
            // Spawn async task since we're in a sync callback
            tokio::spawn(async move {
                let now = now_ms();
                let mesh_guard = mesh.read().await;
                if let Some(result) =
                    mesh_guard.on_ble_data_received_anonymous("gatt-peer", &data, now)
                {
                    log::info!(
                        "Received sync from node 0x{:08X}: counter_changed={}, emergency={}",
                        result.source_node.as_u32(),
                        result.counter_changed,
                        result.is_emergency
                    );
                    if let Some(cs) = &result.callsign {
                        log::info!("  Peer callsign: {}", cs);
                    }
                } else {
                    log::debug!(
                        "Received {} bytes (decrypt/parse failed or no change)",
                        data.len()
                    );
                }
            });
        })
        .await;

    // Start advertising and scanning
    adapter.start().await?;

    log::info!("===========================================");
    log::info!("Responder running. Press Ctrl+C to stop.");
    log::info!("Advertising as: HIVE_{}-{:08X}", mesh_id, node_id.as_u32());
    log::info!("Waiting for connections...");
    log::info!("===========================================");

    // Main event loop
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut tick_count = 0u64;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                tick_count += 1;
                let now = now_ms();

                // Run mesh tick and update sync state
                let mesh_guard = mesh.read().await;
                if let Some(doc) = mesh_guard.tick(now) {
                    // Update GATT sync_state so peers can read our current state
                    adapter.update_sync_state(&doc).await;
                    log::debug!("Tick {} - updated sync_state ({} bytes)", tick_count, doc.len());
                }

                // Log status periodically (every 10 seconds)
                if tick_count % 10 == 0 {
                    let peer_count = mesh_guard.peer_count();
                    let connected = mesh_guard.connected_count();
                    let total = mesh_guard.total_count();
                    log::info!(
                        "Status [tick {}]: {} discovered, {} connected, {} total mesh count",
                        tick_count, peer_count, connected, total
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
    adapter.stop().await?;
    log::info!("Responder stopped");

    Ok(())
}
