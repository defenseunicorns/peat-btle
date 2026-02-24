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

//! macOS CoreBluetooth integration tests
//!
//! These tests verify that the CoreBluetooth adapter works correctly on macOS.
//! They specifically test memory management to prevent segfaults caused by
//! incorrect Objective-C retain/release semantics.
//!
//! Run with: `cargo test --features macos --test apple_corebluetooth_test`
//!
//! Note: These tests require macOS and may require Bluetooth to be available.
//! They will be skipped on non-Apple platforms.

#![cfg(all(target_os = "macos", feature = "macos"))]

use eche_btle::config::BleConfig;
use eche_btle::platform::BleAdapter;
use eche_btle::NodeId;

/// Test that CoreBluetoothAdapter can be created without crashing
///
/// This is a basic smoke test to verify that the ObjC bindings
/// and memory management are working correctly.
#[test]
fn test_adapter_creation() {
    use eche_btle::platform::apple::CoreBluetoothAdapter;

    // Creating the adapter should not crash
    let result = CoreBluetoothAdapter::new();

    match result {
        Ok(_adapter) => {
            println!("CoreBluetoothAdapter created successfully");
        }
        Err(e) => {
            // It's OK if Bluetooth is unavailable, but it shouldn't crash
            println!(
                "CoreBluetoothAdapter creation failed (expected on CI): {}",
                e
            );
        }
    }
}

/// Test that adapter initialization doesn't cause memory issues
///
/// This specifically tests the code path that was causing segfaults
/// due to incorrect autoreleased object handling.
#[tokio::test]
async fn test_adapter_init_no_segfault() {
    use eche_btle::platform::apple::CoreBluetoothAdapter;

    let result = CoreBluetoothAdapter::new();
    let Ok(mut adapter) = result else {
        println!("Skipping test: Bluetooth unavailable");
        return;
    };

    let node_id = NodeId::new(0xDEADBEEF);
    let config = BleConfig::new(node_id);

    // Initialize should not crash
    let init_result = adapter.init(&config).await;
    match init_result {
        Ok(()) => println!("Adapter initialized successfully"),
        Err(e) => println!("Adapter init failed (may be expected): {}", e),
    }

    // Even if init failed, we shouldn't have crashed
    println!("No segfault during initialization - PASS");
}

/// Test that starting the adapter (which registers GATT service and starts advertising)
/// doesn't cause a segfault
///
/// This is the key test for issue 80ff45 - the segfault was occurring
/// when registering the Eche GATT service due to incorrect memory management
/// in the `start_advertising` method.
#[tokio::test]
async fn test_adapter_start_no_segfault() {
    use eche_btle::platform::apple::CoreBluetoothAdapter;

    let result = CoreBluetoothAdapter::new();
    let Ok(mut adapter) = result else {
        println!("Skipping test: Bluetooth unavailable");
        return;
    };

    let node_id = NodeId::new(0xCAFEBABE);
    let config = BleConfig::new(node_id);

    // Initialize first
    if let Err(e) = adapter.init(&config).await {
        println!("Skipping test: init failed: {}", e);
        return;
    }

    // Start the adapter - this triggers:
    // 1. register_hive_service() - creates GATT service
    // 2. start_advertising() - the code path that was causing segfaults
    //
    // The segfault was caused by using `dictionaryWithObjects:forKeys:`
    // which returns an autoreleased object, but treating it as retained.
    let start_result = adapter.start().await;

    match start_result {
        Ok(()) => {
            println!("Adapter started successfully");

            // Clean up
            let _ = adapter.stop().await;
        }
        Err(e) => {
            // Errors are OK (e.g., Bluetooth powered off), crashes are not
            println!("Adapter start failed (may be expected): {}", e);
        }
    }

    // If we got here, no segfault occurred
    println!("No segfault during start - PASS");
}

/// Test that repeated start/stop cycles don't cause memory leaks or crashes
///
/// Memory management bugs often manifest as crashes on repeated operations
/// due to double-free or use-after-free issues.
#[tokio::test]
async fn test_repeated_start_stop_no_crash() {
    use eche_btle::platform::apple::CoreBluetoothAdapter;

    let result = CoreBluetoothAdapter::new();
    let Ok(mut adapter) = result else {
        println!("Skipping test: Bluetooth unavailable");
        return;
    };

    let node_id = NodeId::new(0x12345678);
    let config = BleConfig::new(node_id);

    if let Err(e) = adapter.init(&config).await {
        println!("Skipping test: init failed: {}", e);
        return;
    }

    // Perform multiple start/stop cycles
    for i in 0..3 {
        println!("Cycle {} - starting...", i + 1);

        match adapter.start().await {
            Ok(()) => {
                println!("Cycle {} - started, now stopping...", i + 1);
                let _ = adapter.stop().await;
                println!("Cycle {} - stopped", i + 1);
            }
            Err(e) => {
                println!("Cycle {} - start failed (expected): {}", i + 1, e);
            }
        }

        // Small delay between cycles to allow cleanup
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    println!("Completed {} cycles without crash - PASS", 3);
}

/// Test advertising with custom configuration
///
/// Tests that the advertisement dictionary is properly constructed
/// without causing memory issues.
#[tokio::test]
async fn test_advertising_memory_safety() {
    use eche_btle::config::DiscoveryConfig;
    use eche_btle::platform::apple::CoreBluetoothAdapter;

    let result = CoreBluetoothAdapter::new();
    let Ok(mut adapter) = result else {
        println!("Skipping test: Bluetooth unavailable");
        return;
    };

    let node_id = NodeId::new(0xABCDEF01);
    let config = BleConfig::new(node_id);

    if let Err(e) = adapter.init(&config).await {
        println!("Skipping test: init failed: {}", e);
        return;
    }

    // Test start_advertising directly with custom config
    let discovery_config = DiscoveryConfig::default();

    match adapter.start_advertising(&discovery_config).await {
        Ok(()) => {
            println!("Advertising started successfully");

            // Let it run briefly
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

            let _ = adapter.stop_advertising().await;
            println!("Advertising stopped");
        }
        Err(e) => {
            println!("Advertising failed (may be expected): {}", e);
        }
    }

    println!("Advertising test completed without crash - PASS");
}
