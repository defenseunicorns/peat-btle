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

//! Auto-reconnection manager with exponential backoff
//!
//! BLE connections can be lost due to range, interference, or device issues.
//! This module provides automatic reconnection with configurable exponential
//! backoff, matching the behavior of the Android implementation.
//!
//! # Example
//!
//! ```ignore
//! use peat_btle::reconnect::{ReconnectionManager, ReconnectionConfig};
//!
//! let config = ReconnectionConfig::default();
//! let mut manager = ReconnectionManager::new(config);
//!
//! // When a peer disconnects
//! manager.track_disconnection(peer_address.clone());
//!
//! // Periodically check for peers to reconnect
//! for peer in manager.get_peers_to_reconnect() {
//!     if try_connect(&peer).is_ok() {
//!         manager.on_connection_success(&peer);
//!     }
//! }
//! ```

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Configuration for reconnection behavior
#[derive(Debug, Clone)]
pub struct ReconnectionConfig {
    /// Base delay between reconnection attempts (default: 2 seconds)
    pub base_delay: Duration,
    /// Maximum delay between attempts (default: 60 seconds)
    pub max_delay: Duration,
    /// Maximum number of reconnection attempts before giving up (default: 10)
    pub max_attempts: u32,
    /// Interval for checking which peers need reconnection (default: 5 seconds)
    pub check_interval: Duration,
    /// Use flat delay instead of exponential backoff (default: false)
    /// When true, every attempt uses `base_delay` with no exponential increase.
    pub use_flat_delay: bool,
    /// Auto-reset attempt counter when max_attempts is exhausted (default: false)
    /// When true, reaching max_attempts resets the counter instead of abandoning the peer.
    pub reset_on_exhaustion: bool,
}

impl Default for ReconnectionConfig {
    fn default() -> Self {
        Self {
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(60),
            max_attempts: 10,
            check_interval: Duration::from_secs(5),
            use_flat_delay: false,
            reset_on_exhaustion: false,
        }
    }
}

impl ReconnectionConfig {
    /// Create a new configuration with custom values
    pub fn new(
        base_delay: Duration,
        max_delay: Duration,
        max_attempts: u32,
        check_interval: Duration,
    ) -> Self {
        Self {
            base_delay,
            max_delay,
            max_attempts,
            check_interval,
            use_flat_delay: false,
            reset_on_exhaustion: false,
        }
    }

    /// Create a fast reconnection config for testing
    pub fn fast() -> Self {
        Self {
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(5),
            max_attempts: 5,
            check_interval: Duration::from_secs(1),
            use_flat_delay: false,
            reset_on_exhaustion: false,
        }
    }

    /// Create a conservative config for battery-constrained devices
    pub fn conservative() -> Self {
        Self {
            base_delay: Duration::from_secs(5),
            max_delay: Duration::from_secs(120),
            max_attempts: 5,
            check_interval: Duration::from_secs(10),
            use_flat_delay: false,
            reset_on_exhaustion: false,
        }
    }

    /// Kotlin Android normal mode: base=1s, max=15s, 20 attempts, exponential backoff
    pub fn kotlin_normal() -> Self {
        Self {
            base_delay: Duration::from_millis(1000),
            max_delay: Duration::from_millis(15000),
            max_attempts: 20,
            check_interval: Duration::from_secs(5),
            use_flat_delay: false,
            reset_on_exhaustion: false,
        }
    }

    /// Kotlin Android high-priority mode: base=1s, flat delay, auto-reset on exhaustion
    pub fn kotlin_high_priority() -> Self {
        Self {
            base_delay: Duration::from_millis(1000),
            max_delay: Duration::from_millis(15000),
            max_attempts: 20,
            check_interval: Duration::from_secs(5),
            use_flat_delay: true,
            reset_on_exhaustion: true,
        }
    }
}

/// State for tracking a single peer's reconnection
#[derive(Debug, Clone)]
struct PeerReconnectionState {
    /// Number of reconnection attempts made
    attempts: u32,
    /// When the last attempt was made
    last_attempt: Instant,
    /// When the peer was first marked for reconnection
    disconnected_at: Instant,
}

impl PeerReconnectionState {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            attempts: 0,
            last_attempt: now,
            disconnected_at: now,
        }
    }
}

/// Result of checking if a peer should be reconnected
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconnectionStatus {
    /// Ready to attempt reconnection
    Ready,
    /// Waiting for backoff delay to expire
    Waiting {
        /// Time remaining until next attempt is allowed
        remaining: Duration,
    },
    /// Maximum attempts exceeded, peer is abandoned
    Exhausted {
        /// Number of attempts that were made
        attempts: u32,
    },
    /// Peer is not being tracked for reconnection
    NotTracked,
}

/// Manager for auto-reconnection with exponential backoff
///
/// Tracks disconnected peers and determines when to attempt reconnection
/// based on exponential backoff.
#[derive(Debug)]
pub struct ReconnectionManager {
    /// Configuration
    config: ReconnectionConfig,
    /// Per-peer reconnection state
    peers: HashMap<String, PeerReconnectionState>,
}

impl ReconnectionManager {
    /// Create a new reconnection manager with the given configuration
    pub fn new(config: ReconnectionConfig) -> Self {
        Self {
            config,
            peers: HashMap::new(),
        }
    }

    /// Create a manager with default configuration
    pub fn with_defaults() -> Self {
        Self::new(ReconnectionConfig::default())
    }

    /// Track a peer for reconnection after disconnection
    ///
    /// Call this when a peer disconnects unexpectedly.
    pub fn track_disconnection(&mut self, address: String) {
        use std::collections::hash_map::Entry;

        if let Entry::Vacant(entry) = self.peers.entry(address.clone()) {
            log::debug!("Tracking {} for reconnection", address);
            entry.insert(PeerReconnectionState::new());
        }
    }

    /// Check if a peer is being tracked for reconnection
    pub fn is_tracked(&self, address: &str) -> bool {
        self.peers.contains_key(address)
    }

    /// Get the reconnection status for a peer
    pub fn get_status(&self, address: &str) -> ReconnectionStatus {
        match self.peers.get(address) {
            None => ReconnectionStatus::NotTracked,
            Some(state) => {
                if state.attempts >= self.config.max_attempts {
                    if self.config.reset_on_exhaustion {
                        // Will be reset on next get_peers_to_reconnect() call
                        return ReconnectionStatus::Ready;
                    }
                    return ReconnectionStatus::Exhausted {
                        attempts: state.attempts,
                    };
                }

                // First attempt should be immediate (no delay)
                if state.attempts == 0 {
                    return ReconnectionStatus::Ready;
                }

                // Subsequent attempts use exponential backoff
                let delay = self.calculate_delay(state.attempts);
                let elapsed = state.last_attempt.elapsed();

                if elapsed >= delay {
                    ReconnectionStatus::Ready
                } else {
                    ReconnectionStatus::Waiting {
                        remaining: delay - elapsed,
                    }
                }
            }
        }
    }

    /// Calculate the backoff delay for a given attempt number
    ///
    /// Uses exponential backoff: delay = min(base * 2^attempts, max)
    /// In flat delay mode, always returns base_delay.
    fn calculate_delay(&self, attempts: u32) -> Duration {
        if self.config.use_flat_delay {
            return self.config.base_delay;
        }
        let multiplier = 1u64 << attempts.min(30); // Prevent overflow
        let delay_ms = self.config.base_delay.as_millis() as u64 * multiplier;
        let max_ms = self.config.max_delay.as_millis() as u64;
        Duration::from_millis(delay_ms.min(max_ms))
    }

    /// Get all peers that are ready for a reconnection attempt
    ///
    /// Returns addresses of peers that:
    /// - Haven't exceeded max attempts (or will be auto-reset if `reset_on_exhaustion` is true)
    /// - Have waited long enough since the last attempt (first attempt is immediate)
    pub fn get_peers_to_reconnect(&mut self) -> Vec<String> {
        // Auto-reset exhausted peers when configured
        if self.config.reset_on_exhaustion {
            let max = self.config.max_attempts;
            for state in self.peers.values_mut() {
                if state.attempts >= max {
                    log::debug!("Auto-resetting exhausted peer (reset_on_exhaustion)");
                    state.attempts = 0;
                    state.last_attempt = Instant::now();
                }
            }
        }

        self.peers
            .iter()
            .filter_map(|(address, state)| {
                if state.attempts >= self.config.max_attempts {
                    return None;
                }

                // First attempt is immediate
                if state.attempts == 0 {
                    return Some(address.clone());
                }

                // Subsequent attempts use exponential backoff (or flat delay)
                let delay = self.calculate_delay(state.attempts);
                if state.last_attempt.elapsed() >= delay {
                    Some(address.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Record a reconnection attempt for a peer
    ///
    /// Call this when starting a reconnection attempt.
    pub fn record_attempt(&mut self, address: &str) {
        let attempts = if let Some(state) = self.peers.get_mut(address) {
            state.attempts += 1;
            state.last_attempt = Instant::now();
            Some(state.attempts)
        } else {
            None
        };

        if let Some(attempts) = attempts {
            let next_delay = self.calculate_delay(attempts);
            log::debug!(
                "Reconnection attempt {} for {} (next delay: {:?})",
                attempts,
                address,
                next_delay
            );
        }
    }

    /// Called when a connection succeeds
    ///
    /// Removes the peer from reconnection tracking.
    pub fn on_connection_success(&mut self, address: &str) {
        if self.peers.remove(address).is_some() {
            log::debug!(
                "Connection succeeded for {}, removed from reconnection tracking",
                address
            );
        }
    }

    /// Stop tracking a peer (e.g., peer was intentionally removed)
    pub fn stop_tracking(&mut self, address: &str) {
        if self.peers.remove(address).is_some() {
            log::debug!("Stopped tracking {} for reconnection", address);
        }
    }

    /// Clear all reconnection tracking
    pub fn clear(&mut self) {
        let count = self.peers.len();
        self.peers.clear();
        if count > 0 {
            log::debug!("Cleared reconnection tracking for {} peers", count);
        }
    }

    /// Get the number of peers being tracked
    pub fn tracked_count(&self) -> usize {
        self.peers.len()
    }

    /// Get statistics for a peer
    pub fn get_peer_stats(&self, address: &str) -> Option<PeerReconnectionStats> {
        self.peers.get(address).map(|state| PeerReconnectionStats {
            attempts: state.attempts,
            max_attempts: self.config.max_attempts,
            disconnected_duration: state.disconnected_at.elapsed(),
            next_attempt_delay: if state.attempts >= self.config.max_attempts {
                Duration::MAX // Exhausted
            } else if state.attempts == 0 {
                Duration::ZERO // First attempt is immediate
            } else {
                self.calculate_delay(state.attempts)
            },
        })
    }

    /// Get the check interval from configuration
    pub fn check_interval(&self) -> Duration {
        self.config.check_interval
    }
}

/// Statistics for a peer's reconnection state
#[derive(Debug, Clone)]
pub struct PeerReconnectionStats {
    /// Number of attempts made
    pub attempts: u32,
    /// Maximum allowed attempts
    pub max_attempts: u32,
    /// How long since the peer disconnected
    pub disconnected_duration: Duration,
    /// Computed delay for the next reconnection attempt (ZERO if ready, MAX if exhausted).
    /// This is the configured delay (flat or exponential), not the remaining wait time.
    /// Use `get_status()` for remaining-time semantics.
    pub next_attempt_delay: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exponential_backoff() {
        let config = ReconnectionConfig {
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(60),
            max_attempts: 10,
            check_interval: Duration::from_secs(5),
            use_flat_delay: false,
            reset_on_exhaustion: false,
        };
        let manager = ReconnectionManager::new(config);

        // Check backoff delays
        assert_eq!(manager.calculate_delay(0), Duration::from_secs(2));
        assert_eq!(manager.calculate_delay(1), Duration::from_secs(4));
        assert_eq!(manager.calculate_delay(2), Duration::from_secs(8));
        assert_eq!(manager.calculate_delay(3), Duration::from_secs(16));
        assert_eq!(manager.calculate_delay(4), Duration::from_secs(32));
        assert_eq!(manager.calculate_delay(5), Duration::from_secs(60)); // Capped at max
        assert_eq!(manager.calculate_delay(6), Duration::from_secs(60));
    }

    #[test]
    fn test_track_and_status() {
        let mut manager = ReconnectionManager::new(ReconnectionConfig::fast());

        // Not tracked initially
        assert_eq!(
            manager.get_status("00:11:22:33:44:55"),
            ReconnectionStatus::NotTracked
        );

        // Track a disconnection
        manager.track_disconnection("00:11:22:33:44:55".to_string());
        assert!(manager.is_tracked("00:11:22:33:44:55"));

        // Should be ready immediately
        assert_eq!(
            manager.get_status("00:11:22:33:44:55"),
            ReconnectionStatus::Ready
        );
    }

    #[test]
    fn test_connection_success_clears_tracking() {
        let mut manager = ReconnectionManager::with_defaults();

        manager.track_disconnection("00:11:22:33:44:55".to_string());
        assert!(manager.is_tracked("00:11:22:33:44:55"));

        manager.on_connection_success("00:11:22:33:44:55");
        assert!(!manager.is_tracked("00:11:22:33:44:55"));
        assert_eq!(
            manager.get_status("00:11:22:33:44:55"),
            ReconnectionStatus::NotTracked
        );
        assert_eq!(manager.tracked_count(), 0);
    }

    #[test]
    fn test_max_attempts_exhaustion() {
        let config = ReconnectionConfig {
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            max_attempts: 3,
            check_interval: Duration::from_millis(1),
            use_flat_delay: false,
            reset_on_exhaustion: false,
        };
        let mut manager = ReconnectionManager::new(config);

        manager.track_disconnection("test".to_string());

        // Record 3 attempts
        for _ in 0..3 {
            manager.record_attempt("test");
        }

        // Should be exhausted
        assert_eq!(
            manager.get_status("test"),
            ReconnectionStatus::Exhausted { attempts: 3 }
        );
    }

    // === Kotlin parity tests ===

    #[test]
    fn test_kotlin_normal_config_backoff() {
        // Verify delay sequence matches Kotlin normal mode:
        // base=1000ms, max=15000ms, 20 attempts, exponential backoff
        let config = ReconnectionConfig::kotlin_normal();
        assert_eq!(config.base_delay, Duration::from_millis(1000));
        assert_eq!(config.max_delay, Duration::from_millis(15000));
        assert_eq!(config.max_attempts, 20);
        assert!(!config.use_flat_delay);
        assert!(!config.reset_on_exhaustion);

        let manager = ReconnectionManager::new(config);

        // Kotlin backoff: min(base * 2^attempts, max)
        // attempt 0: 1000 * 1 = 1000ms
        assert_eq!(manager.calculate_delay(0), Duration::from_millis(1000));
        // attempt 1: 1000 * 2 = 2000ms
        assert_eq!(manager.calculate_delay(1), Duration::from_millis(2000));
        // attempt 2: 1000 * 4 = 4000ms
        assert_eq!(manager.calculate_delay(2), Duration::from_millis(4000));
        // attempt 3: 1000 * 8 = 8000ms
        assert_eq!(manager.calculate_delay(3), Duration::from_millis(8000));
        // attempt 4: 1000 * 16 = 15000ms (capped at max)
        assert_eq!(manager.calculate_delay(4), Duration::from_millis(15000));
        // attempt 5: still capped
        assert_eq!(manager.calculate_delay(5), Duration::from_millis(15000));
    }

    #[test]
    fn test_flat_delay_mode() {
        // High-priority mode: flat delay (no exponential backoff)
        let config = ReconnectionConfig::kotlin_high_priority();
        assert!(config.use_flat_delay);

        let manager = ReconnectionManager::new(config);

        // Every attempt should use base_delay = 1000ms
        assert_eq!(manager.calculate_delay(0), Duration::from_millis(1000));
        assert_eq!(manager.calculate_delay(1), Duration::from_millis(1000));
        assert_eq!(manager.calculate_delay(5), Duration::from_millis(1000));
        assert_eq!(manager.calculate_delay(19), Duration::from_millis(1000));
    }

    #[test]
    fn test_reset_on_exhaustion() {
        let config = ReconnectionConfig {
            base_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            max_attempts: 3,
            check_interval: Duration::from_millis(1),
            use_flat_delay: true,
            reset_on_exhaustion: true,
        };
        let mut manager = ReconnectionManager::new(config);

        manager.track_disconnection("test".to_string());

        // Exhaust all 3 attempts
        for _ in 0..3 {
            manager.record_attempt("test");
        }

        // With reset_on_exhaustion, status should show Ready (not Exhausted)
        assert_eq!(manager.get_status("test"), ReconnectionStatus::Ready);

        // get_peers_to_reconnect should reset and return the peer
        std::thread::sleep(Duration::from_millis(5));
        let peers = manager.get_peers_to_reconnect();
        assert!(peers.contains(&"test".to_string()));

        // After reset, attempts should be back to 0
        let stats = manager.get_peer_stats("test").unwrap();
        assert_eq!(stats.attempts, 0);
    }

    #[test]
    fn test_stop_tracking_matches_reset() {
        // Verify stop_tracking() has same effect as on_connection_success():
        // complete removal from tracking
        let mut manager = ReconnectionManager::with_defaults();

        manager.track_disconnection("peer1".to_string());
        manager.track_disconnection("peer2".to_string());
        assert_eq!(manager.tracked_count(), 2);

        manager.stop_tracking("peer1");
        assert!(!manager.is_tracked("peer1"));
        assert_eq!(manager.get_status("peer1"), ReconnectionStatus::NotTracked);
        assert_eq!(manager.tracked_count(), 1);

        // on_connection_success has same effect
        manager.on_connection_success("peer2");
        assert!(!manager.is_tracked("peer2"));
        assert_eq!(manager.get_status("peer2"), ReconnectionStatus::NotTracked);
        assert_eq!(manager.tracked_count(), 0);
    }
}
