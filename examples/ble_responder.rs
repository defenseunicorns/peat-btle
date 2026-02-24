//! BLE Responder - Functional test node for Raspberry Pi
//!
//! This binary runs on a Raspberry Pi and acts as an Eche mesh node that:
//! 1. Advertises as an Eche device with GATT service
//! 2. Accepts connections from other Eche nodes (e.g., Android)
//! 3. Syncs mesh state with connected peers
//! 4. Logs all activity for test verification
//!
//! Usage:
//!   ./ble_responder [--mesh-id TEST] [--callsign PI-TEST] [--encrypt]
//!   ./ble_responder --genesis <BASE64>  [--callsign PI-TEST]
//!
//! Build for Pi:
//!   cargo build --release --features linux --example ble_responder
//!
//! Build with CannedMessage support:
//!   cargo build --release --features "linux,eche-lite-sync" --example ble_responder
//!
//! Run (requires root or bluetooth group):
//!   sudo ./target/release/examples/ble_responder
//!
//! Loopback Test:
//!   1. Run this on a Raspberry Pi
//!   2. Connect with Android device running ATAK + eche-btle plugin
//!   3. Verify mesh state syncs (counters, callsigns, etc.)
//!
//! CannedMessage Test (requires --encrypt and eche-lite-sync feature):
//!   1. Run with `--encrypt` on responder Pi
//!   2. Run ble_test_client with `--encrypt` on client Pi
//!   3. Verify CannedMessage documents sync between nodes

use base64::Engine;
use eche_btle::{
    config::BleConfig,
    platform::{linux::BluerAdapter, BleAdapter, DiscoveredDevice},
    security::MeshGenesis,
    EcheMesh, EcheMeshConfig, NodeId,
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
    let callsign = args
        .iter()
        .position(|a| a == "--callsign")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("PI-RESP");
    let genesis_b64 = args
        .iter()
        .position(|a| a == "--genesis")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let mesh_id_arg = args
        .iter()
        .position(|a| a == "--mesh-id")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());
    let use_encryption = args.iter().any(|a| a == "--encrypt");

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

    // Well-known test key (used by both responder and client with --encrypt)
    const TEST_SECRET: [u8; 32] = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f, 0x20,
    ];

    // Parse genesis or use mesh-id + optional encryption
    let (mesh_id, use_encryption, mesh_config) = if let Some(ref b64) = genesis_b64 {
        let genesis_bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap_or_else(|e| {
                // Try NO_PAD variant (Android uses NO_WRAP which omits padding)
                base64::engine::general_purpose::STANDARD_NO_PAD
                    .decode(b64)
                    .unwrap_or_else(|_| panic!("Invalid base64 genesis: {}", e))
            });
        let genesis =
            MeshGenesis::decode(&genesis_bytes).expect("Failed to decode genesis (invalid format)");
        let mesh_id = genesis.mesh_id();
        let secret = genesis.encryption_secret();
        let config = EcheMeshConfig::new(node_id, callsign, &mesh_id).with_encryption(secret);
        (mesh_id, true, config)
    } else {
        let mesh_id = mesh_id_arg.unwrap_or("TEST").to_string();
        let mut config = EcheMeshConfig::new(node_id, callsign, &mesh_id);
        if use_encryption {
            config = config.with_encryption(TEST_SECRET);
        }
        (mesh_id, use_encryption, config)
    };

    log::info!("===========================================");
    log::info!("Eche BLE Responder (Loopback Test Node)");
    log::info!("===========================================");
    log::info!("Node ID:  0x{:08X}", node_id.as_u32());
    log::info!("Callsign: {}", callsign);
    log::info!("Mesh ID:  {}", mesh_id);
    log::info!("Encrypt:  {}", use_encryption);
    if genesis_b64.is_some() {
        log::info!("Genesis:  YES (shared genesis provided)");
    }
    log::info!("===========================================");

    let mesh = Arc::new(RwLock::new(EcheMesh::new(mesh_config)));

    // Store a CannedMessage at startup (when eche-lite-sync feature is enabled)
    #[cfg(feature = "eche-lite-sync")]
    if use_encryption {
        use eche_btle::eche_lite_sync::CannedMessageDocument;
        use eche_lite::{CannedMessage, CannedMessageAckEvent, NodeId as EcheLiteNodeId};

        let event = CannedMessageAckEvent::new(
            CannedMessage::CheckIn,
            EcheLiteNodeId::new(node_id.as_u32()),
            None,
            now_ms(),
        );
        let mesh_guard = mesh.read().await;
        let stored = mesh_guard.store_app_document(CannedMessageDocument::new(event));
        log::info!(
            "Stored CannedMessage (CheckIn): stored={}, app_doc_count={}",
            stored,
            mesh_guard.app_document_count()
        );
    }

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
                "Discovered Eche node: {} ({})",
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
    log::info!("Advertising as: ECHE_{}-{:08X}", mesh_id, node_id.as_u32());
    log::info!("Waiting for connections...");
    log::info!("===========================================");

    // Main event loop
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    let mut tick_count = 0u64;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                tick_count += 1;

                // Build document and update sync state
                // Use delta format when encrypted (carries app documents like CannedMessages)
                let mesh_guard = mesh.read().await;
                let doc = if use_encryption {
                    mesh_guard.build_full_delta_document(now_ms())
                } else {
                    mesh_guard.build_document()
                };
                adapter.update_sync_state(&doc).await;
                if tick_count % 10 == 0 {
                    log::debug!("Tick {} - updated sync_state ({} bytes, delta={})", tick_count, doc.len(), use_encryption);
                }

                // Log status periodically (every 10 seconds)
                if tick_count % 10 == 0 {
                    let peer_count = mesh_guard.peer_count();
                    let connected = mesh_guard.connected_count();
                    let total = mesh_guard.total_count();
                    let app_docs = mesh_guard.app_document_count();
                    log::info!(
                        "Status [tick {}]: {} discovered, {} connected, {} total mesh count, {} app docs",
                        tick_count, peer_count, connected, total, app_docs
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
