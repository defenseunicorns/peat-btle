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
