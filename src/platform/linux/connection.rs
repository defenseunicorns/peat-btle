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

//! BlueZ connection wrapper
//!
//! Provides a write queue to serialize BLE GATT writes, since BLE only allows
//! one pending write operation per connection at a time.

use bluer::Device;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

use crate::config::BlePhy;
use crate::error::{BleError, Result};
use crate::transport::BleConnection;
use crate::NodeId;

/// A queued write operation
struct QueuedWrite {
    /// Service UUID
    service_uuid: uuid::Uuid,
    /// Characteristic UUID
    char_uuid: uuid::Uuid,
    /// Data to write
    data: Vec<u8>,
    /// Completion notification
    complete_tx: tokio::sync::oneshot::Sender<Result<()>>,
}

/// Internal connection state
struct ConnectionState {
    /// Whether the connection is alive
    alive: bool,
    /// Negotiated MTU
    mtu: u16,
    /// Current PHY
    phy: BlePhy,
    /// Last RSSI reading
    rssi: Option<i8>,
}

/// Write queue state (separate from connection state for finer-grained locking)
struct WriteQueueState {
    /// Queue of pending writes
    queue: VecDeque<QueuedWrite>,
    /// Whether a write is currently in progress
    write_in_progress: bool,
}

/// BlueZ connection wrapper
///
/// Wraps a `bluer::Device` with connection state tracking and write queue.
/// BLE only allows one pending write per connection, so all writes are
/// serialized through the write queue.
#[derive(Clone)]
pub struct BluerConnection {
    /// Remote peer ID
    peer_id: NodeId,
    /// BlueZ device handle
    device: Device,
    /// Connection state
    state: Arc<RwLock<ConnectionState>>,
    /// Write queue state (uses Mutex for write serialization)
    write_queue: Arc<Mutex<WriteQueueState>>,
    /// When the connection was established
    connected_at: Instant,
}

/// Default MTU for BLE 4.2+ devices with data length extension
/// BlueZ typically negotiates 247-517 bytes depending on the remote device
/// We use 185 as a conservative default (matches WearTAK's request)
const DEFAULT_BLE_MTU: u16 = 185;

/// Minimum BLE MTU (ATT_MTU_MIN per Bluetooth spec)
#[allow(dead_code)]
const MIN_BLE_MTU: u16 = 23;

impl BluerConnection {
    /// Create a new connection wrapper
    pub(crate) async fn new(peer_id: NodeId, device: Device) -> Result<Self> {
        // BlueZ negotiates MTU automatically on first ATT operation
        // Use a reasonable default that most modern devices support
        // The actual MTU will be confirmed on the first characteristic access
        let mtu = DEFAULT_BLE_MTU;

        let state = ConnectionState {
            alive: true,
            mtu,
            phy: BlePhy::Le1M, // Default PHY
            rssi: None,
        };

        let write_queue = WriteQueueState {
            queue: VecDeque::new(),
            write_in_progress: false,
        };

        let conn = Self {
            peer_id,
            device,
            state: Arc::new(RwLock::new(state)),
            write_queue: Arc::new(Mutex::new(write_queue)),
            connected_at: Instant::now(),
        };

        // Try to get initial RSSI
        conn.update_rssi().await;

        Ok(conn)
    }

    /// Discover the actual negotiated MTU via a characteristic
    ///
    /// BlueZ negotiates MTU during the first GATT operation.
    /// Call this after connecting to get the actual negotiated value.
    /// Uses AcquireWrite which returns the negotiated MTU.
    pub async fn discover_mtu(
        &self,
        service_uuid: uuid::Uuid,
        char_uuid: uuid::Uuid,
    ) -> Result<u16> {
        let service = self
            .find_service(service_uuid)
            .await?
            .ok_or_else(|| BleError::ServiceNotFound(service_uuid.to_string()))?;

        let characteristics = service
            .characteristics()
            .await
            .map_err(|e| BleError::GattError(format!("Failed to get characteristics: {}", e)))?;

        for char in characteristics {
            if char.uuid().await.ok() == Some(char_uuid) {
                // Try to acquire write IO which returns the negotiated MTU
                match char.write_io().await {
                    Ok(writer) => {
                        let mtu = writer.mtu();
                        self.set_mtu(mtu as u16).await;
                        log::info!("Discovered MTU: {} bytes via {}", mtu, char_uuid);
                        return Ok(mtu as u16);
                    }
                    Err(e) => {
                        log::debug!("Could not acquire write IO for MTU discovery: {}", e);
                        // Fall through to try read/notify
                    }
                }

                // Try notify_io as fallback
                match char.notify_io().await {
                    Ok(reader) => {
                        let mtu = reader.mtu();
                        self.set_mtu(mtu as u16).await;
                        log::info!("Discovered MTU: {} bytes via notify {}", mtu, char_uuid);
                        return Ok(mtu as u16);
                    }
                    Err(e) => {
                        log::debug!("Could not acquire notify IO for MTU discovery: {}", e);
                    }
                }
            }
        }

        // Return current MTU if we couldn't discover it
        Ok(self.mtu())
    }

    /// Get the underlying BlueZ device
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Update RSSI from device
    pub async fn update_rssi(&self) {
        if let Ok(Some(rssi)) = self.device.rssi().await {
            let mut state = self.state.write().await;
            state.rssi = Some(rssi as i8);
        }
    }

    /// Update MTU
    pub async fn set_mtu(&self, mtu: u16) {
        let mut state = self.state.write().await;
        state.mtu = mtu;
    }

    /// Update PHY
    pub async fn set_phy(&self, phy: BlePhy) {
        let mut state = self.state.write().await;
        state.phy = phy;
    }

    /// Mark connection as dead
    pub async fn mark_dead(&self) {
        let mut state = self.state.write().await;
        state.alive = false;
    }

    /// Disconnect from the device
    ///
    /// Clears any pending writes and disconnects the BLE connection.
    pub async fn disconnect(&self) -> Result<()> {
        // Clear any pending writes first
        self.clear_write_queue().await;

        self.device
            .disconnect()
            .await
            .map_err(|e| BleError::ConnectionFailed(format!("Failed to disconnect: {}", e)))?;
        self.mark_dead().await;
        Ok(())
    }

    /// Discover GATT services
    pub async fn discover_services(&self) -> Result<()> {
        // Trigger service discovery
        // In bluer, services are discovered automatically on connect
        // but we can force a refresh
        let _ = self.device.services().await;
        Ok(())
    }

    /// Get GATT services
    pub async fn services(&self) -> Result<Vec<bluer::gatt::remote::Service>> {
        self.device
            .services()
            .await
            .map_err(|e| BleError::GattError(format!("Failed to get services: {}", e)))
    }

    /// Find a service by UUID
    pub async fn find_service(
        &self,
        uuid: uuid::Uuid,
    ) -> Result<Option<bluer::gatt::remote::Service>> {
        let services = self.services().await?;
        for service in services {
            if service.uuid().await.ok() == Some(uuid) {
                return Ok(Some(service));
            }
        }
        Ok(None)
    }

    /// Read a characteristic value
    pub async fn read_characteristic(
        &self,
        service_uuid: uuid::Uuid,
        char_uuid: uuid::Uuid,
    ) -> Result<Vec<u8>> {
        let service = self
            .find_service(service_uuid)
            .await?
            .ok_or_else(|| BleError::ServiceNotFound(service_uuid.to_string()))?;

        let characteristics = service
            .characteristics()
            .await
            .map_err(|e| BleError::GattError(format!("Failed to get characteristics: {}", e)))?;

        for char in characteristics {
            if char.uuid().await.ok() == Some(char_uuid) {
                return char.read().await.map_err(|e| {
                    BleError::GattError(format!("Failed to read characteristic: {}", e))
                });
            }
        }

        Err(BleError::CharacteristicNotFound(char_uuid.to_string()))
    }

    /// Write a characteristic value (direct, non-queued)
    ///
    /// **Warning**: BLE only allows one pending write per connection. Calling this
    /// method concurrently may cause write failures. Use `write_characteristic_queued`
    /// for safe concurrent writes.
    pub async fn write_characteristic(
        &self,
        service_uuid: uuid::Uuid,
        char_uuid: uuid::Uuid,
        value: &[u8],
    ) -> Result<()> {
        let service = self
            .find_service(service_uuid)
            .await?
            .ok_or_else(|| BleError::ServiceNotFound(service_uuid.to_string()))?;

        let characteristics = service
            .characteristics()
            .await
            .map_err(|e| BleError::GattError(format!("Failed to get characteristics: {}", e)))?;

        for char in characteristics {
            if char.uuid().await.ok() == Some(char_uuid) {
                return char.write(value).await.map_err(|e| {
                    BleError::GattError(format!("Failed to write characteristic: {}", e))
                });
            }
        }

        Err(BleError::CharacteristicNotFound(char_uuid.to_string()))
    }

    /// Write a characteristic value with queuing
    ///
    /// BLE only allows one pending write per connection. This method queues writes
    /// and processes them serially, preventing write conflicts. Safe to call
    /// concurrently from multiple tasks.
    ///
    /// Returns when the write completes (or fails).
    pub async fn write_characteristic_queued(
        &self,
        service_uuid: uuid::Uuid,
        char_uuid: uuid::Uuid,
        value: &[u8],
    ) -> Result<()> {
        // Create a oneshot channel for completion notification
        let (tx, rx) = tokio::sync::oneshot::channel();

        // Add to queue
        {
            let mut queue_state = self.write_queue.lock().await;
            queue_state.queue.push_back(QueuedWrite {
                service_uuid,
                char_uuid,
                data: value.to_vec(),
                complete_tx: tx,
            });
            log::debug!(
                "Queued write to {} ({} bytes, queue depth: {})",
                char_uuid,
                value.len(),
                queue_state.queue.len()
            );
        }

        // Try to process the queue (will only proceed if no write in progress)
        self.process_write_queue().await;

        // Wait for completion
        rx.await.map_err(|_| {
            BleError::GattError("Write was cancelled (connection closed?)".to_string())
        })?
    }

    /// Process the write queue
    ///
    /// Processes queued writes one at a time. Only one write can be in progress
    /// per connection (BLE limitation).
    async fn process_write_queue(&self) {
        loop {
            // Get the next write from the queue
            let queued_write = {
                let mut queue_state = self.write_queue.lock().await;

                // If a write is already in progress, exit
                if queue_state.write_in_progress {
                    return;
                }

                // Get next write from queue
                match queue_state.queue.pop_front() {
                    Some(write) => {
                        queue_state.write_in_progress = true;
                        write
                    }
                    None => return, // Queue empty
                }
            };

            // Perform the write (outside the lock)
            let result = self
                .write_characteristic(
                    queued_write.service_uuid,
                    queued_write.char_uuid,
                    &queued_write.data,
                )
                .await;

            // Mark write as complete
            {
                let mut queue_state = self.write_queue.lock().await;
                queue_state.write_in_progress = false;
            }

            // Notify the waiter
            let _ = queued_write.complete_tx.send(result);

            // Continue processing queue (loop will check for more items)
        }
    }

    /// Get the current write queue depth
    ///
    /// Useful for monitoring backpressure. If the queue grows too large,
    /// consider slowing down write requests.
    pub async fn write_queue_depth(&self) -> usize {
        self.write_queue.lock().await.queue.len()
    }

    /// Check if a write is currently in progress
    pub async fn write_in_progress(&self) -> bool {
        self.write_queue.lock().await.write_in_progress
    }

    /// Clear the write queue (e.g., on disconnect)
    ///
    /// All pending writes will receive an error.
    pub async fn clear_write_queue(&self) {
        let mut queue_state = self.write_queue.lock().await;
        let queue_len = queue_state.queue.len();

        // Drain and notify all waiters of cancellation
        while let Some(write) = queue_state.queue.pop_front() {
            let _ = write.complete_tx.send(Err(BleError::GattError(
                "Write queue cleared (disconnected?)".to_string(),
            )));
        }

        if queue_len > 0 {
            log::debug!("Cleared {} pending writes from queue", queue_len);
        }
    }

    /// Subscribe to characteristic notifications
    pub async fn subscribe_characteristic(
        &self,
        service_uuid: uuid::Uuid,
        char_uuid: uuid::Uuid,
    ) -> Result<impl tokio_stream::Stream<Item = Vec<u8>>> {
        let service = self
            .find_service(service_uuid)
            .await?
            .ok_or_else(|| BleError::ServiceNotFound(service_uuid.to_string()))?;

        let characteristics = service
            .characteristics()
            .await
            .map_err(|e| BleError::GattError(format!("Failed to get characteristics: {}", e)))?;

        for char in characteristics {
            if char.uuid().await.ok() == Some(char_uuid) {
                return char.notify().await.map_err(|e| {
                    BleError::GattError(format!("Failed to subscribe to notifications: {}", e))
                });
            }
        }

        Err(BleError::CharacteristicNotFound(char_uuid.to_string()))
    }
}

impl BleConnection for BluerConnection {
    fn peer_id(&self) -> &NodeId {
        &self.peer_id
    }

    fn is_alive(&self) -> bool {
        // Try to read state without blocking
        if let Ok(state) = self.state.try_read() {
            state.alive
        } else {
            // If we can't get the lock, assume alive
            true
        }
    }

    fn mtu(&self) -> u16 {
        if let Ok(state) = self.state.try_read() {
            state.mtu
        } else {
            23 // Default BLE MTU
        }
    }

    fn phy(&self) -> BlePhy {
        if let Ok(state) = self.state.try_read() {
            state.phy
        } else {
            BlePhy::Le1M
        }
    }

    fn rssi(&self) -> Option<i8> {
        if let Ok(state) = self.state.try_read() {
            state.rssi
        } else {
            None
        }
    }

    fn connected_duration(&self) -> Duration {
        self.connected_at.elapsed()
    }
}

#[cfg(test)]
mod tests {
    // Integration tests require actual Bluetooth hardware
    // and a connected device
}
