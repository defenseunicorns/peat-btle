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

//! Peer lifetime management with stale peer cleanup
//!
//! Tracks peer activity and identifies stale peers that should be removed.
//! This prevents memory leaks from accumulated discovered devices that are
//! no longer in range.
//!
//! # Example
//!
//! ```ignore
//! use eche_btle::peer_lifetime::{PeerLifetimeManager, PeerLifetimeConfig};
//!
//! let config = PeerLifetimeConfig::default();
//! let mut manager = PeerLifetimeManager::new(config);
//!
//! // When a peer is discovered or connects
//! manager.on_peer_activity("00:11:22:33:44:55", true); // connected = true
//!
//! // When peer disconnects
//! manager.on_peer_disconnected("00:11:22:33:44:55");
//!
//! // Periodically check for stale peers
//! for address in manager.get_stale_peers() {
//!     // Remove peer from your data structures
//!     manager.remove_peer(&address);
//! }
//! ```

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Configuration for peer lifetime management
#[derive(Debug, Clone)]
pub struct PeerLifetimeConfig {
    /// Timeout for disconnected peers (default: 30 seconds)
    /// Peers that have been disconnected for longer than this are considered stale
    pub disconnected_timeout: Duration,

    /// Timeout for connected peers (default: 60 seconds)
    /// Connected peers that haven't had activity for longer than this are
    /// considered stale (handles ghost connections where disconnect was missed)
    pub connected_timeout: Duration,

    /// Interval for cleanup checks (default: 10 seconds)
    pub cleanup_interval: Duration,
}

impl Default for PeerLifetimeConfig {
    fn default() -> Self {
        Self {
            disconnected_timeout: Duration::from_secs(30),
            connected_timeout: Duration::from_secs(60),
            cleanup_interval: Duration::from_secs(10),
        }
    }
}

impl PeerLifetimeConfig {
    /// Create a new configuration with custom values
    pub fn new(
        disconnected_timeout: Duration,
        connected_timeout: Duration,
        cleanup_interval: Duration,
    ) -> Self {
        Self {
            disconnected_timeout,
            connected_timeout,
            cleanup_interval,
        }
    }

    /// Create a fast configuration for testing
    pub fn fast() -> Self {
        Self {
            disconnected_timeout: Duration::from_secs(5),
            connected_timeout: Duration::from_secs(10),
            cleanup_interval: Duration::from_secs(2),
        }
    }

    /// Create a relaxed configuration for stable networks
    pub fn relaxed() -> Self {
        Self {
            disconnected_timeout: Duration::from_secs(60),
            connected_timeout: Duration::from_secs(120),
            cleanup_interval: Duration::from_secs(30),
        }
    }
}

/// State for tracking a single peer's lifetime
#[derive(Debug, Clone)]
struct PeerState {
    /// Whether the peer is currently connected
    connected: bool,
    /// Last time we saw activity from this peer
    last_seen: Instant,
    /// When the peer was first discovered
    first_seen: Instant,
    /// When the peer disconnected (if disconnected)
    disconnected_at: Option<Instant>,
}

impl PeerState {
    fn new(connected: bool) -> Self {
        let now = Instant::now();
        Self {
            connected,
            last_seen: now,
            first_seen: now,
            disconnected_at: if connected { None } else { Some(now) },
        }
    }
}

/// Reason a peer is considered stale
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaleReason {
    /// Disconnected peer hasn't been seen in a while
    DisconnectedTimeout,
    /// Connected peer hasn't had activity (possible ghost connection)
    ConnectedTimeout,
}

/// Information about a stale peer
#[derive(Debug, Clone)]
pub struct StalePeerInfo {
    /// Peer address
    pub address: String,
    /// Why the peer is considered stale
    pub reason: StaleReason,
    /// How long since the peer was last seen
    pub time_since_last_seen: Duration,
    /// Whether the peer was connected when it went stale
    pub was_connected: bool,
}

/// Manager for peer lifetime and stale peer cleanup
///
/// Tracks peer activity and determines when peers should be removed
/// to prevent memory leaks.
#[derive(Debug)]
pub struct PeerLifetimeManager {
    /// Configuration
    config: PeerLifetimeConfig,
    /// Per-peer state
    peers: HashMap<String, PeerState>,
}

impl PeerLifetimeManager {
    /// Create a new peer lifetime manager with the given configuration
    pub fn new(config: PeerLifetimeConfig) -> Self {
        Self {
            config,
            peers: HashMap::new(),
        }
    }

    /// Create a manager with default configuration
    pub fn with_defaults() -> Self {
        Self::new(PeerLifetimeConfig::default())
    }

    /// Record activity for a peer
    ///
    /// Call this when:
    /// - A peer is discovered via advertisement
    /// - A peer connects successfully
    /// - Data is received from a peer
    ///
    /// This updates the `last_seen` timestamp.
    pub fn on_peer_activity(&mut self, address: &str, connected: bool) {
        let now = Instant::now();

        if let Some(state) = self.peers.get_mut(address) {
            state.last_seen = now;
            if connected && !state.connected {
                // Transitioning to connected state
                state.connected = true;
                state.disconnected_at = None;
                log::debug!("Peer {} connected", address);
            } else if !connected && state.connected {
                // Transitioning to disconnected state
                state.connected = false;
                state.disconnected_at = Some(now);
                log::debug!("Peer {} disconnected", address);
            }
        } else {
            // New peer
            log::debug!("New peer {} (connected: {})", address, connected);
            self.peers
                .insert(address.to_string(), PeerState::new(connected));
        }
    }

    /// Record that a peer has disconnected
    ///
    /// Note: This does NOT update `last_seen` - that's intentional.
    /// We want the stale timeout to start from the last actual activity,
    /// not from the disconnect event.
    pub fn on_peer_disconnected(&mut self, address: &str) {
        if let Some(state) = self.peers.get_mut(address) {
            if state.connected {
                state.connected = false;
                state.disconnected_at = Some(Instant::now());
                log::debug!("Peer {} marked as disconnected", address);
            }
        }
    }

    /// Check if a peer is being tracked
    pub fn is_tracked(&self, address: &str) -> bool {
        self.peers.contains_key(address)
    }

    /// Check if a peer is connected
    pub fn is_connected(&self, address: &str) -> bool {
        self.peers
            .get(address)
            .map(|s| s.connected)
            .unwrap_or(false)
    }

    /// Get the list of stale peers that should be removed
    ///
    /// Returns addresses of peers that have exceeded their timeout:
    /// - Disconnected peers: `disconnected_timeout` since last seen
    /// - Connected peers: `connected_timeout` since last seen (handles ghost connections)
    pub fn get_stale_peers(&self) -> Vec<StalePeerInfo> {
        self.peers
            .iter()
            .filter_map(|(address, state)| {
                let time_since_last_seen = state.last_seen.elapsed();

                let (is_stale, reason) = if state.connected {
                    // Connected peers get longer timeout
                    let is_stale = time_since_last_seen > self.config.connected_timeout;
                    (is_stale, StaleReason::ConnectedTimeout)
                } else {
                    // Disconnected peers have shorter timeout
                    let is_stale = time_since_last_seen > self.config.disconnected_timeout;
                    (is_stale, StaleReason::DisconnectedTimeout)
                };

                if is_stale {
                    Some(StalePeerInfo {
                        address: address.clone(),
                        reason,
                        time_since_last_seen,
                        was_connected: state.connected,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get just the addresses of stale peers
    pub fn get_stale_peer_addresses(&self) -> Vec<String> {
        self.get_stale_peers()
            .into_iter()
            .map(|info| info.address)
            .collect()
    }

    /// Remove a peer from tracking
    ///
    /// Call this after cleaning up the peer's resources.
    pub fn remove_peer(&mut self, address: &str) -> bool {
        if self.peers.remove(address).is_some() {
            log::debug!("Removed peer {} from lifetime tracking", address);
            true
        } else {
            false
        }
    }

    /// Remove all stale peers and return their addresses
    ///
    /// Convenience method that combines `get_stale_peers` and `remove_peer`.
    pub fn cleanup_stale_peers(&mut self) -> Vec<StalePeerInfo> {
        let stale = self.get_stale_peers();

        for info in &stale {
            self.peers.remove(&info.address);
        }

        if !stale.is_empty() {
            log::debug!("Cleaned up {} stale peers", stale.len());
        }

        stale
    }

    /// Get statistics about tracked peers
    pub fn stats(&self) -> PeerLifetimeStats {
        let mut connected = 0;
        let mut disconnected = 0;

        for state in self.peers.values() {
            if state.connected {
                connected += 1;
            } else {
                disconnected += 1;
            }
        }

        PeerLifetimeStats {
            total_tracked: self.peers.len(),
            connected,
            disconnected,
        }
    }

    /// Get detailed info about a specific peer
    pub fn get_peer_info(&self, address: &str) -> Option<PeerInfo> {
        self.peers.get(address).map(|state| PeerInfo {
            connected: state.connected,
            time_since_last_seen: state.last_seen.elapsed(),
            time_since_first_seen: state.first_seen.elapsed(),
            time_since_disconnect: state.disconnected_at.map(|t| t.elapsed()),
        })
    }

    /// Clear all tracked peers
    pub fn clear(&mut self) {
        let count = self.peers.len();
        self.peers.clear();
        if count > 0 {
            log::debug!("Cleared {} peers from lifetime tracking", count);
        }
    }

    /// Get the number of tracked peers
    pub fn tracked_count(&self) -> usize {
        self.peers.len()
    }

    /// Get the cleanup interval from configuration
    pub fn cleanup_interval(&self) -> Duration {
        self.config.cleanup_interval
    }
}

/// Statistics about tracked peers
#[derive(Debug, Clone, Copy)]
pub struct PeerLifetimeStats {
    /// Total number of tracked peers
    pub total_tracked: usize,
    /// Number of connected peers
    pub connected: usize,
    /// Number of disconnected peers
    pub disconnected: usize,
}

/// Detailed information about a peer
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// Whether the peer is currently connected
    pub connected: bool,
    /// Time since last activity
    pub time_since_last_seen: Duration,
    /// Time since first discovery
    pub time_since_first_seen: Duration,
    /// Time since disconnect (if disconnected)
    pub time_since_disconnect: Option<Duration>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_new_peer_tracking() {
        let mut manager = PeerLifetimeManager::with_defaults();

        assert!(!manager.is_tracked("test"));

        manager.on_peer_activity("test", true);

        assert!(manager.is_tracked("test"));
        assert!(manager.is_connected("test"));
    }

    #[test]
    fn test_peer_disconnect() {
        let mut manager = PeerLifetimeManager::with_defaults();

        manager.on_peer_activity("test", true);
        assert!(manager.is_connected("test"));

        manager.on_peer_disconnected("test");
        assert!(!manager.is_connected("test"));
    }

    #[test]
    fn test_stale_peer_detection() {
        let config = PeerLifetimeConfig {
            disconnected_timeout: Duration::from_millis(50),
            connected_timeout: Duration::from_millis(100),
            cleanup_interval: Duration::from_millis(10),
        };
        let mut manager = PeerLifetimeManager::new(config);

        // Add a disconnected peer
        manager.on_peer_activity("test", false);

        // Should not be stale yet
        assert!(manager.get_stale_peers().is_empty());

        // Wait for timeout
        sleep(Duration::from_millis(60));

        // Should be stale now
        let stale = manager.get_stale_peers();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].address, "test");
        assert_eq!(stale[0].reason, StaleReason::DisconnectedTimeout);
    }

    #[test]
    fn test_cleanup_stale_peers() {
        let config = PeerLifetimeConfig {
            disconnected_timeout: Duration::from_millis(10),
            connected_timeout: Duration::from_millis(100),
            cleanup_interval: Duration::from_millis(5),
        };
        let mut manager = PeerLifetimeManager::new(config);

        manager.on_peer_activity("peer1", false);
        manager.on_peer_activity("peer2", true);

        sleep(Duration::from_millis(20));

        // Only peer1 should be stale (disconnected timeout is shorter)
        let cleaned = manager.cleanup_stale_peers();
        assert_eq!(cleaned.len(), 1);
        assert_eq!(cleaned[0].address, "peer1");

        // peer1 should be removed
        assert!(!manager.is_tracked("peer1"));
        // peer2 should still be tracked
        assert!(manager.is_tracked("peer2"));
    }

    #[test]
    fn test_stats() {
        let mut manager = PeerLifetimeManager::with_defaults();

        manager.on_peer_activity("connected1", true);
        manager.on_peer_activity("connected2", true);
        manager.on_peer_activity("disconnected1", false);

        let stats = manager.stats();
        assert_eq!(stats.total_tracked, 3);
        assert_eq!(stats.connected, 2);
        assert_eq!(stats.disconnected, 1);
    }

    // === Kotlin parity tests ===

    #[test]
    fn test_kotlin_timeout_values() {
        // Kotlin uses disconnected=120s, connected=300s
        let config = PeerLifetimeConfig::new(
            Duration::from_secs(120),
            Duration::from_secs(300),
            Duration::from_secs(30),
        );
        assert_eq!(config.disconnected_timeout, Duration::from_secs(120));
        assert_eq!(config.connected_timeout, Duration::from_secs(300));

        let mut manager = PeerLifetimeManager::new(config);

        // Disconnected peer within timeout should not be stale
        manager.on_peer_activity("peer1", false);
        assert!(manager.get_stale_peers().is_empty());

        // Connected peer within timeout should not be stale
        manager.on_peer_activity("peer2", true);
        assert!(manager.get_stale_peers().is_empty());
    }

    #[test]
    fn test_disconnect_does_not_update_last_seen() {
        // Critical: on_peer_disconnected must NOT update last_seen,
        // so stale timeout starts from last actual activity
        let config = PeerLifetimeConfig {
            disconnected_timeout: Duration::from_millis(50),
            connected_timeout: Duration::from_millis(200),
            cleanup_interval: Duration::from_millis(10),
        };
        let mut manager = PeerLifetimeManager::new(config);

        // Add a connected peer
        manager.on_peer_activity("test", true);

        // Wait a bit so last_seen is in the past
        sleep(Duration::from_millis(30));

        // Disconnect - should NOT update last_seen
        manager.on_peer_disconnected("test");

        // The peer's last_seen should still be ~30ms ago (from on_peer_activity),
        // not from on_peer_disconnected. Wait a bit more to exceed timeout.
        sleep(Duration::from_millis(30));

        // Total time since last_seen ~60ms > disconnected_timeout of 50ms
        let stale = manager.get_stale_peers();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].address, "test");
    }

    #[test]
    fn test_activity_resets_stale_timer() {
        let config = PeerLifetimeConfig {
            disconnected_timeout: Duration::from_millis(50),
            connected_timeout: Duration::from_millis(100),
            cleanup_interval: Duration::from_millis(10),
        };
        let mut manager = PeerLifetimeManager::new(config);

        manager.on_peer_activity("test", false);

        // Wait close to timeout
        sleep(Duration::from_millis(40));

        // Activity should reset the timer
        manager.on_peer_activity("test", false);

        // Should not be stale yet (timer was reset)
        assert!(manager.get_stale_peers().is_empty());

        // Wait past original timeout but not past reset
        sleep(Duration::from_millis(20));
        assert!(manager.get_stale_peers().is_empty());

        // Now wait past the reset timeout
        sleep(Duration::from_millis(40));
        assert_eq!(manager.get_stale_peers().len(), 1);
    }

    #[test]
    fn test_connected_peer_longer_timeout() {
        // Connected peers get connected_timeout (longer than disconnected_timeout)
        let config = PeerLifetimeConfig {
            disconnected_timeout: Duration::from_millis(30),
            connected_timeout: Duration::from_millis(80),
            cleanup_interval: Duration::from_millis(10),
        };
        let mut manager = PeerLifetimeManager::new(config);

        manager.on_peer_activity("connected", true);
        manager.on_peer_activity("disconnected", false);

        // After 40ms: disconnected should be stale, connected should not
        sleep(Duration::from_millis(40));

        let stale = manager.get_stale_peers();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].address, "disconnected");
        assert_eq!(stale[0].reason, StaleReason::DisconnectedTimeout);

        // After 90ms total: connected should also be stale
        sleep(Duration::from_millis(50));

        let stale = manager.get_stale_peers();
        assert_eq!(stale.len(), 2);
    }
}
