//! BLE Test Client - Connects to ble_responder for automated testing
//!
//! This binary runs alongside ble_responder to perform automated loopback tests.
//! It connects to an Eche node, syncs mesh state, and verifies the exchange.
//!
//! Usage:
//!   ./ble_test_client [--adapter hci1] [--mesh-id TEST] [--timeout 30] [--encrypt]
//!   ./ble_test_client --adapter hci0 --genesis <BASE64> [--timeout 30]
//!
//! Build:
//!   cargo build --release --features linux --example ble_test_client
//!
//! Build with CannedMessage support:
//!   cargo build --release --features "linux,eche-lite-sync" --example ble_test_client
//!
//! Run (requires root or bluetooth group):
//!   sudo ./target/release/examples/ble_test_client --adapter hci1
//!
//! CannedMessage test (requires --encrypt and eche-lite-sync feature):
//!   sudo ./target/release/examples/ble_test_client --adapter hci0 --encrypt
//!
//! Exit codes:
//!   0 = Test passed (connected, synced, verified)
//!   1 = Test failed (timeout, no sync, verification failed)

use base64::Engine;
use eche_btle::{
    config::BleConfig,
    gatt::EcheCharacteristicUuids,
    platform::{linux::BluerAdapter, BleAdapter, DiscoveredDevice},
    security::MeshGenesis,
    EcheMesh, EcheMeshConfig, NodeId, ECHE_SERVICE_UUID,
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
    let mesh_id_arg = args
        .iter()
        .position(|a| a == "--mesh-id")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());
    let timeout_secs: u64 = args
        .iter()
        .position(|a| a == "--timeout")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let use_encryption_flag = args.iter().any(|a| a == "--encrypt");
    let genesis_b64 = args
        .iter()
        .position(|a| a == "--genesis")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let callsign = "TEST-CLI";
    let node_id = NodeId::new(0xC11E_0001); // Test client node ID

    // Well-known test key (must match ble_responder --encrypt)
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
        if use_encryption_flag {
            config = config.with_encryption(TEST_SECRET);
        }
        (mesh_id, use_encryption_flag, config)
    };

    log::info!("===========================================");
    log::info!("Eche BLE Test Client");
    log::info!("===========================================");
    log::info!("Adapter:  {}", adapter_name);
    log::info!("Node ID:  0x{:08X}", node_id.as_u32());
    log::info!("Callsign: {}", callsign);
    log::info!("Mesh ID:  {}", mesh_id);
    log::info!("Timeout:  {}s", timeout_secs);
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
            CannedMessage::Moving,
            EcheLiteNodeId::new(node_id.as_u32()),
            None,
            now_ms(),
        );
        let mesh_guard = mesh.read().await;
        let stored = mesh_guard.store_app_document(CannedMessageDocument::new(event));
        log::info!(
            "Stored CannedMessage (Moving): stored={}, app_doc_count={}",
            stored,
            mesh_guard.app_document_count()
        );
    }

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
    // Filter by mesh ID prefix in device name (e.g., "ECHE_CITEST-...")
    let found_peer_cb = found_peer.clone();
    let peer_node_id_cb = peer_node_id.clone();
    let mesh_id_prefix = format!("ECHE_{}-", mesh_id);
    adapter.set_discovery_callback(Some(Arc::new(move |device: DiscoveredDevice| {
        if device.is_hive_node {
            let name = device.name.as_deref().unwrap_or("unknown");
            log::info!(
                "Found Eche node: {} ({}) RSSI={}",
                name,
                device.address,
                device.rssi
            );
            // Accept both new format (ECHE_CITEST-...) and legacy (HIVE-...)
            let matches_mesh = name.starts_with(&mesh_id_prefix) || name.starts_with("ECHE-");
            if !matches_mesh {
                log::debug!("Skipping non-Eche peer: {}", name);
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
    log::info!("Scanning for Eche nodes...");

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
            // Use delta format when encrypted (carries app documents like CannedMessages)
            let mesh_guard = mesh.read().await;
            let doc = if use_encryption {
                mesh_guard.build_full_delta_document(now_ms())
            } else {
                mesh_guard.build_document()
            };
            adapter.update_sync_state(&doc).await;
            match conn
                .write_characteristic(
                    ECHE_SERVICE_UUID,
                    EcheCharacteristicUuids::sync_data(),
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
                .read_characteristic(ECHE_SERVICE_UUID, EcheCharacteristicUuids::sync_state())
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
            let app_docs = mesh_guard.app_document_count();
            let peer_cs = peer_callsign.read().await;

            // When running with --encrypt and eche-lite-sync, verify CannedMessage round-trip
            #[cfg(feature = "eche-lite-sync")]
            if use_encryption {
                use eche_btle::eche_lite_sync::CannedMessageDocument;

                let canned_docs =
                    mesh_guard.get_all_app_documents_of_type::<CannedMessageDocument>();
                // We should have our own (Moving) + responder's (CheckIn) = 2
                // But at minimum the responder's should have arrived
                let remote_count = canned_docs
                    .iter()
                    .filter(|d| d.source_node() != node_id.as_u32())
                    .count();

                if remote_count == 0 {
                    log::warn!("No remote CannedMessages received yet, continuing...");
                    drop(mesh_guard);
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    continue;
                }

                log::info!("===========================================");
                log::info!("CANNED MESSAGE VERIFICATION");
                log::info!("===========================================");
                for doc in &canned_docs {
                    log::info!(
                        "  CannedMsg: source=0x{:08X} code=0x{:02X} ts={} acks={}",
                        doc.source_node(),
                        doc.message_code(),
                        doc.timestamp(),
                        doc.ack_count()
                    );
                }
                log::info!(
                    "  Total CannedMessages: {} (local=1, remote={})",
                    canned_docs.len(),
                    remote_count
                );
            }

            log::info!("===========================================");
            log::info!("TEST PASSED!");
            log::info!("===========================================");
            log::info!("  Total mesh count: {}", total);
            log::info!("  App documents: {}", app_docs);
            log::info!("  Peer callsign: {}", *peer_cs);
            log::info!("  Time elapsed: {:?}", start.elapsed());
            log::info!("===========================================");

            adapter.stop().await?;
            std::process::exit(0);
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
