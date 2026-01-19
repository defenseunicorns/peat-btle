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

//! BLE Scanner for discovering HIVE nodes
//!
//! Provides filtering, deduplication, and tracking of discovered HIVE beacons.

#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};
#[cfg(feature = "std")]
use std::collections::HashMap;

#[cfg(feature = "std")]
use crate::config::DiscoveryConfig;
use crate::HierarchyLevel;
#[cfg(feature = "std")]
use crate::NodeId;

use super::beacon::{HiveBeacon, ParsedAdvertisement};
#[cfg(feature = "std")]
use super::encrypted_beacon::{BeaconKey, EncryptedBeacon};

/// Default timeout for considering a device "stale" (ms)
#[cfg(feature = "std")]
const DEFAULT_DEVICE_TIMEOUT_MS: u64 = 30_000;

/// Minimum interval between processing duplicate beacons from same node (ms)
#[cfg(feature = "std")]
const DEDUP_INTERVAL_MS: u64 = 500;

/// Tracked device state
#[derive(Debug, Clone)]
pub struct TrackedDevice {
    /// Last received beacon
    pub beacon: HiveBeacon,
    /// Device address
    pub address: String,
    /// Last RSSI reading
    pub rssi: i8,
    /// RSSI history for averaging (last N readings)
    pub rssi_history: Vec<i8>,
    /// When first discovered (monotonic ms timestamp)
    pub first_seen_ms: u64,
    /// When last beacon received (monotonic ms timestamp)
    pub last_seen_ms: u64,
    /// Estimated distance in meters
    pub estimated_distance: Option<f32>,
    /// Whether this device is currently connectable
    pub connectable: bool,
}

impl TrackedDevice {
    /// Create a new tracked device
    #[cfg(feature = "std")]
    fn new(
        beacon: HiveBeacon,
        address: String,
        rssi: i8,
        connectable: bool,
        current_time_ms: u64,
    ) -> Self {
        Self {
            beacon,
            address,
            rssi,
            rssi_history: vec![rssi],
            first_seen_ms: current_time_ms,
            last_seen_ms: current_time_ms,
            estimated_distance: None,
            connectable,
        }
    }

    /// Update with new beacon data
    #[cfg(feature = "std")]
    fn update(&mut self, beacon: HiveBeacon, rssi: i8, connectable: bool, current_time_ms: u64) {
        self.beacon = beacon;
        self.rssi = rssi;
        self.last_seen_ms = current_time_ms;
        self.connectable = connectable;

        // Keep last 10 RSSI readings for averaging
        self.rssi_history.push(rssi);
        if self.rssi_history.len() > 10 {
            self.rssi_history.remove(0);
        }
    }

    /// Get average RSSI
    pub fn average_rssi(&self) -> i8 {
        if self.rssi_history.is_empty() {
            return self.rssi;
        }
        let sum: i32 = self.rssi_history.iter().map(|&r| r as i32).sum();
        (sum / self.rssi_history.len() as i32) as i8
    }

    /// Check if this device is stale (not seen recently)
    pub fn is_stale(&self, timeout_ms: u64, current_time_ms: u64) -> bool {
        current_time_ms.saturating_sub(self.last_seen_ms) > timeout_ms
    }

    /// Get time since first discovery in milliseconds
    pub fn time_tracked_ms(&self, current_time_ms: u64) -> u64 {
        current_time_ms.saturating_sub(self.first_seen_ms)
    }
}

/// Filter criteria for scanning
#[derive(Debug, Clone, Default)]
pub struct ScanFilter {
    /// Only include HIVE nodes
    pub hive_only: bool,
    /// Only include nodes at or above this hierarchy level
    pub min_hierarchy_level: Option<HierarchyLevel>,
    /// Only include nodes with these capabilities (bitmask)
    pub required_capabilities: Option<u16>,
    /// Exclude nodes with these capabilities
    pub excluded_capabilities: Option<u16>,
    /// Minimum RSSI threshold (exclude weaker signals)
    pub min_rssi: Option<i8>,
    /// Maximum estimated distance in meters
    pub max_distance: Option<f32>,
    /// Only include connectable devices
    pub connectable_only: bool,
}

impl ScanFilter {
    /// Create a filter for HIVE nodes only
    pub fn hive_nodes() -> Self {
        Self {
            hive_only: true,
            ..Default::default()
        }
    }

    /// Create a filter for potential parents (nodes above our level)
    pub fn potential_parents(our_level: HierarchyLevel) -> Self {
        Self {
            hive_only: true,
            min_hierarchy_level: Some(our_level),
            connectable_only: true,
            ..Default::default()
        }
    }

    /// Check if a parsed advertisement passes this filter
    pub fn matches(&self, adv: &ParsedAdvertisement) -> bool {
        // HIVE-only filter
        if self.hive_only && !adv.is_hive_device() {
            return false;
        }

        // RSSI filter
        if let Some(min_rssi) = self.min_rssi {
            if adv.rssi < min_rssi {
                return false;
            }
        }

        // Distance filter
        if let Some(max_distance) = self.max_distance {
            if let Some(distance) = adv.estimated_distance_meters() {
                if distance > max_distance {
                    return false;
                }
            }
        }

        // Connectable filter
        if self.connectable_only && !adv.connectable {
            return false;
        }

        // Beacon-specific filters
        if let Some(ref beacon) = adv.beacon {
            // Hierarchy level filter
            if let Some(min_level) = self.min_hierarchy_level {
                if beacon.hierarchy_level < min_level {
                    return false;
                }
            }

            // Required capabilities
            if let Some(required) = self.required_capabilities {
                if beacon.capabilities & required != required {
                    return false;
                }
            }

            // Excluded capabilities
            if let Some(excluded) = self.excluded_capabilities {
                if beacon.capabilities & excluded != 0 {
                    return false;
                }
            }
        }

        true
    }
}

/// Scanner state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScannerState {
    /// Not scanning
    Idle,
    /// Actively scanning
    Scanning,
    /// Paused (e.g., during connection)
    Paused,
}

/// BLE Scanner for discovering HIVE nodes
///
/// Handles beacon reception, filtering, deduplication, and device tracking.
///
/// Note: This type requires the `std` feature for full functionality.
#[cfg(feature = "std")]
pub struct Scanner {
    /// Scanner configuration (will be used for PHY/power management)
    #[allow(dead_code)]
    config: DiscoveryConfig,
    /// Current state
    state: ScannerState,
    /// Tracked devices by node ID
    devices: HashMap<NodeId, TrackedDevice>,
    /// Address to node ID mapping (for devices without parsed beacon)
    address_map: HashMap<String, NodeId>,
    /// Filter criteria
    filter: ScanFilter,
    /// Device timeout (ms)
    device_timeout_ms: u64,
    /// Last dedup timestamps per node (ms)
    last_processed: HashMap<NodeId, u64>,
    /// Current time (monotonic ms, set externally)
    current_time_ms: u64,
    /// Beacon key for decrypting encrypted beacons (optional)
    beacon_key: Option<BeaconKey>,
    /// Expected mesh ID bytes for filtering decrypted beacons
    mesh_id_bytes: Option<[u8; 4]>,
}

#[cfg(feature = "std")]
impl Scanner {
    /// Create a new scanner with default settings
    pub fn new(config: DiscoveryConfig) -> Self {
        Self {
            config,
            state: ScannerState::Idle,
            devices: HashMap::new(),
            address_map: HashMap::new(),
            filter: ScanFilter::default(),
            device_timeout_ms: DEFAULT_DEVICE_TIMEOUT_MS,
            last_processed: HashMap::new(),
            current_time_ms: 0,
            beacon_key: None,
            mesh_id_bytes: None,
        }
    }

    /// Set the current time (call periodically from platform)
    pub fn set_time_ms(&mut self, time_ms: u64) {
        self.current_time_ms = time_ms;
    }

    /// Set the scan filter
    pub fn set_filter(&mut self, filter: ScanFilter) {
        self.filter = filter;
    }

    /// Set device timeout in milliseconds
    pub fn set_device_timeout_ms(&mut self, timeout_ms: u64) {
        self.device_timeout_ms = timeout_ms;
    }

    /// Configure beacon key for decrypting encrypted advertisements
    ///
    /// # Arguments
    /// * `key` - Beacon encryption key from mesh genesis
    /// * `mesh_id_bytes` - Expected 4-byte mesh identifier for filtering
    ///
    /// When configured, the scanner will attempt to decrypt encrypted beacons
    /// and only accept those from the specified mesh.
    pub fn set_beacon_key(&mut self, key: BeaconKey, mesh_id_bytes: [u8; 4]) {
        self.beacon_key = Some(key);
        self.mesh_id_bytes = Some(mesh_id_bytes);
    }

    /// Clear beacon key (stop accepting encrypted beacons)
    pub fn clear_beacon_key(&mut self) {
        self.beacon_key = None;
        self.mesh_id_bytes = None;
    }

    /// Check if this scanner can decrypt encrypted beacons
    pub fn can_decrypt_beacons(&self) -> bool {
        self.beacon_key.is_some() && self.mesh_id_bytes.is_some()
    }

    /// Get current state
    pub fn state(&self) -> ScannerState {
        self.state
    }

    /// Start scanning
    pub fn start(&mut self) {
        self.state = ScannerState::Scanning;
    }

    /// Pause scanning
    pub fn pause(&mut self) {
        self.state = ScannerState::Paused;
    }

    /// Stop scanning
    pub fn stop(&mut self) {
        self.state = ScannerState::Idle;
    }

    /// Process a received advertisement
    ///
    /// Returns true if this is a new or updated device that passes the filter.
    ///
    /// Handles both plaintext and encrypted beacons:
    /// - Plaintext beacons are processed directly from `adv.beacon`
    /// - Encrypted beacons (in `adv.encrypted_service_data`) are decrypted if a
    ///   beacon key is configured
    pub fn process_advertisement(&mut self, adv: ParsedAdvertisement) -> bool {
        // Apply filter
        if !self.filter.matches(&adv) {
            return false;
        }

        // Extract beacon and node ID - try plaintext first, then encrypted
        let (beacon, node_id) = if let Some(ref b) = adv.beacon {
            // Plaintext beacon
            (b.clone(), b.node_id)
        } else if let Some(ref encrypted_data) = adv.encrypted_service_data {
            // Try to decrypt encrypted beacon
            match self.try_decrypt_beacon(encrypted_data) {
                Some((decrypted_beacon, _mesh_id)) => {
                    let node_id = decrypted_beacon.node_id;
                    (decrypted_beacon, node_id)
                }
                None => return false, // Decryption failed (wrong mesh or no key)
            }
        } else {
            return false; // No beacon = not a HIVE device
        };

        // Check deduplication
        if let Some(&last) = self.last_processed.get(&node_id) {
            if self.current_time_ms.saturating_sub(last) < DEDUP_INTERVAL_MS {
                return false;
            }
        }
        self.last_processed.insert(node_id, self.current_time_ms);

        // Update or create tracked device
        let is_new = !self.devices.contains_key(&node_id);

        if let Some(device) = self.devices.get_mut(&node_id) {
            // Update existing device
            device.update(beacon, adv.rssi, adv.connectable, self.current_time_ms);
        } else {
            // New device
            let device = TrackedDevice::new(
                beacon,
                adv.address.clone(),
                adv.rssi,
                adv.connectable,
                self.current_time_ms,
            );
            self.devices.insert(node_id, device);
            self.address_map.insert(adv.address, node_id);
        }

        is_new
    }

    /// Attempt to decrypt an encrypted beacon
    ///
    /// Returns the decrypted beacon and mesh_id if successful.
    fn try_decrypt_beacon(&self, encrypted_data: &[u8]) -> Option<(HiveBeacon, [u8; 4])> {
        let key = self.beacon_key.as_ref()?;
        let expected_mesh_id = self.mesh_id_bytes?;

        // Try to decrypt
        let (encrypted_beacon, mesh_id) = EncryptedBeacon::decrypt(encrypted_data, key)?;

        // Check mesh ID matches (ensures this is from our mesh)
        if mesh_id != expected_mesh_id {
            return None;
        }

        // Convert EncryptedBeacon to HiveBeacon
        let beacon = HiveBeacon {
            version: 1,
            capabilities: encrypted_beacon.capabilities,
            node_id: encrypted_beacon.node_id,
            hierarchy_level: HierarchyLevel::from(encrypted_beacon.hierarchy_level),
            geohash: 0, // Not included in encrypted beacon
            battery_percent: encrypted_beacon.battery_percent,
            seq_num: 0, // Not included in encrypted beacon
        };

        Some((beacon, mesh_id))
    }

    /// Get a tracked device by node ID
    pub fn get_device(&self, node_id: &NodeId) -> Option<&TrackedDevice> {
        self.devices.get(node_id)
    }

    /// Get node ID for an address
    pub fn get_node_id_for_address(&self, address: &str) -> Option<&NodeId> {
        self.address_map.get(address)
    }

    /// Get all tracked devices
    pub fn devices(&self) -> impl Iterator<Item = &TrackedDevice> {
        self.devices.values()
    }

    /// Get devices sorted by RSSI (strongest first)
    pub fn devices_by_rssi(&self) -> Vec<&TrackedDevice> {
        let mut devices: Vec<_> = self.devices.values().collect();
        devices.sort_by(|a, b| b.rssi.cmp(&a.rssi));
        devices
    }

    /// Get devices sorted by hierarchy level (highest first)
    pub fn devices_by_hierarchy(&self) -> Vec<&TrackedDevice> {
        let mut devices: Vec<_> = self.devices.values().collect();
        devices.sort_by(|a, b| b.beacon.hierarchy_level.cmp(&a.beacon.hierarchy_level));
        devices
    }

    /// Get count of tracked devices
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Remove stale devices
    ///
    /// Returns the number of devices removed.
    pub fn remove_stale(&mut self) -> usize {
        let timeout = self.device_timeout_ms;
        let current_time = self.current_time_ms;
        let stale: Vec<NodeId> = self
            .devices
            .iter()
            .filter(|(_, d)| d.is_stale(timeout, current_time))
            .map(|(id, _)| *id)
            .collect();

        let count = stale.len();
        for node_id in stale {
            if let Some(device) = self.devices.remove(&node_id) {
                self.address_map.remove(&device.address);
                self.last_processed.remove(&node_id);
            }
        }

        count
    }

    /// Clear all tracked devices
    pub fn clear(&mut self) {
        self.devices.clear();
        self.address_map.clear();
        self.last_processed.clear();
    }

    /// Find the best parent candidate
    ///
    /// Selects based on hierarchy level (prefer higher) and RSSI (prefer stronger).
    pub fn find_best_parent(&self, our_level: HierarchyLevel) -> Option<&TrackedDevice> {
        self.devices
            .values()
            .filter(|d| {
                d.beacon.hierarchy_level > our_level && d.connectable && !d.beacon.is_lite_node()
            })
            .max_by(|a, b| {
                // First compare hierarchy level
                match a.beacon.hierarchy_level.cmp(&b.beacon.hierarchy_level) {
                    core::cmp::Ordering::Equal => {
                        // Then compare RSSI
                        a.average_rssi().cmp(&b.average_rssi())
                    }
                    other => other,
                }
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_adv(node_id: u32, rssi: i8, level: HierarchyLevel) -> ParsedAdvertisement {
        let beacon = HiveBeacon::new(NodeId::new(node_id))
            .with_hierarchy_level(level)
            .with_battery(80);

        ParsedAdvertisement {
            address: format!("00:11:22:33:44:{:02X}", node_id as u8),
            rssi,
            beacon: Some(beacon),
            encrypted_service_data: None,
            local_name: Some(format!("HIVE-{:08X}", node_id)),
            tx_power: Some(0),
            connectable: true,
        }
    }

    #[test]
    fn test_scanner_process_advertisement() {
        let config = DiscoveryConfig::default();
        let mut scanner = Scanner::new(config);
        scanner.set_time_ms(1000);

        let adv = make_adv(0x12345678, -60, HierarchyLevel::Platform);
        assert!(scanner.process_advertisement(adv));
        assert_eq!(scanner.device_count(), 1);

        // Duplicate within dedup interval should be ignored
        scanner.set_time_ms(1100);
        let adv2 = make_adv(0x12345678, -65, HierarchyLevel::Platform);
        assert!(!scanner.process_advertisement(adv2));
        assert_eq!(scanner.device_count(), 1);
    }

    #[test]
    fn test_scan_filter_hive_only() {
        let filter = ScanFilter::hive_nodes();

        let hive_adv = make_adv(0x12345678, -60, HierarchyLevel::Platform);
        assert!(filter.matches(&hive_adv));

        let non_hive = ParsedAdvertisement {
            address: "AA:BB:CC:DD:EE:FF".to_string(),
            rssi: -50,
            beacon: None,
            encrypted_service_data: None,
            local_name: Some("Other Device".to_string()),
            tx_power: None,
            connectable: true,
        };
        assert!(!filter.matches(&non_hive));
    }

    #[test]
    fn test_scan_filter_rssi() {
        let filter = ScanFilter {
            hive_only: true,
            min_rssi: Some(-70),
            ..Default::default()
        };

        let strong = make_adv(0x11111111, -60, HierarchyLevel::Platform);
        assert!(filter.matches(&strong));

        let weak = make_adv(0x22222222, -80, HierarchyLevel::Platform);
        assert!(!filter.matches(&weak));
    }

    #[test]
    fn test_find_best_parent() {
        let config = DiscoveryConfig::default();
        let mut scanner = Scanner::new(config);
        scanner.set_time_ms(0);

        // Add a squad leader
        let squad = make_adv(0x11111111, -60, HierarchyLevel::Squad);
        scanner.process_advertisement(squad);

        // Add a platoon leader (higher hierarchy)
        scanner.set_time_ms(501); // Avoid dedup
        let platoon = make_adv(0x22222222, -70, HierarchyLevel::Platoon);
        scanner.process_advertisement(platoon);

        // Find parent for platform node
        let parent = scanner.find_best_parent(HierarchyLevel::Platform);
        assert!(parent.is_some());
        // Should prefer platoon (higher hierarchy) despite weaker signal
        assert_eq!(
            parent.unwrap().beacon.hierarchy_level,
            HierarchyLevel::Platoon
        );
    }

    #[test]
    fn test_devices_by_rssi() {
        let config = DiscoveryConfig::default();
        let mut scanner = Scanner::new(config);
        scanner.set_time_ms(0);

        scanner.process_advertisement(make_adv(0x11111111, -80, HierarchyLevel::Platform));
        scanner.set_time_ms(501);
        scanner.process_advertisement(make_adv(0x22222222, -50, HierarchyLevel::Platform));
        scanner.set_time_ms(1002);
        scanner.process_advertisement(make_adv(0x33333333, -70, HierarchyLevel::Platform));

        let sorted = scanner.devices_by_rssi();
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].rssi, -50); // Strongest first
        assert_eq!(sorted[1].rssi, -70);
        assert_eq!(sorted[2].rssi, -80);
    }

    #[test]
    fn test_remove_stale() {
        let config = DiscoveryConfig::default();
        let mut scanner = Scanner::new(config);
        scanner.set_time_ms(0);

        scanner.process_advertisement(make_adv(0x11111111, -60, HierarchyLevel::Platform));
        assert_eq!(scanner.device_count(), 1);

        // Fast forward past timeout
        scanner.set_time_ms(35_000);
        let removed = scanner.remove_stale();
        assert_eq!(removed, 1);
        assert_eq!(scanner.device_count(), 0);
    }

    #[test]
    fn test_encrypted_beacon_scanning() {
        use crate::discovery::{mesh_id_to_bytes, EncryptedBeacon as EB};

        let config = DiscoveryConfig::default();
        let mut scanner = Scanner::new(config);
        scanner.set_time_ms(0);

        let beacon_key = BeaconKey::from_base(&[0x42; 32]);
        let mesh_id_bytes = mesh_id_to_bytes("TEST-MESH");
        let node_id = NodeId::new(0x12345678);

        // Configure scanner for encrypted beacons
        scanner.set_beacon_key(beacon_key.clone(), mesh_id_bytes);
        assert!(scanner.can_decrypt_beacons());

        // Create an encrypted beacon
        let encrypted_beacon = EB::new(node_id, 0x0F00, u8::from(HierarchyLevel::Squad), 85);
        let encrypted_data = encrypted_beacon.encrypt(&beacon_key, &mesh_id_bytes);

        // Create advertisement with encrypted data
        let adv = ParsedAdvertisement {
            address: "00:11:22:33:44:55".to_string(),
            rssi: -60,
            beacon: None,
            encrypted_service_data: Some(encrypted_data),
            local_name: Some("HIVE".to_string()),
            tx_power: None,
            connectable: true,
        };

        // Process advertisement
        assert!(scanner.process_advertisement(adv));
        assert_eq!(scanner.device_count(), 1);

        // Verify decrypted device
        let device = scanner.get_device(&node_id).unwrap();
        assert_eq!(device.beacon.node_id, node_id);
        assert_eq!(device.beacon.capabilities, 0x0F00);
        assert_eq!(device.beacon.hierarchy_level, HierarchyLevel::Squad);
        assert_eq!(device.beacon.battery_percent, 85);
    }

    #[test]
    fn test_encrypted_beacon_wrong_mesh_rejected() {
        use crate::discovery::{mesh_id_to_bytes, EncryptedBeacon as EB};

        let config = DiscoveryConfig::default();
        let mut scanner = Scanner::new(config);
        scanner.set_time_ms(0);

        let beacon_key = BeaconKey::from_base(&[0x42; 32]);
        let our_mesh_id = mesh_id_to_bytes("OUR-MESH");
        let other_mesh_id = mesh_id_to_bytes("OTHER-MESH");
        let node_id = NodeId::new(0x12345678);

        // Configure scanner for our mesh
        scanner.set_beacon_key(beacon_key.clone(), our_mesh_id);

        // Create encrypted beacon for a different mesh
        let encrypted_beacon = EB::new(node_id, 0x0F00, u8::from(HierarchyLevel::Squad), 85);
        let encrypted_data = encrypted_beacon.encrypt(&beacon_key, &other_mesh_id);

        let adv = ParsedAdvertisement {
            address: "00:11:22:33:44:55".to_string(),
            rssi: -60,
            beacon: None,
            encrypted_service_data: Some(encrypted_data),
            local_name: Some("HIVE".to_string()),
            tx_power: None,
            connectable: true,
        };

        // Should be rejected - wrong mesh
        assert!(!scanner.process_advertisement(adv));
        assert_eq!(scanner.device_count(), 0);
    }
}
