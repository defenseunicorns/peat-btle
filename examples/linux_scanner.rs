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


//! Linux BLE Scanner Example
//!
//! Demonstrates scanning for HIVE BLE devices on Linux using BlueZ.
//! Requires the `linux` feature and a Bluetooth adapter.
//!
//! Run with: cargo run --example linux_scanner --features linux
//!
//! Note: May require root privileges or bluetooth group membership.

#[cfg(all(feature = "linux", target_os = "linux"))]
mod scanner {
    use hive_btle::observer::{HiveEvent, HiveObserver};
    use hive_btle::{BleConfig, HiveMesh, HiveMeshConfig, NodeId, HIVE_SERVICE_UUID};
    use std::sync::Arc;
    use std::time::Duration;

    /// Observer for mesh events
    struct ScanObserver;

    impl HiveObserver for ScanObserver {
        fn on_event(&self, event: HiveEvent) {
            match event {
                HiveEvent::PeerDiscovered { peer } => {
                    println!(
                        "Discovered HIVE peer: {} (Node ID: {:08X}, RSSI: {} dBm)",
                        peer.display_name(),
                        peer.node_id.as_u32(),
                        peer.rssi
                    );
                }
                HiveEvent::PeerConnected { node_id } => {
                    println!("Connected to: {:08X}", node_id.as_u32());
                }
                HiveEvent::MeshStateChanged {
                    peer_count,
                    connected_count,
                } => {
                    println!(
                        "Mesh: {} peers discovered, {} connected",
                        peer_count, connected_count
                    );
                }
                _ => {}
            }
        }
    }

    pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
        use hive_btle::platform::linux::BluerAdapter;
        use hive_btle::platform::BleAdapter;

        println!("=== HIVE-BTLE Linux Scanner ===\n");
        println!("HIVE Service UUID: {}", HIVE_SERVICE_UUID);
        println!();

        // Generate a node ID from random bytes (in production, use MAC address)
        let node_id = NodeId::new(rand::random::<u32>());
        println!("Our Node ID: {:08X}", node_id.as_u32());

        // Create BLE configuration
        let config = BleConfig::hive_lite(node_id);
        println!("Power profile: {:?}", config.power.profile);
        println!();

        // Create the adapter
        println!("Initializing Bluetooth adapter...");
        let mut adapter = BluerAdapter::new().await?;
        adapter.init(&config).await?;

        println!("Adapter address: {:?}", adapter.address());
        println!("Coded PHY support: {}", adapter.supports_coded_phy());
        println!(
            "Extended advertising: {}",
            adapter.supports_extended_advertising()
        );
        println!("Max MTU: {}", adapter.max_mtu());
        println!();

        // Create mesh for state management
        let mesh_config = HiveMeshConfig::new(node_id, "SCANNER", "DEMO");
        let mesh = Arc::new(HiveMesh::new(mesh_config));
        mesh.add_observer(Arc::new(ScanObserver));

        // Set discovery callback
        let mesh_clone = mesh.clone();
        adapter.set_discovery_callback(Some(Box::new(move |device| {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            if device.is_hive_node {
                mesh_clone.on_ble_discovered(
                    &device.address,
                    device.name.as_deref(),
                    device.rssi,
                    Some("DEMO"), // Filter by mesh ID
                    now_ms,
                );
            } else {
                // Non-HIVE device
                if let Some(name) = &device.name {
                    println!("Non-HIVE device: {} ({})", name, device.address);
                }
            }
        })));

        // Start scanning
        println!("Starting BLE scan...");
        println!("Press Ctrl+C to stop.\n");

        adapter.start_scan(&config.discovery).await?;

        // Run for 30 seconds
        for i in 0..30 {
            tokio::time::sleep(Duration::from_secs(1)).await;

            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            // Periodic tick
            if let Some(_doc) = mesh.tick(now_ms) {
                // Would broadcast to connected peers
            }

            // Progress indicator
            if (i + 1) % 5 == 0 {
                println!(
                    "... {} seconds elapsed, {} peers found",
                    i + 1,
                    mesh.peer_count()
                );
            }
        }

        // Stop scanning
        println!("\nStopping scan...");
        adapter.stop_scan().await?;

        // Final summary
        println!("\n=== Scan Summary ===");
        println!("Total peers discovered: {}", mesh.peer_count());
        for peer in mesh.get_peers() {
            println!(
                "  - {} ({:08X}) RSSI: {} dBm",
                peer.display_name(),
                peer.node_id.as_u32(),
                peer.rssi
            );
        }

        Ok(())
    }
}

#[cfg(all(feature = "linux", target_os = "linux"))]
#[tokio::main]
async fn main() {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    if let Err(e) = scanner::run().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

#[cfg(not(all(feature = "linux", target_os = "linux")))]
fn main() {
    println!("This example requires the 'linux' feature and Linux OS.");
    println!("Run with: cargo run --example linux_scanner --features linux");
}
