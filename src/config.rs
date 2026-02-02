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

//! Configuration types for HIVE-BTLE
//!
//! Provides configuration structures for BLE transport, discovery,
//! GATT, mesh, power management, and security settings.

use crate::NodeId;

/// BLE Physical Layer (PHY) type
///
/// BLE 5.0+ supports multiple PHY options with different
/// trade-offs between range, throughput, and power consumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlePhy {
    /// LE 1M PHY - 1 Mbps, ~100m range (default, most compatible)
    #[default]
    Le1M,
    /// LE 2M PHY - 2 Mbps, ~50m range (higher throughput)
    Le2M,
    /// LE Coded S=2 - 500 kbps, ~200m range
    LeCodedS2,
    /// LE Coded S=8 - 125 kbps, ~400m range (maximum range)
    LeCodedS8,
}

impl BlePhy {
    /// Get the theoretical bandwidth in bytes per second
    pub fn bandwidth_bps(&self) -> u32 {
        match self {
            BlePhy::Le1M => 1_000_000,
            BlePhy::Le2M => 2_000_000,
            BlePhy::LeCodedS2 => 500_000,
            BlePhy::LeCodedS8 => 125_000,
        }
    }

    /// Get the typical range in meters
    pub fn typical_range_meters(&self) -> u32 {
        match self {
            BlePhy::Le1M => 100,
            BlePhy::Le2M => 50,
            BlePhy::LeCodedS2 => 200,
            BlePhy::LeCodedS8 => 400,
        }
    }

    /// Check if this PHY requires BLE 5.0+
    pub fn requires_ble5(&self) -> bool {
        matches!(self, BlePhy::Le2M | BlePhy::LeCodedS2 | BlePhy::LeCodedS8)
    }
}

/// Power management profile
///
/// Controls radio duty cycle and timing parameters to balance
/// responsiveness against battery consumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PowerProfile {
    /// Aggressive - ~20% duty cycle, ~6 hour watch battery
    /// Use for high-activity scenarios
    Aggressive,

    /// Balanced - ~10% duty cycle, ~12 hour watch battery
    #[default]
    Balanced,

    /// Low Power - ~2% duty cycle, ~20+ hour watch battery
    /// Recommended for HIVE-Lite nodes
    LowPower,

    /// Custom power profile with explicit parameters
    Custom {
        /// Scan interval in milliseconds
        scan_interval_ms: u32,
        /// Scan window in milliseconds
        scan_window_ms: u32,
        /// Advertisement interval in milliseconds
        adv_interval_ms: u32,
        /// Connection interval in milliseconds
        conn_interval_ms: u32,
    },
}

impl PowerProfile {
    /// Get scan interval in milliseconds
    pub fn scan_interval_ms(&self) -> u32 {
        match self {
            PowerProfile::Aggressive => 100,
            PowerProfile::Balanced => 500,
            PowerProfile::LowPower => 5000,
            PowerProfile::Custom {
                scan_interval_ms, ..
            } => *scan_interval_ms,
        }
    }

    /// Get scan window in milliseconds
    pub fn scan_window_ms(&self) -> u32 {
        match self {
            PowerProfile::Aggressive => 50,
            PowerProfile::Balanced => 50,
            PowerProfile::LowPower => 100,
            PowerProfile::Custom { scan_window_ms, .. } => *scan_window_ms,
        }
    }

    /// Get advertisement interval in milliseconds
    pub fn adv_interval_ms(&self) -> u32 {
        match self {
            PowerProfile::Aggressive => 100,
            PowerProfile::Balanced => 500,
            PowerProfile::LowPower => 2000,
            PowerProfile::Custom {
                adv_interval_ms, ..
            } => *adv_interval_ms,
        }
    }

    /// Get connection interval in milliseconds
    pub fn conn_interval_ms(&self) -> u32 {
        match self {
            PowerProfile::Aggressive => 15,
            PowerProfile::Balanced => 30,
            PowerProfile::LowPower => 100,
            PowerProfile::Custom {
                conn_interval_ms, ..
            } => *conn_interval_ms,
        }
    }

    /// Estimated radio duty cycle as percentage
    pub fn duty_cycle_percent(&self) -> u8 {
        match self {
            PowerProfile::Aggressive => 20,
            PowerProfile::Balanced => 10,
            PowerProfile::LowPower => 2,
            PowerProfile::Custom {
                scan_interval_ms,
                scan_window_ms,
                ..
            } => {
                if *scan_interval_ms == 0 {
                    0
                } else {
                    ((scan_window_ms * 100) / scan_interval_ms) as u8
                }
            }
        }
    }
}

/// Discovery configuration
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Scan interval in milliseconds
    pub scan_interval_ms: u32,
    /// Scan window in milliseconds (must be <= scan_interval_ms)
    pub scan_window_ms: u32,
    /// Advertisement interval in milliseconds
    pub adv_interval_ms: u32,
    /// Transmit power in dBm (-20 to +10 typical)
    pub tx_power_dbm: i8,
    /// PHY for advertising
    pub adv_phy: BlePhy,
    /// PHY for scanning
    pub scan_phy: BlePhy,
    /// Enable active scanning (requests scan response)
    pub active_scan: bool,
    /// Filter duplicates during scan
    pub filter_duplicates: bool,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            scan_interval_ms: 500,
            scan_window_ms: 50,
            adv_interval_ms: 500,
            tx_power_dbm: 0,
            adv_phy: BlePhy::Le1M,
            scan_phy: BlePhy::Le1M,
            active_scan: true,
            filter_duplicates: true,
        }
    }
}

/// GATT configuration
#[derive(Debug, Clone)]
pub struct GattConfig {
    /// Preferred MTU size (23-517 bytes)
    pub preferred_mtu: u16,
    /// Minimum acceptable MTU
    pub min_mtu: u16,
    /// Enable GATT server (peripheral) mode
    pub enable_server: bool,
    /// Enable GATT client (central) mode
    pub enable_client: bool,
}

impl Default for GattConfig {
    fn default() -> Self {
        Self {
            preferred_mtu: 251,
            min_mtu: 23,
            enable_server: true,
            enable_client: true,
        }
    }
}

/// Default mesh ID for demos and testing
pub const DEFAULT_MESH_ID: &str = "DEMO";

/// Mesh configuration
#[derive(Debug, Clone)]
pub struct MeshConfig {
    /// Mesh identifier - nodes only auto-connect to peers with matching mesh ID
    ///
    /// Format: 4-character alphanumeric (e.g., "DEMO", "ALFA", "TEST")
    /// This maps to the `app_id` concept in hive-protocol.
    pub mesh_id: String,
    /// Maximum number of simultaneous connections
    pub max_connections: u8,
    /// Maximum children for this node (0 = leaf node)
    pub max_children: u8,
    /// Connection supervision timeout in milliseconds
    pub supervision_timeout_ms: u16,
    /// Slave latency (number of connection events to skip)
    pub slave_latency: u16,
    /// Minimum connection interval in milliseconds
    pub conn_interval_min_ms: u16,
    /// Maximum connection interval in milliseconds
    pub conn_interval_max_ms: u16,
}

impl MeshConfig {
    /// Create a new mesh config with the given mesh ID
    pub fn new(mesh_id: impl Into<String>) -> Self {
        Self {
            mesh_id: mesh_id.into(),
            ..Default::default()
        }
    }

    /// Generate the BLE device name for this node
    ///
    /// Format: `HIVE_<MESH_ID>-<NODE_ID>` (e.g., "HIVE_DEMO-12345678")
    pub fn device_name(&self, node_id: NodeId) -> String {
        format!("HIVE_{}-{:08X}", self.mesh_id, node_id.as_u32())
    }

    /// Parse mesh ID and node ID from a device name
    ///
    /// Returns `Some((mesh_id, node_id))` for valid names, `None` otherwise.
    ///
    /// Supports both formats:
    /// - New: `HIVE_<MESH_ID>-<NODE_ID>` (e.g., "HIVE_DEMO-12345678")
    /// - Legacy: `HIVE-<NODE_ID>` (e.g., "HIVE-12345678") - returns None for mesh_id
    pub fn parse_device_name(name: &str) -> Option<(Option<String>, NodeId)> {
        if let Some(rest) = name.strip_prefix("HIVE_") {
            // New format: HIVE_MESHID-NODEID
            let (mesh_id, node_id_str) = rest.split_once('-')?;
            let node_id = u32::from_str_radix(node_id_str, 16).ok()?;
            Some((Some(mesh_id.to_string()), NodeId::new(node_id)))
        } else if let Some(node_id_str) = name.strip_prefix("HIVE-") {
            // Legacy format: HIVE-NODEID (no mesh ID)
            let node_id = u32::from_str_radix(node_id_str, 16).ok()?;
            Some((None, NodeId::new(node_id)))
        } else {
            None
        }
    }

    /// Check if a discovered device matches this mesh
    ///
    /// Returns true if:
    /// - The device has the same mesh ID, OR
    /// - The device has no mesh ID (legacy format - backwards compatible)
    pub fn matches_mesh(&self, device_mesh_id: Option<&str>) -> bool {
        match device_mesh_id {
            Some(id) => id == self.mesh_id,
            None => true, // Legacy devices match any mesh
        }
    }
}

impl Default for MeshConfig {
    fn default() -> Self {
        Self {
            mesh_id: DEFAULT_MESH_ID.to_string(),
            max_connections: 7,
            max_children: 3,
            supervision_timeout_ms: 4000,
            slave_latency: 0,
            conn_interval_min_ms: 30,
            conn_interval_max_ms: 50,
        }
    }
}

/// PHY selection strategy
#[derive(Debug, Clone)]
pub enum PhyStrategy {
    /// Use a fixed PHY
    Fixed(BlePhy),
    /// Adaptive PHY selection based on RSSI
    Adaptive {
        /// RSSI threshold to switch to high-throughput PHY (dBm)
        rssi_high_threshold: i8,
        /// RSSI threshold to switch to long-range PHY (dBm)
        rssi_low_threshold: i8,
        /// Hysteresis to prevent oscillation (dB)
        hysteresis_db: u8,
    },
    /// Always use maximum range (Coded S=8)
    MaxRange,
    /// Always use maximum throughput (2M)
    MaxThroughput,
}

impl Default for PhyStrategy {
    fn default() -> Self {
        PhyStrategy::Fixed(BlePhy::Le1M)
    }
}

/// PHY configuration
#[derive(Debug, Clone, Default)]
pub struct PhyConfig {
    /// PHY selection strategy
    pub strategy: PhyStrategy,
    /// Preferred PHY for connections
    pub preferred_phy: BlePhy,
    /// Allow PHY upgrade after connection
    pub allow_phy_update: bool,
}

/// Security configuration
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Require pairing before data exchange
    pub require_pairing: bool,
    /// Require encrypted connections
    pub require_encryption: bool,
    /// Enable MITM protection
    pub require_mitm_protection: bool,
    /// Enable Secure Connections (BLE 4.2+)
    pub require_secure_connections: bool,
    /// Enable application-layer encryption (in addition to BLE)
    pub app_layer_encryption: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            require_pairing: false,
            require_encryption: true,
            require_mitm_protection: false,
            require_secure_connections: false,
            app_layer_encryption: false,
        }
    }
}

/// Main BLE transport configuration
#[derive(Debug, Clone)]
pub struct BleConfig {
    /// This node's identifier
    pub node_id: NodeId,
    /// Node capabilities flags
    pub capabilities: u16,
    /// Hierarchy level (0 = platform/leaf)
    pub hierarchy_level: u8,
    /// Geohash for location (24-bit, 6-char precision)
    pub geohash: u32,
    /// Discovery configuration
    pub discovery: DiscoveryConfig,
    /// GATT configuration
    pub gatt: GattConfig,
    /// Mesh configuration
    pub mesh: MeshConfig,
    /// Power profile
    pub power_profile: PowerProfile,
    /// PHY configuration
    pub phy: PhyConfig,
    /// Security configuration
    pub security: SecurityConfig,
}

impl BleConfig {
    /// Create a new configuration with the given node ID
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            capabilities: 0,
            hierarchy_level: 0,
            geohash: 0,
            discovery: DiscoveryConfig::default(),
            gatt: GattConfig::default(),
            mesh: MeshConfig::default(),
            power_profile: PowerProfile::default(),
            phy: PhyConfig::default(),
            security: SecurityConfig::default(),
        }
    }

    /// Create a HIVE-Lite optimized configuration
    ///
    /// Optimized for battery efficiency with:
    /// - Low power profile (~2% duty cycle)
    /// - Leaf node (no children)
    /// - Minimal scanning
    pub fn hive_lite(node_id: NodeId) -> Self {
        let mut config = Self::new(node_id);
        config.power_profile = PowerProfile::LowPower;
        config.mesh.max_children = 0; // Leaf node
        config.discovery.scan_interval_ms = 5000;
        config.discovery.scan_window_ms = 100;
        config.discovery.adv_interval_ms = 2000;
        config
    }

    /// Apply power profile settings to discovery config
    pub fn apply_power_profile(&mut self) {
        self.discovery.scan_interval_ms = self.power_profile.scan_interval_ms();
        self.discovery.scan_window_ms = self.power_profile.scan_window_ms();
        self.discovery.adv_interval_ms = self.power_profile.adv_interval_ms();
        self.mesh.conn_interval_min_ms = self.power_profile.conn_interval_ms() as u16;
        self.mesh.conn_interval_max_ms = self.power_profile.conn_interval_ms() as u16 + 20;
    }
}

impl Default for BleConfig {
    fn default() -> Self {
        Self::new(NodeId::default())
    }
}

// ============================================================================
// Build-time Embedded Secrets
// ============================================================================

/// Get the compile-time embedded encryption secret, if set.
///
/// Set the `HIVE_ENCRYPTION_SECRET` environment variable during build to embed
/// a 64-character hex string (32 bytes) as the default mesh encryption secret.
///
/// # Example
///
/// Build with embedded secret:
/// ```bash
/// HIVE_ENCRYPTION_SECRET=0102030405060708091011121314151617181920212223242526272829303132 \
///   cargo build --release
/// ```
///
/// Use in code:
/// ```ignore
/// use hive_btle::config::embedded_encryption_secret;
/// use hive_btle::hive_mesh::HiveMeshConfig;
///
/// let config = if let Some(secret) = embedded_encryption_secret() {
///     HiveMeshConfig::new(node_id, "CALL", "MESH").with_encryption(secret)
/// } else {
///     HiveMeshConfig::new(node_id, "CALL", "MESH")
/// };
/// ```
///
/// # Security Note
///
/// The embedded secret is compiled into the binary. This is suitable for:
/// - Development/testing with a fixed secret
/// - Closed deployments where binaries are distributed securely
///
/// For dynamic secret management, use `MeshGenesis` or runtime configuration.
pub fn embedded_encryption_secret() -> Option<[u8; 32]> {
    // Read at compile time - returns None if not set
    option_env!("HIVE_ENCRYPTION_SECRET").and_then(parse_hex_secret)
}

/// Get the compile-time embedded mesh ID, if set.
///
/// Set the `HIVE_MESH_ID` environment variable during build to embed
/// a default mesh ID.
///
/// # Example
///
/// ```bash
/// HIVE_MESH_ID=ALPHA cargo build --release
/// ```
pub fn embedded_mesh_id() -> Option<&'static str> {
    option_env!("HIVE_MESH_ID")
}

/// Check if a compile-time encryption secret was embedded.
pub fn has_embedded_encryption_secret() -> bool {
    option_env!("HIVE_ENCRYPTION_SECRET").is_some()
}

/// Parse a 64-character hex string into a 32-byte array.
fn parse_hex_secret(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }

    let mut result = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        if i >= 32 {
            return None;
        }
        let high = hex_digit(chunk[0])?;
        let low = hex_digit(chunk[1])?;
        result[i] = (high << 4) | low;
    }
    Some(result)
}

/// Convert a hex character to its numeric value.
fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phy_properties() {
        assert_eq!(BlePhy::Le1M.bandwidth_bps(), 1_000_000);
        assert_eq!(BlePhy::LeCodedS8.typical_range_meters(), 400);
        assert!(!BlePhy::Le1M.requires_ble5());
        assert!(BlePhy::Le2M.requires_ble5());
    }

    #[test]
    fn test_power_profile_duty_cycle() {
        assert_eq!(PowerProfile::Aggressive.duty_cycle_percent(), 20);
        assert_eq!(PowerProfile::Balanced.duty_cycle_percent(), 10);
        assert_eq!(PowerProfile::LowPower.duty_cycle_percent(), 2);
    }

    #[test]
    fn test_hive_lite_config() {
        let config = BleConfig::hive_lite(NodeId::new(0x12345678));
        assert_eq!(config.mesh.max_children, 0);
        assert_eq!(config.power_profile, PowerProfile::LowPower);
        assert_eq!(config.discovery.scan_interval_ms, 5000);
    }

    #[test]
    fn test_apply_power_profile() {
        let mut config = BleConfig::new(NodeId::new(0x12345678));
        config.power_profile = PowerProfile::LowPower;
        config.apply_power_profile();
        assert_eq!(config.discovery.scan_interval_ms, 5000);
        assert_eq!(config.discovery.adv_interval_ms, 2000);
    }

    #[test]
    fn test_mesh_config_default() {
        let config = MeshConfig::default();
        assert_eq!(config.mesh_id, DEFAULT_MESH_ID);
        assert_eq!(config.mesh_id, "DEMO");
    }

    #[test]
    fn test_mesh_config_new() {
        let config = MeshConfig::new("ALFA");
        assert_eq!(config.mesh_id, "ALFA");
    }

    #[test]
    fn test_device_name_generation() {
        let config = MeshConfig::new("DEMO");
        let name = config.device_name(NodeId::new(0x12345678));
        assert_eq!(name, "HIVE_DEMO-12345678");

        let config = MeshConfig::new("ALFA");
        let name = config.device_name(NodeId::new(0xDEADBEEF));
        assert_eq!(name, "HIVE_ALFA-DEADBEEF");
    }

    #[test]
    fn test_parse_device_name_new_format() {
        // New format: HIVE_MESHID-NODEID
        let result = MeshConfig::parse_device_name("HIVE_DEMO-12345678");
        assert!(result.is_some());
        let (mesh_id, node_id) = result.unwrap();
        assert_eq!(mesh_id, Some("DEMO".to_string()));
        assert_eq!(node_id.as_u32(), 0x12345678);

        let result = MeshConfig::parse_device_name("HIVE_ALFA-DEADBEEF");
        assert!(result.is_some());
        let (mesh_id, node_id) = result.unwrap();
        assert_eq!(mesh_id, Some("ALFA".to_string()));
        assert_eq!(node_id.as_u32(), 0xDEADBEEF);
    }

    #[test]
    fn test_parse_device_name_legacy_format() {
        // Legacy format: HIVE-NODEID (no mesh ID)
        let result = MeshConfig::parse_device_name("HIVE-12345678");
        assert!(result.is_some());
        let (mesh_id, node_id) = result.unwrap();
        assert_eq!(mesh_id, None);
        assert_eq!(node_id.as_u32(), 0x12345678);
    }

    #[test]
    fn test_parse_device_name_invalid() {
        assert!(MeshConfig::parse_device_name("NotHIVE").is_none());
        assert!(MeshConfig::parse_device_name("HIVE_DEMO").is_none()); // Missing node ID
        assert!(MeshConfig::parse_device_name("").is_none());
    }

    #[test]
    fn test_matches_mesh() {
        let config = MeshConfig::new("DEMO");

        // Same mesh ID matches
        assert!(config.matches_mesh(Some("DEMO")));

        // Different mesh ID does not match
        assert!(!config.matches_mesh(Some("ALFA")));

        // Legacy devices (no mesh ID) match any mesh for backwards compatibility
        assert!(config.matches_mesh(None));
    }

    #[test]
    fn test_parse_hex_secret() {
        // Valid 64-char hex
        let hex = "0102030405060708091011121314151617181920212223242526272829303132";
        let result = parse_hex_secret(hex);
        assert!(result.is_some());
        let secret = result.unwrap();
        assert_eq!(secret[0], 0x01);
        assert_eq!(secret[1], 0x02);
        assert_eq!(secret[31], 0x32);

        // Mixed case hex (64 chars = 32 bytes)
        let hex = "AABBCCDD01020304050607080910111213141516171819202122232425262728";
        let result = parse_hex_secret(hex);
        assert!(result.is_some());
        let secret = result.unwrap();
        assert_eq!(secret[0], 0xAA);
        assert_eq!(secret[1], 0xBB);
    }

    #[test]
    fn test_parse_hex_secret_invalid() {
        // Too short
        assert!(parse_hex_secret("0102030405").is_none());

        // Too long
        assert!(parse_hex_secret(
            "01020304050607080910111213141516171819202122232425262728293031323334"
        )
        .is_none());

        // Invalid characters
        assert!(
            parse_hex_secret("GGHHIIJJ0102030405060708091011121314151617181920212223242526")
                .is_none()
        );

        // Empty
        assert!(parse_hex_secret("").is_none());
    }

    #[test]
    fn test_embedded_functions_exist() {
        // These just verify the functions compile and can be called
        // The actual values depend on build-time env vars
        let _ = embedded_encryption_secret();
        let _ = embedded_mesh_id();
        let _ = has_embedded_encryption_secret();
    }
}
