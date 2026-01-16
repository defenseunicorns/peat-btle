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


package com.revolveteam.hive

import android.util.Log
import androidx.annotation.IntDef
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Native HiveMesh JNI wrapper for centralized peer and document management.
 *
 * This class provides a Kotlin interface to the native Rust HiveMesh implementation,
 * which handles CRDT document synchronization, peer management, and event propagation.
 *
 * Unlike [HiveBtle] which handles BLE operations in pure Kotlin, this class delegates
 * to the native Rust implementation for mesh logic, ensuring consistency across all
 * platforms (iOS, Android, ESP32).
 *
 * ## Usage
 *
 * ```kotlin
 * // Create mesh with configuration
 * val mesh = HiveMesh(
 *     nodeId = 0x12345678,
 *     callsign = "ALPHA-1",
 *     meshId = "DEMO",
 *     peripheralType = PeripheralType.SOLDIER_SENSOR
 * )
 *
 * // Send emergency event
 * val documentBytes = mesh.sendEmergency()
 * // ... broadcast documentBytes via BLE
 *
 * // Process received data
 * val result = mesh.onBleDataReceived(peerAddress, receivedBytes)
 * if (result?.isEmergency == true) {
 *     // Handle emergency from peer
 * }
 *
 * // Periodic maintenance
 * val syncDoc = mesh.tick()
 * if (syncDoc != null) {
 *     // ... send syncDoc to connected peers
 * }
 *
 * // Clean up
 * mesh.destroy()
 * ```
 *
 * @property nodeId This node's unique identifier (32-bit)
 * @property callsign Human-readable identifier for this node
 * @property meshId Mesh network identifier for isolation
 * @property peripheralType Type of peripheral device
 */
class HiveMesh(
    val nodeId: Long,
    val callsign: String = "ANDROID",
    val meshId: String = "DEMO",
    @PeripheralType val peripheralType: Int = PeripheralType.SOLDIER_SENSOR
) : AutoCloseable {

    companion object {
        private const val TAG = "HiveMesh"

        init {
            try {
                System.loadLibrary("hive_btle")
                Log.i(TAG, "Loaded hive_btle native library")
            } catch (e: UnsatisfiedLinkError) {
                Log.e(TAG, "Failed to load hive_btle native library", e)
            }
        }

        // Native methods
        @JvmStatic
        private external fun nativeCreate(
            nodeId: Long,
            callsign: String,
            meshId: String,
            peripheralType: Int
        ): Long

        @JvmStatic
        private external fun nativeDestroy(handle: Long)

        @JvmStatic
        private external fun nativeGetDeviceName(handle: Long): String

        @JvmStatic
        private external fun nativeSendEmergency(handle: Long, timestamp: Long): ByteArray

        @JvmStatic
        private external fun nativeSendAck(handle: Long, timestamp: Long): ByteArray

        @JvmStatic
        private external fun nativeBuildDocument(handle: Long): ByteArray

        @JvmStatic
        private external fun nativeOnBleDiscovered(
            handle: Long,
            identifier: String,
            name: String?,
            rssi: Int,
            meshId: String?,
            nowMs: Long
        ): Boolean

        @JvmStatic
        private external fun nativeOnBleConnected(
            handle: Long,
            identifier: String,
            nowMs: Long
        ): Long

        @JvmStatic
        private external fun nativeOnBleDisconnected(
            handle: Long,
            identifier: String,
            reason: Int
        ): Long

        @JvmStatic
        private external fun nativeOnBleDataReceived(
            handle: Long,
            identifier: String,
            data: ByteArray,
            nowMs: Long
        ): ByteArray

        @JvmStatic
        private external fun nativeTick(handle: Long, nowMs: Long): ByteArray

        @JvmStatic
        private external fun nativeGetPeerCount(handle: Long): Int

        @JvmStatic
        private external fun nativeGetConnectedCount(handle: Long): Int

        @JvmStatic
        private external fun nativeGetTotalCount(handle: Long): Long

        @JvmStatic
        private external fun nativeIsEmergencyActive(handle: Long): Boolean

        @JvmStatic
        private external fun nativeIsAckActive(handle: Long): Boolean

        @JvmStatic
        private external fun nativeMatchesMesh(handle: Long, deviceMeshId: String?): Boolean

        // Connection State Graph methods
        @JvmStatic
        private external fun nativeGetConnectionStateCounts(handle: Long): ByteArray

        @JvmStatic
        private external fun nativeGetAllPeerStates(handle: Long): ByteArray

        @JvmStatic
        private external fun nativeGetPeerConnectionState(handle: Long, nodeId: Long): ByteArray

        @JvmStatic
        private external fun nativeGetConnectedPeers(handle: Long): ByteArray

        @JvmStatic
        private external fun nativeGetDegradedPeers(handle: Long): ByteArray

        @JvmStatic
        private external fun nativeGetRecentlyDisconnected(handle: Long, withinMs: Long, nowMs: Long): ByteArray

        @JvmStatic
        private external fun nativeGetLostPeers(handle: Long): ByteArray

        // Delta Sync methods
        @JvmStatic
        private external fun nativeRegisterPeerForDelta(handle: Long, peerNodeId: Long)

        @JvmStatic
        private external fun nativeUnregisterPeerForDelta(handle: Long, peerNodeId: Long)

        @JvmStatic
        private external fun nativeResetPeerDeltaState(handle: Long, peerNodeId: Long)

        @JvmStatic
        private external fun nativeBuildDeltaDocumentForPeer(handle: Long, peerNodeId: Long, nowMs: Long): ByteArray

        @JvmStatic
        private external fun nativeBuildFullDeltaDocument(handle: Long, nowMs: Long): ByteArray

        @JvmStatic
        private external fun nativeGetDeltaStats(handle: Long): ByteArray

        @JvmStatic
        private external fun nativeGetPeerDeltaStats(handle: Long, peerNodeId: Long): ByteArray

        // Indirect Peers methods
        @JvmStatic
        private external fun nativeGetIndirectPeers(handle: Long): ByteArray

        @JvmStatic
        private external fun nativeGetPeerDegree(handle: Long, nodeId: Long): Int

        @JvmStatic
        private external fun nativeGetFullStateCounts(handle: Long): ByteArray

        @JvmStatic
        private external fun nativeGetIndirectPeerCount(handle: Long): Int

        @JvmStatic
        private external fun nativeIsPeerKnown(handle: Long, nodeId: Long): Boolean

        // Chat CRDT methods
        @JvmStatic
        private external fun nativeSendChat(handle: Long, sender: String, text: String): ByteArray

        @JvmStatic
        private external fun nativeSendChatReply(
            handle: Long,
            sender: String,
            text: String,
            replyToNode: Long,
            replyToTimestamp: Long
        ): ByteArray

        @JvmStatic
        private external fun nativeChatCount(handle: Long): Int

        @JvmStatic
        private external fun nativeGetAllChatMessages(handle: Long): String

        @JvmStatic
        private external fun nativeGetChatMessagesSince(handle: Long, sinceTimestamp: Long): String
    }

    /** Native handle returned by nativeCreate */
    private var handle: Long = 0

    /** Whether this mesh instance has been destroyed */
    private var isDestroyed = false

    init {
        handle = nativeCreate(nodeId, callsign, meshId, peripheralType)
        if (handle == 0L) {
            throw IllegalStateException("Failed to create native HiveMesh")
        }
        Log.i(TAG, "Created HiveMesh: nodeId=${String.format("%08X", nodeId)}, mesh=$meshId")
    }

    /**
     * Destroy the native HiveMesh instance and release resources.
     *
     * After calling this method, all other methods will throw [IllegalStateException].
     */
    fun destroy() {
        if (!isDestroyed) {
            nativeDestroy(handle)
            isDestroyed = true
            handle = 0
            Log.i(TAG, "Destroyed HiveMesh")
        }
    }

    /**
     * AutoCloseable implementation - calls [destroy].
     */
    override fun close() = destroy()

    /**
     * Get the BLE device name for advertising.
     *
     * Format: `HIVE_<MESH_ID>-<NODE_ID>` (e.g., "HIVE_DEMO-12345678")
     *
     * @return Device name string for BLE advertising
     */
    fun getDeviceName(): String {
        checkNotDestroyed()
        return nativeGetDeviceName(handle)
    }

    /**
     * Send an emergency event.
     *
     * Creates a CRDT document with the Emergency event type set. The returned
     * bytes should be broadcast to all connected peers via BLE.
     *
     * @param timestamp Event timestamp in milliseconds (defaults to current time)
     * @return Encoded document bytes to send to peers
     */
    fun sendEmergency(timestamp: Long = System.currentTimeMillis()): ByteArray {
        checkNotDestroyed()
        return nativeSendEmergency(handle, timestamp)
    }

    /**
     * Send an acknowledgment event.
     *
     * Creates a CRDT document with the ACK event type set. Used to acknowledge
     * receipt of emergency or other important events.
     *
     * @param timestamp Event timestamp in milliseconds (defaults to current time)
     * @return Encoded document bytes to send to peers
     */
    fun sendAck(timestamp: Long = System.currentTimeMillis()): ByteArray {
        checkNotDestroyed()
        return nativeSendAck(handle, timestamp)
    }

    /**
     * Build the current document state for synchronization.
     *
     * Creates a CRDT document representing the current state without triggering
     * any new events. Used for periodic sync broadcasts.
     *
     * @return Encoded document bytes
     */
    fun buildDocument(): ByteArray {
        checkNotDestroyed()
        return nativeBuildDocument(handle)
    }

    // ==================== Chat CRDT Methods ====================

    /**
     * Send a chat message via CRDT.
     *
     * Adds the message to the local chat CRDT and returns the document bytes
     * to broadcast. The message is automatically deduplicated across the mesh.
     *
     * @param sender The sender's callsign (max 12 chars)
     * @param text The message text (max 128 chars)
     * @return Encoded document bytes to send to peers, or empty array if duplicate
     */
    fun sendChat(sender: String, text: String): ByteArray {
        checkNotDestroyed()
        return nativeSendChat(handle, sender, text)
    }

    /**
     * Send a chat reply via CRDT.
     *
     * Adds a reply message to the local chat CRDT with reference to the original message.
     *
     * @param sender The sender's callsign (max 12 chars)
     * @param text The message text (max 128 chars)
     * @param replyToNode Origin node of the message being replied to
     * @param replyToTimestamp Timestamp of the message being replied to
     * @return Encoded document bytes to send to peers, or empty array if duplicate
     */
    fun sendChatReply(
        sender: String,
        text: String,
        replyToNode: Long,
        replyToTimestamp: Long
    ): ByteArray {
        checkNotDestroyed()
        return nativeSendChatReply(handle, sender, text, replyToNode, replyToTimestamp)
    }

    /**
     * Get the number of chat messages in the local CRDT.
     *
     * @return Number of messages stored locally
     */
    fun chatCount(): Int {
        checkNotDestroyed()
        return nativeChatCount(handle)
    }

    /**
     * Get all chat messages from the local CRDT.
     *
     * Returns a JSON array string of chat message objects with fields:
     * - originNode: Long - sender's node ID
     * - timestamp: Long - message timestamp
     * - sender: String - sender's callsign
     * - text: String - message text
     *
     * @return JSON array string of chat messages
     */
    fun getAllChatMessages(): String {
        checkNotDestroyed()
        return nativeGetAllChatMessages(handle)
    }

    /**
     * Get chat messages since a given timestamp.
     *
     * Returns only messages newer than the specified timestamp.
     *
     * @param sinceTimestamp Only return messages with timestamp > this value
     * @return JSON array string of chat messages
     */
    fun getChatMessagesSince(sinceTimestamp: Long): String {
        checkNotDestroyed()
        return nativeGetChatMessagesSince(handle, sinceTimestamp)
    }

    /**
     * Handle BLE device discovery.
     *
     * Called when a HIVE device is discovered during BLE scanning. The native
     * code will parse the device name/advertisement to determine if it should
     * be tracked as a peer.
     *
     * @param identifier BLE device identifier (MAC address on Android)
     * @param name Device name from advertisement (may be null)
     * @param rssi Signal strength in dBm
     * @param deviceMeshId Mesh ID parsed from device name (null for legacy devices)
     * @param nowMs Current timestamp in milliseconds
     * @return true if this device is tracked as a HIVE peer
     */
    fun onBleDiscovered(
        identifier: String,
        name: String?,
        rssi: Int,
        deviceMeshId: String?,
        nowMs: Long = System.currentTimeMillis()
    ): Boolean {
        checkNotDestroyed()
        return nativeOnBleDiscovered(handle, identifier, name, rssi, deviceMeshId, nowMs)
    }

    /**
     * Handle BLE connection established.
     *
     * Called when a GATT connection is successfully established with a peer.
     *
     * @param identifier BLE device identifier
     * @param nowMs Current timestamp in milliseconds
     * @return The peer's node ID, or 0 if not a known peer
     */
    fun onBleConnected(
        identifier: String,
        nowMs: Long = System.currentTimeMillis()
    ): Long {
        checkNotDestroyed()
        return nativeOnBleConnected(handle, identifier, nowMs)
    }

    /**
     * Handle BLE disconnection.
     *
     * Called when a GATT connection is lost with a peer.
     *
     * @param identifier BLE device identifier
     * @param reason Disconnection reason (see [DisconnectReason])
     * @return The peer's node ID, or 0 if not a known peer
     */
    fun onBleDisconnected(
        identifier: String,
        @DisconnectReason reason: Int = DisconnectReason.UNKNOWN
    ): Long {
        checkNotDestroyed()
        return nativeOnBleDisconnected(handle, identifier, reason)
    }

    /**
     * Handle received BLE data.
     *
     * Called when CRDT document data is received from a peer via GATT
     * characteristic read or notification.
     *
     * @param identifier BLE device identifier
     * @param data Raw document bytes received
     * @param nowMs Current timestamp in milliseconds
     * @return Parsed result with event flags, or null if parsing failed
     */
    fun onBleDataReceived(
        identifier: String,
        data: ByteArray,
        nowMs: Long = System.currentTimeMillis()
    ): DataReceivedResult? {
        checkNotDestroyed()
        val resultBytes = nativeOnBleDataReceived(handle, identifier, data, nowMs)
        return DataReceivedResult.decode(resultBytes)
    }

    /**
     * Perform periodic maintenance.
     *
     * Should be called periodically (e.g., every 3-5 seconds) to:
     * - Clean up stale peers
     * - Build sync documents if needed
     * - Clear expired events
     *
     * @param nowMs Current timestamp in milliseconds
     * @return Document bytes to broadcast if sync is needed, or empty array
     */
    fun tick(nowMs: Long = System.currentTimeMillis()): ByteArray {
        checkNotDestroyed()
        return nativeTick(handle, nowMs)
    }

    /**
     * Get the total number of known peers (connected + disconnected).
     */
    fun getPeerCount(): Int {
        checkNotDestroyed()
        return nativeGetPeerCount(handle)
    }

    /**
     * Get the number of currently connected peers.
     */
    fun getConnectedCount(): Int {
        checkNotDestroyed()
        return nativeGetConnectedCount(handle)
    }

    /**
     * Get the total CRDT counter value across all nodes.
     */
    fun getTotalCount(): Long {
        checkNotDestroyed()
        return nativeGetTotalCount(handle)
    }

    /**
     * Check if any node in the mesh has an active emergency.
     */
    fun isEmergencyActive(): Boolean {
        checkNotDestroyed()
        return nativeIsEmergencyActive(handle)
    }

    /**
     * Check if any node in the mesh has acknowledged the emergency.
     */
    fun isAckActive(): Boolean {
        checkNotDestroyed()
        return nativeIsAckActive(handle)
    }

    /**
     * Check if a device's mesh ID matches this mesh.
     *
     * @param deviceMeshId The device's mesh ID (null for legacy devices)
     * @return true if the device belongs to this mesh
     */
    fun matchesMesh(deviceMeshId: String?): Boolean {
        checkNotDestroyed()
        return nativeMatchesMesh(handle, deviceMeshId)
    }

    // ========================================================================
    // Connection State Graph API
    // ========================================================================

    /**
     * Get summary counts of peers in each connection state.
     *
     * Useful for displaying badge counts or summary statistics in UI.
     *
     * @return StateCountSummary with counts per state, or null on error
     */
    fun getConnectionStateCounts(): StateCountSummary? {
        checkNotDestroyed()
        val data = nativeGetConnectionStateCounts(handle)
        return StateCountSummary.decode(data)
    }

    /**
     * Get all tracked peers with their connection states.
     *
     * @return List of all peer connection states
     */
    fun getAllPeerStates(): List<PeerConnectionState> {
        checkNotDestroyed()
        val data = nativeGetAllPeerStates(handle)
        return PeerConnectionState.decodeList(data)
    }

    /**
     * Get connection state for a specific peer.
     *
     * @param nodeId The peer's node ID
     * @return PeerConnectionState if found, null otherwise
     */
    fun getPeerConnectionState(nodeId: Long): PeerConnectionState? {
        checkNotDestroyed()
        val data = nativeGetPeerConnectionState(handle, nodeId)
        if (data.isEmpty()) return null
        return PeerConnectionState.decode(data)
    }

    /**
     * Get all currently connected peers (Connected or Degraded state).
     *
     * @return List of connected peer states
     */
    fun getConnectedPeers(): List<PeerConnectionState> {
        checkNotDestroyed()
        val data = nativeGetConnectedPeers(handle)
        return PeerConnectionState.decodeList(data)
    }

    /**
     * Get all peers in Degraded state (connected but with poor signal).
     *
     * @return List of degraded peer states
     */
    fun getDegradedPeers(): List<PeerConnectionState> {
        checkNotDestroyed()
        val data = nativeGetDegradedPeers(handle)
        return PeerConnectionState.decodeList(data)
    }

    /**
     * Get peers that disconnected within the specified time window.
     *
     * Useful for showing "stale" peers that might still have relevant data.
     *
     * @param withinMs Time window in milliseconds (e.g., 30000 for last 30 seconds)
     * @param nowMs Current timestamp (defaults to System.currentTimeMillis())
     * @return List of recently disconnected peer states
     */
    fun getRecentlyDisconnected(
        withinMs: Long,
        nowMs: Long = System.currentTimeMillis()
    ): List<PeerConnectionState> {
        checkNotDestroyed()
        val data = nativeGetRecentlyDisconnected(handle, withinMs, nowMs)
        return PeerConnectionState.decodeList(data)
    }

    /**
     * Get all peers in Lost state (disconnected and not seen in advertisements).
     *
     * @return List of lost peer states
     */
    fun getLostPeers(): List<PeerConnectionState> {
        checkNotDestroyed()
        val data = nativeGetLostPeers(handle)
        return PeerConnectionState.decodeList(data)
    }

    // ========================================================================
    // Delta Sync API
    // ========================================================================

    /**
     * Register a peer for delta sync tracking.
     *
     * Call this when a peer connects to enable bandwidth-efficient delta sync.
     * Once registered, [buildDeltaDocumentForPeer] will track what has been
     * sent and only return new operations.
     *
     * @param peerNodeId The peer's node ID (32-bit)
     */
    fun registerPeerForDelta(peerNodeId: Long) {
        checkNotDestroyed()
        nativeRegisterPeerForDelta(handle, peerNodeId)
    }

    /**
     * Unregister a peer from delta sync tracking.
     *
     * Call this when a peer disconnects to clean up tracking state and
     * free memory.
     *
     * @param peerNodeId The peer's node ID (32-bit)
     */
    fun unregisterPeerForDelta(peerNodeId: Long) {
        checkNotDestroyed()
        nativeUnregisterPeerForDelta(handle, peerNodeId)
    }

    /**
     * Reset delta sync state for a peer.
     *
     * Call this when a peer reconnects to force a full sync on the next
     * call to [buildDeltaDocumentForPeer]. This ensures the peer receives
     * complete state after reconnection.
     *
     * @param peerNodeId The peer's node ID (32-bit)
     */
    fun resetPeerDeltaState(peerNodeId: Long) {
        checkNotDestroyed()
        nativeResetPeerDeltaState(handle, peerNodeId)
    }

    /**
     * Build a delta document for a specific peer.
     *
     * Returns only operations that have changed since the last sync with
     * this peer. This significantly reduces bandwidth usage compared to
     * [buildDocument] or [buildFullDeltaDocument].
     *
     * **Usage pattern:**
     * ```kotlin
     * // When peer connects
     * mesh.registerPeerForDelta(peerNodeId)
     *
     * // Periodic sync - only sends changes
     * val delta = mesh.buildDeltaDocumentForPeer(peerNodeId)
     * if (delta != null) {
     *     sendToPeer(delta)
     * }
     *
     * // When peer disconnects
     * mesh.unregisterPeerForDelta(peerNodeId)
     * ```
     *
     * @param peerNodeId The peer's node ID (32-bit)
     * @param nowMs Current timestamp (defaults to System.currentTimeMillis())
     * @return Encoded delta document bytes, or null if nothing new to send
     */
    fun buildDeltaDocumentForPeer(
        peerNodeId: Long,
        nowMs: Long = System.currentTimeMillis()
    ): ByteArray? {
        checkNotDestroyed()
        val result = nativeBuildDeltaDocumentForPeer(handle, peerNodeId, nowMs)
        return if (result.isEmpty()) null else result
    }

    /**
     * Build a full delta document for broadcast.
     *
     * Returns complete state in the new wire format v2. Use this for:
     * - Broadcasting to all peers (not tracking individual states)
     * - Sending to newly discovered peers
     * - Initial sync before registering for delta tracking
     *
     * @param nowMs Current timestamp (defaults to System.currentTimeMillis())
     * @return Encoded delta document bytes with complete state
     */
    fun buildFullDeltaDocument(nowMs: Long = System.currentTimeMillis()): ByteArray {
        checkNotDestroyed()
        return nativeBuildFullDeltaDocument(handle, nowMs)
    }

    /**
     * Get aggregate delta sync statistics.
     *
     * Returns overall statistics across all tracked peers.
     *
     * @return DeltaStats with aggregate metrics, or null on error
     */
    fun getDeltaStats(): DeltaStats? {
        checkNotDestroyed()
        val data = nativeGetDeltaStats(handle)
        return DeltaStats.decode(data)
    }

    /**
     * Get delta sync statistics for a specific peer.
     *
     * @param peerNodeId The peer's node ID (32-bit)
     * @return PeerDeltaStats if peer is registered, null otherwise
     */
    fun getPeerDeltaStats(peerNodeId: Long): PeerDeltaStats? {
        checkNotDestroyed()
        val data = nativeGetPeerDeltaStats(handle, peerNodeId)
        return PeerDeltaStats.decode(data)
    }

    // ========================================================================
    // Indirect Peers API (Multi-Hop Mesh Topology)
    // ========================================================================

    /**
     * Get all indirect peers (peers reachable via relay hops).
     *
     * Indirect peers are discovered through relay messages - when we receive
     * a message from peer A with origin B, we know B is reachable via A.
     * This method returns all such indirectly reachable peers.
     *
     * @return List of indirect peers with routing information
     */
    fun getIndirectPeers(): List<IndirectPeer> {
        checkNotDestroyed()
        val data = nativeGetIndirectPeers(handle)
        return IndirectPeer.decodeList(data)
    }

    /**
     * Get the degree (hop count) for a specific peer.
     *
     * - 0 = Direct peer (BLE connection)
     * - 1 = One-hop peer (reachable via one relay)
     * - 2 = Two-hop peer (reachable via two relays)
     * - 3 = Three-hop peer (reachable via three relays)
     *
     * @param nodeId The peer's node ID (32-bit)
     * @return Hop count (0-3), or null if peer is unknown
     */
    fun getPeerDegree(nodeId: Long): Int? {
        checkNotDestroyed()
        val result = nativeGetPeerDegree(handle, nodeId)
        return if (result < 0) null else result
    }

    /**
     * Get full peer counts including both direct and indirect peers.
     *
     * This provides a complete view of the mesh topology as seen from this node,
     * including counts of peers at each hop distance.
     *
     * @return FullStateCounts with direct and indirect peer counts
     */
    fun getFullStateCounts(): FullStateCounts? {
        checkNotDestroyed()
        val data = nativeGetFullStateCounts(handle)
        return FullStateCounts.decode(data)
    }

    /**
     * Get the count of indirect peers (multi-hop peers).
     *
     * @return Number of peers reachable via relay hops
     */
    fun getIndirectPeerCount(): Int {
        checkNotDestroyed()
        return nativeGetIndirectPeerCount(handle)
    }

    /**
     * Check if a peer is known (either direct or indirect).
     *
     * @param nodeId The peer's node ID (32-bit)
     * @return true if the peer is tracked (direct or indirect)
     */
    fun isPeerKnown(nodeId: Long): Boolean {
        checkNotDestroyed()
        return nativeIsPeerKnown(handle, nodeId)
    }

    private fun checkNotDestroyed() {
        if (isDestroyed) {
            throw IllegalStateException("HiveMesh has been destroyed")
        }
    }
}

/**
 * Result from processing received BLE data.
 *
 * Contains information about the source node and any events detected
 * in the received document.
 */
data class DataReceivedResult(
    /** Source node ID that sent the document */
    val sourceNode: Long,
    /** Whether an emergency event was detected */
    val isEmergency: Boolean,
    /** Whether an ACK event was detected */
    val isAck: Boolean,
    /** Whether the CRDT counter changed (new data) */
    val counterChanged: Boolean,
    /** Total counter value after merge */
    val totalCount: Long
) {
    companion object {
        /**
         * Decode result from native byte format.
         *
         * Format: [source_node: 4][is_emergency: 1][is_ack: 1][counter_changed: 1][total_count: 8]
         */
        fun decode(data: ByteArray): DataReceivedResult? {
            if (data.size < 15) return null

            val buffer = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN)
            val sourceNode = buffer.int.toLong() and 0xFFFFFFFFL
            val isEmergency = buffer.get() != 0.toByte()
            val isAck = buffer.get() != 0.toByte()
            val counterChanged = buffer.get() != 0.toByte()
            val totalCount = buffer.long

            return DataReceivedResult(
                sourceNode = sourceNode,
                isEmergency = isEmergency,
                isAck = isAck,
                counterChanged = counterChanged,
                totalCount = totalCount
            )
        }
    }
}

/**
 * Peripheral type classification.
 *
 * Matches the Rust PeripheralType enum values.
 */
@Retention(AnnotationRetention.SOURCE)
@IntDef(
    PeripheralType.UNKNOWN,
    PeripheralType.SOLDIER_SENSOR,
    PeripheralType.FIXED_SENSOR,
    PeripheralType.RELAY
)
annotation class PeripheralType {
    companion object {
        const val UNKNOWN = 0
        const val SOLDIER_SENSOR = 1
        const val FIXED_SENSOR = 2
        const val RELAY = 3
    }
}

/**
 * BLE disconnection reason codes.
 *
 * Matches the Rust DisconnectReason enum values.
 */
@Retention(AnnotationRetention.SOURCE)
@IntDef(
    DisconnectReason.LOCAL_REQUEST,
    DisconnectReason.REMOTE_REQUEST,
    DisconnectReason.TIMEOUT,
    DisconnectReason.LINK_LOSS,
    DisconnectReason.CONNECTION_FAILED,
    DisconnectReason.UNKNOWN
)
annotation class DisconnectReason {
    companion object {
        /** Disconnection initiated by local device */
        const val LOCAL_REQUEST = 0
        /** Disconnection initiated by remote device */
        const val REMOTE_REQUEST = 1
        /** Connection timed out */
        const val TIMEOUT = 2
        /** Link supervision timeout */
        const val LINK_LOSS = 3
        /** Initial connection failed */
        const val CONNECTION_FAILED = 4
        /** Unknown reason */
        const val UNKNOWN = 5
    }
}

/**
 * Connection state aligned with hive-protocol abstractions.
 *
 * Represents the lifecycle states of a peer connection, from initial
 * discovery through connection, degradation, and disconnection.
 */
@Retention(AnnotationRetention.SOURCE)
@IntDef(
    ConnectionState.DISCOVERED,
    ConnectionState.CONNECTING,
    ConnectionState.CONNECTED,
    ConnectionState.DEGRADED,
    ConnectionState.DISCONNECTING,
    ConnectionState.DISCONNECTED,
    ConnectionState.LOST
)
annotation class ConnectionState {
    companion object {
        /** Peer has been seen via BLE advertisement but never connected */
        const val DISCOVERED = 0
        /** BLE connection is being established */
        const val CONNECTING = 1
        /** Active BLE connection with healthy signal */
        const val CONNECTED = 2
        /** Connected but with degraded quality (low RSSI) */
        const val DEGRADED = 3
        /** Graceful disconnect in progress */
        const val DISCONNECTING = 4
        /** Was previously connected, now disconnected */
        const val DISCONNECTED = 5
        /** Disconnected and no longer seen in advertisements */
        const val LOST = 6

        /** Returns true if this state represents an active connection */
        fun isConnected(state: Int): Boolean = state == CONNECTED || state == DEGRADED

        /** Returns true if this state indicates the peer was previously known */
        fun wasConnected(state: Int): Boolean = state in listOf(
            CONNECTED, DEGRADED, DISCONNECTING, DISCONNECTED, LOST
        )
    }
}

/**
 * Per-peer connection state with history.
 *
 * Provides a comprehensive view of a peer's connection lifecycle,
 * including timestamps, statistics, and associated data metrics.
 * This enables apps to display appropriate UI indicators and track
 * data provenance.
 */
data class PeerConnectionState(
    /** HIVE node identifier (32-bit) */
    val nodeId: Long,
    /** Platform-specific BLE identifier (MAC address on Android) */
    val identifier: String,
    /** Current connection state */
    @ConnectionState val state: Int,
    /** Timestamp when peer was first discovered (ms since epoch) */
    val discoveredAt: Long,
    /** Timestamp of most recent connection (ms since epoch), or null if never connected */
    val connectedAt: Long?,
    /** Timestamp of most recent disconnection (ms since epoch), or null if never disconnected */
    val disconnectedAt: Long?,
    /** Reason for most recent disconnection */
    @DisconnectReason val disconnectReason: Int?,
    /** Most recent RSSI reading (dBm) */
    val lastRssi: Int?,
    /** Total number of successful connections to this peer */
    val connectionCount: Int,
    /** Number of documents synced with this peer */
    val documentsSynced: Int,
    /** Bytes received from this peer */
    val bytesReceived: Long,
    /** Bytes sent to this peer */
    val bytesSent: Long,
    /** Last time peer was seen (advertisement or data, ms since epoch) */
    val lastSeenMs: Long,
    /** Optional device name */
    val name: String?,
    /** Mesh ID this peer belongs to */
    val meshId: String?
) {
    /** Returns true if peer is currently connected (Connected or Degraded state) */
    fun isConnected(): Boolean = ConnectionState.isConnected(state)

    /** Get time since last connection in milliseconds */
    fun timeSinceConnected(nowMs: Long = System.currentTimeMillis()): Long? =
        connectedAt?.let { nowMs - it }

    /** Get time since disconnection in milliseconds */
    fun timeSinceDisconnected(nowMs: Long = System.currentTimeMillis()): Long? =
        disconnectedAt?.let { nowMs - it }

    /** Get connection duration if currently connected */
    fun connectionDuration(nowMs: Long = System.currentTimeMillis()): Long? =
        if (isConnected()) connectedAt?.let { nowMs - it } else null

    companion object {
        /**
         * Decode peer connection state from native byte format.
         *
         * Format: [node_id: 4][state: 1][discovered_at: 8][connected_at: 8][disconnected_at: 8]
         *         [disconnect_reason: 1][last_rssi: 1][connection_count: 4][documents_synced: 4]
         *         [bytes_received: 8][bytes_sent: 8][last_seen_ms: 8]
         *         [identifier_len: 2][identifier: N][name_len: 2][name: N][mesh_id_len: 2][mesh_id: N]
         */
        fun decode(data: ByteArray): PeerConnectionState? {
            if (data.size < 65) return null // Minimum size without strings

            val buffer = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN)

            val nodeId = buffer.int.toLong() and 0xFFFFFFFFL
            val state = buffer.get().toInt() and 0xFF
            val discoveredAt = buffer.long
            val connectedAtRaw = buffer.long
            val connectedAt = if (connectedAtRaw == 0L) null else connectedAtRaw
            val disconnectedAtRaw = buffer.long
            val disconnectedAt = if (disconnectedAtRaw == 0L) null else disconnectedAtRaw
            val disconnectReasonRaw = buffer.get().toInt() and 0xFF
            val disconnectReason = if (disconnectReasonRaw == 0xFF) null else disconnectReasonRaw
            val lastRssiRaw = buffer.get().toInt()
            val lastRssi = if (lastRssiRaw == -128) null else lastRssiRaw
            val connectionCount = buffer.int
            val documentsSynced = buffer.int
            val bytesReceived = buffer.long
            val bytesSent = buffer.long
            val lastSeenMs = buffer.long

            // Read strings
            val identifierLen = buffer.short.toInt() and 0xFFFF
            val identifierBytes = ByteArray(identifierLen)
            buffer.get(identifierBytes)
            val identifier = String(identifierBytes, Charsets.UTF_8)

            val nameLen = buffer.short.toInt() and 0xFFFF
            val name = if (nameLen > 0) {
                val nameBytes = ByteArray(nameLen)
                buffer.get(nameBytes)
                String(nameBytes, Charsets.UTF_8)
            } else null

            val meshIdLen = buffer.short.toInt() and 0xFFFF
            val meshId = if (meshIdLen > 0) {
                val meshIdBytes = ByteArray(meshIdLen)
                buffer.get(meshIdBytes)
                String(meshIdBytes, Charsets.UTF_8)
            } else null

            return PeerConnectionState(
                nodeId = nodeId,
                identifier = identifier,
                state = state,
                discoveredAt = discoveredAt,
                connectedAt = connectedAt,
                disconnectedAt = disconnectedAt,
                disconnectReason = disconnectReason,
                lastRssi = lastRssi,
                connectionCount = connectionCount,
                documentsSynced = documentsSynced,
                bytesReceived = bytesReceived,
                bytesSent = bytesSent,
                lastSeenMs = lastSeenMs,
                name = name,
                meshId = meshId
            )
        }

        /**
         * Decode a list of peer connection states from native byte format.
         *
         * Format: [count: 4][peer1_len: 4][peer1: N][peer2_len: 4][peer2: N]...
         */
        fun decodeList(data: ByteArray): List<PeerConnectionState> {
            if (data.size < 4) return emptyList()

            val buffer = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN)
            val count = buffer.int

            val result = mutableListOf<PeerConnectionState>()
            repeat(count) {
                if (buffer.remaining() < 4) return result
                val peerLen = buffer.int
                if (buffer.remaining() < peerLen) return result

                val peerBytes = ByteArray(peerLen)
                buffer.get(peerBytes)
                decode(peerBytes)?.let { result.add(it) }
            }
            return result
        }
    }
}

/**
 * Summary of peer counts by connection state.
 *
 * Useful for displaying badge counts or summary statistics in UI.
 */
data class StateCountSummary(
    /** Peers discovered but never connected */
    val discovered: Int,
    /** Peers currently connecting */
    val connecting: Int,
    /** Peers with healthy connection */
    val connected: Int,
    /** Peers connected but with degraded signal */
    val degraded: Int,
    /** Peers currently disconnecting */
    val disconnecting: Int,
    /** Peers recently disconnected */
    val disconnected: Int,
    /** Peers disconnected and not seen in advertisements */
    val lost: Int
) {
    /** Total number of peers actively connected */
    val activeConnections: Int get() = connected + degraded

    /** Total number of tracked peers */
    val total: Int get() = discovered + connecting + connected + degraded + disconnecting + disconnected + lost

    companion object {
        /**
         * Decode from native byte format.
         *
         * Format: [discovered: 4][connecting: 4][connected: 4][degraded: 4]
         *         [disconnecting: 4][disconnected: 4][lost: 4]
         */
        fun decode(data: ByteArray): StateCountSummary? {
            if (data.size < 28) return null

            val buffer = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN)
            return StateCountSummary(
                discovered = buffer.int,
                connecting = buffer.int,
                connected = buffer.int,
                degraded = buffer.int,
                disconnecting = buffer.int,
                disconnected = buffer.int,
                lost = buffer.int
            )
        }
    }
}

/**
 * Aggregate delta sync statistics across all tracked peers.
 *
 * Provides metrics for monitoring delta sync efficiency and bandwidth savings.
 */
data class DeltaStats(
    /** Number of peers currently registered for delta sync */
    val peerCount: Int,
    /** Total bytes sent via delta sync */
    val totalBytesSent: Long,
    /** Total bytes received via delta sync */
    val totalBytesReceived: Long,
    /** Total number of sync operations performed */
    val totalSyncs: Int
) {
    companion object {
        /**
         * Decode from native byte format.
         *
         * Format: [peer_count: 4][total_bytes_sent: 8][total_bytes_received: 8][total_syncs: 4]
         */
        fun decode(data: ByteArray): DeltaStats? {
            if (data.size < 24) return null

            val buffer = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN)
            return DeltaStats(
                peerCount = buffer.int,
                totalBytesSent = buffer.long,
                totalBytesReceived = buffer.long,
                totalSyncs = buffer.int
            )
        }
    }
}

/**
 * Delta sync statistics for a specific peer.
 *
 * Tracks bandwidth usage and sync frequency for individual peer connections.
 */
data class PeerDeltaStats(
    /** Bytes sent to this peer via delta sync */
    val bytesSent: Long,
    /** Bytes received from this peer via delta sync */
    val bytesReceived: Long,
    /** Number of sync operations with this peer */
    val syncCount: Int
) {
    companion object {
        /**
         * Decode from native byte format.
         *
         * Format: [bytes_sent: 8][bytes_received: 8][sync_count: 4]
         */
        fun decode(data: ByteArray): PeerDeltaStats? {
            if (data.size < 20) return null

            val buffer = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN)
            return PeerDeltaStats(
                bytesSent = buffer.long,
                bytesReceived = buffer.long,
                syncCount = buffer.int
            )
        }
    }
}

/**
 * Information about an indirect peer (reachable via relay hops).
 *
 * Indirect peers are discovered through relay messages. When we receive a
 * message from peer A with origin B (where B != A), we learn that B is
 * reachable via A. This class tracks all such indirectly reachable peers
 * and the routes to reach them.
 */
data class IndirectPeer(
    /** The indirect peer's node ID (32-bit) */
    val nodeId: Long,
    /** Minimum hop count to reach this peer */
    val minHops: Int,
    /** Direct peers through which we can reach this peer (via_peer_id -> hop_count) */
    val viaPeers: Map<Long, Int>,
    /** Timestamp when this peer was first discovered (ms since epoch) */
    val discoveredAt: Long,
    /** Last time we received data from/about this peer (ms since epoch) */
    val lastSeenMs: Long,
    /** Number of messages received from this peer */
    val messagesReceived: Int,
    /** Optional callsign if learned from documents */
    val callsign: String?
) {
    companion object {
        /**
         * Decode indirect peer from native byte format (streaming from buffer).
         *
         * Format: [node_id: 4][min_hops: 1][via_peers_count: 1]
         *         [[via_peer_id: 4][hop_count: 1]...]
         *         [discovered_at: 8][last_seen_ms: 8][messages_received: 4]
         *         [callsign_len: 1][callsign: N]
         */
        fun decodeFromBuffer(buffer: ByteBuffer): IndirectPeer? {
            if (buffer.remaining() < 26) return null // Minimum size without via_peers

            val nodeId = buffer.int.toLong() and 0xFFFFFFFFL
            val minHops = buffer.get().toInt() and 0xFF
            val viaPeersCount = buffer.get().toInt() and 0xFF

            val viaPeers = mutableMapOf<Long, Int>()
            repeat(viaPeersCount) {
                if (buffer.remaining() < 5) return null
                val viaPeerId = buffer.int.toLong() and 0xFFFFFFFFL
                val hopCount = buffer.get().toInt() and 0xFF
                viaPeers[viaPeerId] = hopCount
            }

            if (buffer.remaining() < 21) return null

            val discoveredAt = buffer.long
            val lastSeenMs = buffer.long
            val messagesReceived = buffer.int

            val callsignLen = buffer.get().toInt() and 0xFF
            val callsign = if (callsignLen > 0 && buffer.remaining() >= callsignLen) {
                val callsignBytes = ByteArray(callsignLen)
                buffer.get(callsignBytes)
                String(callsignBytes, Charsets.UTF_8)
            } else null

            return IndirectPeer(
                nodeId = nodeId,
                minHops = minHops,
                viaPeers = viaPeers,
                discoveredAt = discoveredAt,
                lastSeenMs = lastSeenMs,
                messagesReceived = messagesReceived,
                callsign = callsign
            )
        }

        /**
         * Decode a list of indirect peers from native byte format.
         *
         * Format: [count: 4][peer1][peer2]... (peers are NOT length-prefixed)
         */
        fun decodeList(data: ByteArray): List<IndirectPeer> {
            if (data.size < 4) return emptyList()

            val buffer = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN)
            val count = buffer.int

            val result = mutableListOf<IndirectPeer>()
            repeat(count) {
                decodeFromBuffer(buffer)?.let { result.add(it) } ?: return result
            }
            return result
        }
    }
}

/**
 * Complete peer count summary including direct and indirect peers.
 *
 * Provides a full view of the mesh topology as seen from this node.
 */
data class FullStateCounts(
    /** Counts of direct peers by connection state */
    val direct: StateCountSummary,
    /** Number of one-hop indirect peers */
    val oneHop: Int,
    /** Number of two-hop indirect peers */
    val twoHop: Int,
    /** Number of three-hop indirect peers */
    val threeHop: Int
) {
    /** Total number of indirect peers */
    val totalIndirect: Int get() = oneHop + twoHop + threeHop

    /** Total number of known peers (direct + indirect) */
    val totalKnown: Int get() = direct.total + totalIndirect

    companion object {
        /**
         * Decode from native byte format.
         *
         * Format: [direct: 28 bytes (StateCountSummary)][one_hop: 4][two_hop: 4][three_hop: 4]
         */
        fun decode(data: ByteArray): FullStateCounts? {
            if (data.size < 40) return null

            val buffer = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN)

            // Decode direct counts (StateCountSummary format)
            val direct = StateCountSummary(
                discovered = buffer.int,
                connecting = buffer.int,
                connected = buffer.int,
                degraded = buffer.int,
                disconnecting = buffer.int,
                disconnected = buffer.int,
                lost = buffer.int
            )

            val oneHop = buffer.int
            val twoHop = buffer.int
            val threeHop = buffer.int

            return FullStateCounts(
                direct = direct,
                oneHop = oneHop,
                twoHop = twoHop,
                threeHop = threeHop
            )
        }
    }
}
