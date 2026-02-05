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

//! BLE address rotation handling
//!
//! WearOS and other privacy-focused BLE devices rotate their MAC addresses
//! periodically. This module provides utilities to identify devices across
//! address changes using stable identifiers like device names.
//!
//! # Example
//!
//! ```
//! use hive_btle::address_rotation::AddressRotationHandler;
//! use hive_btle::NodeId;
//!
//! let mut handler = AddressRotationHandler::new();
//!
//! // First discovery
//! let node_id = NodeId::new(0x12345678);
//! handler.register_device("WEAROS-ABCD", "AA:BB:CC:DD:EE:01", node_id);
//!
//! // Later, same device with rotated address
//! if let Some(existing) = handler.lookup_by_name("WEAROS-ABCD") {
//!     // Update the address mapping
//!     handler.update_address("WEAROS-ABCD", "AA:BB:CC:DD:EE:02");
//!     println!("Address rotated for node {:?}", existing);
//! }
//! ```

use std::collections::HashMap;

use crate::NodeId;

/// Patterns that indicate a device may rotate its BLE address
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevicePattern {
    /// WearTAK on WearOS (WT-WEAROS-XXXX)
    WearTak,
    /// Generic WearOS device (WEAROS-XXXX)
    WearOs,
    /// HIVE mesh device (HIVE_MESH-XXXX or HIVE-XXXX)
    Hive,
    /// Unknown pattern (may still rotate addresses)
    Unknown,
}

impl DevicePattern {
    /// Check if this device type is known to rotate addresses
    pub fn rotates_addresses(&self) -> bool {
        matches!(self, DevicePattern::WearTak | DevicePattern::WearOs)
    }
}

/// Detect the device pattern from a BLE device name
pub fn detect_device_pattern(name: &str) -> DevicePattern {
    if name.starts_with("WT-WEAROS-") {
        DevicePattern::WearTak
    } else if name.starts_with("WEAROS-") {
        DevicePattern::WearOs
    } else if name.starts_with("HIVE_") || name.starts_with("HIVE-") {
        DevicePattern::Hive
    } else {
        DevicePattern::Unknown
    }
}

/// Check if a device name matches a WearTAK/WearOS pattern
pub fn is_weartak_device(name: &str) -> bool {
    name.starts_with("WT-WEAROS-") || name.starts_with("WEAROS-")
}

/// Normalize a WearTAK device name
///
/// Removes the "WT-" prefix if present to get a consistent "WEAROS-XXXX" format.
pub fn normalize_weartak_name(name: &str) -> &str {
    name.strip_prefix("WT-").unwrap_or(name)
}

/// Result of looking up a device by name
#[derive(Debug, Clone)]
pub struct DeviceLookupResult {
    /// The node ID for this device
    pub node_id: NodeId,
    /// The current known address
    pub current_address: String,
    /// Whether the address has changed
    pub address_changed: bool,
    /// The previous address (if changed)
    pub previous_address: Option<String>,
}

/// Handler for BLE address rotation
///
/// Maintains mappings between device names and node IDs to handle
/// address rotation gracefully.
#[derive(Debug, Default)]
pub struct AddressRotationHandler {
    /// Device name to node ID mapping
    name_to_node: HashMap<String, NodeId>,
    /// Node ID to device name mapping (reverse lookup)
    node_to_name: HashMap<NodeId, String>,
    /// Node ID to current address mapping
    node_to_address: HashMap<NodeId, String>,
    /// Address to node ID mapping
    address_to_node: HashMap<String, NodeId>,
}

impl AddressRotationHandler {
    /// Create a new address rotation handler
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new device
    ///
    /// Creates mappings for name, address, and node ID.
    pub fn register_device(&mut self, name: &str, address: &str, node_id: NodeId) {
        // Register name mapping
        if !name.is_empty() {
            self.name_to_node.insert(name.to_string(), node_id);
            self.node_to_name.insert(node_id, name.to_string());
        }

        // Register address mapping
        self.address_to_node.insert(address.to_string(), node_id);
        self.node_to_address.insert(node_id, address.to_string());

        log::debug!(
            "Registered device: name='{}' address='{}' node={:?}",
            name,
            address,
            node_id
        );
    }

    /// Look up a device by name
    ///
    /// Returns the node ID if the name is known.
    pub fn lookup_by_name(&self, name: &str) -> Option<NodeId> {
        self.name_to_node.get(name).copied()
    }

    /// Look up a device by address
    ///
    /// Returns the node ID if the address is known.
    pub fn lookup_by_address(&self, address: &str) -> Option<NodeId> {
        self.address_to_node.get(address).copied()
    }

    /// Get the current address for a node
    pub fn get_address(&self, node_id: &NodeId) -> Option<&String> {
        self.node_to_address.get(node_id)
    }

    /// Get the name for a node
    pub fn get_name(&self, node_id: &NodeId) -> Option<&String> {
        self.node_to_name.get(node_id)
    }

    /// Handle a device discovery, detecting address rotation
    ///
    /// This is the main entry point for handling discovered devices.
    /// It checks if we know this device by name and handles address
    /// rotation automatically.
    ///
    /// Returns:
    /// - `Some(DeviceLookupResult)` if the device was found by name (existing device)
    /// - `None` if this is a new device
    pub fn on_device_discovered(
        &mut self,
        name: &str,
        address: &str,
    ) -> Option<DeviceLookupResult> {
        // First, try to find by name (handles address rotation)
        if !name.is_empty() {
            if let Some(node_id) = self.name_to_node.get(name).copied() {
                let current_address = self.node_to_address.get(&node_id).cloned();
                let address_changed = current_address.as_ref() != Some(&address.to_string());
                let previous_address = if address_changed {
                    current_address.clone()
                } else {
                    None
                };

                // Update address mapping if changed
                if address_changed {
                    self.update_address_internal(node_id, address, current_address.as_deref());
                }

                return Some(DeviceLookupResult {
                    node_id,
                    current_address: address.to_string(),
                    address_changed,
                    previous_address,
                });
            }
        }

        // Not found by name, try by address (no rotation)
        if let Some(node_id) = self.address_to_node.get(address).copied() {
            return Some(DeviceLookupResult {
                node_id,
                current_address: address.to_string(),
                address_changed: false,
                previous_address: None,
            });
        }

        // New device
        None
    }

    /// Update the address for a device (used when address rotation is detected)
    pub fn update_address(&mut self, name: &str, new_address: &str) -> bool {
        if let Some(node_id) = self.name_to_node.get(name).copied() {
            let old_address = self.node_to_address.get(&node_id).cloned();
            self.update_address_internal(node_id, new_address, old_address.as_deref());
            true
        } else {
            false
        }
    }

    /// Internal helper to update address mappings
    fn update_address_internal(
        &mut self,
        node_id: NodeId,
        new_address: &str,
        old_address: Option<&str>,
    ) {
        // Remove old address mapping
        if let Some(old) = old_address {
            self.address_to_node.remove(old);
            log::info!(
                "Address rotation detected for {:?}: {} -> {}",
                node_id,
                old,
                new_address
            );
        }

        // Add new address mapping
        self.address_to_node
            .insert(new_address.to_string(), node_id);
        self.node_to_address
            .insert(node_id, new_address.to_string());
    }

    /// Update the name for a device (e.g., when callsign is received)
    pub fn update_name(&mut self, node_id: NodeId, new_name: &str) {
        // Remove old name mapping
        if let Some(old_name) = self.node_to_name.get(&node_id).cloned() {
            if old_name != new_name {
                self.name_to_node.remove(&old_name);
                log::debug!(
                    "Name updated for {:?}: '{}' -> '{}'",
                    node_id,
                    old_name,
                    new_name
                );
            }
        }

        // Add new name mapping
        if !new_name.is_empty() {
            self.name_to_node.insert(new_name.to_string(), node_id);
            self.node_to_name.insert(node_id, new_name.to_string());
        }
    }

    /// Remove a device from all mappings
    pub fn remove_device(&mut self, node_id: &NodeId) {
        // Remove name mappings
        if let Some(name) = self.node_to_name.remove(node_id) {
            self.name_to_node.remove(&name);
        }

        // Remove address mappings
        if let Some(address) = self.node_to_address.remove(node_id) {
            self.address_to_node.remove(&address);
        }

        log::debug!("Removed device {:?} from rotation handler", node_id);
    }

    /// Clear all mappings
    pub fn clear(&mut self) {
        self.name_to_node.clear();
        self.node_to_name.clear();
        self.node_to_address.clear();
        self.address_to_node.clear();
    }

    /// Get the number of tracked devices
    pub fn device_count(&self) -> usize {
        self.node_to_address.len()
    }

    /// Get statistics about tracked mappings
    pub fn stats(&self) -> AddressRotationStats {
        AddressRotationStats {
            devices_with_names: self.name_to_node.len(),
            total_devices: self.node_to_address.len(),
            address_mappings: self.address_to_node.len(),
        }
    }
}

/// Statistics about address rotation handling
#[derive(Debug, Clone, Copy)]
pub struct AddressRotationStats {
    /// Number of devices tracked by name
    pub devices_with_names: usize,
    /// Total number of devices tracked
    pub total_devices: usize,
    /// Number of address mappings
    pub address_mappings: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_pattern_detection() {
        assert_eq!(
            detect_device_pattern("WT-WEAROS-ABCD"),
            DevicePattern::WearTak
        );
        assert_eq!(detect_device_pattern("WEAROS-1234"), DevicePattern::WearOs);
        assert_eq!(
            detect_device_pattern("HIVE_MESH-12345678"),
            DevicePattern::Hive
        );
        assert_eq!(detect_device_pattern("HIVE-12345678"), DevicePattern::Hive);
        assert_eq!(
            detect_device_pattern("SomeOtherDevice"),
            DevicePattern::Unknown
        );
    }

    #[test]
    fn test_weartak_detection() {
        assert!(is_weartak_device("WT-WEAROS-ABCD"));
        assert!(is_weartak_device("WEAROS-1234"));
        assert!(!is_weartak_device("HIVE-12345678"));
    }

    #[test]
    fn test_normalize_weartak_name() {
        assert_eq!(normalize_weartak_name("WT-WEAROS-ABCD"), "WEAROS-ABCD");
        assert_eq!(normalize_weartak_name("WEAROS-1234"), "WEAROS-1234");
    }

    #[test]
    fn test_register_and_lookup() {
        let mut handler = AddressRotationHandler::new();
        let node_id = NodeId::new(0x12345678);

        handler.register_device("WEAROS-ABCD", "AA:BB:CC:DD:EE:01", node_id);

        assert_eq!(handler.lookup_by_name("WEAROS-ABCD"), Some(node_id));
        assert_eq!(
            handler.lookup_by_address("AA:BB:CC:DD:EE:01"),
            Some(node_id)
        );
        assert_eq!(
            handler.get_address(&node_id),
            Some(&"AA:BB:CC:DD:EE:01".to_string())
        );
    }

    #[test]
    fn test_address_rotation_detection() {
        let mut handler = AddressRotationHandler::new();
        let node_id = NodeId::new(0x12345678);

        // Initial registration
        handler.register_device("WEAROS-ABCD", "AA:BB:CC:DD:EE:01", node_id);

        // Simulate address rotation - same name, new address
        let result = handler
            .on_device_discovered("WEAROS-ABCD", "AA:BB:CC:DD:EE:02")
            .unwrap();

        assert_eq!(result.node_id, node_id);
        assert!(result.address_changed);
        assert_eq!(
            result.previous_address,
            Some("AA:BB:CC:DD:EE:01".to_string())
        );
        assert_eq!(result.current_address, "AA:BB:CC:DD:EE:02");

        // Verify mappings updated
        assert_eq!(
            handler.lookup_by_address("AA:BB:CC:DD:EE:02"),
            Some(node_id)
        );
        assert_eq!(handler.lookup_by_address("AA:BB:CC:DD:EE:01"), None);
    }

    #[test]
    fn test_new_device_discovery() {
        let mut handler = AddressRotationHandler::new();

        // New device should return None
        let result = handler.on_device_discovered("WEAROS-NEW", "AA:BB:CC:DD:EE:FF");
        assert!(result.is_none());
    }

    #[test]
    fn test_remove_device() {
        let mut handler = AddressRotationHandler::new();
        let node_id = NodeId::new(0x12345678);

        handler.register_device("WEAROS-ABCD", "AA:BB:CC:DD:EE:01", node_id);
        assert_eq!(handler.device_count(), 1);

        handler.remove_device(&node_id);

        assert_eq!(handler.device_count(), 0);
        assert!(handler.lookup_by_name("WEAROS-ABCD").is_none());
        assert!(handler.lookup_by_address("AA:BB:CC:DD:EE:01").is_none());
    }

    #[test]
    fn test_update_name() {
        let mut handler = AddressRotationHandler::new();
        let node_id = NodeId::new(0x12345678);

        handler.register_device("WEAROS-ABCD", "AA:BB:CC:DD:EE:01", node_id);
        handler.update_name(node_id, "MyCallsign");

        assert!(handler.lookup_by_name("WEAROS-ABCD").is_none());
        assert_eq!(handler.lookup_by_name("MyCallsign"), Some(node_id));
    }
}
