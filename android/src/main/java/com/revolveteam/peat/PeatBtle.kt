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


package com.revolveteam.peat

import android.Manifest
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothGattDescriptor
import android.bluetooth.BluetoothGattServer
import android.bluetooth.BluetoothGattServerCallback
import android.bluetooth.BluetoothGattService
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.bluetooth.le.AdvertiseData
import android.bluetooth.le.AdvertiseSettings
import android.bluetooth.le.BluetoothLeAdvertiser
import android.bluetooth.le.BluetoothLeScanner
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanSettings
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.os.Build
import android.os.ParcelUuid
import android.util.Log
import androidx.core.content.ContextCompat
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicLong
import android.os.Handler
import android.os.Looper

// UniFFI-generated bindings for Rust PeatMesh
import uniffi.peat_btle.PeatMesh
import uniffi.peat_btle.DeviceIdentity
import uniffi.peat_btle.MeshGenesis
import uniffi.peat_btle.IdentityAttestation
import uniffi.peat_btle.PeripheralType
import uniffi.peat_btle.EventType
import uniffi.peat_btle.DisconnectReason
import uniffi.peat_btle.ConnectionState
import uniffi.peat_btle.PeerConnectionState
import uniffi.peat_btle.StateCountSummary
import uniffi.peat_btle.FullStateCountSummary
import uniffi.peat_btle.IndirectPeer
import uniffi.peat_btle.ViaPeerRoute
import uniffi.peat_btle.CannedMessageInfo
import uniffi.peat_btle.deriveNodeIdFromMac
import uniffi.peat_btle.ReconnectionManager as RustReconnectionManager
import uniffi.peat_btle.ReconnectionConfig as RustReconnectionConfig
import uniffi.peat_btle.PeerLifetimeManager as RustPeerLifetimeManager
import uniffi.peat_btle.PeerLifetimeConfig as RustPeerLifetimeConfig

/**
 * Configuration for high-priority sync mode.
 *
 * High-priority mode trades battery life for communication reliability,
 * useful for tactical/emergency scenarios where mesh connectivity is critical.
 *
 * @param enabled Whether high-priority mode is active
 * @param autoDisableAfterMs Auto-disable timeout in ms (null = never auto-disable)
 * @param autoEnableOnEmergency Automatically enable when any peer enters emergency state
 */
data class HighPriorityConfig(
    val enabled: Boolean = false,
    val autoDisableAfterMs: Long? = 3600000L,  // 1 hour default
    val autoEnableOnEmergency: Boolean = true
) {
    companion object {
        /** Auto-disable options */
        val AUTO_DISABLE_OFF: Long? = null
        val AUTO_DISABLE_30_MIN: Long = 1800000L
        val AUTO_DISABLE_1_HOUR: Long = 3600000L
        val AUTO_DISABLE_2_HOURS: Long = 7200000L
    }
}

/**
 * Main entry point for Peat BLE operations on Android.
 *
 * This class provides a high-level API for BLE scanning, advertising, and
 * GATT connections, bridging Android's Bluetooth APIs with the native
 * peat-btle Rust implementation.
 *
 * ## Permissions
 *
 * Required permissions depend on Android version:
 * - Android 12+ (API 31): BLUETOOTH_SCAN, BLUETOOTH_CONNECT, BLUETOOTH_ADVERTISE
 * - Android 6-11: BLUETOOTH, BLUETOOTH_ADMIN, ACCESS_FINE_LOCATION
 *
 * ## Usage
 *
 * ### Basic (Unencrypted)
 * ```kotlin
 * val hiveBtle = PeatBtle(context, nodeId = 0x12345678)
 * hiveBtle.init()
 * ```
 *
 * ### Encrypted Mesh (Recommended)
 * ```kotlin
 * // Create or load identity (persist this!)
 * val identity = DeviceIdentity.generate()
 *
 * // Create or load mesh genesis (share with team members)
 * val genesis = MeshGenesis.create("ALPHA-TEAM", identity, MembershipPolicy.CONTROLLED)
 *
 * // Create encrypted mesh
 * val hiveBtle = PeatBtle(
 *     context = context,
 *     identity = identity,
 *     genesis = genesis
 * )
 * hiveBtle.init()
 *
 * // Mesh is now encrypted - only team members can read beacons/documents
 * ```
 *
 * ### Scanning & Advertising
 * ```kotlin
 * // Start scanning for Peat nodes
 * hiveBtle.startScan { device ->
 *     Log.d("PEAT", "Found: ${device.address}")
 * }
 *
 * // Connect to a device
 * val connection = hiveBtle.connect(deviceAddress)
 *
 * // Start advertising
 * hiveBtle.startAdvertising()
 * ```
 *
 * @param context Android context (Activity, Service, or Application)
 * @param nodeId This node's Peat ID (32-bit unsigned). If null, auto-generated from Bluetooth MAC address.
 * @param meshId Mesh identifier for mesh isolation (e.g., "DEMO", "ALFA"). Defaults to "DEMO".
 * @param identity Optional DeviceIdentity for cryptographic operations. Required for encrypted mesh.
 * @param genesis Optional MeshGenesis for encrypted mesh. When provided with identity, enables encryption.
 */
class PeatBtle(
    private val context: Context,
    private var _nodeId: Long? = null,
    private val meshId: String = DEFAULT_MESH_ID,
    private val identity: DeviceIdentity? = null,
    private val genesis: MeshGenesis? = null
) {
    /**
     * This node's Peat ID. Auto-generated from Bluetooth MAC address if not specified.
     * Available after init() is called.
     */
    val nodeId: Long
        get() = _nodeId ?: 0L

    /**
     * Get the mesh ID this node belongs to.
     */
    fun getMeshId(): String = meshId

    /**
     * Check if this instance has encryption enabled.
     *
     * Encryption is enabled when both identity and genesis are provided.
     */
    fun isEncryptionEnabled(): Boolean = identity != null && genesis != null

    /**
     * Get the device identity if available.
     */
    fun getIdentity(): DeviceIdentity? = identity

    /**
     * Get the mesh genesis if available.
     */
    fun getGenesis(): MeshGenesis? = genesis

    companion object {
        private const val TAG = "PeatBtle"

        /** Wire marker for app-layer messages (0xAF) - passed to onDecryptedData for apps to handle */
        private const val APP_LAYER_MARKER: Byte = 0xAF.toByte()

        /**
         * Peat BLE Service UUID (canonical: f47ac10b-58cc-4372-a567-0e02b2c3d479)
         *
         * This is the canonical Peat service UUID used across all platforms.
         */
        val PEAT_SERVICE_UUID: UUID = UUID.fromString("f47ac10b-58cc-4372-a567-0e02b2c3d479")

        /**
         * Peat BLE Service UUID - 16-bit alias (0xF47A) for space-constrained advertising
         *
         * Used by ESP32/Core2 devices to fit service UUID in BLE advertising payload.
         * Expands to standard Bluetooth SIG base: 0000f47a-0000-1000-8000-00805f9b34fb
         */
        val PEAT_SERVICE_UUID_16: UUID = UUID.fromString("0000f47a-0000-1000-8000-00805f9b34fb")

        /**
         * Peat Document Characteristic UUID (canonical: f47a0003-58cc-4372-a567-0e02b2c3d479)
         *
         * Used for exchanging CRDT document data between peers.
         * Supports read, write, and notify operations.
         * Maps to CHAR_SYNC_DATA in the canonical protocol.
         */
        val PEAT_CHAR_DOCUMENT: UUID = UUID.fromString("f47a0003-58cc-4372-a567-0e02b2c3d479")

        /** Peat Node Info Characteristic UUID (canonical) */
        val PEAT_CHAR_NODE_INFO: UUID = UUID.fromString("f47a0001-58cc-4372-a567-0e02b2c3d479")

        /** Peat Sync State Characteristic UUID (canonical) */
        val PEAT_CHAR_SYNC_STATE: UUID = UUID.fromString("f47a0002-58cc-4372-a567-0e02b2c3d479")

        /** Peat Sync Data Characteristic UUID (canonical) - same as PEAT_CHAR_DOCUMENT */
        val PEAT_CHAR_SYNC_DATA: UUID = UUID.fromString("f47a0003-58cc-4372-a567-0e02b2c3d479")

        /** Peat Command Characteristic UUID (canonical) */
        val PEAT_CHAR_COMMAND: UUID = UUID.fromString("f47a0004-58cc-4372-a567-0e02b2c3d479")

        /** Peat Status Characteristic UUID (canonical) */
        val PEAT_CHAR_STATUS: UUID = UUID.fromString("f47a0005-58cc-4372-a567-0e02b2c3d479")

        /** Client Characteristic Configuration Descriptor UUID */
        val CCCD_UUID: UUID = UUID.fromString("00002902-0000-1000-8000-00805F9B34FB")

        /** Peat device name prefix (legacy format) */
        const val PEAT_NAME_PREFIX = "PEAT-"

        /** Peat device name prefix with mesh ID (new format) */
        const val PEAT_MESH_PREFIX = "PEAT_"

        /** Default mesh ID for demos and testing */
        const val DEFAULT_MESH_ID = "DEMO"

        /**
         * Derive a short mesh ID from an app ID string.
         *
         * The mesh ID is used in BLE device names and should be short (4-8 chars).
         * This function takes a potentially long app_id (e.g., "default-atak-formation")
         * and derives a short mesh ID from it.
         *
         * Strategy:
         * - If app_id is 8 chars or less, use it directly (uppercased)
         * - Otherwise, use first 4 chars of a hash (uppercased hex)
         *
         * @param appId The application/formation ID (e.g., from PEAT_APP_ID env var)
         * @return A short mesh ID suitable for BLE device names
         */
        @JvmStatic
        fun deriveMeshId(appId: String): String {
            val normalized = appId.trim().uppercase()
            return if (normalized.length <= 8) {
                normalized.ifEmpty { DEFAULT_MESH_ID }
            } else {
                // Use first 4 hex chars of hash for longer app IDs
                String.format("%04X", appId.hashCode() and 0xFFFF)
            }
        }

        /**
         * Get the mesh ID from environment or system properties.
         *
         * Checks in order:
         * 1. System property "hive.mesh_id"
         * 2. System property "hive.app_id" (derives mesh ID from it)
         * 3. Environment variable "PEAT_MESH_ID"
         * 4. Environment variable "PEAT_APP_ID" (derives mesh ID from it)
         * 5. Falls back to DEFAULT_MESH_ID ("DEMO")
         *
         * @return The mesh ID to use for this node
         */
        @JvmStatic
        fun getMeshIdFromEnvironment(): String {
            // Direct mesh ID takes priority
            System.getProperty("hive.mesh_id")?.let { return it }
            System.getenv("PEAT_MESH_ID")?.let { return it }

            // Derive from app ID if available
            System.getProperty("hive.app_id")?.let { return deriveMeshId(it) }
            System.getenv("PEAT_APP_ID")?.let { return deriveMeshId(it) }

            return DEFAULT_MESH_ID
        }

        /**
         * Generate a device name in the new mesh format: PEAT_<MESH_ID>-<NODE_ID>
         *
         * @param meshId Mesh identifier (e.g., "DEMO", "ALFA")
         * @param nodeId Node ID as 32-bit unsigned long
         * @return Device name string (e.g., "PEAT_DEMO-12345678")
         */
        @JvmStatic
        fun generateDeviceName(meshId: String, nodeId: Long): String {
            return "PEAT_${meshId}-${String.format("%08X", nodeId)}"
        }

        /**
         * Parse mesh ID and node ID from a device name.
         *
         * Supports both formats:
         * - New: PEAT_<MESH_ID>-<NODE_ID> (e.g., "PEAT_DEMO-12345678")
         * - Legacy: PEAT-<NODE_ID> (e.g., "PEAT-12345678") - returns null meshId
         *
         * @param name Device name to parse
         * @return Pair of (meshId, nodeId) or null if parsing fails
         */
        @JvmStatic
        fun parseDeviceName(name: String): Pair<String?, Long>? {
            return when {
                name.startsWith(PEAT_MESH_PREFIX) -> {
                    // New format: PEAT_MESHID-NODEID
                    val rest = name.removePrefix(PEAT_MESH_PREFIX)
                    val dashIndex = rest.indexOf('-')
                    if (dashIndex <= 0) return null
                    val meshId = rest.substring(0, dashIndex)
                    val nodeIdStr = rest.substring(dashIndex + 1)
                    try {
                        val nodeId = nodeIdStr.toLong(16)
                        Pair(meshId, nodeId)
                    } catch (e: NumberFormatException) {
                        null
                    }
                }
                name.startsWith(PEAT_NAME_PREFIX) -> {
                    // Legacy format: PEAT-NODEID (no mesh ID)
                    val nodeIdStr = name.removePrefix(PEAT_NAME_PREFIX)
                    try {
                        val nodeId = nodeIdStr.toLong(16)
                        Pair(null, nodeId)
                    } catch (e: NumberFormatException) {
                        null
                    }
                }
                else -> null
            }
        }

        /**
         * Check if a device matches our mesh.
         *
         * Returns true if:
         * - The device has the same mesh ID, OR
         * - The device has no mesh ID (legacy format - backwards compatible)
         *
         * @param ourMeshId Our mesh ID
         * @param deviceMeshId Device's mesh ID (null for legacy format)
         * @return true if the device matches our mesh
         */
        @JvmStatic
        fun matchesMesh(ourMeshId: String, deviceMeshId: String?): Boolean {
            return deviceMeshId == null || deviceMeshId == ourMeshId
        }

        /**
         * Derive a NodeId from a BLE MAC address using the native Rust implementation.
         * This ensures consistency across all platforms (Android, iOS, ESP32).
         *
         * @param macAddress MAC address in "AA:BB:CC:DD:EE:FF" format
         * @return NodeId derived from last 4 bytes of MAC, or 0 if parsing fails
         */
        @JvmStatic
        fun nativeDeriveNodeId(macAddress: String): Long {
            return deriveNodeIdFromMac(macAddress).toLong()
        }

        // ==================== Build-time Configuration ====================

        /**
         * Get the build-time embedded encryption secret, if configured.
         *
         * Set via environment variable when building:
         * ```
         * PEAT_ENCRYPTION_SECRET=0102030405060708091011121314151617181920212223242526272829303132 \
         *   ./gradlew assembleRelease
         * ```
         *
         * Or override in downstream project's build.gradle.kts:
         * ```
         * buildConfigField("String", "PEAT_ENCRYPTION_SECRET", "\"<64-char-hex>\"")
         * ```
         *
         * @return 32-byte secret array, or null if not configured or invalid
         */
        @JvmStatic
        fun getEmbeddedEncryptionSecret(): ByteArray? {
            val hex = BuildConfig.PEAT_ENCRYPTION_SECRET
            if (hex.isNullOrEmpty() || hex.length != 64) return null
            return try {
                hex.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
            } catch (e: NumberFormatException) {
                Log.w(TAG, "Invalid PEAT_ENCRYPTION_SECRET format: $e")
                null
            }
        }

        /**
         * Get the build-time embedded mesh ID, if configured.
         *
         * Set via environment variable when building:
         * ```
         * PEAT_MESH_ID=ALPHA ./gradlew assembleRelease
         * ```
         *
         * @return Mesh ID string, or null if not configured
         */
        @JvmStatic
        fun getEmbeddedMeshId(): String? {
            val meshId = BuildConfig.PEAT_MESH_ID
            return if (meshId.isNullOrEmpty()) null else meshId
        }

        /**
         * Check if build-time encryption credentials are configured.
         */
        @JvmStatic
        fun hasEmbeddedEncryption(): Boolean {
            return getEmbeddedEncryptionSecret() != null
        }

        /**
         * Get effective mesh ID, checking embedded config first, then environment.
         *
         * Priority:
         * 1. Build-time PEAT_MESH_ID
         * 2. Runtime environment/system property
         * 3. DEFAULT_MESH_ID ("DEMO")
         */
        @JvmStatic
        fun getEffectiveMeshId(): String {
            return getEmbeddedMeshId() ?: getMeshIdFromEnvironment()
        }

        // Note: Native library loading is handled automatically by UniFFI/JNA
        // when the first UniFFI type is accessed. No manual System.loadLibrary needed.
    }

    // Android Bluetooth components
    private var bluetoothManager: BluetoothManager? = null
    private var bluetoothAdapter: BluetoothAdapter? = null
    private var leScanner: BluetoothLeScanner? = null
    private var leAdvertiser: BluetoothLeAdvertiser? = null

    // Callbacks
    private var scanCallback: ScanCallbackProxy? = null
    private var advertiseCallback: AdvertiseCallbackProxy? = null

    // Active GATT connections (as Central - connecting to others)
    private val connections = ConcurrentHashMap<String, BluetoothGatt>()
    private val gattCallbacks = ConcurrentHashMap<String, GattCallbackProxy>()
    private val connectionIdCounter = AtomicLong(0)

    // Write queues for serializing BLE writes (BLE only allows one pending write at a time)
    private val writeQueues = ConcurrentHashMap<String, java.util.concurrent.ConcurrentLinkedQueue<ByteArray>>()
    private val writeInProgress = ConcurrentHashMap<String, Boolean>()

    // GATT Server (as Peripheral - others connect to us)
    private var gattServer: BluetoothGattServer? = null
    private var gattServerCallback: GattServerCallback? = null
    private val connectedCentrals = ConcurrentHashMap<String, BluetoothDevice>() // address -> device
    private var syncDataCharacteristic: BluetoothGattCharacteristic? = null

    // Message relay deduplication cache (message hash -> timestamp)
    // Uses LinkedHashMap with access-order for LRU eviction
    private val seenMessages = object : LinkedHashMap<Long, Long>(100, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<Long, Long>?): Boolean {
            return size > 1000  // Keep last 1000 messages
        }
    }
    private val seenMessagesLock = Any()

    // State
    private var isInitialized = false

    // Pairing request cancellation receiver
    // Cancels unwanted pairing requests that some Android devices (e.g., Samsung) trigger
    // when connecting to BLE GATT servers. Peat uses application-layer encryption,
    // not BLE pairing, so these prompts are unnecessary and disruptive.
    private val pairingRequestReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            if (intent?.action == BluetoothDevice.ACTION_PAIRING_REQUEST) {
                val device = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    intent.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE, BluetoothDevice::class.java)
                } else {
                    @Suppress("DEPRECATION")
                    intent.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE)
                }
                val deviceName = device?.name ?: "unknown"
                val deviceAddress = device?.address ?: "unknown"

                // Check if this is a Peat device (by name pattern)
                val isPeatDevice = deviceName.startsWith("PEAT_") || deviceName.startsWith("PEAT-")

                if (isPeatDevice) {
                    Log.i(TAG, "Cancelling pairing request for Peat device: $deviceName ($deviceAddress)")
                    // Cancel the pairing by aborting the broadcast
                    abortBroadcast()
                    // Also try to cancel via the device API
                    try {
                        val cancelMethod = device?.javaClass?.getMethod("cancelPairingUserInput")
                        cancelMethod?.invoke(device)
                    } catch (e: Exception) {
                        Log.d(TAG, "cancelPairingUserInput not available: ${e.message}")
                    }
                    try {
                        val cancelBondMethod = device?.javaClass?.getMethod("cancelBondProcess")
                        cancelBondMethod?.invoke(device)
                    } catch (e: Exception) {
                        Log.d(TAG, "cancelBondProcess not available: ${e.message}")
                    }
                } else {
                    Log.d(TAG, "Allowing pairing request for non-Peat device: $deviceName")
                }
            }
        }
    }
    private var pairingReceiverRegistered = false
    private var isScanning = false
    private var isAdvertising = false
    private var isMeshRunning = false

    // PeatMesh instance for ConnectionStateGraph API
    private var _mesh: PeatMesh? = null

    /**
     * The PeatMesh instance for accessing ConnectionStateGraph API.
     *
     * Available after init() is called. Provides methods for querying peer
     * connection states:
     * - mesh.getConnectionStateCounts() - summary counts for UI badges
     * - mesh.getConnectedPeers() - peers with active connections
     * - mesh.getDegradedPeers() - peers with weak signal
     * - mesh.getLostPeers() - peers no longer seen
     * - mesh.getPeerConnectionState(nodeId) - specific peer state
     *
     * Example:
     * ```kotlin
     * hiveBtle.mesh?.getConnectedPeers()?.forEach { peer ->
     *     Log.d(TAG, "Connected: ${peer.name} (${peer.state})")
     * }
     * ```
     */
    val mesh: PeatMesh?
        get() = _mesh

    // Mesh management
    private val peers = ConcurrentHashMap<Long, PeatPeer>() // nodeId -> peer
    private val addressToNodeId = ConcurrentHashMap<String, Long>() // address -> nodeId
    private val nameToNodeId = ConcurrentHashMap<String, Long>() // device name -> nodeId (for address rotation dedup)
    private val callsignToNodeId = ConcurrentHashMap<String, Long>() // callsign -> nodeId (for identity resolution)
    private val nodeIdToCallsign = ConcurrentHashMap<Long, String>() // nodeId -> callsign (reverse lookup, persisted)
    private val peerSyncState = ConcurrentHashMap<Long, PeerSyncState>() // nodeId -> sync state for delta tracking
    // Track processed chat messages to avoid duplicate notifications (key = "originNode:timestamp")
    private var meshListener: PeatMeshListener? = null
    private val handler = Handler(Looper.getMainLooper())
    private var localDocument: PeatDocument? = null
    private var localPeripheral: PeatPeripheral? = null  // Persistent peripheral state (location, health, etc.)
    private var localCounter = mutableListOf<GCounterEntry>()

    // High-priority sync mode configuration
    private var _highPriorityConfig = HighPriorityConfig()
    private var highPriorityEnabledAt: Long? = null
    private var highPriorityListener: HighPriorityModeListener? = null

    /**
     * Listener for high-priority mode changes.
     */
    interface HighPriorityModeListener {
        fun onHighPriorityModeChanged(enabled: Boolean, reason: String)
    }

    // Dynamic timing based on high-priority mode
    private val PEER_TIMEOUT_MS: Long get() = 120000L
    private val CONNECTED_PEER_TIMEOUT_MS: Long get() = 300000L
    private val CLEANUP_INTERVAL_MS: Long get() = 10000L
    private val SYNC_INTERVAL_MS: Long get() = if (_highPriorityConfig.enabled) 1000L else 3000L
    private val RECONNECT_INTERVAL_MS: Long get() = if (_highPriorityConfig.enabled) 1000L else 3000L
    private val RECONNECT_BASE_DELAY_MS: Long get() = if (_highPriorityConfig.enabled) 500L else 1000L
    private val RECONNECT_MAX_DELAY_MS: Long get() = if (_highPriorityConfig.enabled) 5000L else 15000L
    private val RECONNECT_MAX_ATTEMPTS: Int get() = if (_highPriorityConfig.enabled) 50 else 20
    private val SCAN_RESTART_INTERVAL_MS: Long get() = if (_highPriorityConfig.enabled) 30000L else 120000L
    private val KEEP_ALIVE_INTERVAL_MS: Long get() = if (_highPriorityConfig.enabled) 3000L else 10000L

    // RSSI polling configuration (0 = disabled in normal mode)
    // When enabled, periodically reads RSSI from connected peers for realtime signal strength updates
    // In high-priority mode, RSSI polling is automatically enabled at 3 second intervals
    private var _rssiPollingIntervalMs: Long = 0L  // Manual override (0 = use automatic behavior)
    private val RSSI_POLLING_INTERVAL_MS: Long get() = when {
        _highPriorityConfig.enabled -> 3000L  // Always poll every 3s in high-priority mode
        _rssiPollingIntervalMs > 0 -> _rssiPollingIntervalMs  // Use manually configured interval
        else -> 0L  // Disabled in normal mode
    }

    // Rust-backed reconnection manager (replaces inline reconnectAttempts/lastReconnectAttempt maps)
    private var reconnectionManager: RustReconnectionManager? = null
    // Rust-backed peer lifetime manager (replaces inline stale peer detection)
    private var peerLifetimeManager: RustPeerLifetimeManager? = null

    // Grace period: peer stays visible as "reconnecting" for a few seconds after disconnect
    private val RECONNECT_GRACE_MS = 5000L
    private val reconnectGraceRunnables = mutableMapOf<String, Runnable>()

    // Store discovery callback for scan restart
    private var discoveryCallback: ((DiscoveredDevice) -> Unit)? = null

    // Auto-disable runnable for high-priority mode
    private val highPriorityAutoDisableRunnable = Runnable {
        if (_highPriorityConfig.enabled && _highPriorityConfig.autoDisableAfterMs != null) {
            Log.i(TAG, "[HIGH-PRIORITY] Auto-disabling after timeout")
            setHighPriorityMode(false, "auto-disable timeout")
        }
    }

    private val cleanupRunnable = object : Runnable {
        override fun run() {
            cleanupStalePeers()
            if (isMeshRunning) {
                handler.postDelayed(this, CLEANUP_INTERVAL_MS)
            }
        }
    }

    // Periodic reconnection runnable - attempts to reconnect lost peers
    private val reconnectRunnable = object : Runnable {
        override fun run() {
            reconnectLostPeers()
            if (isMeshRunning) {
                handler.postDelayed(this, RECONNECT_INTERVAL_MS)
            }
        }
    }

    private val syncRunnable = object : Runnable {
        override fun run() {
            syncWithPeers()
            if (isMeshRunning) {
                handler.postDelayed(this, SYNC_INTERVAL_MS)
            }
        }
    }

    // WearOS workaround: Periodically restart BLE scan
    // WearOS silently kills scans after ~5 minutes without any error callback
    private val scanRestartRunnable = object : Runnable {
        override fun run() {
            if (isMeshRunning && isScanning) {
                Log.i(TAG, "[SCAN-RESTART] Restarting BLE scan (WearOS workaround)")
                stopScan()
                val scanRestartDelay = if (_highPriorityConfig.enabled) 100L else 500L
                handler.postDelayed({
                    if (isMeshRunning) {
                        discoveryCallback?.let { startScan(it) }
                    }
                }, scanRestartDelay)
            }
            if (isMeshRunning) {
                handler.postDelayed(this, SCAN_RESTART_INTERVAL_MS)
            }
        }
    }

    // RSSI polling runnable - reads RSSI from connected peers for realtime signal strength
    private val rssiPollingRunnable = object : Runnable {
        override fun run() {
            if (isMeshRunning && RSSI_POLLING_INTERVAL_MS > 0) {
                pollConnectedPeersRssi()
                handler.postDelayed(this, RSSI_POLLING_INTERVAL_MS)
            }
        }
    }

    /**
     * Poll RSSI for all connected peers.
     * Results come back asynchronously via onReadRemoteRssi callback.
     */
    private fun pollConnectedPeersRssi() {
        val connectedPeers = peers.values.filter { it.isConnected }
        if (connectedPeers.isEmpty()) return

        Log.v(TAG, "[RSSI-POLL] Polling ${connectedPeers.size} connected peers")
        for (peer in connectedPeers) {
            val gatt = connections[peer.address]
            if (gatt != null) {
                try {
                    gatt.readRemoteRssi()
                } catch (e: Exception) {
                    Log.w(TAG, "[RSSI-POLL] Failed to read RSSI for ${peer.displayName()}: ${e.message}")
                }
            }
        }
    }

    /**
     * Initialize the Peat BLE adapter.
     *
     * Must be called before any other operations. Checks for Bluetooth
     * availability and required permissions.
     *
     * @throws IllegalStateException if Bluetooth is not available
     * @throws SecurityException if required permissions are not granted
     */
    fun init() {
        if (isInitialized) {
            Log.w(TAG, "Already initialized")
            return
        }

        // Get Bluetooth manager
        bluetoothManager = context.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager
            ?: throw IllegalStateException("Bluetooth not available on this device")

        // Get adapter
        bluetoothAdapter = bluetoothManager?.adapter
            ?: throw IllegalStateException("Bluetooth adapter not available")

        // Check if enabled
        if (bluetoothAdapter?.isEnabled != true) {
            throw IllegalStateException("Bluetooth is not enabled")
        }

        // Get LE scanner
        leScanner = bluetoothAdapter?.bluetoothLeScanner

        // Get LE advertiser (may be null if not supported)
        leAdvertiser = bluetoothAdapter?.bluetoothLeAdvertiser

        // Use identity.nodeId when identity is provided (Ed25519-derived),
        // otherwise auto-generate from Bluetooth adapter MAC address
        if (identity != null) {
            _nodeId = identity.getNodeId().toLong()
            Log.i(TAG, "Using identity-derived nodeId: ${String.format("%08X", nodeId)}")
        } else if (_nodeId == null) {
            _nodeId = generateNodeIdFromAdapter()
            Log.i(TAG, "Auto-generated nodeId from adapter: ${String.format("%08X", nodeId)}")
        }

        // Create PeatMesh for ConnectionStateGraph API
        // Use encrypted mesh if identity and genesis are provided
        _mesh = if (identity != null && genesis != null) {
            Log.i(TAG, "Creating encrypted mesh from genesis")
            PeatMesh.newFromGenesis("ANDROID", identity, genesis)
        } else {
            Log.i(TAG, "Creating unencrypted mesh")
            PeatMesh.newWithPeripheral(
                nodeId.toUInt(),
                "ANDROID",
                meshId,
                PeripheralType.SOLDIER_SENSOR
            )
        }

        // Load persisted callsign mappings
        loadCallsignCache()

        // Register pairing request receiver to cancel unwanted pairing dialogs
        // Samsung and some other Android devices prompt for pairing on BLE connections
        // Peat doesn't need BLE pairing (uses app-layer encryption), so we cancel these
        if (!pairingReceiverRegistered) {
            try {
                val filter = IntentFilter(BluetoothDevice.ACTION_PAIRING_REQUEST)
                filter.priority = IntentFilter.SYSTEM_HIGH_PRIORITY
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    context.registerReceiver(pairingRequestReceiver, filter, Context.RECEIVER_NOT_EXPORTED)
                } else {
                    context.registerReceiver(pairingRequestReceiver, filter)
                }
                pairingReceiverRegistered = true
                Log.i(TAG, "Registered pairing request cancellation receiver")
            } catch (e: Exception) {
                Log.w(TAG, "Failed to register pairing receiver: ${e.message}")
            }
        }

        isInitialized = true
        Log.i(TAG, "Initialized for node ${String.format("%08X", nodeId)}")
    }

    /**
     * Check if Bluetooth permissions are granted.
     *
     * @return true if all required permissions are granted
     */
    fun hasPermissions(): Boolean {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            // Android 12+
            hasPermission(Manifest.permission.BLUETOOTH_SCAN) &&
            hasPermission(Manifest.permission.BLUETOOTH_CONNECT) &&
            hasPermission(Manifest.permission.BLUETOOTH_ADVERTISE)
        } else {
            // Android 6-11
            hasPermission(Manifest.permission.BLUETOOTH) &&
            hasPermission(Manifest.permission.BLUETOOTH_ADMIN) &&
            hasPermission(Manifest.permission.ACCESS_FINE_LOCATION)
        }
    }

    private fun hasPermission(permission: String): Boolean {
        return ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED
    }

    /**
     * Get the list of required permissions for the current Android version.
     *
     * @return Array of permission strings to request
     */
    fun getRequiredPermissions(): Array<String> {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            arrayOf(
                Manifest.permission.BLUETOOTH_SCAN,
                Manifest.permission.BLUETOOTH_CONNECT,
                Manifest.permission.BLUETOOTH_ADVERTISE
            )
        } else {
            arrayOf(
                Manifest.permission.BLUETOOTH,
                Manifest.permission.BLUETOOTH_ADMIN,
                Manifest.permission.ACCESS_FINE_LOCATION
            )
        }
    }

    /**
     * Start scanning for Peat BLE devices.
     *
     * Scans for devices advertising the Peat service UUID or with names
     * matching the PEAT-XXXXXXXX pattern.
     *
     * @param onDeviceFound Callback invoked when a Peat device is discovered
     */
    fun startScan(onDeviceFound: ((DiscoveredDevice) -> Unit)? = null) {
        checkInitialized()

        if (isScanning) {
            Log.w(TAG, "Already scanning")
            return
        }

        val scanner = leScanner
            ?: throw IllegalStateException("BLE scanner not available")

        // Scan without strict UUID filter - the M5Stack may not advertise the UUID
        // in a way Android's filter recognizes. We filter by name prefix instead.
        // An empty filter list means scan for all devices.
        val filters = emptyList<ScanFilter>()

        // Build scan settings
        val settings = ScanSettings.Builder()
            .setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY)
            .setCallbackType(ScanSettings.CALLBACK_TYPE_ALL_MATCHES)
            .setMatchMode(ScanSettings.MATCH_MODE_AGGRESSIVE)
            .setNumOfMatches(ScanSettings.MATCH_NUM_MAX_ADVERTISEMENT)
            .setReportDelay(0)
            .build()

        // Store callback for scan restart workaround
        discoveryCallback = onDeviceFound

        // Create callback proxy with the onDeviceFound callback
        scanCallback = ScanCallbackProxy(onDeviceFound)

        try {
            scanner.startScan(filters, settings, scanCallback)
            isScanning = true
            Log.i(TAG, "Started scanning for Peat devices (no UUID filter)")
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_SCAN permission", e)
            throw e
        }
    }

    /**
     * Stop scanning for BLE devices.
     */
    fun stopScan() {
        if (!isScanning) {
            return
        }

        try {
            scanCallback?.let { leScanner?.stopScan(it) }
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_SCAN permission", e)
        }

        scanCallback = null
        isScanning = false
        Log.i(TAG, "Stopped scanning")
    }

    /**
     * Start advertising as a Peat node.
     *
     * Advertises the Peat service UUID with this node's ID in the
     * service data.
     *
     * @param mode Advertising mode (default: balanced)
     * @param txPower TX power level (default: medium)
     */
    fun startAdvertising(
        mode: Int = AdvertiseSettings.ADVERTISE_MODE_BALANCED,
        txPower: Int = AdvertiseSettings.ADVERTISE_TX_POWER_MEDIUM
    ) {
        checkInitialized()

        if (isAdvertising) {
            Log.w(TAG, "Already advertising")
            return
        }

        val advertiser = leAdvertiser
            ?: throw IllegalStateException("BLE advertising not supported on this device")

        // Build advertise settings
        val settings = AdvertiseSettings.Builder()
            .setAdvertiseMode(mode)
            .setTxPowerLevel(txPower)
            .setConnectable(true)
            .setTimeout(0) // Advertise indefinitely
            .build()

        // Build service data containing node ID and mesh ID for reliable discovery
        // Format: [nodeId:4 bytes BE][meshId: up to 8 chars UTF-8]
        val meshIdBytes = meshId.toByteArray(Charsets.UTF_8).take(8).toByteArray()
        val serviceDataBytes = ByteArray(4 + meshIdBytes.size)
        serviceDataBytes[0] = ((nodeId shr 24) and 0xFF).toByte()
        serviceDataBytes[1] = ((nodeId shr 16) and 0xFF).toByte()
        serviceDataBytes[2] = ((nodeId shr 8) and 0xFF).toByte()
        serviceDataBytes[3] = (nodeId and 0xFF).toByte()
        meshIdBytes.copyInto(serviceDataBytes, 4)

        // Build advertise data with 16-bit service UUID alias and service data
        // Device name goes in scan response to stay within 31-byte advertising limit
        val data = AdvertiseData.Builder()
            .setIncludeDeviceName(false)
            .addServiceUuid(ParcelUuid(PEAT_SERVICE_UUID_16))
            .addServiceData(ParcelUuid(PEAT_SERVICE_UUID_16), serviceDataBytes)
            .build()

        // Scan response with device name
        val scanResponse = AdvertiseData.Builder()
            .setIncludeDeviceName(true)
            .build()

        Log.d(TAG, "Advertising service data: ${serviceDataBytes.joinToString(" ") { String.format("%02X", it) }}")

        // Create callback proxy
        advertiseCallback = AdvertiseCallbackProxy()

        try {
            advertiser.startAdvertising(settings, data, scanResponse, advertiseCallback)
            isAdvertising = true
            Log.i(TAG, "Started advertising as ${generateDeviceName(meshId, nodeId)}")
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_ADVERTISE permission", e)
            throw e
        }
    }

    /**
     * Stop advertising.
     */
    fun stopAdvertising() {
        if (!isAdvertising) {
            return
        }

        try {
            advertiseCallback?.let { leAdvertiser?.stopAdvertising(it) }
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_ADVERTISE permission", e)
        }

        advertiseCallback = null
        isAdvertising = false
        Log.i(TAG, "Stopped advertising")
    }

    // ==================== GATT Server (Peripheral Mode) ====================

    /**
     * Start the GATT server to accept incoming connections.
     *
     * This allows iOS and other devices to connect to this Android device
     * and read/write the Peat document characteristic.
     *
     * The GATT server is persistent across mesh restarts to avoid Android
     * Bluetooth stack leaks where closed servers don't immediately release
     * their registration.
     */
    private fun startGattServer() {
        // Reuse existing GATT server if already created (prevents registration leaks)
        if (gattServer != null) {
            Log.i(TAG, "Reusing existing GATT server")
            return
        }

        val manager = bluetoothManager ?: return

        try {
            gattServerCallback = GattServerCallback()
            gattServer = manager.openGattServer(context, gattServerCallback)

            if (gattServer == null) {
                Log.e(TAG, "Failed to open GATT server")
                return
            }

            // Create the Peat service
            val service = BluetoothGattService(
                PEAT_SERVICE_UUID,
                BluetoothGattService.SERVICE_TYPE_PRIMARY
            )

            // Create the sync data characteristic with read, write, notify properties
            syncDataCharacteristic = BluetoothGattCharacteristic(
                PEAT_CHAR_DOCUMENT,
                BluetoothGattCharacteristic.PROPERTY_READ or
                        BluetoothGattCharacteristic.PROPERTY_WRITE or
                        BluetoothGattCharacteristic.PROPERTY_NOTIFY,
                BluetoothGattCharacteristic.PERMISSION_READ or
                        BluetoothGattCharacteristic.PERMISSION_WRITE
            )

            // Add CCCD for notifications
            val cccd = BluetoothGattDescriptor(
                CCCD_UUID,
                BluetoothGattDescriptor.PERMISSION_READ or BluetoothGattDescriptor.PERMISSION_WRITE
            )
            syncDataCharacteristic?.addDescriptor(cccd)

            service.addCharacteristic(syncDataCharacteristic)

            // Add the service to the server
            val added = gattServer?.addService(service) ?: false
            if (added) {
                Log.i(TAG, "GATT server started with Peat service")
            } else {
                Log.e(TAG, "Failed to add Peat service to GATT server")
            }

        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_CONNECT permission for GATT server", e)
        }
    }

    /**
     * Pause the GATT server (disconnect centrals but keep server open).
     *
     * This does NOT close the GATT server to avoid Android Bluetooth stack
     * registration leaks. The server remains open and is reused on next mesh start.
     */
    private fun pauseGattServer() {
        connectedCentrals.clear()
        Log.i(TAG, "GATT server paused (connections cleared, server kept open)")
    }

    /**
     * Close the GATT server permanently (only called from shutdown).
     *
     * This should only be called when the PeatBtle instance is being destroyed.
     * Calling this during mesh stop/restart causes GATT server registration leaks
     * on Android where closed servers don't immediately release their registration.
     */
    private fun closeGattServer() {
        try {
            gattServer?.close()
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing permission to close GATT server", e)
        }
        gattServer = null
        gattServerCallback = null
        syncDataCharacteristic = null
        connectedCentrals.clear()
        Log.i(TAG, "GATT server closed")
    }

    /**
     * Send a notification to all connected centrals (devices that connected to us).
     */
    private fun notifyConnectedCentrals(data: ByteArray) {
        val server = gattServer ?: return
        val characteristic = syncDataCharacteristic ?: return

        if (connectedCentrals.isEmpty()) {
            Log.d(TAG, "No connected centrals to notify")
            return
        }

        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                for ((address, device) in connectedCentrals) {
                    val result = server.notifyCharacteristicChanged(device, characteristic, false, data)
                    Log.d(TAG, "Notified central $address: result=$result")
                }
            } else {
                @Suppress("DEPRECATION")
                characteristic.value = data
                for ((address, device) in connectedCentrals) {
                    @Suppress("DEPRECATION")
                    val result = server.notifyCharacteristicChanged(device, characteristic, false)
                    Log.d(TAG, "Notified central $address: result=$result")
                }
            }
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing permission to notify centrals", e)
        }
    }

    /**
     * GATT Server callback for handling incoming connections and requests.
     */
    private inner class GattServerCallback : BluetoothGattServerCallback() {

        override fun onConnectionStateChange(device: BluetoothDevice, status: Int, newState: Int) {
            try {
                val address = device.address
                val name = device.name ?: "Unknown"

                when (newState) {
                    BluetoothProfile.STATE_CONNECTED -> {
                        Log.i(TAG, "Central connected: $name ($address)")
                        connectedCentrals[address] = device

                        // Find and update peer, notify listener
                        val nodeId = addressToNodeId[address]
                        val peer = nodeId?.let { peers[it] }
                        if (peer != null) {
                            peer.isConnected = true
                            cancelReconnectGrace(peer)
                            peer.lastSeen = System.currentTimeMillis()
                            peerLifetimeManager?.onPeerActivity(address, true)
                            deduplicateConnectedCentrals(address, nodeId)
                            notifyPeerConnected(peer)
                        }
                        // Update PeatMesh ConnectionStateGraph
                        // For incoming peripheral connections, use onIncomingConnection()
                        // which creates the peer in Rust if it doesn't exist yet
                        val now = System.currentTimeMillis().toULong()
                        if (nodeId != null) {
                            _mesh?.onIncomingConnection(address, nodeId.toUInt(), now)
                        } else {
                            _mesh?.onBleConnected(address, now)
                        }
                        notifyMeshUpdated()
                    }
                    BluetoothProfile.STATE_DISCONNECTED -> {
                        Log.i(TAG, "Central disconnected: $name ($address)")
                        connectedCentrals.remove(address)

                        // Find and update peer, notify listener for immediate UI update
                        val nodeId = addressToNodeId[address]
                        val peer = nodeId?.let { peers[it] }
                        if (peer != null) {
                            peer.isConnected = false
                            startReconnectGrace(peer)
                            notifyPeerDisconnected(peer)
                        }
                        peerLifetimeManager?.onPeerDisconnected(address)
                        reconnectionManager?.trackDisconnection(address)
                        // Update PeatMesh ConnectionStateGraph
                        _mesh?.onBleDisconnected(address, DisconnectReason.REMOTE_REQUEST)
                        notifyMeshUpdated()
                    }
                }
            } catch (e: SecurityException) {
                Log.e(TAG, "Missing permission in onConnectionStateChange", e)
            }
        }

        override fun onCharacteristicReadRequest(
            device: BluetoothDevice,
            requestId: Int,
            offset: Int,
            characteristic: BluetoothGattCharacteristic
        ) {
            try {
                val address = device.address
                Log.d(TAG, "Read request from $address for ${characteristic.uuid}")

                if (characteristic.uuid == PEAT_CHAR_DOCUMENT) {
                    // Return current document state
                    // When encryption is enabled, use native mesh to get properly encrypted document
                    val documentBytes = if (isEncryptionEnabled() && _mesh != null) {
                        // Sync local peripheral state to native before building document
                        syncLocalPeripheralToNative(System.currentTimeMillis())
                        Log.d(TAG, "[ENCRYPTED] Read request: using native buildDocument")
                        _mesh!!.buildDocument()
                    } else {
                        PeatDocument.encode(nodeId, localCounter, localPeripheral)
                    }
                    val response = if (offset > documentBytes.size) {
                        ByteArray(0)
                    } else {
                        documentBytes.copyOfRange(offset, documentBytes.size)
                    }

                    gattServer?.sendResponse(
                        device,
                        requestId,
                        BluetoothGatt.GATT_SUCCESS,
                        offset,
                        response
                    )
                    Log.d(TAG, "Sent ${response.size} bytes to $address")
                } else {
                    gattServer?.sendResponse(
                        device,
                        requestId,
                        BluetoothGatt.GATT_REQUEST_NOT_SUPPORTED,
                        0,
                        null
                    )
                }
            } catch (e: SecurityException) {
                Log.e(TAG, "Missing permission in onCharacteristicReadRequest", e)
            }
        }

        override fun onCharacteristicWriteRequest(
            device: BluetoothDevice,
            requestId: Int,
            characteristic: BluetoothGattCharacteristic,
            preparedWrite: Boolean,
            responseNeeded: Boolean,
            offset: Int,
            value: ByteArray?
        ) {
            try {
                val address = device.address
                val dataSize = value?.size ?: 0
                Log.i(TAG, "Write request from $address: $dataSize bytes")

                if (characteristic.uuid == PEAT_CHAR_DOCUMENT && value != null) {
                    // Log raw data for debugging
                    val hexData = value.joinToString(" ") { String.format("%02X", it) }
                    Log.d(TAG, "Received data: $hexData")

                    // Check for special document markers first
                    if (value.isNotEmpty() && value[0] == CHAT_SECTION_MARKER) {
                        // Chat document (0xAD) - find/create peer and handle
                        val chat = PeatChat.decode(value)
                        if (chat != null) {
                            val sourceNodeId = chat.originNode
                            if (sourceNodeId != nodeId && sourceNodeId != 0L) {
                                var peer = peers.values.find { it.address == address }
                                    ?: peers[sourceNodeId]
                                    ?: run {
                                        // Create peer for incoming chat
                                        val now = System.currentTimeMillis()
                                        val newPeer = PeatPeer(
                                            nodeId = sourceNodeId,
                                            address = address,
                                            name = generateDeviceName(meshId, sourceNodeId),
                                            meshId = meshId,
                                            rssi = 0,
                                            isConnected = true,
                                            lastDocument = null,
                                            lastSeen = now
                                        )
                                        peers[sourceNodeId] = newPeer
                                        addressToNodeId[address] = sourceNodeId
                                        peerLifetimeManager?.onPeerActivity(address, true)
                                        Log.i(TAG, "Added peer from chat write: ${newPeer.displayName()}")
                                        newPeer
                                    }
                                handlePeerChatDocument(peer, value)
                            }
                        } else {
                            Log.w(TAG, "Failed to decode chat document from $address")
                        }
                        if (responseNeeded) {
                            gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, 0, null)
                        }
                        return
                    }

                    if (value.isNotEmpty() && value[0] == MARKER_SECTION_MARKER) {
                        // Marker document (0xAC) - find peer by address and handle
                        val peer = peers.values.find { it.address == address }
                        if (peer != null) {
                            handlePeerMarkerDocument(peer, value)
                        } else {
                            Log.w(TAG, "Received marker from unknown peer $address")
                        }
                        if (responseNeeded) {
                            gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, 0, null)
                        }
                        return
                    }

                    if (PeatDeltaDocument.isDeltaDocument(value)) {
                        // Delta document (0xB2) - find peer by address, fall back to nodeId mapping
                        var peer = peers.values.find { it.address == address }
                        if (peer == null) {
                            val knownNodeId = addressToNodeId[address]
                            peer = knownNodeId?.let { peers[it] }
                            peer?.also { it.address = address }
                        }
                        if (peer != null) {
                            handlePeerDeltaDocument(peer, value)
                        } else {
                            Log.w(TAG, "Received delta from unknown peer $address")
                        }
                        if (responseNeeded) {
                            gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, 0, null)
                        }
                        return
                    }

                    if (value.isNotEmpty() && value[0] == APP_LAYER_MARKER) {
                        // app-layer message (0xAF) - hive-lite tactical messaging
                        val peer = peers.values.find { it.address == address }
                            ?: PeatPeer(
                                nodeId = 0,
                                address = address,
                                name = "Unknown",
                                meshId = meshId,
                                rssi = 0,
                                isConnected = true,
                                lastDocument = null,
                                lastSeen = System.currentTimeMillis()
                            )
                        handleAppLayerMessage(peer, value)
                        if (responseNeeded) {
                            gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, 0, null)
                        }
                        return
                    }

                    // Parse the document (regular PeatDocument format)
                    val document = PeatDocument.decode(value)
                    if (document != null) {
                        Log.i(TAG, "Received document from ${String.format("%08X", document.nodeId)}, event=${document.currentEventType()}")

                        // Handle the document - find or create peer
                        val sourceNodeId = document.nodeId
                        if (sourceNodeId != nodeId && sourceNodeId != 0L) {
                            // Find existing peer or create new one
                            var peer = peers.values.find { it.address == address }
                            if (peer == null) {
                                // Check if we know this node by nodeId
                                peer = peers[sourceNodeId]
                            }

                            if (peer == null) {
                                // New peer from incoming connection
                                // Set lastDocument = null so the first event triggers onPeerEvent
                                val peerName = generateDeviceName(meshId, sourceNodeId)
                                val now = System.currentTimeMillis()
                                peer = PeatPeer(
                                    nodeId = sourceNodeId,
                                    address = address,
                                    name = peerName,
                                    meshId = meshId,
                                    rssi = 0,
                                    isConnected = true,
                                    lastDocument = null,
                                    lastSeen = now
                                )
                                peers[sourceNodeId] = peer
                                addressToNodeId[address] = sourceNodeId
                                peerLifetimeManager?.onPeerActivity(address, true)
                                Log.i(TAG, "Added peer from GATT write: ${peer.displayName()}")

                                // Update PeatMesh — use onIncomingConnection() which creates
                                // the peer and marks it connected in one atomic call
                                _mesh?.onIncomingConnection(address, sourceNodeId.toUInt(), now.toULong())
                            } else {
                                // Update existing peer
                                val now = System.currentTimeMillis()
                                if (peer.nodeId != sourceNodeId) {
                                    // NodeId changed - update mapping
                                    peers.remove(peer.nodeId)
                                    val updatedPeer = peer.copy(nodeId = sourceNodeId)
                                    peers[sourceNodeId] = updatedPeer
                                    peer = updatedPeer
                                }
                                // Always update address mapping - central may connect with different
                                // address than scan address due to BLE address randomization
                                addressToNodeId[address] = sourceNodeId

                                // Update PeatMesh — onIncomingConnection() creates the peer if
                                // needed (e.g. after mesh recreation) and marks it connected
                                _mesh?.onIncomingConnection(address, sourceNodeId.toUInt(), now.toULong())
                                Log.d(TAG, "[GATT-SERVER] Registered incoming peer: ${peer.displayName()}")
                            }

                            // Handle document content (pass raw bytes for CRDT merge)
                            handlePeerDocumentInternal(peer, document, value, address)
                        }
                    } else if (value.isNotEmpty() && value[0] == 0xAE.toByte()) {
                        // Encrypted document (0xAE marker) - pass directly to native mesh for decryption
                        Log.d(TAG, "[ENCRYPTED] Received ${value.size} byte encrypted document from $address")

                        val now = System.currentTimeMillis()

                        // Use anonymous decryption path - decrypts first, extracts source_node from
                        // decrypted document header, and registers the identifier->nodeId mapping.
                        // This handles BLE address rotation where peer connects from different address.
                        val result = _mesh?.onBleDataReceivedAnonymous(address, value, now.toULong())
                        if (result != null) {
                            Log.i(TAG, "[ENCRYPTED-MERGE] sourceNode=${String.format("%08X", result.sourceNode.toLong())}, isAck=${result.isAck}, counterChanged=${result.counterChanged}, total=${result.totalCount}")

                            // Update peer mapping with source node from decrypted document
                            val sourceNodeId = result.sourceNode.toLong()
                            if (sourceNodeId != 0L && sourceNodeId != nodeId) {
                                addressToNodeId[address] = sourceNodeId
                                deduplicateConnectedCentrals(address, sourceNodeId)
                                var peer = peers[sourceNodeId]
                                if (peer == null) {
                                    // New peer discovered through encrypted document
                                    val peerName = generateDeviceName(meshId, sourceNodeId)
                                    peer = PeatPeer(
                                        nodeId = sourceNodeId,
                                        address = address,
                                        name = peerName,
                                        meshId = meshId,
                                        rssi = 0,
                                        isConnected = true,
                                        lastDocument = null,
                                        lastSeen = now
                                    )
                                    peers[sourceNodeId] = peer
                                    peerLifetimeManager?.onPeerActivity(address, true)
                                    Log.i(TAG, "[ENCRYPTED] Added peer from encrypted doc: ${peer.displayName()}")
                                    // Update native mesh — incoming connection creates peer + marks connected
                                    _mesh?.onIncomingConnection(address, sourceNodeId.toUInt(), now.toULong())
                                    // Notify listener about new peer - triggers platform creation
                                    handler.post {
                                        meshListener?.onPeerConnected(peer)
                                    }
                                    // Notify mesh updated so UI reflects the new peer
                                    notifyMeshUpdated()
                                } else {
                                    // Existing peer - update last seen and ensure listener is notified
                                    peer.lastSeen = now
                                    peer.isConnected = true
                                    peerLifetimeManager?.onPeerActivity(address, true)
                                    resetReconnectTracking(address)
                                }

                                // Check for relayData containing app-layer message (0xAF marker)
                                // app-layer messages are app-layer protocol; peat-btle just transports them
                                val relay = result.relayData
                                if (relay != null && relay.isNotEmpty() && relay[0] == APP_LAYER_MARKER) {
                                    Log.i(TAG, "[ENCRYPTED-CANNED] app-layer message ${relay.size} bytes from ${peer.displayName()}")
                                    handleAppLayerMessage(peer, relay)
                                }

                                // Check for ACK/emergency events (server callback path)
                                // ACK can come either as emergency ACK (is_ack flag) or peripheral event (eventType=6)
                                if (result.isAck || result.eventType == EventType.ACK) {
                                    Log.i(TAG, "[ENCRYPTED-SERVER] ACK received from ${peer.displayName()} (isAck=${result.isAck}, eventType=${result.eventType})")
                                    handler.post {
                                        meshListener?.onPeerEvent(peer, PeatEventType.ACK)
                                    }
                                }
                                if (result.isEmergency || result.eventType == EventType.EMERGENCY) {
                                    Log.i(TAG, "[ENCRYPTED-SERVER] EMERGENCY from ${peer.displayName()} (isEmergency=${result.isEmergency}, eventType=${result.eventType})")
                                    handler.post {
                                        meshListener?.onPeerEvent(peer, PeatEventType.EMERGENCY)
                                        onPeerEmergencyDetected(peer)
                                    }
                                }

                                // Notify document synced callback with peripheral data from result
                                // Build PeatPeripheral from DataReceivedResult fields
                                val eventType = result.eventType?.let { PeatEventType.fromEventType(it) } ?: PeatEventType.NONE
                                val lat = result.latitude
                                val lon = result.longitude
                                val alt = result.altitude
                                val peerPeripheral = PeatPeripheral(
                                    id = sourceNodeId,
                                    parentNode = sourceNodeId,
                                    peripheralType = PeatPeripheralType.SOLDIER_SENSOR,
                                    callsign = result.callsign ?: "",
                                    health = PeatHealthStatus(
                                        batteryPercent = result.batteryPercent?.toInt() ?: 0,
                                        heartRate = result.heartRate?.toInt(),
                                        activityLevel = 0,
                                        alerts = 0
                                    ),
                                    lastEvent = if (eventType != PeatEventType.NONE)
                                        PeatPeripheralEvent(eventType, System.currentTimeMillis()) else null,
                                    location = if (lat != null && lon != null)
                                        PeatLocation(lat, lon, alt ?: 0f) else null,
                                    timestamp = System.currentTimeMillis()
                                )
                                // Update callsign cache if we received a valid callsign
                                result.callsign?.let { updateCallsignForNode(sourceNodeId, it) }

                                // Include peripheral if ANY data is present (callsign, location, battery, etc.)
                                val hasPeripheralData = result.callsign != null ||
                                    result.latitude != null ||
                                    result.batteryPercent != null ||
                                    result.heartRate != null ||
                                    result.eventType != null
                                val syntheticDoc = PeatDocument(
                                    version = 1,
                                    nodeId = sourceNodeId,
                                    counter = emptyList(),
                                    peripheral = if (hasPeripheralData) peerPeripheral else null
                                )
                                handler.post {
                                    meshListener?.onDocumentSynced(syntheticDoc)
                                }
                            }
                        } else {
                            Log.w(TAG, "[ENCRYPTED-MERGE] onBleDataReceived returned null - decryption may have failed")
                        }
                    } else {
                        Log.w(TAG, "Failed to decode document from $address (${value.size} bytes, first byte: ${if (value.isNotEmpty()) String.format("0x%02X", value[0]) else "empty"})")
                    }

                    if (responseNeeded) {
                        gattServer?.sendResponse(
                            device,
                            requestId,
                            BluetoothGatt.GATT_SUCCESS,
                            0,
                            null
                        )
                    }
                } else {
                    if (responseNeeded) {
                        gattServer?.sendResponse(
                            device,
                            requestId,
                            BluetoothGatt.GATT_REQUEST_NOT_SUPPORTED,
                            0,
                            null
                        )
                    }
                }
            } catch (e: SecurityException) {
                Log.e(TAG, "Missing permission in onCharacteristicWriteRequest", e)
            }
        }

        override fun onDescriptorReadRequest(
            device: BluetoothDevice,
            requestId: Int,
            offset: Int,
            descriptor: BluetoothGattDescriptor
        ) {
            try {
                if (descriptor.uuid == CCCD_UUID) {
                    val value = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
                    gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, 0, value)
                } else {
                    gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_REQUEST_NOT_SUPPORTED, 0, null)
                }
            } catch (e: SecurityException) {
                Log.e(TAG, "Missing permission in onDescriptorReadRequest", e)
            }
        }

        override fun onDescriptorWriteRequest(
            device: BluetoothDevice,
            requestId: Int,
            descriptor: BluetoothGattDescriptor,
            preparedWrite: Boolean,
            responseNeeded: Boolean,
            offset: Int,
            value: ByteArray?
        ) {
            try {
                val address = device.address
                if (descriptor.uuid == CCCD_UUID) {
                    // Client is subscribing to notifications
                    val enabled = value?.contentEquals(BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE) == true
                    Log.i(TAG, "Notification ${if (enabled) "enabled" else "disabled"} for $address")

                    if (responseNeeded) {
                        gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, 0, null)
                    }
                } else {
                    if (responseNeeded) {
                        gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_REQUEST_NOT_SUPPORTED, 0, null)
                    }
                }
            } catch (e: SecurityException) {
                Log.e(TAG, "Missing permission in onDescriptorWriteRequest", e)
            }
        }

        override fun onServiceAdded(status: Int, service: BluetoothGattService) {
            if (status == BluetoothGatt.GATT_SUCCESS) {
                Log.i(TAG, "GATT service added: ${service.uuid}")
            } else {
                Log.e(TAG, "Failed to add GATT service: status=$status")
            }
        }

        override fun onMtuChanged(device: BluetoothDevice, mtu: Int) {
            try {
                Log.d(TAG, "MTU changed to $mtu for ${device.address}")
            } catch (e: SecurityException) {
                Log.e(TAG, "Missing permission in onMtuChanged", e)
            }
        }
    }

    /**
     * Connect to a Peat device by address.
     *
     * @param address Bluetooth device address (MAC)
     * @param autoConnect Use autoConnect mode (reconnect automatically)
     * @return Connection handle, or null if connection failed
     */
    fun connect(address: String, autoConnect: Boolean = false): PeatConnection? {
        checkInitialized()

        if (connections.containsKey(address)) {
            Log.w(TAG, "Already connected to $address")
            return null
        }

        val adapter = bluetoothAdapter
            ?: throw IllegalStateException("Bluetooth adapter not available")

        try {
            val device = adapter.getRemoteDevice(address)
            val connectionId = connectionIdCounter.incrementAndGet()
            val callback = GattCallbackProxy(connectionId)

            val gatt = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                device.connectGatt(context, autoConnect, callback, BluetoothDevice.TRANSPORT_LE)
            } else {
                device.connectGatt(context, autoConnect, callback)
            }

            if (gatt != null) {
                connections[address] = gatt
                gattCallbacks[address] = callback
                Log.i(TAG, "Connecting to $address")
                return PeatConnection(address, gatt, callback)
            }
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_CONNECT permission", e)
            throw e
        } catch (e: IllegalArgumentException) {
            Log.e(TAG, "Invalid address: $address", e)
        }

        return null
    }

    /**
     * Disconnect from a device.
     *
     * @param address Device address to disconnect
     */
    fun disconnect(address: String) {
        val gatt = connections.remove(address)
        gattCallbacks.remove(address)
        writeQueues.remove(address)
        writeInProgress.remove(address)

        try {
            gatt?.disconnect()
            gatt?.close()
            Log.i(TAG, "Disconnected from $address")
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_CONNECT permission", e)
        }
    }

    // ==================== Mesh Management API ====================

    /**
     * Start the Peat mesh network.
     *
     * This starts scanning, advertising, and automatically manages
     * connections to discovered Peat peers. The mesh handles document
     * synchronization automatically.
     *
     * @param listener Callback for mesh events (peer updates, events)
     */
    fun startMesh(listener: PeatMeshListener) {
        checkInitialized()

        if (isMeshRunning) {
            Log.w(TAG, "Mesh already running")
            return
        }

        meshListener = listener
        isMeshRunning = true

        // Initialize Rust-backed managers with Kotlin config values
        reconnectionManager = RustReconnectionManager(buildReconnectionConfig())
        peerLifetimeManager = RustPeerLifetimeManager(RustPeerLifetimeConfig(
            disconnectedTimeoutMs = PEER_TIMEOUT_MS.toULong(),
            connectedTimeoutMs = CONNECTED_PEER_TIMEOUT_MS.toULong(),
            cleanupIntervalMs = CLEANUP_INTERVAL_MS.toULong()
        ))

        // Start GATT server first (so iOS can connect to us)
        try {
            startGattServer()
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start GATT server", e)
        }

        // Start advertising
        try {
            startAdvertising()
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start advertising", e)
        }

        // Start scanning with internal handler
        try {
            startScan { device -> onDeviceDiscovered(device) }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start scanning", e)
        }

        // Start periodic tasks
        handler.post(cleanupRunnable)
        handler.postDelayed(syncRunnable, SYNC_INTERVAL_MS)
        handler.postDelayed(reconnectRunnable, RECONNECT_INTERVAL_MS)
        handler.postDelayed(scanRestartRunnable, SCAN_RESTART_INTERVAL_MS)

        // Start RSSI polling if enabled
        if (RSSI_POLLING_INTERVAL_MS > 0) {
            handler.postDelayed(rssiPollingRunnable, RSSI_POLLING_INTERVAL_MS)
            Log.i(TAG, "RSSI polling enabled: interval=${RSSI_POLLING_INTERVAL_MS}ms")
        }

        Log.i(TAG, "Mesh started for PEAT-${String.format("%08X", nodeId)} with GATT server")
    }

    /**
     * Stop the Peat mesh network.
     */
    fun stopMesh() {
        if (!isMeshRunning) return

        isMeshRunning = false
        handler.removeCallbacks(cleanupRunnable)
        handler.removeCallbacks(syncRunnable)
        handler.removeCallbacks(reconnectRunnable)
        handler.removeCallbacks(scanRestartRunnable)
        handler.removeCallbacks(highPriorityAutoDisableRunnable)
        handler.removeCallbacks(rssiPollingRunnable)
        reconnectionManager?.clear()
        reconnectionManager = null
        peerLifetimeManager?.clear()
        peerLifetimeManager = null
        reconnectGraceRunnables.values.forEach { handler.removeCallbacks(it) }
        reconnectGraceRunnables.clear()
        discoveryCallback = null

        // Reset high-priority mode on mesh stop
        if (_highPriorityConfig.enabled) {
            setHighPriorityMode(false, "mesh stopped")
        }

        stopScan()
        stopAdvertising()
        pauseGattServer()

        // Disconnect all peers
        for (address in connections.keys.toList()) {
            disconnect(address)
        }

        peers.clear()
        addressToNodeId.clear()
        nameToNodeId.clear()
        meshListener = null

        Log.i(TAG, "Mesh stopped")
    }

    // ==================== High-Priority Sync Mode API ====================

    /**
     * Get the current high-priority mode configuration.
     */
    fun getHighPriorityConfig(): HighPriorityConfig = _highPriorityConfig

    /**
     * Check if high-priority mode is currently enabled.
     */
    fun isHighPriorityMode(): Boolean = _highPriorityConfig.enabled

    /**
     * Set the high-priority mode listener.
     */
    fun setHighPriorityModeListener(listener: HighPriorityModeListener?) {
        highPriorityListener = listener
    }

    /**
     * Enable or disable high-priority sync mode.
     *
     * High-priority mode trades battery life for communication reliability:
     * - Faster sync intervals (1s vs 3s)
     * - Faster reconnection (1s polling vs 3s)
     * - More frequent scan restarts (30s vs 2min)
     * - Shorter reconnection delays
     *
     * @param enabled Whether to enable high-priority mode
     * @param reason Reason for the change (for logging/UI)
     */
    fun setHighPriorityMode(enabled: Boolean, reason: String = "user") {
        if (_highPriorityConfig.enabled == enabled) return

        Log.i(TAG, "[HIGH-PRIORITY] ${if (enabled) "ENABLED" else "DISABLED"} - reason: $reason")

        _highPriorityConfig = _highPriorityConfig.copy(enabled = enabled)

        if (enabled) {
            highPriorityEnabledAt = System.currentTimeMillis()

            // Schedule auto-disable if configured
            _highPriorityConfig.autoDisableAfterMs?.let { timeout ->
                handler.removeCallbacks(highPriorityAutoDisableRunnable)
                handler.postDelayed(highPriorityAutoDisableRunnable, timeout)
                Log.d(TAG, "[HIGH-PRIORITY] Auto-disable scheduled in ${timeout / 60000} minutes")
            }

            // Request high connection priority for all connected peers
            requestHighConnectionPriority()
        } else {
            highPriorityEnabledAt = null
            handler.removeCallbacks(highPriorityAutoDisableRunnable)

            // Restore balanced connection priority
            requestBalancedConnectionPriority()
        }

        // Recreate reconnection manager with updated config (flat delay / reset behavior changes)
        reconnectionManager = RustReconnectionManager(buildReconnectionConfig())

        // Notify listener
        highPriorityListener?.onHighPriorityModeChanged(enabled, reason)

        // Reschedule periodic tasks with new intervals
        if (isMeshRunning) {
            reschedulePeriodicTasks()
        }
    }

    /**
     * Update high-priority mode configuration.
     *
     * @param config New configuration
     */
    fun updateHighPriorityConfig(config: HighPriorityConfig) {
        val wasEnabled = _highPriorityConfig.enabled
        _highPriorityConfig = config

        // Handle enable state change
        if (config.enabled != wasEnabled) {
            setHighPriorityMode(config.enabled, "config update")
        } else if (config.enabled && config.autoDisableAfterMs != null) {
            // Reschedule auto-disable with new timeout
            handler.removeCallbacks(highPriorityAutoDisableRunnable)
            val elapsed = System.currentTimeMillis() - (highPriorityEnabledAt ?: System.currentTimeMillis())
            val remaining = config.autoDisableAfterMs - elapsed
            if (remaining > 0) {
                handler.postDelayed(highPriorityAutoDisableRunnable, remaining)
            }
        }
    }

    /**
     * Called when a peer enters emergency state.
     * Auto-enables high-priority mode if configured.
     */
    internal fun onPeerEmergencyDetected(peer: PeatPeer) {
        if (_highPriorityConfig.autoEnableOnEmergency && !_highPriorityConfig.enabled) {
            Log.w(TAG, "[HIGH-PRIORITY] Auto-enabling due to peer emergency: ${peer.displayName()}")
            setHighPriorityMode(true, "peer emergency: ${peer.displayName()}")
        }
    }

    /**
     * Build a ReconnectionConfig matching the current high-priority mode state.
     */
    private fun buildReconnectionConfig(): RustReconnectionConfig {
        return RustReconnectionConfig(
            baseDelayMs = RECONNECT_BASE_DELAY_MS.toULong(),
            maxDelayMs = RECONNECT_MAX_DELAY_MS.toULong(),
            maxAttempts = RECONNECT_MAX_ATTEMPTS.toUInt(),
            checkIntervalMs = RECONNECT_INTERVAL_MS.toULong(),
            useFlatDelay = _highPriorityConfig.enabled,
            resetOnExhaustion = _highPriorityConfig.enabled
        )
    }

    /**
     * Request high connection priority for all connected GATT connections.
     */
    private fun requestHighConnectionPriority() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
            connections.values.forEach { gatt ->
                try {
                    gatt.requestConnectionPriority(BluetoothGatt.CONNECTION_PRIORITY_HIGH)
                } catch (e: SecurityException) {
                    Log.w(TAG, "Failed to request high connection priority: ${e.message}")
                }
            }
        }
    }

    /**
     * Request balanced connection priority for all connected GATT connections.
     */
    private fun requestBalancedConnectionPriority() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
            connections.values.forEach { gatt ->
                try {
                    gatt.requestConnectionPriority(BluetoothGatt.CONNECTION_PRIORITY_BALANCED)
                } catch (e: SecurityException) {
                    Log.w(TAG, "Failed to request balanced connection priority: ${e.message}")
                }
            }
        }
    }

    /**
     * Reschedule periodic tasks with current interval settings.
     */
    private fun reschedulePeriodicTasks() {
        // Remove existing callbacks
        handler.removeCallbacks(syncRunnable)
        handler.removeCallbacks(reconnectRunnable)
        handler.removeCallbacks(scanRestartRunnable)

        // Reschedule with new intervals
        handler.postDelayed(syncRunnable, SYNC_INTERVAL_MS)
        handler.postDelayed(reconnectRunnable, RECONNECT_INTERVAL_MS)
        handler.postDelayed(scanRestartRunnable, SCAN_RESTART_INTERVAL_MS)

        Log.d(TAG, "[HIGH-PRIORITY] Rescheduled tasks - sync=${SYNC_INTERVAL_MS}ms, reconnect=${RECONNECT_INTERVAL_MS}ms, scanRestart=${SCAN_RESTART_INTERVAL_MS}ms")

        // Also reschedule RSSI polling if enabled
        handler.removeCallbacks(rssiPollingRunnable)
        if (RSSI_POLLING_INTERVAL_MS > 0) {
            handler.postDelayed(rssiPollingRunnable, RSSI_POLLING_INTERVAL_MS)
            Log.d(TAG, "[HIGH-PRIORITY] RSSI polling interval: ${RSSI_POLLING_INTERVAL_MS}ms")
        }
    }

    // ==================== RSSI Polling API ====================

    /**
     * Set the RSSI polling interval for connected peers.
     *
     * When enabled (interval > 0), periodically reads RSSI from all connected
     * GATT connections. Useful for field testing and debugging range issues.
     *
     * Note: In high-priority mode, the interval is capped at 2 seconds.
     *
     * @param intervalMs Polling interval in milliseconds (0 = disabled)
     */
    fun setRssiPollingInterval(intervalMs: Long) {
        val wasEnabled = _rssiPollingIntervalMs > 0
        _rssiPollingIntervalMs = maxOf(0, intervalMs)
        val isEnabled = _rssiPollingIntervalMs > 0

        Log.i(TAG, "[RSSI-POLL] Interval set to ${_rssiPollingIntervalMs}ms (effective: ${RSSI_POLLING_INTERVAL_MS}ms)")

        // Update runnable if mesh is running
        if (isMeshRunning) {
            handler.removeCallbacks(rssiPollingRunnable)
            if (isEnabled) {
                handler.postDelayed(rssiPollingRunnable, RSSI_POLLING_INTERVAL_MS)
            }
        }
    }

    /**
     * Get the current RSSI polling interval.
     *
     * @return Polling interval in milliseconds (0 = disabled)
     */
    fun getRssiPollingInterval(): Long = _rssiPollingIntervalMs

    /**
     * Check if RSSI polling is enabled.
     */
    fun isRssiPollingEnabled(): Boolean = _rssiPollingIntervalMs > 0

    // ==================== Peripheral State API ====================

    /**
     * Update local peripheral state for document sync.
     *
     * This sets the peripheral data that will be included in documents
     * sent during periodic syncWithPeers(). Unlike sendEvent(), this does
     * NOT trigger an immediate send - the next sync cycle will include
     * the updated peripheral data.
     *
     * @param callsign Device callsign (max 12 chars)
     * @param batteryPercent Battery level 0-100
     * @param heartRate Optional heart rate
     * @param location Optional location
     * @param eventType Optional event type (PING, ACK, etc.)
     */
    fun updatePeripheralState(
        callsign: String = "ANDROID",
        batteryPercent: Int = 100,
        heartRate: Int? = null,
        location: PeatLocation? = null,
        eventType: PeatEventType? = null
    ) {
        val peripheral = PeatPeripheral(
            id = nodeId,
            parentNode = 0,
            peripheralType = PeatPeripheralType.SOLDIER_SENSOR,
            callsign = callsign.take(12),
            health = PeatHealthStatus(batteryPercent, heartRate, 0, 0),
            lastEvent = eventType?.let { PeatPeripheralEvent(it, System.currentTimeMillis()) },
            location = location,
            timestamp = System.currentTimeMillis()
        )

        localPeripheral = peripheral
        Log.d(TAG, "Updated peripheral state: callsign=$callsign, battery=$batteryPercent%, " +
                "location=${location?.let { "(${it.latitude}, ${it.longitude})" } ?: "null"}, " +
                "event=$eventType")
    }

    /**
     * Send an event to all peers in the mesh.
     *
     * @param eventType The event to broadcast
     * @param location Optional location to include in the broadcast
     * @param callsign Optional callsign to include
     * @param battery Optional battery percentage (0-100)
     * @param heartRate Optional heart rate
     */
    fun sendEvent(
        eventType: PeatEventType,
        location: PeatLocation? = null,
        callsign: String = "ANDROID",
        battery: Int = 100,
        heartRate: Int? = null
    ) {
        if (!isMeshRunning) {
            Log.w(TAG, "Mesh not running, cannot send event")
            return
        }

        val isEmergency = eventType == PeatEventType.EMERGENCY || eventType == PeatEventType.ACK
        Log.i(TAG, "Broadcasting event: $eventType to ${connections.size} peripherals and ${connectedCentrals.size} centrals" +
                (location?.let { " with location (${it.latitude}, ${it.longitude})" } ?: "") +
                if (isEmergency) " [EMERGENCY - FULL DOCUMENT]" else "")

        // Increment our counter
        incrementLocalCounter()

        // Create peripheral with current state and store it for future sync cycles
        val peripheral = PeatPeripheral(
            id = nodeId,
            parentNode = 0,
            peripheralType = PeatPeripheralType.SOLDIER_SENSOR,
            callsign = callsign.take(12),
            health = PeatHealthStatus(battery, heartRate, 0, 0),
            lastEvent = PeatPeripheralEvent(eventType, System.currentTimeMillis()),
            location = location,
            timestamp = System.currentTimeMillis()
        )

        // Store the peripheral so syncWithPeers() and read requests use it
        localPeripheral = peripheral

        // Emergency events always use full documents for reliability
        // Also reset peer sync state to ensure next sync sends full update
        if (isEmergency) {
            peerSyncState.clear()
            Log.d(TAG, "Emergency bypass: cleared peer sync state for full document sync")
        }

        // When encryption is enabled, use native mesh for document building
        // This ensures documents have correct format (0xAE encrypted header)
        val documentBytes = if (isEncryptionEnabled()) {
            val mesh = _mesh
            if (mesh == null) {
                Log.w(TAG, "Encryption enabled but mesh not initialized, using unencrypted encoding")
                PeatDocument.encode(nodeId, localCounter, peripheral)
            } else {
                // Update native peripheral state BEFORE building document
                // This ensures location, callsign, and other state is included in encrypted docs
                val nativeEventType: EventType? = when (eventType) {
                    PeatEventType.NONE -> EventType.NONE
                    PeatEventType.PING -> EventType.PING
                    PeatEventType.NEED_ASSIST -> EventType.NEED_ASSIST
                    PeatEventType.EMERGENCY -> EventType.EMERGENCY
                    PeatEventType.MOVING -> EventType.MOVING
                    PeatEventType.IN_POSITION -> EventType.IN_POSITION
                    PeatEventType.ACK -> EventType.ACK
                }
                mesh.updatePeripheralState(
                    callsign = callsign.take(12),
                    batteryPercent = battery.coerceIn(0, 255).toUByte(),
                    heartRate = heartRate?.coerceIn(0, 255)?.toUByte(),
                    latitude = location?.latitude?.toFloat(),
                    longitude = location?.longitude?.toFloat(),
                    altitude = location?.altitude?.toFloat(),
                    eventType = nativeEventType,
                    timestampMs = System.currentTimeMillis().toULong()
                )
                Log.d(TAG, "[ENCRYPTED] Updated native peripheral state: location=${location != null}, callsign=$callsign")

                when (eventType) {
                    PeatEventType.EMERGENCY -> {
                        Log.d(TAG, "[ENCRYPTED] Using native sendEmergency")
                        mesh.sendEmergency(System.currentTimeMillis().toULong())
                    }
                    PeatEventType.ACK -> {
                        Log.d(TAG, "[ENCRYPTED] Using native sendAck")
                        mesh.sendAck(System.currentTimeMillis().toULong())
                    }
                    else -> {
                        // For non-emergency events, use buildDocument which includes all CRDT state
                        Log.d(TAG, "[ENCRYPTED] Using native buildDocument for event type: $eventType")
                        mesh.buildDocument()
                    }
                }
            }
        } else {
            PeatDocument.encode(nodeId, localCounter, peripheral)
        }

        // Send to all connected peripherals (devices we connected to as Central)
        for ((address, gatt) in connections) {
            writeDocumentToGatt(gatt, documentBytes)
        }

        // Send to all connected centrals (devices that connected to us as Peripheral)
        notifyConnectedCentrals(documentBytes)
    }

    /**
     * Send a map marker to all connected peers.
     *
     * Markers are sent as a separate marker document (0xAC format) to avoid
     * interfering with the regular track sync. The receiving peer will call
     * onMarkerSynced on the listener.
     *
     * @param marker The marker to send
     */
    fun sendMarker(marker: PeatMarker) {
        if (!isMeshRunning) {
            Log.w(TAG, "Mesh not running, cannot send marker")
            return
        }

        Log.i(TAG, "Broadcasting marker: uid=${marker.uid}, callsign=${marker.callsign} to ${connections.size} peripherals and ${connectedCentrals.size} centrals")

        // Encode marker document: 0xAC marker + nodeId(4) + count(2) + marker data
        val markerBytes = PeatMarker.encode(marker)
        val documentBytes = ByteArray(1 + 4 + 2 + markerBytes.size)
        var offset = 0

        documentBytes[offset++] = MARKER_SECTION_MARKER
        // Write nodeId (4 bytes LE)
        documentBytes[offset++] = (nodeId and 0xFF).toByte()
        documentBytes[offset++] = ((nodeId shr 8) and 0xFF).toByte()
        documentBytes[offset++] = ((nodeId shr 16) and 0xFF).toByte()
        documentBytes[offset++] = ((nodeId shr 24) and 0xFF).toByte()
        // Write marker count (2 bytes LE)
        documentBytes[offset++] = 1.toByte()  // Single marker
        documentBytes[offset++] = 0.toByte()
        // Copy marker data
        markerBytes.copyInto(documentBytes, offset)

        // Send to all connected peripherals
        for ((address, gatt) in connections) {
            writeDocumentToGatt(gatt, documentBytes)
        }

        // Send to all connected centrals
        notifyConnectedCentrals(documentBytes)
    }

    /**
     * Broadcast raw bytes to all connected peers.
     *
     * Takes raw payload bytes, encrypts them (if encryption is enabled),
     * and sends to all connected peripherals and centrals.
     *
     * This is useful for sending extension data like CannedMessages from hive-lite.
     *
     * @param payload The raw bytes to broadcast
     */
    fun broadcastBytes(payload: ByteArray) {
        if (!isMeshRunning) {
            Log.w(TAG, "Mesh not running, cannot broadcast bytes")
            return
        }

        val mesh = _mesh
        if (mesh == null) {
            Log.w(TAG, "Mesh not initialized, cannot broadcast bytes")
            return
        }

        // Encrypt the payload
        val docBytes = mesh.broadcastBytes(payload)

        Log.i(TAG, "[BROADCAST] Sending ${docBytes.size} bytes to ${connections.size} peripherals and ${connectedCentrals.size} centrals")

        // Send to all connected peripherals
        for ((_, gatt) in connections) {
            writeDocumentToGatt(gatt, docBytes)
        }

        // Send to all connected centrals
        notifyConnectedCentrals(docBytes)
    }

    /**
     * Store a CannedMessage document for CRDT sync.
     *
     * Takes raw hive-lite encoded bytes (including 0xAF marker).
     * The document will be stored and synced to peers via delta sync.
     *
     * @param encodedBytes The hive-lite encoded CannedMessageAckEvent bytes
     * @return true if the document was newly added or changed via merge
     */
    fun storeCannedMessageDocument(encodedBytes: ByteArray): Boolean {
        val mesh = _mesh
        if (mesh == null) {
            Log.w(TAG, "Mesh not initialized, cannot store canned message")
            return false
        }
        val result = mesh.storeCannedMessageDocument(encodedBytes)
        Log.d(TAG, "[CANNED-MSG] Stored document: ${encodedBytes.size} bytes, changed=$result")
        return result
    }

    /**
     * Record an ACK on a stored CannedMessage document.
     *
     * @param sourceNode The source node that created the document
     * @param timestamp The document timestamp
     * @param ackerNode The node recording the ACK
     * @param ackTimestamp The ACK timestamp
     * @return true if the ACK was new (document changed)
     */
    fun ackCannedMessage(sourceNode: UInt, timestamp: ULong, ackerNode: UInt, ackTimestamp: ULong): Boolean {
        val mesh = _mesh
        if (mesh == null) {
            Log.w(TAG, "Mesh not initialized, cannot ACK canned message")
            return false
        }
        val result = mesh.ackCannedMessage(sourceNode, timestamp, ackerNode, ackTimestamp)
        Log.d(TAG, "[CANNED-MSG] ACK recorded: source=$sourceNode ts=$timestamp acker=$ackerNode, changed=$result")
        return result
    }

    /**
     * Get the number of stored app documents.
     */
    fun appDocumentCount(): Int {
        return (_mesh?.appDocumentCount() ?: 0u).toInt()
    }

    /**
     * Get all stored CannedMessage documents.
     *
     * Returns a list of CannedMessageInfo objects containing:
     * - sourceNode: The node that created the message
     * - timestamp: When the message was created
     * - encodedBytes: The hive-lite encoded bytes (with 0xAF marker)
     *
     * @return List of CannedMessageInfo, or empty list if mesh not initialized
     */
    fun getAllCannedMessages(): List<CannedMessageInfo> {
        return _mesh?.getAllCannedMessages() ?: emptyList()
    }

    /**
     * Get a specific CannedMessage document as encoded bytes.
     *
     * @param sourceNode The source node of the message
     * @param timestamp The timestamp of the message
     * @return The encoded bytes (with 0xAF marker), or null if not found
     */
    fun getCannedMessageDocument(sourceNode: UInt, timestamp: ULong): ByteArray? {
        return _mesh?.getCannedMessageDocument(sourceNode, timestamp)
    }

    /**
     * Get the current list of peers in the mesh.
     */
    fun getPeers(): List<PeatPeer> = peers.values.toList()

    /**
     * Get a specific peer by node ID.
     */
    fun getPeer(nodeId: Long): PeatPeer? = peers[nodeId]

    /**
     * Check if the mesh is running.
     */
    fun isMeshRunning(): Boolean = isMeshRunning

    // ==================== Internal Mesh Methods ====================

    private fun onDeviceDiscovered(device: DiscoveredDevice) {
        if (!device.isPeatDevice) return

        // Check if we already know this address (peer might have been renamed by document)
        val knownNodeId = addressToNodeId[device.address]
        if (knownNodeId != null) {
            peers[knownNodeId]?.let { peer ->
                peer.rssi = device.rssi
                peer.lastSeen = System.currentTimeMillis()
                peerLifetimeManager?.onPeerActivity(device.address, peer.isConnected)
                notifyMeshUpdated()

                // If peer is disconnected and re-discovered via scan, reconnect
                if (!peer.isConnected && !connections.containsKey(peer.address)) {
                    Log.i(TAG, "[SCAN-RECONNECT] Re-discovered disconnected peer ${peer.displayName()}, reconnecting")
                    resetReconnectTracking(peer.address)
                    connectToPeer(peer)
                }
            }
            return
        }

        // Handle BLE address rotation for ALL devices with stable names
        // This prevents duplicate peers when the same device advertises from a new MAC address
        // Works for: WearTAK (WEAROS-*), Peat devices (PEAT_*), and any device with consistent naming
        if (device.name.isNotEmpty()) {
            // O(1) lookup using nameToNodeId map
            val existingNodeId = nameToNodeId[device.name]
            if (existingNodeId != null) {
                val existingPeer = peers[existingNodeId]
                if (existingPeer != null) {
                    // Same device seen from new address - update mappings
                    val oldAddress = existingPeer.address

                    // Clean up old address mapping to prevent memory leak
                    if (oldAddress.isNotEmpty() && oldAddress != device.address) {
                        addressToNodeId.remove(oldAddress)
                        Log.d(TAG, "Address rotation: ${device.name} moved from $oldAddress to ${device.address}")
                    }

                    // Update peer with new address and RSSI
                    existingPeer.address = device.address
                    existingPeer.rssi = device.rssi
                    existingPeer.lastSeen = System.currentTimeMillis()
                    peerLifetimeManager?.onPeerActivity(device.address, existingPeer.isConnected)
                    addressToNodeId[device.address] = existingNodeId

                    Log.d(TAG, "Device ${device.name} seen from new address ${device.address}, mapped to existing nodeId ${String.format("%08X", existingNodeId)}")
                    notifyMeshUpdated()

                    // If peer is disconnected and re-discovered via scan, reconnect
                    if (!existingPeer.isConnected && !connections.containsKey(existingPeer.address)) {
                        Log.i(TAG, "[SCAN-RECONNECT] Re-discovered disconnected peer ${existingPeer.displayName()} (address rotated), reconnecting")
                        resetReconnectTracking(existingPeer.address)
                        connectToPeer(existingPeer)
                    }

                    return
                }
            }
        }

        // Derive nodeId from name, service data, or address for new peers
        val peerNodeId = device.nodeId ?: nativeDeriveNodeId(device.address)

        // Don't track ourselves
        if (peerNodeId == nodeId) return

        // Check mesh ID - only auto-connect to peers in the same mesh or legacy peers
        val sameMesh = matchesMesh(meshId, device.meshId)
        if (!sameMesh) {
            Log.d(TAG, "Skipping peer ${String.format("%08X", peerNodeId)} - different mesh (${device.meshId} != $meshId)")
            return
        }

        // Final deduplication check: see if we already have this nodeId
        // This catches cases where nodeId was derived differently but is actually the same device
        val existingPeer = peers[peerNodeId]
        if (existingPeer != null) {
            // Update existing peer - also update address if it changed
            val oldAddress = existingPeer.address
            if (oldAddress != device.address) {
                if (oldAddress.isNotEmpty()) {
                    addressToNodeId.remove(oldAddress)
                }
                existingPeer.address = device.address
                addressToNodeId[device.address] = peerNodeId
            }
            existingPeer.rssi = device.rssi
            existingPeer.lastSeen = System.currentTimeMillis()
            peerLifetimeManager?.onPeerActivity(device.address, existingPeer.isConnected)

            // Update name mapping if we have a name
            if (device.name.isNotEmpty()) {
                nameToNodeId[device.name] = peerNodeId
            }

            // CRITICAL: If peer is disconnected and re-discovered, reconnect!
            // This handles the case where a peer walked out of range, exhausted
            // all reconnection attempts, then came back in range.
            if (!existingPeer.isConnected && !connections.containsKey(existingPeer.address)) {
                Log.i(TAG, "Re-discovered disconnected peer ${existingPeer.displayName()}, attempting reconnect")
                resetReconnectTracking(existingPeer.address)
                connectToPeer(existingPeer)
            }

            notifyMeshUpdated()
            return
        }

        // New peer discovered
        val now = System.currentTimeMillis()
        // Use cached callsign if available, otherwise BLE name, otherwise generate
        val peerName = nodeIdToCallsign[peerNodeId]
            ?: device.name.ifEmpty { generateDeviceName(device.meshId ?: meshId, peerNodeId) }
        val peer = PeatPeer(
            nodeId = peerNodeId,
            address = device.address,
            name = peerName,
            meshId = device.meshId,
            rssi = device.rssi,
            isConnected = false,
            lastDocument = null,
            lastSeen = now
        )

        // Add to all maps
        peers[peerNodeId] = peer
        addressToNodeId[device.address] = peerNodeId
        if (peerName.isNotEmpty()) {
            nameToNodeId[peerName] = peerNodeId
        }

        peerLifetimeManager?.onPeerActivity(device.address, false)

        Log.i(TAG, "New peer discovered: ${peer.displayName()} (mesh: ${device.meshId ?: "legacy"})")

        // Auto-connect to new peer
        connectToPeer(peer)

        // Update PeatMesh ConnectionStateGraph
        _mesh?.onBleDiscovered(
            identifier = device.address,
            name = device.name.ifEmpty { null },
            rssi = device.rssi.coerceIn(-128, 127).toByte(),
            meshId = device.meshId,
            nowMs = now.toULong()
        )

        notifyMeshUpdated()
    }

    private fun connectToPeer(peer: PeatPeer) {
        if (connections.containsKey(peer.address)) {
            Log.d(TAG, "Already connected to ${peer.displayName()}")
            return
        }

        Log.i(TAG, "Connecting to peer: ${peer.displayName()}")

        val adapter = bluetoothAdapter ?: return

        try {
            val device = adapter.getRemoteDevice(peer.address)
            val connectionId = connectionIdCounter.incrementAndGet()
            val callback = GattCallbackProxy(connectionId)

            // Set up document listener
            callback.documentListener = object : PeatDocumentListener {
                override fun onDocumentReceived(data: ByteArray) {
                    handlePeerDocument(peer, data)
                }

                override fun onServicesDiscovered() {
                    Log.i(TAG, "Services discovered for ${peer.displayName()}")
                    peer.isConnected = true
                    notifyMeshUpdated()

                    // Enable notifications first, then read after a delay
                    connections[peer.address]?.let { gatt ->
                        enableNotificationsForGatt(gatt)
                        // Delay read to allow descriptor write to complete
                        handler.postDelayed({
                            readDocumentFromGatt(gatt)
                        }, 500)
                    }
                }

                override fun onConnectionStateChanged(connected: Boolean) {
                    Log.i(TAG, "Peer ${peer.displayName()} connected: $connected")
                    // Find the current peer entry (may have been updated with new nodeId)
                    val currentPeer = peers.values.find { it.address == peer.address }
                    if (currentPeer != null) {
                        currentPeer.isConnected = connected
                        // Only update lastSeen on successful connection, not disconnection.
                        // This allows stale peer cleanup to work after disconnect + failed reconnects.
                        if (connected) {
                            cancelReconnectGrace(currentPeer)
                            currentPeer.lastSeen = System.currentTimeMillis()
                            peerLifetimeManager?.onPeerActivity(peer.address, true)
                        } else {
                            startReconnectGrace(currentPeer)
                            peerLifetimeManager?.onPeerDisconnected(peer.address)
                            reconnectionManager?.trackDisconnection(peer.address)
                        }
                    }
                    if (connected) {
                        // Update PeatMesh ConnectionStateGraph
                        _mesh?.onBleConnected(peer.address, System.currentTimeMillis().toULong())
                        // Notify listener of peer connection for immediate UI update
                        currentPeer?.let { notifyPeerConnected(it) }
                        // Reset reconnection tracking on successful connection
                        resetReconnectTracking(peer.address)
                    } else {
                        // Update PeatMesh ConnectionStateGraph
                        _mesh?.onBleDisconnected(peer.address, DisconnectReason.LINK_LOSS)
                        // Notify listener of peer disconnection for immediate UI update
                        currentPeer?.let { notifyPeerDisconnected(it) }
                        connections.remove(peer.address)
                        gattCallbacks.remove(peer.address)
                        // Clean up write queue for disconnected peer
                        writeQueues.remove(peer.address)
                        writeInProgress.remove(peer.address)
                        // Note: reconnection is handled by reconnectLostPeers() with exponential backoff
                        Log.d(TAG, "Peer ${peer.displayName()} disconnected, will retry via reconnectLostPeers()")
                        // Immediate reconnect attempt for fast range-testing feedback
                        if (isMeshRunning) {
                            val reconnectPeer = currentPeer ?: peer
                            handler.postDelayed({
                                if (isMeshRunning && !reconnectPeer.isConnected &&
                                    !connections.containsKey(reconnectPeer.address)) {
                                    Log.i(TAG, "[RECONNECT-FAST] Immediate reconnect for ${reconnectPeer.displayName()}")
                                    resetReconnectTracking(reconnectPeer.address)
                                    try { connectToPeer(reconnectPeer) } catch (e: Exception) {
                                        Log.e(TAG, "[RECONNECT-FAST] Failed: ${e.message}")
                                    }
                                }
                            }, 200)
                        }
                    }
                    notifyMeshUpdated()
                }

                override fun onWriteComplete(success: Boolean) {
                    // Process next item in write queue
                    onWriteCompleteForConnection(peer.address)
                }

                override fun onRssiRead(rssi: Int) {
                    // Update peer RSSI from live polling
                    val currentPeer = peers.values.find { it.address == peer.address }
                    if (currentPeer != null && currentPeer.rssi != rssi) {
                        Log.v(TAG, "[RSSI] ${currentPeer.displayName()}: ${currentPeer.rssi} -> $rssi dBm")
                        currentPeer.rssi = rssi
                        currentPeer.lastSeen = System.currentTimeMillis()
                        peerLifetimeManager?.onPeerActivity(currentPeer.address, currentPeer.isConnected)
                        resetReconnectTracking(peer.address)
                        notifyMeshUpdated()
                    }
                }
            }

            val gatt = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                device.connectGatt(context, false, callback, BluetoothDevice.TRANSPORT_LE)
            } else {
                device.connectGatt(context, false, callback)
            }

            if (gatt != null) {
                connections[peer.address] = gatt
                gattCallbacks[peer.address] = callback
            }
        } catch (e: SecurityException) {
            Log.e(TAG, "Missing BLUETOOTH_CONNECT permission", e)
        } catch (e: Exception) {
            Log.e(TAG, "Failed to connect to peer", e)
        }
    }

    private fun handlePeerDocument(peer: PeatPeer, data: ByteArray) {
        val firstByte = if (data.isNotEmpty()) String.format("0x%02X", data[0]) else "empty"
        Log.i(TAG, "[DOC-RX] From ${peer.displayName()}: ${data.size} bytes, first=$firstByte")

        // Check for marker document (0xAC)
        if (data.isNotEmpty() && data[0] == MARKER_SECTION_MARKER) {
            handlePeerMarkerDocument(peer, data)
            return
        }

        // Check for chat document (0xAD)
        if (data.isNotEmpty() && data[0] == CHAT_SECTION_MARKER) {
            handlePeerChatDocument(peer, data)
            return
        }

        // Check for canned message (0xAF) - hive-lite tactical messaging
        if (data.isNotEmpty() && data[0] == APP_LAYER_MARKER) {
            handleAppLayerMessage(peer, data)
            return
        }

        // Check for delta document marker (0xB2)
        if (PeatDeltaDocument.isDeltaDocument(data)) {
            handlePeerDeltaDocument(peer, data)
            return
        }

        // Check for encrypted document marker (0xAE) - process via native mesh for decryption
        if (data.isNotEmpty() && data[0] == 0xAE.toByte()) {
            handlePeerEncryptedDocument(peer, data)
            return
        }

        val document = PeatDocument.decode(data) ?: return
        val docNodeId = document.nodeId

        Log.d(TAG, "Received document from ${peer.displayName()} (docNodeId=${String.format("%08X", docNodeId)}): event=${document.currentEventType()}")

        // Skip if document is from ourselves
        if (docNodeId == nodeId || docNodeId == 0L) return

        // Check if document is from the connected peer or relayed from another node
        val connectedPeer = peers.values.find { it.address == peer.address }

        if (connectedPeer != null && connectedPeer.nodeId == docNodeId) {
            // Document is from the directly connected peer
            handlePeerDocumentInternal(connectedPeer, document, data, peer.address)
        } else if (connectedPeer != null && connectedPeer.nodeId != docNodeId) {
            // Document is RELAYED through connectedPeer from a different originating node
            // Find or create peer entry for the originating nodeId
            var originatingPeer = peers[docNodeId]
            if (originatingPeer == null) {
                // Create a virtual peer for the relayed node (we don't have direct connection)
                originatingPeer = PeatPeer(
                    nodeId = docNodeId,
                    address = "", // No direct address - relayed via mesh
                    name = generateDeviceName(meshId, docNodeId),
                    meshId = meshId,
                    rssi = 0,
                    isConnected = false, // Not directly connected
                    lastDocument = null,
                    lastSeen = System.currentTimeMillis()
                )
                peers[docNodeId] = originatingPeer
                Log.i(TAG, "Created relayed peer ${originatingPeer.displayName()} (via ${connectedPeer.displayName()})")
            }
            // Process document for the originating peer
            Log.d(TAG, "Processing relayed document from ${originatingPeer.displayName()} via ${connectedPeer.displayName()}")
            handlePeerDocumentInternal(originatingPeer, document, data, peer.address)
        } else {
            // Fallback: peer not in our list yet, use document nodeId
            val newPeer = peers[docNodeId] ?: PeatPeer(
                nodeId = docNodeId,
                address = peer.address,
                name = peer.name.ifEmpty { generateDeviceName(meshId, docNodeId) },
                meshId = peer.meshId,
                rssi = peer.rssi,
                isConnected = peer.isConnected,
                lastDocument = null,
                lastSeen = System.currentTimeMillis()
            ).also { peers[docNodeId] = it }
            handlePeerDocumentInternal(newPeer, document, data, peer.address)
        }
    }

    /**
     * Process a document internally and forward to other connected peers.
     *
     * @param peer The peer that sent/originated this document
     * @param document The decoded document
     * @param rawBytes The raw bytes to forward (null to skip forwarding)
     * @param sourceAddress The BLE address of the peer we received this from (to exclude from forwarding)
     */
    private fun handlePeerDocumentInternal(
        peer: PeatPeer,
        document: PeatDocument,
        rawBytes: ByteArray? = null,
        sourceAddress: String? = null
    ) {
        // Store last document
        val previousEvent = peer.lastDocument?.peripheral?.lastEvent
        val previousEventType = previousEvent?.eventType ?: PeatEventType.NONE
        val previousEventTimestamp = previousEvent?.timestamp ?: 0L
        peer.lastDocument = document
        peer.lastSeen = System.currentTimeMillis()
        peerLifetimeManager?.onPeerActivity(peer.address, peer.isConnected)

        // Update callsign cache if document has callsign
        document.peripheral?.callsign?.takeIf { it.isNotEmpty() }?.let {
            updateCallsignForNode(document.nodeId, it)
        }

        // Merge counters (CRDT merge)
        mergeCounter(document.counter)

        // Ensure native mesh knows about this peer before CRDT merge
        // (Kotlin peers map survives across mesh recreation, but native peer_manager is fresh)
        if (sourceAddress != null && _mesh != null) {
            val now = System.currentTimeMillis()
            _mesh?.onIncomingConnection(sourceAddress, peer.nodeId.toUInt(), now.toULong())
            Log.d(TAG, "[CRDT-REGISTER] onIncomingConnection(addr=$sourceAddress, nodeId=${peer.nodeId})")
        }

        // Merge document into native CRDT
        Log.d(TAG, "[CRDT-DEBUG] rawBytes=${rawBytes?.size}, sourceAddress=$sourceAddress, _mesh=${_mesh != null}")
        if (rawBytes != null && rawBytes.isNotEmpty() && sourceAddress != null) {
            val result = _mesh?.onBleDataReceived(sourceAddress, rawBytes, System.currentTimeMillis().toULong())
            if (result != null) {
                Log.i(TAG, "[CRDT-MERGE] From ${peer.displayName()}: counterChanged=${result.counterChanged}, total=${result.totalCount}")
            } else {
                Log.w(TAG, "[CRDT-MERGE] onBleDataReceived returned null - native peer_manager may not know this peer")
            }
        } else {
            Log.w(TAG, "[CRDT-SKIP] Missing data: rawBytes=${rawBytes?.size}, sourceAddress=$sourceAddress")
        }

        // Check for new events - trigger if event type changed OR same type with newer timestamp
        val currentEvent = document.peripheral?.lastEvent
        val eventType = currentEvent?.eventType ?: PeatEventType.NONE
        val eventTimestamp = currentEvent?.timestamp ?: 0L
        val isNewEvent = eventType != PeatEventType.NONE && (
            eventType != previousEventType ||
            (eventType == previousEventType && eventTimestamp > previousEventTimestamp)
        )
        if (isNewEvent) {
            Log.i(TAG, "New event from ${peer.displayName()}: $eventType (timestamp=$eventTimestamp, prev=$previousEventTimestamp)")
            handler.post {
                meshListener?.onPeerEvent(peer, eventType)
            }
        }

        handler.post {
            meshListener?.onDocumentSynced(document)
        }

        notifyMeshUpdated()

        // Forward document to other connected peers (multi-hop relay)
        if (rawBytes != null && rawBytes.isNotEmpty()) {
            forwardDocumentToOtherPeers(document.nodeId, rawBytes, sourceAddress)
        }
    }

    /**
     * Forward a document to all connected peers except the source.
     * Uses deduplication cache to prevent forwarding loops.
     */
    private fun forwardDocumentToOtherPeers(originNodeId: Long, rawBytes: ByteArray, sourceAddress: String?) {
        // Skip if document is from ourselves
        if (originNodeId == nodeId) return

        // Compute message hash for deduplication (origin + content hash)
        val contentHash = rawBytes.contentHashCode().toLong()
        val messageHash = (originNodeId shl 32) or (contentHash and 0xFFFFFFFFL)

        // Check deduplication cache
        val now = System.currentTimeMillis()
        synchronized(seenMessagesLock) {
            val lastSeen = seenMessages[messageHash]
            if (lastSeen != null && (now - lastSeen) < 30_000) {
                // Already forwarded this message within last 30 seconds
                Log.v(TAG, "[RELAY-SKIP] Already forwarded message from ${String.format("%08X", originNodeId)}")
                return
            }
            seenMessages[messageHash] = now
        }

        // Count targets for logging
        var forwardCount = 0

        // Forward to peripherals (devices we connected to)
        for ((address, gatt) in connections) {
            if (address == sourceAddress) continue  // Don't echo back to source
            writeDocumentToGatt(gatt, rawBytes)
            forwardCount++
        }

        // Forward to centrals (devices that connected to us)
        for ((address, _) in connectedCentrals) {
            if (address == sourceAddress) continue  // Don't echo back to source
            // notifyConnectedCentrals handles the actual write
        }

        // Use existing notify mechanism for centrals
        if (sourceAddress != null) {
            // Notify all centrals except source
            val centralsExcludingSource = connectedCentrals.filter { it.key != sourceAddress }
            if (centralsExcludingSource.isNotEmpty()) {
                notifySpecificCentrals(centralsExcludingSource.keys.toList(), rawBytes)
                forwardCount += centralsExcludingSource.size
            }
        }

        if (forwardCount > 0) {
            Log.i(TAG, "[RELAY] Forwarded document from ${String.format("%08X", originNodeId)} to $forwardCount peers")
        }
    }

    /**
     * Notify specific centrals with document data.
     */
    private fun notifySpecificCentrals(addresses: List<String>, data: ByteArray) {
        val server = gattServer ?: return
        val characteristic = syncDataCharacteristic ?: return

        for (address in addresses) {
            val device = connectedCentrals[address] ?: continue
            try {
                characteristic.value = data
                server.notifyCharacteristicChanged(device, characteristic, false)
            } catch (e: Exception) {
                Log.e(TAG, "Failed to notify central $address: ${e.message}")
            }
        }
    }

    /**
     * Handle an incoming marker document from a peer.
     * Decodes markers, notifies listener, and forwards to other peers.
     */
    private fun handlePeerMarkerDocument(peer: PeatPeer, data: ByteArray) {
        // Marker document format: marker(1) + nodeId(4) + count(2) + markers...
        if (data.size < 7) {
            Log.e(TAG, "Marker document too short: ${data.size} bytes")
            return
        }

        var offset = 1  // Skip marker byte

        // Read source nodeId (4 bytes LE)
        val sourceNodeId = ((data[offset].toLong() and 0xFF)) or
                ((data[offset + 1].toLong() and 0xFF) shl 8) or
                ((data[offset + 2].toLong() and 0xFF) shl 16) or
                ((data[offset + 3].toLong() and 0xFF) shl 24)
        offset += 4

        // Skip if from ourselves
        if (sourceNodeId == nodeId) return

        // Read marker count (2 bytes LE)
        val markerCount = ((data[offset].toInt() and 0xFF)) or
                ((data[offset + 1].toInt() and 0xFF) shl 8)
        offset += 2

        Log.i(TAG, "[MARKER-RX] From ${peer.displayName()} (origin=${String.format("%08X", sourceNodeId.toLong())}): $markerCount markers")

        // Find the source peer (might be relayed)
        val sourcePeer = peers[sourceNodeId] ?: peer

        // Decode and notify for each marker
        for (i in 0 until markerCount) {
            val (marker, newOffset) = PeatMarker.decode(data, offset)
            if (marker != null) {
                Log.d(TAG, "[MARKER-RX] Marker #$i: uid=${marker.uid}, type=${marker.type}, callsign=${marker.callsign}")
                handler.post {
                    meshListener?.onMarkerSynced(sourcePeer, marker)
                }
                offset = newOffset
            } else {
                Log.e(TAG, "Failed to decode marker #$i at offset $offset")
                break
            }
        }

        // Forward marker document to other connected peers
        forwardDocumentToOtherPeers(sourceNodeId, data, peer.address)
    }

    /**
     * Handle an incoming chat document from a peer.
     * Decodes the chat message, notifies listener, and forwards to other peers.
     * Uses deduplication to prevent displaying/forwarding the same message twice.
     */
    private fun handlePeerChatDocument(peer: PeatPeer, data: ByteArray) {
        val chat = PeatChat.decode(data) ?: run {
            Log.e(TAG, "[CHAT-RX] Failed to decode chat message")
            return
        }

        val sourceNodeId = chat.originNode

        // Skip if from ourselves
        if (sourceNodeId == nodeId) return

        // Deduplication check - prevent processing same message twice (from multi-hop relay)
        val contentHash = data.contentHashCode().toLong()
        val messageHash = (sourceNodeId shl 32) or (contentHash and 0xFFFFFFFFL)
        val now = System.currentTimeMillis()
        synchronized(seenMessagesLock) {
            val lastSeen = seenMessages[messageHash]
            if (lastSeen != null && (now - lastSeen) < 30_000) {
                Log.v(TAG, "[CHAT-RX] Skipping duplicate chat from ${String.format("%08X", sourceNodeId.toLong())}")
                return
            }
            seenMessages[messageHash] = now
        }

        Log.i(TAG, "[CHAT-RX] From ${peer.displayName()} (origin=${String.format("%08X", sourceNodeId.toLong())}): '${chat.sender}' says '${chat.message}'")

        // Find the source peer (might be relayed)
        val sourcePeer = peers[sourceNodeId] ?: peer

        // Notify listener
        handler.post {
            meshListener?.onChatReceived(chat, sourcePeer)
        }

        // Forward chat document to other connected peers (no separate dedup needed - already marked as seen)
        forwardChatToOtherPeers(sourceNodeId, data, peer.address)
    }

    /**
     * Forward a chat document to all connected peers except the source.
     * Deduplication is already handled in handlePeerChatDocument.
     */
    private fun forwardChatToOtherPeers(originNodeId: Long, rawBytes: ByteArray, sourceAddress: String?) {
        var forwardCount = 0

        // Forward to peripherals (devices we connected to)
        for ((address, gatt) in connections) {
            if (address == sourceAddress) continue
            writeDocumentToGatt(gatt, rawBytes)
            forwardCount++
            Log.v(TAG, "[CHAT-RELAY] Sent to peripheral $address")
        }

        // Forward to centrals (devices that connected to us) using batch notify
        val centralsToNotify = connectedCentrals.keys.filter { it != sourceAddress }
        if (centralsToNotify.isNotEmpty()) {
            notifySpecificCentrals(centralsToNotify, rawBytes)
            forwardCount += centralsToNotify.size
            Log.v(TAG, "[CHAT-RELAY] Notified ${centralsToNotify.size} centrals")
        }

        if (forwardCount > 0) {
            Log.d(TAG, "[CHAT-RELAY] Forwarded to $forwardCount peers")
        }
    }

    /**
     * Handle an incoming app-layer message (0xAF marker) from a peer.
     *
     * peat-btle is transport-only: we pass raw bytes to the app via onDecryptedData
     * and relay to other connected peers. Apps use hive-lite to decode the content.
     */
    private fun handleAppLayerMessage(peer: PeatPeer, data: ByteArray) {
        Log.d(TAG, "[APP-LAYER] Received ${data.size} byte app-layer message from ${peer.displayName()}")

        // Pass raw bytes to app - apps use hive-lite to decode
        handler.post {
            meshListener?.onDecryptedData(peer, data)
        }

        // Relay to other connected peers (transport layer mesh forwarding)
        relayToOtherPeers(data, peer.address)
    }

    /**
     * Relay data to all connected peers except the source.
     */
    private fun relayToOtherPeers(rawBytes: ByteArray, sourceAddress: String?) {
        // Deduplication: Use Rust-side deduplication via PeatMesh.
        // This prevents broadcast storms when relaying CannedMessages.
        //
        // CannedMessage wire format: 0xAF marker, msg_code, source_node (4B LE), target (4B), timestamp (8B LE)
        // Document identity is at bytes 2-5 (source_node) and 10-17 (timestamp)
        if (rawBytes.size >= 18 && rawBytes[0] == APP_LAYER_MARKER) {
            // Extract document identity: source_node (bytes 2-5) + timestamp (bytes 10-17)
            val sourceNode = ((rawBytes[2].toInt() and 0xFF)) or
                ((rawBytes[3].toInt() and 0xFF) shl 8) or
                ((rawBytes[4].toInt() and 0xFF) shl 16) or
                ((rawBytes[5].toInt() and 0xFF) shl 24)
            val timestamp = ((rawBytes[10].toLong() and 0xFF)) or
                ((rawBytes[11].toLong() and 0xFF) shl 8) or
                ((rawBytes[12].toLong() and 0xFF) shl 16) or
                ((rawBytes[13].toLong() and 0xFF) shl 24) or
                ((rawBytes[14].toLong() and 0xFF) shl 32) or
                ((rawBytes[15].toLong() and 0xFF) shl 40) or
                ((rawBytes[16].toLong() and 0xFF) shl 48) or
                ((rawBytes[17].toLong() and 0xFF) shl 56)

            // Use Rust-side deduplication (centralizes logic in PeatMesh)
            val mesh = _mesh
            if (mesh != null) {
                val isNew = mesh.checkCannedMessage(sourceNode.toUInt(), timestamp.toULong(), 30_000UL)
                if (!isNew) {
                    Log.v(TAG, "[RELAY-SKIP] Already relayed CannedMessage from ${String.format("%08X", sourceNode)} ts=$timestamp")
                    return
                }
                // Mark as seen to prevent future relays
                mesh.markCannedMessageSeen(sourceNode.toUInt(), timestamp.toULong())
            } else {
                // Fallback to local deduplication if mesh not available
                val now = System.currentTimeMillis()
                val docId = (sourceNode.toLong() shl 32) or (timestamp and 0xFFFFFFFFL)
                synchronized(seenMessagesLock) {
                    val lastSeen = seenMessages[docId]
                    if (lastSeen != null && (now - lastSeen) < 30_000) {
                        Log.v(TAG, "[RELAY-SKIP] Already relayed CannedMessage from ${String.format("%08X", sourceNode)} ts=$timestamp (fallback)")
                        return
                    }
                    seenMessages[docId] = now
                }
            }
        }

        var forwardCount = 0

        // Debug: log connection counts
        val connCount = connections.size
        val centralCount = connectedCentrals.size
        if (connCount + centralCount > 4) {
            Log.w(TAG, "[RELAY-DEBUG] Unexpected peer count: connections=$connCount (${connections.keys}), centrals=$centralCount (${connectedCentrals.keys})")
        }

        // Forward to peripherals (devices we connected to)
        for ((address, gatt) in connections) {
            if (address == sourceAddress) continue
            writeDocumentToGatt(gatt, rawBytes)
            forwardCount++
        }

        // Forward to centrals (devices that connected to us)
        val centralsToNotify = connectedCentrals.keys.filter { it != sourceAddress }
        if (centralsToNotify.isNotEmpty()) {
            notifySpecificCentrals(centralsToNotify, rawBytes)
            forwardCount += centralsToNotify.size
        }

        if (forwardCount > 0) {
            Log.d(TAG, "[RELAY] Forwarded ${rawBytes.size} bytes to $forwardCount peers (conn=$connCount, central=$centralCount)")
        }
    }

    /**
     * Handle an incoming encrypted document (0xAE) from a peer via notifications.
     * Decrypts and passes raw bytes to app via onDecryptedData callback,
     * then continues with legacy parsing for backward compatibility.
     */
    private fun handlePeerEncryptedDocument(peer: PeatPeer, data: ByteArray) {
        val headerHex = data.take(16).joinToString(" ") { String.format("%02X", it) }
        Log.d(TAG, "[ENCRYPTED-NOTIFY] Received ${data.size} byte encrypted document from ${peer.displayName()}, header: $headerHex")

        val now = System.currentTimeMillis()
        val address = peer.address

        // TRANSPORT LAYER: Decrypt and pass raw bytes to app
        val decryptedBytes = _mesh?.decryptOnly(data)
        if (decryptedBytes != null && decryptedBytes.isNotEmpty()) {
            val marker = decryptedBytes[0]
            Log.d(TAG, "[TRANSPORT] Decrypted ${decryptedBytes.size} bytes, marker=0x${String.format("%02X", marker)}")
            handler.post {
                meshListener?.onDecryptedData(peer, decryptedBytes)
            }

            // NOTE: App-layer messages (0xAF) now flow through delta sync for proper CRDT handling.
            // They are stored in the document registry and synced via Operation::App.
            // The onDecryptedData callback above provides raw bytes for legacy apps.
        }

        // LEGACY: Continue with existing parsing for backward compatibility
        // Only for document types (0xAA, 0xB2, etc.) - not app-layer message (0xAF)
        // Use anonymous decryption path - decrypts first, extracts source_node from
        // decrypted document header, and registers the identifier->nodeId mapping.
        val result = _mesh?.onBleDataReceivedAnonymous(address, data, System.currentTimeMillis().toULong())
        if (result != null) {
            Log.i(TAG, "[ENCRYPTED-MERGE] sourceNode=${String.format("%08X", result.sourceNode.toLong())}, isAck=${result.isAck}, counterChanged=${result.counterChanged}, total=${result.totalCount}")

            val sourceNodeId = result.sourceNode.toLong()
            if (sourceNodeId != 0L && sourceNodeId != nodeId) {
                addressToNodeId[address] = sourceNodeId

                // Update peer info
                var sourcePeer = peers[sourceNodeId]
                if (sourcePeer == null) {
                    val peerName = generateDeviceName(meshId, sourceNodeId)
                    sourcePeer = PeatPeer(
                        nodeId = sourceNodeId,
                        address = address,
                        name = peerName,
                        meshId = meshId,
                        rssi = peer.rssi,
                        isConnected = true,
                        lastDocument = null,
                        lastSeen = now
                    )
                    peers[sourceNodeId] = sourcePeer
                    peerLifetimeManager?.onPeerActivity(address, true)
                    Log.i(TAG, "[ENCRYPTED-NOTIFY] Added peer: ${sourcePeer.displayName()}")
                } else {
                    sourcePeer.lastSeen = now
                    sourcePeer.isConnected = true
                    peerLifetimeManager?.onPeerActivity(address, true)
                    resetReconnectTracking(address)
                }

                // Check for ACK/emergency events
                // ACK can come either as emergency ACK (is_ack flag) or peripheral event (eventType=6)
                if (result.isAck || result.eventType == EventType.ACK) {
                    Log.i(TAG, "[ENCRYPTED-NOTIFY] ACK received from ${sourcePeer.displayName()} (isAck=${result.isAck}, eventType=${result.eventType})")
                    handler.post {
                        meshListener?.onPeerEvent(sourcePeer, PeatEventType.ACK)
                    }
                }
                if (result.isEmergency || result.eventType == EventType.EMERGENCY) {
                    Log.i(TAG, "[ENCRYPTED-NOTIFY] EMERGENCY from ${sourcePeer.displayName()} (isEmergency=${result.isEmergency}, eventType=${result.eventType})")
                    handler.post {
                        meshListener?.onPeerEvent(sourcePeer, PeatEventType.EMERGENCY)
                        onPeerEmergencyDetected(sourcePeer)
                    }
                }

                // Update callsign cache if we received a valid callsign
                result.callsign?.let { updateCallsignForNode(sourceNodeId, it) }

                // Build and notify document synced
                val eventType = result.eventType?.let { PeatEventType.fromEventType(it) } ?: PeatEventType.NONE
                val hasPeripheralData = result.callsign != null ||
                    result.latitude != null ||
                    result.batteryPercent != null ||
                    result.heartRate != null ||
                    result.eventType != null

                if (hasPeripheralData) {
                    val lat = result.latitude
                    val lon = result.longitude
                    val alt = result.altitude
                    val peerPeripheral = PeatPeripheral(
                        id = sourceNodeId,
                        parentNode = sourceNodeId,
                        peripheralType = PeatPeripheralType.SOLDIER_SENSOR,
                        callsign = result.callsign ?: "",
                        health = PeatHealthStatus(
                            batteryPercent = result.batteryPercent?.toInt() ?: 0,
                            heartRate = result.heartRate?.toInt(),
                            activityLevel = 0,
                            alerts = 0
                        ),
                        lastEvent = if (eventType != PeatEventType.NONE)
                            PeatPeripheralEvent(eventType, now) else null,
                        location = if (lat != null && lon != null)
                            PeatLocation(lat, lon, alt ?: 0f) else null,
                        timestamp = now
                    )
                    val syntheticDoc = PeatDocument(
                        version = 1,
                        nodeId = sourceNodeId,
                        counter = emptyList(),
                        peripheral = peerPeripheral
                    )
                    handler.post {
                        meshListener?.onDocumentSynced(syntheticDoc)
                    }

                    // Update peer's last document
                    sourcePeer.lastDocument = syntheticDoc
                }

                notifyMeshUpdated()
            }
        } else {
            Log.w(TAG, "[ENCRYPTED-NOTIFY] Failed to decrypt/process ${data.size} byte document from ${peer.displayName()}")
        }
    }

    /**
     * Handle an incoming delta document from a peer.
     * Applies operations incrementally to local state.
     */
    private fun handlePeerDeltaDocument(peer: PeatPeer, data: ByteArray) {
        val deltaDoc = PeatDeltaDocument.decode(data) ?: return
        val docNodeId = deltaDoc.originNode

        Log.d(TAG, "[DELTA-RX] From ${peer.displayName()} (origin=${String.format("%08X", docNodeId)}): ${deltaDoc.operations.size} ops")

        // Skip if document is from ourselves
        if (docNodeId == nodeId || docNodeId == 0L) return

        // Find or create peer for this origin node
        val targetPeer = peers[docNodeId] ?: peer.also {
            if (docNodeId != peer.nodeId) {
                // This is a relayed delta - create virtual peer for origin
                val newPeer = PeatPeer(
                    nodeId = docNodeId,
                    address = "",
                    name = generateDeviceName(meshId, docNodeId),
                    meshId = meshId,
                    rssi = 0,
                    isConnected = false,
                    lastDocument = null,
                    lastSeen = System.currentTimeMillis()
                )
                peers[docNodeId] = newPeer
            }
        }
        peers[docNodeId]?.let {
            it.lastSeen = System.currentTimeMillis()
            peerLifetimeManager?.onPeerActivity(it.address, it.isConnected)
        }

        // Apply each operation
        for (op in deltaDoc.operations) {
            when (op) {
                is DeltaOperation.IncrementCounter -> {
                    // Merge counter increment
                    val existing = localCounter.find { it.nodeId == op.nodeId }
                    if (existing != null) {
                        val newCount = existing.count + op.amount
                        val index = localCounter.indexOf(existing)
                        localCounter[index] = GCounterEntry(op.nodeId, newCount)
                    } else {
                        localCounter.add(GCounterEntry(op.nodeId, op.amount))
                    }
                    Log.v(TAG, "  - IncrementCounter: node=${String.format("%08X", op.nodeId.toLong())}, +${op.amount}")
                }

                is DeltaOperation.UpdatePeripheral -> {
                    // Update callsign cache if peripheral has callsign
                    op.peripheral.callsign.takeIf { it.isNotEmpty() }?.let {
                        updateCallsignForNode(docNodeId, it)
                    }

                    // Update peer's peripheral state
                    val currentPeer = peers[docNodeId]
                    if (currentPeer != null) {
                        val previousEvent = currentPeer.lastDocument?.peripheral?.lastEvent
                        val previousEventType = previousEvent?.eventType ?: PeatEventType.NONE
                        val previousEventTimestamp = previousEvent?.timestamp ?: 0L

                        // Create a synthetic document with the updated peripheral
                        val syntheticDoc = PeatDocument(
                            version = 1,
                            nodeId = docNodeId,
                            counter = localCounter.toList(),
                            peripheral = op.peripheral
                        )
                        currentPeer.lastDocument = syntheticDoc

                        // Check for new events
                        val currentEvent = op.peripheral.lastEvent
                        val eventType = currentEvent?.eventType ?: PeatEventType.NONE
                        val eventTimestamp = currentEvent?.timestamp ?: 0L
                        val isNewEvent = eventType != PeatEventType.NONE && (
                            eventType != previousEventType ||
                            (eventType == previousEventType && eventTimestamp > previousEventTimestamp)
                        )
                        if (isNewEvent) {
                            Log.i(TAG, "  - New event: $eventType")
                            handler.post {
                                meshListener?.onPeerEvent(currentPeer, eventType)
                            }
                        }

                        // Notify document synced
                        handler.post {
                            meshListener?.onDocumentSynced(syntheticDoc)
                        }
                    }
                    Log.v(TAG, "  - UpdatePeripheral: callsign=${op.peripheral.callsign}, loc=${op.peripheral.location != null}")
                }

                is DeltaOperation.SetEmergency -> {
                    Log.i(TAG, "  - SetEmergency: source=${String.format("%08X", op.sourceNode.toLong())}")
                    peers[op.sourceNode]?.let { emergencyPeer ->
                        handler.post {
                            meshListener?.onPeerEvent(emergencyPeer, PeatEventType.EMERGENCY)
                            onPeerEmergencyDetected(emergencyPeer)
                        }
                    }
                }

                is DeltaOperation.AckEmergency -> {
                    Log.i(TAG, "  - AckEmergency: from=${String.format("%08X", op.nodeId.toLong())}")
                    peers[op.nodeId]?.let { ackPeer ->
                        handler.post {
                            meshListener?.onPeerEvent(ackPeer, PeatEventType.ACK)
                        }
                    }
                }

                is DeltaOperation.ClearEmergency -> {
                    Log.i(TAG, "  - ClearEmergency")
                    // Clear emergency state - could notify listener
                }

                is DeltaOperation.UpdateLocation -> {
                    // Field-level location update - apply to existing peripheral
                    val currentPeer = peers[docNodeId]
                    if (currentPeer != null) {
                        val existingPeripheral = currentPeer.lastDocument?.peripheral
                        val updatedLocation = PeatLocation(
                            latitude = op.latitude,
                            longitude = op.longitude,
                            altitude = op.altitude
                        )
                        val updatedPeripheral = existingPeripheral?.copy(location = updatedLocation)
                            ?: PeatPeripheral(
                                id = docNodeId,
                                parentNode = docNodeId,
                                peripheralType = PeatPeripheralType.SOLDIER_SENSOR,
                                callsign = "",
                                health = PeatHealthStatus(0, null, 0, 0),
                                lastEvent = null,
                                location = updatedLocation,
                                timestamp = deltaDoc.timestampMs
                            )
                        val syntheticDoc = PeatDocument(
                            version = 1,
                            nodeId = docNodeId,
                            counter = localCounter.toList(),
                            peripheral = updatedPeripheral
                        )
                        currentPeer.lastDocument = syntheticDoc
                        handler.post { meshListener?.onDocumentSynced(syntheticDoc) }
                    }
                    Log.v(TAG, "  - UpdateLocation: lat=${op.latitude}, lon=${op.longitude}, alt=${op.altitude}")
                }

                is DeltaOperation.UpdateHealth -> {
                    // Field-level health update - apply to existing peripheral
                    val currentPeer = peers[docNodeId]
                    if (currentPeer != null) {
                        val existingPeripheral = currentPeer.lastDocument?.peripheral
                        val updatedHealth = PeatHealthStatus(
                            batteryPercent = op.batteryPercent,
                            heartRate = op.heartRate,
                            activityLevel = op.activityLevel,
                            alerts = op.alerts
                        )
                        val updatedPeripheral = existingPeripheral?.copy(health = updatedHealth)
                            ?: PeatPeripheral(
                                id = docNodeId,
                                parentNode = docNodeId,
                                peripheralType = PeatPeripheralType.SOLDIER_SENSOR,
                                callsign = "",
                                health = updatedHealth,
                                lastEvent = null,
                                location = null,
                                timestamp = deltaDoc.timestampMs
                            )
                        val syntheticDoc = PeatDocument(
                            version = 1,
                            nodeId = docNodeId,
                            counter = localCounter.toList(),
                            peripheral = updatedPeripheral
                        )
                        currentPeer.lastDocument = syntheticDoc
                        handler.post { meshListener?.onDocumentSynced(syntheticDoc) }
                    }
                    Log.v(TAG, "  - UpdateHealth: bat=${op.batteryPercent}%, hr=${op.heartRate}, activity=${op.activityLevel}")
                }

                is DeltaOperation.UpdateCallsign -> {
                    // Update callsign cache
                    updateCallsignForNode(docNodeId, op.callsign)

                    // Field-level callsign update - apply to existing peripheral
                    val currentPeer = peers[docNodeId]
                    if (currentPeer != null) {
                        val existingPeripheral = currentPeer.lastDocument?.peripheral
                        val updatedPeripheral = existingPeripheral?.copy(callsign = op.callsign)
                            ?: PeatPeripheral(
                                id = docNodeId,
                                parentNode = docNodeId,
                                peripheralType = PeatPeripheralType.SOLDIER_SENSOR,
                                callsign = op.callsign,
                                health = PeatHealthStatus(0, null, 0, 0),
                                lastEvent = null,
                                location = null,
                                timestamp = deltaDoc.timestampMs
                            )
                        val syntheticDoc = PeatDocument(
                            version = 1,
                            nodeId = docNodeId,
                            counter = localCounter.toList(),
                            peripheral = updatedPeripheral
                        )
                        currentPeer.lastDocument = syntheticDoc
                        handler.post { meshListener?.onDocumentSynced(syntheticDoc) }
                    }
                    Log.v(TAG, "  - UpdateCallsign: ${op.callsign}")
                }

                is DeltaOperation.UpdateEvent -> {
                    // Field-level event update - apply and trigger callback
                    val currentPeer = peers[docNodeId]
                    if (currentPeer != null) {
                        val existingPeripheral = currentPeer.lastDocument?.peripheral
                        val previousEvent = existingPeripheral?.lastEvent
                        val previousEventType = previousEvent?.eventType ?: PeatEventType.NONE
                        val previousEventTimestamp = previousEvent?.timestamp ?: 0L

                        val updatedEvent = PeatPeripheralEvent(op.eventType, op.timestamp)
                        val updatedPeripheral = existingPeripheral?.copy(lastEvent = updatedEvent)
                            ?: PeatPeripheral(
                                id = docNodeId,
                                parentNode = docNodeId,
                                peripheralType = PeatPeripheralType.SOLDIER_SENSOR,
                                callsign = "",
                                health = PeatHealthStatus(0, null, 0, 0),
                                lastEvent = updatedEvent,
                                location = null,
                                timestamp = deltaDoc.timestampMs
                            )
                        val syntheticDoc = PeatDocument(
                            version = 1,
                            nodeId = docNodeId,
                            counter = localCounter.toList(),
                            peripheral = updatedPeripheral
                        )
                        currentPeer.lastDocument = syntheticDoc

                        // Check for new events - trigger callback
                        val isNewEvent = op.eventType != PeatEventType.NONE && (
                            op.eventType != previousEventType ||
                            (op.eventType == previousEventType && op.timestamp > previousEventTimestamp)
                        )
                        if (isNewEvent) {
                            Log.i(TAG, "  - New event from delta: ${op.eventType}")
                            handler.post { meshListener?.onPeerEvent(currentPeer, op.eventType) }
                        }
                        handler.post { meshListener?.onDocumentSynced(syntheticDoc) }
                    }
                    Log.v(TAG, "  - UpdateEvent: type=${op.eventType}, ts=${op.timestamp}")
                }
            }
        }

        notifyMeshUpdated()

        // Forward delta document to other connected peers
        forwardDocumentToOtherPeers(docNodeId, data, peer.address)
    }

    private fun mergeCounter(remoteCounter: List<GCounterEntry>) {
        for (entry in remoteCounter) {
            val existing = localCounter.find { it.nodeId == entry.nodeId }
            if (existing != null) {
                if (entry.count > existing.count) {
                    val index = localCounter.indexOf(existing)
                    localCounter[index] = entry
                }
            } else {
                localCounter.add(entry)
            }
        }
    }

    private fun incrementLocalCounter() {
        val existing = localCounter.find { it.nodeId == nodeId }
        if (existing != null) {
            val index = localCounter.indexOf(existing)
            localCounter[index] = GCounterEntry(nodeId, existing.count + 1)
        } else {
            localCounter.add(GCounterEntry(nodeId, 1))
        }
    }

    /**
     * Sync localPeripheral state to native PeatMesh.
     *
     * This ensures that when buildDocument() is called on the native side,
     * it includes the current location, callsign, health, and event data.
     * Without this, encrypted documents would be missing positional data.
     */
    private fun syncLocalPeripheralToNative(timestamp: Long) {
        val mesh = _mesh ?: return
        val peripheral = localPeripheral ?: return

        // Map Kotlin event type to native event type
        val nativeEventType: EventType? = peripheral.lastEvent?.let { event ->
            when (event.eventType) {
                PeatEventType.NONE -> EventType.NONE
                PeatEventType.PING -> EventType.PING
                PeatEventType.NEED_ASSIST -> EventType.NEED_ASSIST
                PeatEventType.EMERGENCY -> EventType.EMERGENCY
                PeatEventType.MOVING -> EventType.MOVING
                PeatEventType.IN_POSITION -> EventType.IN_POSITION
                PeatEventType.ACK -> EventType.ACK
            }
        }

        mesh.updatePeripheralState(
            callsign = peripheral.callsign,
            batteryPercent = peripheral.health.batteryPercent.coerceIn(0, 255).toUByte(),
            heartRate = peripheral.health.heartRate?.coerceIn(0, 255)?.toUByte(),
            latitude = peripheral.location?.latitude?.toFloat(),
            longitude = peripheral.location?.longitude?.toFloat(),
            altitude = peripheral.location?.altitude?.toFloat(),
            eventType = nativeEventType,
            timestampMs = timestamp.toULong()
        )
    }

    private fun syncWithPeers() {
        if (connections.isEmpty() && connectedCentrals.isEmpty()) return

        val now = System.currentTimeMillis()
        val currentCounterValue = localCounter.sumOf { it.count }
        val hasLoc = localPeripheral?.location != null

        // Send to peripherals we connected to (with per-peer delta logic)
        for ((address, gatt) in connections) {
            val peerId = addressToNodeId[address] ?: continue
            val documentBytes = buildSyncDocumentForPeer(peerId, now, currentCounterValue)
            if (documentBytes != null) {
                writeDocumentToGatt(gatt, documentBytes)
            }
        }

        // Send to centrals that connected to us (with per-peer delta logic)
        for ((address, _) in connectedCentrals) {
            val peerId = addressToNodeId[address] ?: continue
            val documentBytes = buildSyncDocumentForPeer(peerId, now, currentCounterValue)
            if (documentBytes != null) {
                notifyCentral(address, documentBytes)
            }
        }

        Log.d(TAG, "syncWithPeers: peers=${connections.size + connectedCentrals.size}, hasPeripheral=${localPeripheral != null}, hasLocation=$hasLoc")
    }

    /**
     * Build sync document for a specific peer, using delta encoding when possible.
     *
     * Returns null if nothing has changed (skip sync entirely).
     * Returns full document on first sync or every FULL_SYNC_INTERVAL syncs.
     * Returns delta document otherwise.
     */
    private fun buildSyncDocumentForPeer(peerId: Long, now: Long, currentCounterValue: Long): ByteArray? {
        val mesh = _mesh ?: return null
        val state = peerSyncState.getOrPut(peerId) { PeerSyncState() }

        // Sync local peripheral state to native before building document
        // This ensures location and other state is included in encrypted docs
        syncLocalPeripheralToNative(now)

        // Determine if we need a full sync (first sync or every FULL_SYNC_INTERVAL)
        val needsFullSync = state.syncCount == 0 ||
                            state.syncCount % FULL_SYNC_INTERVAL == 0

        if (needsFullSync) {
            // Full delta document - includes all state including app documents
            val documentBytes = mesh.buildFullDeltaDocument(now.toULong())
            state.lastSentTimestamp = now
            state.lastSentPeripheral = localPeripheral?.copy()
            state.lastSentCounterValue = currentCounterValue
            state.syncCount++
            Log.d(TAG, "[FULL] Peer ${String.format("%08X", peerId)}: ${documentBytes.size} bytes (sync #${state.syncCount})")
            return documentBytes
        }

        // Per-peer delta - only sends what's new for this peer, including app documents
        // The Rust side tracks per-peer state and filters operations
        val deltaBytes = mesh.buildDeltaDocumentForPeer(peerId.toUInt(), now.toULong())
        if (deltaBytes == null) {
            Log.v(TAG, "[SKIP] Peer ${String.format("%08X", peerId)}: no changes")
            state.syncCount++
            return null
        }

        // Update local state tracking
        state.lastSentTimestamp = now
        state.lastSentPeripheral = localPeripheral?.copy()
        state.lastSentCounterValue = currentCounterValue
        state.syncCount++

        Log.d(TAG, "[DELTA] Peer ${String.format("%08X", peerId)}: ${deltaBytes.size} bytes (sync #${state.syncCount})")
        return deltaBytes
    }

    /**
     * Check if peripheral state has meaningfully changed.
     */
    private fun peripheralChanged(current: PeatPeripheral?, last: PeatPeripheral?): Boolean {
        if (current == null && last == null) return false
        if (current == null || last == null) return true

        // Check location change (most common)
        val locChanged = current.location != last.location

        // Check health changes
        val healthChanged = current.health.batteryPercent != last.health.batteryPercent ||
                           current.health.heartRate != last.health.heartRate ||
                           current.health.activityLevel != last.health.activityLevel

        // Check event change
        val eventChanged = current.lastEvent != last.lastEvent

        // Check callsign change
        val callsignChanged = current.callsign != last.callsign

        return locChanged || healthChanged || eventChanged || callsignChanged
    }

    /**
     * Notify a specific central device with document bytes.
     */
    private fun notifyCentral(address: String, documentBytes: ByteArray) {
        val device = connectedCentrals[address] ?: return
        val gattServer = this.gattServer ?: return
        val service = gattServer.getService(PEAT_SERVICE_UUID) ?: return
        val characteristic = service.getCharacteristic(PEAT_CHAR_DOCUMENT) ?: return

        // BLE notifications have max size (typically 512 bytes, can be higher with MTU negotiation)
        // Skip notification if document is too large to prevent crash
        if (documentBytes.size > 512) {
            Log.w(TAG, "Document too large for BLE notification: ${documentBytes.size} bytes (max 512), skipping notify to $address")
            return
        }

        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                gattServer.notifyCharacteristicChanged(device, characteristic, false, documentBytes)
            } else {
                @Suppress("DEPRECATION")
                characteristic.value = documentBytes
                @Suppress("DEPRECATION")
                gattServer.notifyCharacteristicChanged(device, characteristic, false)
            }
        } catch (e: IllegalArgumentException) {
            Log.e(TAG, "Failed to notify central $address: ${e.message} (doc size: ${documentBytes.size})")
        }
    }

    private fun cleanupStalePeers() {
        val now = System.currentTimeMillis()
        val manager = peerLifetimeManager

        // Use Rust PeerLifetimeManager for stale detection if available
        val staleAddresses = manager?.getStalePeerAddresses() ?: emptyList()

        // Resolve stale addresses to nodeIds
        val staleNodeIds = staleAddresses.mapNotNull { address ->
            addressToNodeId[address]
        }.toSet()

        if (staleNodeIds.isNotEmpty()) {
            Log.d(TAG, "Removing ${staleNodeIds.size} stale peers")
            for (nodeId in staleNodeIds) {
                val peer = peers.remove(nodeId)
                peer?.let {
                    Log.i(TAG, "Removed stale peer: ${it.displayName()} (connected=${it.isConnected}, lastSeen=${now - it.lastSeen}ms ago)")
                    // Clean up all maps to prevent memory leaks
                    addressToNodeId.remove(it.address)
                    nameToNodeId.remove(it.name)
                    // Also clean up callsign mappings for stale peers
                    val cachedCallsign = nodeIdToCallsign.remove(nodeId)
                    cachedCallsign?.let { cs -> callsignToNodeId.remove(cs) }
                    disconnect(it.address)
                    // Clear from Rust managers
                    reconnectionManager?.stopTracking(it.address)
                    manager?.removePeer(it.address)
                }
            }
            // Persist the updated callsign cache
            saveCallsignCache()
            notifyMeshUpdated()
        }
    }

    /**
     * Attempt to reconnect lost peers using Rust ReconnectionManager.
     *
     * The Rust manager handles exponential backoff (or flat delay in high-priority mode),
     * max attempt tracking, and auto-reset on exhaustion.
     */
    private fun reconnectLostPeers() {
        if (!isMeshRunning) return
        val manager = reconnectionManager ?: return

        // Get addresses the Rust manager says are ready for reconnection
        val readyAddresses = manager.getPeersToReconnect()
        if (readyAddresses.isEmpty()) return

        Log.d(TAG, "[RECONNECT] ${readyAddresses.size} peers ready for reconnection")

        for (address in readyAddresses) {
            // Find the peer object for this address
            val nodeId = addressToNodeId[address] ?: continue
            val peer = peers[nodeId] ?: continue

            // Skip if already connected or connection in progress
            if (peer.isConnected || connections.containsKey(address)) continue

            val stats = manager.getPeerStats(address)
            val attempt = stats?.attempts?.toInt() ?: 0
            Log.i(TAG, "[RECONNECT] Attempting to reconnect to ${peer.displayName()} (attempt ${attempt + 1}/$RECONNECT_MAX_ATTEMPTS)")

            manager.recordAttempt(address)

            try {
                connectToPeer(peer)
            } catch (e: Exception) {
                Log.e(TAG, "[RECONNECT] Failed to reconnect to ${peer.displayName()}: ${e.message}")
            }
        }
    }

    /**
     * Reset reconnection tracking for a peer (called when connection succeeds).
     */
    private fun resetReconnectTracking(address: String) {
        reconnectionManager?.onConnectionSuccess(address)
    }

    private fun startReconnectGrace(peer: PeatPeer) {
        reconnectGraceRunnables.remove(peer.address)?.let { handler.removeCallbacks(it) }
        peer.isReconnecting = true
        val address = peer.address
        val runnable = Runnable {
            peer.isReconnecting = false
            reconnectGraceRunnables.remove(address)
            notifyMeshUpdated()
        }
        reconnectGraceRunnables[address] = runnable
        handler.postDelayed(runnable, RECONNECT_GRACE_MS)
    }

    private fun cancelReconnectGrace(peer: PeatPeer) {
        peer.isReconnecting = false
        reconnectGraceRunnables.remove(peer.address)?.let { handler.removeCallbacks(it) }
    }

    /**
     * Remove stale connectedCentrals entries for the same nodeId but different BLE address.
     * BLE address rotation causes the same peer to appear as multiple centrals.
     */
    private fun deduplicateConnectedCentrals(currentAddress: String, nodeId: Long) {
        val staleAddresses = connectedCentrals.keys.filter { addr ->
            addr != currentAddress && addressToNodeId[addr] == nodeId
        }
        for (staleAddr in staleAddresses) {
            Log.i(TAG, "[CENTRAL-DEDUP] Removing stale central $staleAddr (same nodeId ${String.format("%08X", nodeId)}, current: $currentAddress)")
            connectedCentrals.remove(staleAddr)
            addressToNodeId.remove(staleAddr)
        }
    }

    private fun notifyMeshUpdated() {
        handler.post {
            meshListener?.onMeshUpdated(peers.values.toList())
        }
    }

    private fun notifyPeerConnected(peer: PeatPeer) {
        handler.post {
            meshListener?.onPeerConnected(peer)
        }
    }

    private fun notifyPeerDisconnected(peer: PeatPeer) {
        handler.post {
            meshListener?.onPeerDisconnected(peer)
        }
    }

    /**
     * Generate nodeId from the local Bluetooth adapter's address.
     * Falls back to a persistent random ID if adapter address is unavailable (Android 12+ restrictions).
     * The nodeId is persisted to SharedPreferences to remain consistent across app restarts.
     */
    @Suppress("MissingPermission")
    private fun generateNodeIdFromAdapter(): Long {
        val prefs = context.getSharedPreferences("peat_btle", Context.MODE_PRIVATE)
        val savedNodeId = prefs.getLong("node_id", 0L)

        // Return saved nodeId if we have one
        if (savedNodeId != 0L) {
            Log.i(TAG, "Using persisted nodeId: ${String.format("%08X", savedNodeId)}")
            return savedNodeId
        }

        // Try to get from adapter address first
        val nodeId = try {
            val address = bluetoothAdapter?.address
            if (address != null && address != "02:00:00:00:00:00") {
                // Use native Rust implementation for consistency across platforms
                val derived = nativeDeriveNodeId(address)
                if (derived != 0L) {
                    derived
                } else {
                    deriveNodeIdFromAddressFallback(address)
                }
            } else {
                // Generate random nodeId from UUID (similar to iOS approach)
                val uuid = java.util.UUID.randomUUID()
                val bytes = java.nio.ByteBuffer.allocate(16)
                    .putLong(uuid.mostSignificantBits)
                    .putLong(uuid.leastSignificantBits)
                    .array()
                // Use last 4 bytes like iOS does
                ((bytes[12].toLong() and 0xFF) shl 24) or
                    ((bytes[13].toLong() and 0xFF) shl 16) or
                    ((bytes[14].toLong() and 0xFF) shl 8) or
                    (bytes[15].toLong() and 0xFF)
            }
        } catch (e: SecurityException) {
            // Generate random nodeId
            val uuid = java.util.UUID.randomUUID()
            (uuid.leastSignificantBits and 0xFFFFFFFFL)
        }

        // Persist the nodeId
        prefs.edit().putLong("node_id", nodeId).apply()
        Log.i(TAG, "Generated and persisted new nodeId: ${String.format("%08X", nodeId)}")

        return nodeId
    }

    /**
     * Derive a nodeId from a BLE MAC address (fallback if native call fails).
     * Uses the last 4 bytes of the MAC as a 32-bit node ID.
     */
    private fun deriveNodeIdFromAddressFallback(address: String): Long {
        // MAC format: "AA:BB:CC:DD:EE:FF"
        val parts = address.split(":")
        if (parts.size != 6) return 0L

        return try {
            // Use last 4 bytes of MAC as node ID
            val b2 = parts[2].toLong(16)
            val b3 = parts[3].toLong(16)
            val b4 = parts[4].toLong(16)
            val b5 = parts[5].toLong(16)
            (b2 shl 24) or (b3 shl 16) or (b4 shl 8) or b5
        } catch (e: NumberFormatException) {
            0L
        }
    }

    // ========================================================================
    // Callsign Cache - Maps nodeId <-> callsign for identity resolution
    // ========================================================================

    /**
     * Load persisted callsign mappings from SharedPreferences.
     * Called during init() to restore mappings across app restarts.
     */
    private fun loadCallsignCache() {
        try {
            val prefs = context.getSharedPreferences("peat_btle_callsigns", Context.MODE_PRIVATE)
            val mappingsJson = prefs.getString("callsign_mappings", null)
            if (mappingsJson != null) {
                val mappings = org.json.JSONObject(mappingsJson)
                for (key in mappings.keys()) {
                    val nodeId = key.toLongOrNull() ?: continue
                    val callsign = mappings.getString(key)
                    if (callsign.isNotBlank() && !callsign.equals("ANDROID", ignoreCase = true)) {
                        nodeIdToCallsign[nodeId] = callsign
                        callsignToNodeId[callsign] = nodeId
                    }
                }
                Log.i(TAG, "Loaded ${nodeIdToCallsign.size} callsign mappings from cache")
            }
        } catch (e: Exception) {
            Log.w(TAG, "Failed to load callsign cache: ${e.message}")
        }
    }

    /**
     * Save callsign mappings to SharedPreferences.
     */
    private fun saveCallsignCache() {
        try {
            val mappings = org.json.JSONObject()
            for ((nodeId, callsign) in nodeIdToCallsign) {
                mappings.put(nodeId.toString(), callsign)
            }
            val prefs = context.getSharedPreferences("peat_btle_callsigns", Context.MODE_PRIVATE)
            prefs.edit().putString("callsign_mappings", mappings.toString()).apply()
            Log.d(TAG, "Saved ${nodeIdToCallsign.size} callsign mappings to cache")
        } catch (e: Exception) {
            Log.w(TAG, "Failed to save callsign cache: ${e.message}")
        }
    }

    /**
     * Update the callsign for a nodeId.
     * Updates both the cache and any existing PeatPeer with matching nodeId.
     *
     * @param nodeId The node ID
     * @param callsign The callsign (ignored if blank or "ANDROID")
     * @return true if the callsign was updated
     */
    private fun updateCallsignForNode(nodeId: Long, callsign: String): Boolean {
        // Filter out empty or default callsigns
        val trimmedCallsign = callsign.trim()
        if (trimmedCallsign.isBlank() || trimmedCallsign.equals("ANDROID", ignoreCase = true)) {
            return false
        }

        val existingCallsign = nodeIdToCallsign[nodeId]
        if (existingCallsign == trimmedCallsign) {
            return false // No change
        }

        // Update mappings
        nodeIdToCallsign[nodeId] = trimmedCallsign
        callsignToNodeId[trimmedCallsign] = nodeId

        // Remove old callsign mapping if it changed
        if (existingCallsign != null && existingCallsign != trimmedCallsign) {
            callsignToNodeId.remove(existingCallsign)
        }

        // Update the peer's name if it exists
        val peer = peers[nodeId]
        if (peer != null && peer.name != trimmedCallsign) {
            val oldName = peer.name
            peer.name = trimmedCallsign  // Direct update since name is now var
            // Also update nameToNodeId mapping
            nameToNodeId.remove(oldName)
            nameToNodeId[trimmedCallsign] = nodeId
            Log.i(TAG, "[CALLSIGN] Updated peer ${String.format("%08X", nodeId)}: '$oldName' -> '$trimmedCallsign'")
            notifyMeshUpdated()  // Notify listeners of the name change
        } else if (peer == null) {
            Log.d(TAG, "[CALLSIGN] Cached callsign for ${String.format("%08X", nodeId)}: '$trimmedCallsign' (peer not yet created)")
        }

        // Persist the mapping
        saveCallsignCache()
        return true
    }

    /**
     * Get the cached callsign for a nodeId, or null if not known.
     */
    fun getCachedCallsign(nodeId: Long): String? = nodeIdToCallsign[nodeId]

    /**
     * Get the nodeId for a callsign, or null if not known.
     */
    fun getNodeIdForCallsign(callsign: String): Long? = callsignToNodeId[callsign]

    /**
     * Queue a document write for a GATT connection.
     * BLE only allows one pending write at a time, so we queue writes and process them sequentially.
     */
    private fun writeDocumentToGatt(gatt: BluetoothGatt, data: ByteArray) {
        val address = gatt.device?.address ?: return

        // Get or create the queue for this connection
        val queue = writeQueues.getOrPut(address) { java.util.concurrent.ConcurrentLinkedQueue() }
        queue.add(data)

        // Try to process the queue (will only proceed if no write is in progress)
        processWriteQueue(address, gatt)
    }

    /**
     * Process the write queue for a connection.
     * Called when a new item is queued or when a previous write completes.
     */
    private fun processWriteQueue(address: String, gatt: BluetoothGatt) {
        // Check if a write is already in progress
        if (writeInProgress.getOrDefault(address, false)) {
            return
        }

        val queue = writeQueues[address] ?: return
        val data = queue.poll() ?: return

        // Mark write as in progress
        writeInProgress[address] = true

        try {
            val service = gatt.getService(PEAT_SERVICE_UUID)
            if (service == null) {
                Log.w(TAG, "[WRITE-QUEUE] No Peat service for $address, dropping write")
                writeInProgress[address] = false
                processWriteQueue(address, gatt)  // Try next item
                return
            }

            val char = service.getCharacteristic(PEAT_CHAR_DOCUMENT)
            if (char == null) {
                Log.w(TAG, "[WRITE-QUEUE] No document characteristic for $address, dropping write")
                writeInProgress[address] = false
                processWriteQueue(address, gatt)  // Try next item
                return
            }

            Log.d(TAG, "[WRITE-QUEUE] Writing ${data.size} bytes to $address (queue size: ${queue.size})")

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                gatt.writeCharacteristic(char, data, BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT)
            } else {
                @Suppress("DEPRECATION")
                char.value = data
                @Suppress("DEPRECATION")
                gatt.writeCharacteristic(char)
            }
        } catch (e: Exception) {
            Log.e(TAG, "[WRITE-QUEUE] Failed to write document to $address", e)
            writeInProgress[address] = false
            processWriteQueue(address, gatt)  // Try next item
        }
    }

    /**
     * Called when a write operation completes for a connection.
     * Processes the next item in the queue.
     */
    internal fun onWriteCompleteForConnection(address: String) {
        writeInProgress[address] = false
        val gatt = connections[address]
        if (gatt != null) {
            processWriteQueue(address, gatt)
        }
    }

    private fun readDocumentFromGatt(gatt: BluetoothGatt) {
        try {
            val service = gatt.getService(PEAT_SERVICE_UUID) ?: return
            val char = service.getCharacteristic(PEAT_CHAR_DOCUMENT) ?: return
            gatt.readCharacteristic(char)
        } catch (e: Exception) {
            Log.e(TAG, "Failed to read document", e)
        }
    }

    private fun enableNotificationsForGatt(gatt: BluetoothGatt) {
        try {
            val service = gatt.getService(PEAT_SERVICE_UUID) ?: return
            val char = service.getCharacteristic(PEAT_CHAR_DOCUMENT) ?: return

            gatt.setCharacteristicNotification(char, true)

            val descriptor = char.getDescriptor(CCCD_UUID)
            if (descriptor != null) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    gatt.writeDescriptor(descriptor, BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE)
                } else {
                    @Suppress("DEPRECATION")
                    descriptor.value = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
                    @Suppress("DEPRECATION")
                    gatt.writeDescriptor(descriptor)
                }
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to enable notifications", e)
        }
    }

    /**
     * Disconnect all devices and clean up resources.
     */
    fun shutdown() {
        stopMesh()
        stopScan()
        stopAdvertising()

        // Close GATT server permanently (only done in shutdown to avoid registration leaks)
        closeGattServer()

        // Disconnect all
        for (address in connections.keys.toList()) {
            disconnect(address)
        }

        // Unregister pairing request receiver
        if (pairingReceiverRegistered) {
            try {
                context.unregisterReceiver(pairingRequestReceiver)
                pairingReceiverRegistered = false
                Log.i(TAG, "Unregistered pairing request receiver")
            } catch (e: Exception) {
                Log.w(TAG, "Failed to unregister pairing receiver: ${e.message}")
            }
        }

        // Destroy PeatMesh (UniFFI handles resource cleanup)
        _mesh?.destroy()
        _mesh = null

        isInitialized = false
        Log.i(TAG, "Shutdown complete")
    }

    /**
     * Check if scanning is currently active.
     */
    fun isScanning(): Boolean = isScanning

    /**
     * Check if advertising is currently active.
     */
    fun isAdvertising(): Boolean = isAdvertising

    /**
     * Get the number of active connections.
     */
    fun connectionCount(): Int = connections.size

    /**
     * Get list of connected device addresses (devices we connected to as Central).
     */
    fun connectedDevices(): List<String> = connections.keys.toList()

    /**
     * Get the number of connected centrals (devices that connected to us as Peripheral).
     */
    fun connectedCentralsCount(): Int = connectedCentrals.size

    /**
     * Check if GATT server is running.
     */
    fun isGattServerRunning(): Boolean = gattServer != null

    private fun checkInitialized() {
        if (!isInitialized) {
            throw IllegalStateException("PeatBtle not initialized. Call init() first.")
        }
    }

}

/**
 * Represents a discovered Peat BLE device.
 */
data class DiscoveredDevice(
    val address: String,
    val name: String,
    val rssi: Int,
    val nodeId: Long?,
    val meshId: String?,
    val timestampNanos: Long,
    val isPeatDevice: Boolean = false
)

/**
 * Represents a peer in the Peat mesh network.
 */
data class PeatPeer(
    val nodeId: Long,
    var address: String,  // Mutable to support BLE address rotation
    var name: String,     // Mutable to update when callsign is received
    val meshId: String?,
    var rssi: Int,
    var isConnected: Boolean,
    var isReconnecting: Boolean = false,
    var lastDocument: PeatDocument?,
    var lastSeen: Long
) {
    /**
     * Get the display name for this peer.
     * Priority: 1) callsign from document, 2) BLE device name, 3) Peat format with nodeId
     */
    fun displayName(): String {
        // First: try callsign from received document (most user-friendly)
        val docCallsign = lastDocument?.peripheral?.callsign?.takeIf { it.isNotEmpty() }
        if (docCallsign != null) {
            return docCallsign
        }

        // Second: use BLE device name if it looks like a WearTAK name
        if (name.isNotEmpty() && (name.startsWith("WEAROS-") || name.startsWith("WT-WEAROS-"))) {
            return name.removePrefix("WT-")  // Normalize to "WEAROS-XXXX"
        }

        // Third: fall back to Peat format
        return if (meshId != null) {
            "PEAT_${meshId}-${String.format("%08X", nodeId)}"
        } else {
            "PEAT-${String.format("%08X", nodeId)}"
        }
    }

    /**
     * Get the current event type from this peer's last document.
     */
    fun currentEventType(): PeatEventType = lastDocument?.currentEventType() ?: PeatEventType.NONE
}

/**
 * Listener interface for Peat mesh events.
 */
interface PeatMeshListener {
    /**
     * Called when the mesh state changes (peers added/removed/updated).
     * @param peers Current list of all known peers
     */
    fun onMeshUpdated(peers: List<PeatPeer>)

    /**
     * Called when a peer sends an event (Emergency, ACK, etc.).
     * @param peer The peer that sent the event
     * @param eventType The event type
     */
    fun onPeerEvent(peer: PeatPeer, eventType: PeatEventType)

    /**
     * Called when mesh document is synced.
     * @param document The merged document state
     */
    fun onDocumentSynced(document: PeatDocument) {}

    /**
     * Called when a peer connection is established.
     * @param peer The connected peer
     */
    fun onPeerConnected(peer: PeatPeer) {}

    /**
     * Called when a peer connection is lost.
     * Use this for immediate UI updates when a peer disconnects.
     * @param peer The disconnected peer
     */
    fun onPeerDisconnected(peer: PeatPeer) {}

    /**
     * Called when a map marker is synced from a peer.
     * @param peer The peer that sent the marker
     * @param marker The marker data
     */
    fun onMarkerSynced(peer: PeatPeer, marker: PeatMarker) {}

    /**
     * Called when a chat message is received from a mesh peer.
     * @param chat The received chat message
     * @param fromPeer The peer that relayed this message (may differ from chat.originNode for multi-hop)
     */
    fun onChatReceived(chat: PeatChat, fromPeer: PeatPeer) {}

    /**
     * Called when decrypted data is received from a peer.
     *
     * This is the raw transport callback - peat-btle only handles encryption/decryption,
     * the app is responsible for parsing message types using hive-lite or other libraries.
     *
     * Inspect data[0] to determine message type:
     * - 0xAF: app-layer message (use hive-lite app-layer messageEvent.decode())
     * - 0xAA: PeatDocument (legacy standalone format)
     * - 0xB2: DeltaDocument (legacy delta sync)
     *
     * @param peer The peer that sent the data (null if from unknown/anonymous source)
     * @param data Raw decrypted bytes
     */
    fun onDecryptedData(peer: PeatPeer?, data: ByteArray) {}
}

/**
 * Represents an active GATT connection to a Peat device.
 */
class PeatConnection internal constructor(
    val address: String,
    private val gatt: BluetoothGatt,
    private val callback: GattCallbackProxy
) {
    /**
     * Set a listener for document events.
     */
    fun setDocumentListener(listener: PeatDocumentListener?) {
        callback.documentListener = listener
    }

    /**
     * Request MTU change.
     *
     * @param mtu Desired MTU size (max 517 for BLE 5.0)
     * @return true if request was initiated
     */
    fun requestMtu(mtu: Int): Boolean {
        return try {
            gatt.requestMtu(mtu)
        } catch (e: SecurityException) {
            Log.e("PeatConnection", "Missing BLUETOOTH_CONNECT permission", e)
            false
        }
    }

    /**
     * Discover GATT services.
     *
     * @return true if discovery was initiated
     */
    fun discoverServices(): Boolean {
        return try {
            gatt.discoverServices()
        } catch (e: SecurityException) {
            Log.e("PeatConnection", "Missing BLUETOOTH_CONNECT permission", e)
            false
        }
    }

    /**
     * Read RSSI for this connection.
     *
     * @return true if read was initiated
     */
    fun readRssi(): Boolean {
        return try {
            gatt.readRemoteRssi()
        } catch (e: SecurityException) {
            Log.e("PeatConnection", "Missing BLUETOOTH_CONNECT permission", e)
            false
        }
    }

    /**
     * Read the Peat document characteristic.
     *
     * @return true if read was initiated
     */
    fun readDocument(): Boolean {
        return try {
            val service = gatt.getService(PeatBtle.PEAT_SERVICE_UUID)
            if (service == null) {
                Log.e("PeatConnection", "Peat service not found")
                return false
            }
            val char = service.getCharacteristic(PeatBtle.PEAT_CHAR_DOCUMENT)
            if (char == null) {
                Log.e("PeatConnection", "Peat document characteristic not found")
                return false
            }
            gatt.readCharacteristic(char)
        } catch (e: SecurityException) {
            Log.e("PeatConnection", "Missing BLUETOOTH_CONNECT permission", e)
            false
        }
    }

    /**
     * Write data to the Peat document characteristic.
     *
     * @param data The document data to write
     * @return true if write was initiated
     */
    fun writeDocument(data: ByteArray): Boolean {
        return try {
            val service = gatt.getService(PeatBtle.PEAT_SERVICE_UUID)
            if (service == null) {
                Log.e("PeatConnection", "Peat service not found")
                return false
            }
            val char = service.getCharacteristic(PeatBtle.PEAT_CHAR_DOCUMENT)
            if (char == null) {
                Log.e("PeatConnection", "Peat document characteristic not found")
                return false
            }
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
                gatt.writeCharacteristic(char, data, BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT)
            } else {
                @Suppress("DEPRECATION")
                char.value = data
                @Suppress("DEPRECATION")
                gatt.writeCharacteristic(char)
            }
            true
        } catch (e: SecurityException) {
            Log.e("PeatConnection", "Missing BLUETOOTH_CONNECT permission", e)
            false
        }
    }

    /**
     * Enable notifications for the Peat document characteristic.
     *
     * @return true if notification was enabled
     */
    fun enableDocumentNotifications(): Boolean {
        return try {
            val service = gatt.getService(PeatBtle.PEAT_SERVICE_UUID)
            if (service == null) {
                Log.e("PeatConnection", "Peat service not found")
                return false
            }
            val char = service.getCharacteristic(PeatBtle.PEAT_CHAR_DOCUMENT)
            if (char == null) {
                Log.e("PeatConnection", "Peat document characteristic not found")
                return false
            }

            // Enable local notifications
            if (!gatt.setCharacteristicNotification(char, true)) {
                Log.e("PeatConnection", "Failed to enable local notifications")
                return false
            }

            // Write to CCCD to enable remote notifications
            val descriptor = char.getDescriptor(PeatBtle.CCCD_UUID)
            if (descriptor == null) {
                Log.w("PeatConnection", "CCCD descriptor not found, notifications may not work")
                return true  // Local notifications are enabled at least
            }

            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.TIRAMISU) {
                gatt.writeDescriptor(descriptor, BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE)
            } else {
                @Suppress("DEPRECATION")
                descriptor.value = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
                @Suppress("DEPRECATION")
                gatt.writeDescriptor(descriptor)
            }
            true
        } catch (e: SecurityException) {
            Log.e("PeatConnection", "Missing BLUETOOTH_CONNECT permission", e)
            false
        }
    }
}

/**
 * Peat document event types.
 * Values must match the Rust EventType enum in peat-btle/src/sync/crdt.rs
 */
enum class PeatEventType(val value: Int) {
    NONE(0),
    PING(1),
    NEED_ASSIST(2),
    EMERGENCY(3),
    MOVING(4),
    IN_POSITION(5),
    ACK(6);

    companion object {
        fun fromValue(v: Int): PeatEventType = entries.find { it.value == v } ?: NONE

        /** Convert from UniFFI EventType enum */
        fun fromEventType(et: EventType): PeatEventType = when (et) {
            EventType.NONE -> NONE
            EventType.PING -> PING
            EventType.NEED_ASSIST -> NEED_ASSIST
            EventType.EMERGENCY -> EMERGENCY
            EventType.MOVING -> MOVING
            EventType.IN_POSITION -> IN_POSITION
            EventType.ACK -> ACK
        }
    }
}

/**
 * Peat Peripheral type.
 */
enum class PeatPeripheralType(val value: Int) {
    UNKNOWN(0),
    SOLDIER_SENSOR(1),
    VEHICLE(2),
    ASSET_TAG(3);

    companion object {
        fun fromValue(v: Int): PeatPeripheralType = entries.find { it.value == v } ?: UNKNOWN
    }
}

/**
 * Peat health status data.
 */
data class PeatHealthStatus(
    val batteryPercent: Int,
    val heartRate: Int?,
    val activityLevel: Int,
    val alerts: Int
) {
    companion object {
        const val ALERT_MAN_DOWN = 0x01
        const val ALERT_LOW_BATTERY = 0x02
        const val ALERT_GEOFENCE = 0x04
        const val ALERT_PANIC = 0x08

        fun decode(data: ByteArray, offset: Int): PeatHealthStatus? {
            if (data.size < offset + 4) return null
            val battery = data[offset].toInt() and 0xFF
            val hr = data[offset + 1].toInt() and 0xFF
            val activity = data[offset + 2].toInt() and 0xFF
            val alerts = data[offset + 3].toInt() and 0xFF
            return PeatHealthStatus(
                batteryPercent = battery,
                heartRate = if (hr > 0) hr else null,
                activityLevel = activity,
                alerts = alerts
            )
        }

        fun encode(status: PeatHealthStatus): ByteArray {
            return byteArrayOf(
                status.batteryPercent.toByte(),
                (status.heartRate ?: 0).toByte(),
                status.activityLevel.toByte(),
                status.alerts.toByte()
            )
        }
    }

    fun hasAlert(flag: Int): Boolean = (alerts and flag) != 0
}

/**
 * Peat peripheral event.
 */
data class PeatPeripheralEvent(
    val eventType: PeatEventType,
    val timestamp: Long
) {
    companion object {
        private const val SIZE = 9

        fun decode(data: ByteArray, offset: Int): PeatPeripheralEvent? {
            if (data.size < offset + SIZE) return null
            val eventType = PeatEventType.fromValue(data[offset].toInt() and 0xFF)
            val timestamp = readU64LE(data, offset + 1)
            return PeatPeripheralEvent(eventType, timestamp)
        }

        fun encode(event: PeatPeripheralEvent): ByteArray {
            val buf = ByteArray(SIZE)
            buf[0] = event.eventType.value.toByte()
            writeU64LE(buf, 1, event.timestamp)
            return buf
        }

        private fun readU64LE(data: ByteArray, offset: Int): Long {
            return ((data[offset].toLong() and 0xFF) or
                    ((data[offset + 1].toLong() and 0xFF) shl 8) or
                    ((data[offset + 2].toLong() and 0xFF) shl 16) or
                    ((data[offset + 3].toLong() and 0xFF) shl 24) or
                    ((data[offset + 4].toLong() and 0xFF) shl 32) or
                    ((data[offset + 5].toLong() and 0xFF) shl 40) or
                    ((data[offset + 6].toLong() and 0xFF) shl 48) or
                    ((data[offset + 7].toLong() and 0xFF) shl 56))
        }

        private fun writeU64LE(data: ByteArray, offset: Int, value: Long) {
            data[offset] = (value and 0xFF).toByte()
            data[offset + 1] = ((value shr 8) and 0xFF).toByte()
            data[offset + 2] = ((value shr 16) and 0xFF).toByte()
            data[offset + 3] = ((value shr 24) and 0xFF).toByte()
            data[offset + 4] = ((value shr 32) and 0xFF).toByte()
            data[offset + 5] = ((value shr 40) and 0xFF).toByte()
            data[offset + 6] = ((value shr 48) and 0xFF).toByte()
            data[offset + 7] = ((value shr 56) and 0xFF).toByte()
        }
    }
}

/**
 * Peat location data.
 */
data class PeatLocation(
    val latitude: Float,
    val longitude: Float,
    val altitude: Float
) {
    companion object {
        const val SIZE = 12  // 3 floats x 4 bytes

        fun decode(data: ByteArray, offset: Int): PeatLocation? {
            if (data.size < offset + SIZE) return null
            val lat = bytesToFloat(data, offset)
            val lon = bytesToFloat(data, offset + 4)
            val alt = bytesToFloat(data, offset + 8)
            return PeatLocation(lat, lon, alt)
        }

        fun encode(location: PeatLocation): ByteArray {
            val buf = ByteArray(SIZE)
            floatToBytes(location.latitude, buf, 0)
            floatToBytes(location.longitude, buf, 4)
            floatToBytes(location.altitude, buf, 8)
            return buf
        }

        private fun bytesToFloat(data: ByteArray, offset: Int): Float {
            val bits = ((data[offset].toInt() and 0xFF)) or
                    ((data[offset + 1].toInt() and 0xFF) shl 8) or
                    ((data[offset + 2].toInt() and 0xFF) shl 16) or
                    ((data[offset + 3].toInt() and 0xFF) shl 24)
            return java.lang.Float.intBitsToFloat(bits)
        }

        private fun floatToBytes(f: Float, buf: ByteArray, offset: Int) {
            val bits = java.lang.Float.floatToIntBits(f)
            buf[offset] = (bits and 0xFF).toByte()
            buf[offset + 1] = ((bits shr 8) and 0xFF).toByte()
            buf[offset + 2] = ((bits shr 16) and 0xFF).toByte()
            buf[offset + 3] = ((bits shr 24) and 0xFF).toByte()
        }
    }
}

/**
 * Peat Peripheral data structure.
 * Format: [id:4][parent:4][type:1][callsign:12][health:4][has_event:1][event:9?][has_location:1][location:12?][timestamp:8]
 */
data class PeatPeripheral(
    val id: Long,
    val parentNode: Long,
    val peripheralType: PeatPeripheralType,
    val callsign: String,
    val health: PeatHealthStatus,
    val lastEvent: PeatPeripheralEvent?,
    val location: PeatLocation?,
    val timestamp: Long
) {
    companion object {
        private const val TAG = "PeatPeripheral"
        private const val MIN_SIZE = 35  // Without event or location (added 1 byte for hasLocation flag)
        private const val SIZE_WITH_EVENT = 44  // With event, no location
        private const val SIZE_WITH_LOCATION = 47  // No event, with location
        private const val SIZE_WITH_BOTH = 56  // With event and location

        fun decode(data: ByteArray, offset: Int = 0): PeatPeripheral? {
            if (data.size < offset + MIN_SIZE) {
                Log.e(TAG, "Peripheral data too short: ${data.size - offset} bytes (need $MIN_SIZE)")
                return null
            }

            var pos = offset
            val id = readU32LE(data, pos)
            pos += 4
            val parentNode = readU32LE(data, pos)
            pos += 4
            val peripheralType = PeatPeripheralType.fromValue(data[pos].toInt() and 0xFF)
            pos += 1

            // Read callsign (12 bytes, null-terminated string)
            val callsignBytes = data.sliceArray(pos until pos + 12)
            val nullIndex = callsignBytes.indexOf(0)
            val callsign = if (nullIndex >= 0) {
                String(callsignBytes, 0, nullIndex, Charsets.UTF_8)
            } else {
                String(callsignBytes, Charsets.UTF_8)
            }
            pos += 12

            val health = PeatHealthStatus.decode(data, pos)
            if (health == null) {
                Log.e(TAG, "Failed to decode health status")
                return null
            }
            pos += 4

            val hasEvent = data[pos] != 0.toByte()
            pos += 1

            val lastEvent = if (hasEvent) {
                val event = PeatPeripheralEvent.decode(data, pos)
                pos += 9
                event
            } else {
                null
            }

            // Read location flag (new field - check if we have enough bytes)
            val hasLocation = if (data.size > pos) {
                data[pos] != 0.toByte()
            } else {
                false  // Old format without location flag
            }
            if (data.size > pos) pos += 1

            val location = if (hasLocation && data.size >= pos + PeatLocation.SIZE) {
                val loc = PeatLocation.decode(data, pos)
                pos += PeatLocation.SIZE
                loc
            } else {
                null
            }

            if (data.size < pos + 8) {
                Log.e(TAG, "No room for timestamp at offset $pos")
                return null
            }
            val timestamp = readU64LE(data, pos)

            Log.d(TAG, "Decoded: id=${String.format("%08X", id)}, type=$peripheralType, " +
                    "event=${lastEvent?.eventType}, health=${health.batteryPercent}%, " +
                    "location=${location?.let { "(${it.latitude}, ${it.longitude})" } ?: "none"}")

            return PeatPeripheral(
                id = id,
                parentNode = parentNode,
                peripheralType = peripheralType,
                callsign = callsign,
                health = health,
                lastEvent = lastEvent,
                location = location,
                timestamp = timestamp
            )
        }

        fun encode(peripheral: PeatPeripheral): ByteArray {
            val hasEvent = peripheral.lastEvent != null
            val hasLocation = peripheral.location != null
            val size = when {
                hasEvent && hasLocation -> SIZE_WITH_BOTH
                hasEvent -> SIZE_WITH_EVENT
                hasLocation -> SIZE_WITH_LOCATION
                else -> MIN_SIZE
            }
            val buf = ByteArray(size)
            var pos = 0

            writeU32LE(buf, pos, peripheral.id)
            pos += 4
            writeU32LE(buf, pos, peripheral.parentNode)
            pos += 4
            buf[pos] = peripheral.peripheralType.value.toByte()
            pos += 1

            // Write callsign (12 bytes)
            val callsignBytes = peripheral.callsign.toByteArray(Charsets.UTF_8)
            for (i in 0 until 12) {
                buf[pos + i] = if (i < callsignBytes.size) callsignBytes[i] else 0
            }
            pos += 12

            val healthBytes = PeatHealthStatus.encode(peripheral.health)
            healthBytes.copyInto(buf, pos)
            pos += 4

            buf[pos] = if (hasEvent) 1 else 0
            pos += 1

            if (hasEvent && peripheral.lastEvent != null) {
                val eventBytes = PeatPeripheralEvent.encode(peripheral.lastEvent)
                eventBytes.copyInto(buf, pos)
                pos += 9
            }

            // Write location flag and data
            buf[pos] = if (hasLocation) 1 else 0
            pos += 1

            if (hasLocation && peripheral.location != null) {
                val locationBytes = PeatLocation.encode(peripheral.location)
                locationBytes.copyInto(buf, pos)
                pos += PeatLocation.SIZE
            }

            writeU64LE(buf, pos, peripheral.timestamp)
            return buf
        }

        private fun readU32LE(data: ByteArray, offset: Int): Long {
            return ((data[offset].toLong() and 0xFF) or
                    ((data[offset + 1].toLong() and 0xFF) shl 8) or
                    ((data[offset + 2].toLong() and 0xFF) shl 16) or
                    ((data[offset + 3].toLong() and 0xFF) shl 24))
        }

        private fun readU64LE(data: ByteArray, offset: Int): Long {
            return ((data[offset].toLong() and 0xFF) or
                    ((data[offset + 1].toLong() and 0xFF) shl 8) or
                    ((data[offset + 2].toLong() and 0xFF) shl 16) or
                    ((data[offset + 3].toLong() and 0xFF) shl 24) or
                    ((data[offset + 4].toLong() and 0xFF) shl 32) or
                    ((data[offset + 5].toLong() and 0xFF) shl 40) or
                    ((data[offset + 6].toLong() and 0xFF) shl 48) or
                    ((data[offset + 7].toLong() and 0xFF) shl 56))
        }

        private fun writeU32LE(data: ByteArray, offset: Int, value: Long) {
            data[offset] = (value and 0xFF).toByte()
            data[offset + 1] = ((value shr 8) and 0xFF).toByte()
            data[offset + 2] = ((value shr 16) and 0xFF).toByte()
            data[offset + 3] = ((value shr 24) and 0xFF).toByte()
        }

        private fun writeU64LE(data: ByteArray, offset: Int, value: Long) {
            data[offset] = (value and 0xFF).toByte()
            data[offset + 1] = ((value shr 8) and 0xFF).toByte()
            data[offset + 2] = ((value shr 16) and 0xFF).toByte()
            data[offset + 3] = ((value shr 24) and 0xFF).toByte()
            data[offset + 4] = ((value shr 32) and 0xFF).toByte()
            data[offset + 5] = ((value shr 40) and 0xFF).toByte()
            data[offset + 6] = ((value shr 48) and 0xFF).toByte()
            data[offset + 7] = ((value shr 56) and 0xFF).toByte()
        }
    }

    /**
     * Get the current event type, or NONE if no event.
     */
    fun currentEventType(): PeatEventType = lastEvent?.eventType ?: PeatEventType.NONE
}

/**
 * Peat CRDT GCounter entry.
 */
data class GCounterEntry(
    val nodeId: Long,
    val count: Long
)

/**
 * Peat document format (compatible with M5Stack peat-lite).
 *
 * Wire format:
 * - Header: version (u32 LE) + node_id (u32 LE)
 * - GCounter: num_entries (u32 LE) + [node_id (u32 LE) + count (u64 LE)] * N
 * - Extended: 0xAB marker + reserved (u8) + peripheral_len (u16 LE) + peripheral data
 */
data class PeatDocument(
    val version: Long,
    val nodeId: Long,
    val counter: List<GCounterEntry>,
    val peripheral: PeatPeripheral?
) {
    companion object {
        private const val TAG = "PeatDocument"
        private const val EXTENDED_MARKER: Byte = 0xAB.toByte()

        /**
         * Decode a Peat document from raw bytes.
         *
         * @param data Raw document bytes
         * @return Decoded document, or null if parsing failed
         */
        fun decode(data: ByteArray): PeatDocument? {
            if (data.size < 8) {
                Log.e(TAG, "Document too short: ${data.size} bytes (minimum 8)")
                return null
            }

            // Check for encrypted document marker (0xAE) - these are handled by native CRDT
            if (data[0] == 0xAE.toByte()) {
                Log.d(TAG, "Skipping encrypted document (${data.size} bytes) - handled by native layer")
                return null
            }

            try {
                var offset = 0

                // Read header
                val version = readU32LE(data, offset)
                offset += 4
                val nodeId = readU32LE(data, offset)
                offset += 4

                Log.d(TAG, "Header: version=$version, nodeId=${String.format("%08X", nodeId)}")

                // Read GCounter
                if (data.size < offset + 4) {
                    Log.e(TAG, "Document too short for GCounter count")
                    return null
                }
                val numEntries = readU32LE(data, offset).toInt()
                offset += 4

                if (data.size < offset + numEntries * 12) {
                    Log.e(TAG, "Document too short for GCounter entries: need ${offset + numEntries * 12}, have ${data.size}")
                    return null
                }

                val counter = mutableListOf<GCounterEntry>()
                for (i in 0 until numEntries) {
                    val entryNodeId = readU32LE(data, offset)
                    offset += 4
                    val count = readU64LE(data, offset)
                    offset += 8
                    counter.add(GCounterEntry(entryNodeId, count))
                    Log.d(TAG, "GCounter[$i]: nodeId=${String.format("%08X", entryNodeId)}, count=$count")
                }

                // Check for extended data (peripheral)
                var peripheral: PeatPeripheral? = null
                if (data.size > offset && data[offset] == EXTENDED_MARKER) {
                    offset += 1  // Skip marker
                    if (data.size >= offset + 3) {
                        val reserved = data[offset].toInt() and 0xFF
                        offset += 1
                        val peripheralLen = readU16LE(data, offset).toInt()
                        offset += 2

                        Log.d(TAG, "Extended: reserved=$reserved, peripheralLen=$peripheralLen")

                        if (data.size >= offset + peripheralLen && peripheralLen > 0) {
                            // Decode full Peripheral structure
                            peripheral = PeatPeripheral.decode(data, offset)
                            if (peripheral != null) {
                                Log.d(TAG, "Peripheral: eventType=${peripheral.currentEventType()}, " +
                                        "battery=${peripheral.health.batteryPercent}%")
                            } else {
                                Log.w(TAG, "Failed to decode peripheral data ($peripheralLen bytes)")
                            }
                        }
                    }
                }

                return PeatDocument(version, nodeId, counter, peripheral)

            } catch (e: Exception) {
                Log.e(TAG, "Failed to decode document", e)
                return null
            }
        }

        /**
         * Create an encoded Peat document with full Peripheral structure.
         *
         * @param nodeId This node's ID
         * @param counter GCounter entries
         * @param peripheral Optional Peripheral data (contains event, health, etc.)
         * @return Encoded document bytes
         */
        fun encode(nodeId: Long, counter: List<GCounterEntry>, peripheral: PeatPeripheral? = null): ByteArray {
            val headerSize = 8  // version + nodeId
            val counterSize = 4 + counter.size * 12  // count + entries
            val peripheralBytes = peripheral?.let { PeatPeripheral.encode(it) }
            val extendedSize = if (peripheralBytes != null) 4 + peripheralBytes.size else 0  // marker + reserved + len(2) + data
            val totalSize = headerSize + counterSize + extendedSize

            val data = ByteArray(totalSize)
            var offset = 0

            // Write header
            writeU32LE(data, offset, 1)  // version = 1
            offset += 4
            writeU32LE(data, offset, nodeId)
            offset += 4

            // Write GCounter
            writeU32LE(data, offset, counter.size.toLong())
            offset += 4
            for (entry in counter) {
                writeU32LE(data, offset, entry.nodeId)
                offset += 4
                writeU64LE(data, offset, entry.count)
                offset += 8
            }

            // Write extended data (Peripheral)
            if (peripheralBytes != null) {
                data[offset] = EXTENDED_MARKER
                offset += 1
                data[offset] = 0  // reserved
                offset += 1
                writeU16LE(data, offset, peripheralBytes.size.toLong())
                offset += 2
                peripheralBytes.copyInto(data, offset)
            }

            return data
        }

        /**
         * Create an encoded Peat document with just an event type (simple form).
         *
         * @param nodeId This node's ID
         * @param counter GCounter entries
         * @param eventType Optional event type
         * @param location Optional location data
         * @return Encoded document bytes
         */
        fun encodeWithEvent(
            nodeId: Long,
            counter: List<GCounterEntry>,
            eventType: PeatEventType = PeatEventType.NONE,
            location: PeatLocation? = null
        ): ByteArray {
            val peripheral = if (eventType != PeatEventType.NONE || location != null) {
                val timestamp = System.currentTimeMillis()
                PeatPeripheral(
                    id = nodeId,
                    parentNode = 0,
                    peripheralType = PeatPeripheralType.SOLDIER_SENSOR,
                    callsign = "",
                    health = PeatHealthStatus(100, null, 0, 0),
                    lastEvent = if (eventType != PeatEventType.NONE) PeatPeripheralEvent(eventType, timestamp) else null,
                    location = location,
                    timestamp = timestamp
                )
            } else null
            return encode(nodeId, counter, peripheral)
        }

        private fun readU16LE(data: ByteArray, offset: Int): Long {
            return ((data[offset].toLong() and 0xFF) or
                    ((data[offset + 1].toLong() and 0xFF) shl 8))
        }

        private fun readU32LE(data: ByteArray, offset: Int): Long {
            return ((data[offset].toLong() and 0xFF) or
                    ((data[offset + 1].toLong() and 0xFF) shl 8) or
                    ((data[offset + 2].toLong() and 0xFF) shl 16) or
                    ((data[offset + 3].toLong() and 0xFF) shl 24))
        }

        private fun readU64LE(data: ByteArray, offset: Int): Long {
            return ((data[offset].toLong() and 0xFF) or
                    ((data[offset + 1].toLong() and 0xFF) shl 8) or
                    ((data[offset + 2].toLong() and 0xFF) shl 16) or
                    ((data[offset + 3].toLong() and 0xFF) shl 24) or
                    ((data[offset + 4].toLong() and 0xFF) shl 32) or
                    ((data[offset + 5].toLong() and 0xFF) shl 40) or
                    ((data[offset + 6].toLong() and 0xFF) shl 48) or
                    ((data[offset + 7].toLong() and 0xFF) shl 56))
        }

        private fun writeU16LE(data: ByteArray, offset: Int, value: Long) {
            data[offset] = (value and 0xFF).toByte()
            data[offset + 1] = ((value shr 8) and 0xFF).toByte()
        }

        private fun writeU32LE(data: ByteArray, offset: Int, value: Long) {
            data[offset] = (value and 0xFF).toByte()
            data[offset + 1] = ((value shr 8) and 0xFF).toByte()
            data[offset + 2] = ((value shr 16) and 0xFF).toByte()
            data[offset + 3] = ((value shr 24) and 0xFF).toByte()
        }

        private fun writeU64LE(data: ByteArray, offset: Int, value: Long) {
            data[offset] = (value and 0xFF).toByte()
            data[offset + 1] = ((value shr 8) and 0xFF).toByte()
            data[offset + 2] = ((value shr 16) and 0xFF).toByte()
            data[offset + 3] = ((value shr 24) and 0xFF).toByte()
            data[offset + 4] = ((value shr 32) and 0xFF).toByte()
            data[offset + 5] = ((value shr 40) and 0xFF).toByte()
            data[offset + 6] = ((value shr 48) and 0xFF).toByte()
            data[offset + 7] = ((value shr 56) and 0xFF).toByte()
        }
    }

    /**
     * Get the total count from all GCounter entries.
     */
    fun totalCount(): Long = counter.sumOf { it.count }

    /**
     * Get the current event type from peripheral data.
     */
    fun currentEventType(): PeatEventType = peripheral?.currentEventType() ?: PeatEventType.NONE

    /**
     * Get the battery percentage from peripheral health data.
     */
    fun batteryPercent(): Int? = peripheral?.health?.batteryPercent

    /**
     * Get the location from peripheral data.
     */
    fun location(): PeatLocation? = peripheral?.location

    /**
     * Get the callsign from peripheral data.
     */
    fun callsign(): String? = peripheral?.callsign?.takeIf { it.isNotEmpty() }

    /**
     * Get the heart rate from peripheral health data.
     */
    fun heartRate(): Int? = peripheral?.health?.heartRate
}

// =============================================================================
// MARKER SUPPORT
// =============================================================================

/**
 * Marker section marker byte (0xAC).
 * Used after peripheral section to encode map markers for mesh sync.
 */
const val MARKER_SECTION_MARKER: Byte = 0xAC.toByte()

/**
 * Compact marker format for BLE transmission (~84 bytes typical).
 * Compatible with CotPeatTranslator.CompactMarker format.
 */
data class PeatMarker(
    val uid: String,        // 36 bytes max (UUID)
    val type: String,       // 12 bytes max (a-f-G-U-C)
    val lat: Float,         // 4 bytes
    val lon: Float,         // 4 bytes
    val hae: Float,         // 4 bytes
    val callsign: String,   // 16 bytes max
    val time: Long          // 8 bytes
) {
    companion object {
        private const val TAG = "PeatMarker"

        /**
         * Encode a marker to compact binary format.
         */
        fun encode(marker: PeatMarker): ByteArray {
            val uidBytes = marker.uid.take(36).toByteArray(Charsets.UTF_8)
            val typeBytes = marker.type.take(12).toByteArray(Charsets.UTF_8)
            val csBytes = marker.callsign.take(16).toByteArray(Charsets.UTF_8)

            // Simple length-prefixed encoding
            val result = mutableListOf<Byte>()

            // UID (length + bytes)
            result.add(uidBytes.size.toByte())
            result.addAll(uidBytes.toList())

            // Type (length + bytes)
            result.add(typeBytes.size.toByte())
            result.addAll(typeBytes.toList())

            // Lat/Lon/Hae as floats (12 bytes, big-endian for consistency)
            result.addAll(floatToBytesLE(marker.lat).toList())
            result.addAll(floatToBytesLE(marker.lon).toList())
            result.addAll(floatToBytesLE(marker.hae).toList())

            // Callsign (length + bytes)
            result.add(csBytes.size.toByte())
            result.addAll(csBytes.toList())

            // Time (8 bytes, little-endian)
            result.addAll(longToBytesLE(marker.time).toList())

            return result.toByteArray()
        }

        /**
         * Decode a marker from compact binary format.
         */
        fun decode(data: ByteArray, startOffset: Int = 0): Pair<PeatMarker?, Int> {
            try {
                var offset = startOffset

                // UID
                val uidLen = data[offset++].toInt() and 0xFF
                if (offset + uidLen > data.size) return null to startOffset
                val uid = String(data, offset, uidLen, Charsets.UTF_8)
                offset += uidLen

                // Type
                val typeLen = data[offset++].toInt() and 0xFF
                if (offset + typeLen > data.size) return null to startOffset
                val type = String(data, offset, typeLen, Charsets.UTF_8)
                offset += typeLen

                // Lat/Lon/Hae (12 bytes)
                if (offset + 12 > data.size) return null to startOffset
                val lat = bytesToFloatLE(data.sliceArray(offset until offset + 4))
                offset += 4
                val lon = bytesToFloatLE(data.sliceArray(offset until offset + 4))
                offset += 4
                val hae = bytesToFloatLE(data.sliceArray(offset until offset + 4))
                offset += 4

                // Callsign
                val csLen = data[offset++].toInt() and 0xFF
                if (offset + csLen > data.size) return null to startOffset
                val callsign = String(data, offset, csLen, Charsets.UTF_8)
                offset += csLen

                // Time (8 bytes)
                if (offset + 8 > data.size) return null to startOffset
                val time = bytesToLongLE(data.sliceArray(offset until offset + 8))
                offset += 8

                return PeatMarker(uid, type, lat, lon, hae, callsign, time) to offset
            } catch (e: Exception) {
                Log.e(TAG, "Failed to decode PeatMarker: ${e.message}")
                return null to startOffset
            }
        }

        private fun floatToBytesLE(f: Float): ByteArray {
            val bits = java.lang.Float.floatToIntBits(f)
            return byteArrayOf(
                bits.toByte(),
                (bits shr 8).toByte(),
                (bits shr 16).toByte(),
                (bits shr 24).toByte()
            )
        }

        private fun bytesToFloatLE(bytes: ByteArray): Float {
            val bits = ((bytes[0].toInt() and 0xFF)) or
                    ((bytes[1].toInt() and 0xFF) shl 8) or
                    ((bytes[2].toInt() and 0xFF) shl 16) or
                    ((bytes[3].toInt() and 0xFF) shl 24)
            return java.lang.Float.intBitsToFloat(bits)
        }

        private fun longToBytesLE(l: Long): ByteArray {
            return byteArrayOf(
                l.toByte(),
                (l shr 8).toByte(),
                (l shr 16).toByte(),
                (l shr 24).toByte(),
                (l shr 32).toByte(),
                (l shr 40).toByte(),
                (l shr 48).toByte(),
                (l shr 56).toByte()
            )
        }

        private fun bytesToLongLE(bytes: ByteArray): Long {
            return ((bytes[0].toLong() and 0xFF)) or
                    ((bytes[1].toLong() and 0xFF) shl 8) or
                    ((bytes[2].toLong() and 0xFF) shl 16) or
                    ((bytes[3].toLong() and 0xFF) shl 24) or
                    ((bytes[4].toLong() and 0xFF) shl 32) or
                    ((bytes[5].toLong() and 0xFF) shl 40) or
                    ((bytes[6].toLong() and 0xFF) shl 48) or
                    ((bytes[7].toLong() and 0xFF) shl 56)
        }
    }
}

// =============================================================================
// CHAT DOCUMENT SUPPORT
// =============================================================================

/**
 * Chat document marker byte (0xAD).
 * Documents starting with this byte contain chat messages.
 */
const val CHAT_SECTION_MARKER: Byte = 0xAD.toByte()

/**
 * Chat message format for BLE transmission (typically 30-180 bytes).
 *
 * Wire format:
 * - marker:     1 byte  (0xAD)
 * - flags:      1 byte  (bit 0: is_broadcast, bit 1: requires_ack)
 * - originNode: 4 bytes (LE)
 * - timestamp:  8 bytes (LE)
 * - senderLen:  1 byte
 * - sender:     N bytes (max 16)
 * - msgLen:     1 byte
 * - message:    N bytes (max 140)
 * - replyToNode: 4 bytes (LE) - originNode of message being replied to (0 = not a reply)
 * - replyToTimestamp: 8 bytes (LE) - timestamp of message being replied to
 *
 * Message ID is implicitly (originNode, timestamp) which uniquely identifies each message.
 */
data class PeatChat(
    val sender: String,         // Sender callsign (max 16 chars)
    val message: String,        // Message text (max 140 chars)
    val timestamp: Long,        // Epoch milliseconds
    val originNode: Long,       // Sender's node ID
    val isBroadcast: Boolean = true,
    val requiresAck: Boolean = false,
    val replyToNode: Long = 0,  // originNode of message being replied to (0 = not a reply)
    val replyToTimestamp: Long = 0  // timestamp of message being replied to
) {
    /**
     * Check if this message is a reply to another message.
     */
    fun isReply(): Boolean = replyToNode != 0L || replyToTimestamp != 0L

    /**
     * Get the message ID as a string for display/logging.
     * Format: "XXXXXXXX:timestamp"
     */
    fun messageIdString(): String = "${String.format("%08X", originNode)}:$timestamp"

    /**
     * Get the ID of the message being replied to as a string.
     */
    fun replyToIdString(): String? = if (isReply()) "${String.format("%08X", replyToNode)}:$replyToTimestamp" else null

    companion object {
        private const val TAG = "PeatChat"
        /** Maximum sender length (12 chars for CRDT compatibility) */
        const val MAX_SENDER_LENGTH = 12
        /** Maximum message length (128 chars for CRDT compatibility) */
        const val MAX_MESSAGE_LENGTH = 128

        /**
         * Encode a chat message to binary format.
         */
        fun encode(chat: PeatChat): ByteArray {
            val senderBytes = chat.sender.take(MAX_SENDER_LENGTH).toByteArray(Charsets.UTF_8)
            val messageBytes = chat.message.take(MAX_MESSAGE_LENGTH).toByteArray(Charsets.UTF_8)

            val result = mutableListOf<Byte>()

            // Marker byte (0xAD)
            result.add(CHAT_SECTION_MARKER)

            // Flags
            var flags: Byte = 0
            if (chat.isBroadcast) flags = (flags.toInt() or 0x01).toByte()
            if (chat.requiresAck) flags = (flags.toInt() or 0x02).toByte()
            result.add(flags)

            // Origin node (4 bytes LE)
            result.add(chat.originNode.toByte())
            result.add((chat.originNode shr 8).toByte())
            result.add((chat.originNode shr 16).toByte())
            result.add((chat.originNode shr 24).toByte())

            // Timestamp (8 bytes LE)
            result.add(chat.timestamp.toByte())
            result.add((chat.timestamp shr 8).toByte())
            result.add((chat.timestamp shr 16).toByte())
            result.add((chat.timestamp shr 24).toByte())
            result.add((chat.timestamp shr 32).toByte())
            result.add((chat.timestamp shr 40).toByte())
            result.add((chat.timestamp shr 48).toByte())
            result.add((chat.timestamp shr 56).toByte())

            // Sender (length + bytes)
            result.add(senderBytes.size.toByte())
            result.addAll(senderBytes.toList())

            // Message (length + bytes)
            result.add(messageBytes.size.toByte())
            result.addAll(messageBytes.toList())

            // ReplyTo node (4 bytes LE) - for threading support
            result.add(chat.replyToNode.toByte())
            result.add((chat.replyToNode shr 8).toByte())
            result.add((chat.replyToNode shr 16).toByte())
            result.add((chat.replyToNode shr 24).toByte())

            // ReplyTo timestamp (8 bytes LE)
            result.add(chat.replyToTimestamp.toByte())
            result.add((chat.replyToTimestamp shr 8).toByte())
            result.add((chat.replyToTimestamp shr 16).toByte())
            result.add((chat.replyToTimestamp shr 24).toByte())
            result.add((chat.replyToTimestamp shr 32).toByte())
            result.add((chat.replyToTimestamp shr 40).toByte())
            result.add((chat.replyToTimestamp shr 48).toByte())
            result.add((chat.replyToTimestamp shr 56).toByte())

            return result.toByteArray()
        }

        /**
         * Decode a chat message from binary format.
         */
        fun decode(data: ByteArray, startOffset: Int = 0): PeatChat? {
            try {
                var offset = startOffset

                // Check marker
                if (data[offset] != CHAT_SECTION_MARKER) {
                    Log.w(TAG, "Invalid chat marker: ${data[offset]}")
                    return null
                }
                offset++

                // Flags
                val flags = data[offset++].toInt() and 0xFF
                val isBroadcast = (flags and 0x01) != 0
                val requiresAck = (flags and 0x02) != 0

                // Origin node (4 bytes LE)
                if (offset + 4 > data.size) return null
                val originNode = ((data[offset].toLong() and 0xFF)) or
                        ((data[offset + 1].toLong() and 0xFF) shl 8) or
                        ((data[offset + 2].toLong() and 0xFF) shl 16) or
                        ((data[offset + 3].toLong() and 0xFF) shl 24)
                offset += 4

                // Timestamp (8 bytes LE)
                if (offset + 8 > data.size) return null
                val timestamp = ((data[offset].toLong() and 0xFF)) or
                        ((data[offset + 1].toLong() and 0xFF) shl 8) or
                        ((data[offset + 2].toLong() and 0xFF) shl 16) or
                        ((data[offset + 3].toLong() and 0xFF) shl 24) or
                        ((data[offset + 4].toLong() and 0xFF) shl 32) or
                        ((data[offset + 5].toLong() and 0xFF) shl 40) or
                        ((data[offset + 6].toLong() and 0xFF) shl 48) or
                        ((data[offset + 7].toLong() and 0xFF) shl 56)
                offset += 8

                // Sender
                val senderLen = data[offset++].toInt() and 0xFF
                if (offset + senderLen > data.size) return null
                val sender = String(data, offset, senderLen, Charsets.UTF_8)
                offset += senderLen

                // Message
                val msgLen = data[offset++].toInt() and 0xFF
                if (offset + msgLen > data.size) return null
                val message = String(data, offset, msgLen, Charsets.UTF_8)
                offset += msgLen

                // ReplyTo fields (optional, for backward compatibility)
                var replyToNode: Long = 0
                var replyToTimestamp: Long = 0

                // Check if there's enough data for replyToNode (4 bytes)
                if (offset + 4 <= data.size) {
                    replyToNode = ((data[offset].toLong() and 0xFF)) or
                            ((data[offset + 1].toLong() and 0xFF) shl 8) or
                            ((data[offset + 2].toLong() and 0xFF) shl 16) or
                            ((data[offset + 3].toLong() and 0xFF) shl 24)
                    offset += 4

                    // Check if there's enough data for replyToTimestamp (8 bytes)
                    if (offset + 8 <= data.size) {
                        replyToTimestamp = ((data[offset].toLong() and 0xFF)) or
                                ((data[offset + 1].toLong() and 0xFF) shl 8) or
                                ((data[offset + 2].toLong() and 0xFF) shl 16) or
                                ((data[offset + 3].toLong() and 0xFF) shl 24) or
                                ((data[offset + 4].toLong() and 0xFF) shl 32) or
                                ((data[offset + 5].toLong() and 0xFF) shl 40) or
                                ((data[offset + 6].toLong() and 0xFF) shl 48) or
                                ((data[offset + 7].toLong() and 0xFF) shl 56)
                    }
                }

                return PeatChat(
                    sender = sender,
                    message = message,
                    timestamp = timestamp,
                    originNode = originNode,
                    isBroadcast = isBroadcast,
                    requiresAck = requiresAck,
                    replyToNode = replyToNode,
                    replyToTimestamp = replyToTimestamp
                )
            } catch (e: Exception) {
                Log.e(TAG, "Failed to decode PeatChat: ${e.message}")
                return null
            }
        }
    }
}

// =============================================================================
// DELTA DOCUMENT SUPPORT
// =============================================================================

/**
 * Delta document marker byte (0xB2).
 * Documents starting with this byte are delta-encoded for bandwidth efficiency.
 */
const val DELTA_DOCUMENT_MARKER: Byte = 0xB2.toByte()

/**
 * Full sync interval - send full document every N syncs for consistency.
 */
const val FULL_SYNC_INTERVAL: Int = 10

/**
 * Delta operation type constants (matching Rust wire format).
 */
object DeltaOpType {
    const val INCREMENT_COUNTER: Byte = 0x01
    const val UPDATE_PERIPHERAL: Byte = 0x02  // Full peripheral (legacy, avoid)
    const val SET_EMERGENCY: Byte = 0x03
    const val ACK_EMERGENCY: Byte = 0x04
    const val CLEAR_EMERGENCY: Byte = 0x05

    // Field-level delta operations (bandwidth efficient)
    const val UPDATE_LOCATION: Byte = 0x10    // 12 bytes: lat(4) + lon(4) + alt(4)
    const val UPDATE_HEALTH: Byte = 0x11      // 4 bytes: battery(1) + hr(1) + activity(1) + alerts(1)
    const val UPDATE_CALLSIGN: Byte = 0x12    // 1-13 bytes: len(1) + callsign(0-12)
    const val UPDATE_EVENT: Byte = 0x13       // 9 bytes: type(1) + timestamp(8)
}

/**
 * Flags for delta document header.
 */
data class DeltaFlags(
    val hasVectorClock: Boolean = false,
    val isResponse: Boolean = false
) {
    fun toByte(): Byte {
        var flags = 0
        if (hasVectorClock) flags = flags or 0x01
        if (isResponse) flags = flags or 0x02
        return flags.toByte()
    }

    companion object {
        fun fromByte(byte: Byte): DeltaFlags {
            val b = byte.toInt() and 0xFF
            return DeltaFlags(
                hasVectorClock = (b and 0x01) != 0,
                isResponse = (b and 0x02) != 0
            )
        }
    }
}

/**
 * Delta operation sealed class - represents a single change to sync.
 */
sealed class DeltaOperation {
    abstract fun encode(): ByteArray

    data class IncrementCounter(
        val counterId: Byte,
        val nodeId: Long,
        val amount: Long,
        val timestamp: Long
    ) : DeltaOperation() {
        override fun encode(): ByteArray {
            val data = ByteArray(22)  // type(1) + counterId(1) + nodeId(4) + amount(8) + timestamp(8)
            var offset = 0
            data[offset++] = DeltaOpType.INCREMENT_COUNTER
            data[offset++] = counterId
            writeU32LE(data, offset, nodeId); offset += 4
            writeU64LE(data, offset, amount); offset += 8
            writeU64LE(data, offset, timestamp)
            return data
        }

        companion object {
            fun decode(data: ByteArray, offset: Int): Pair<IncrementCounter, Int>? {
                if (data.size < offset + 21) return null
                var pos = offset
                val counterId = data[pos++]
                val nodeId = readU32LE(data, pos); pos += 4
                val amount = readU64LE(data, pos); pos += 8
                val timestamp = readU64LE(data, pos); pos += 8
                return IncrementCounter(counterId, nodeId, amount, timestamp) to pos
            }
        }
    }

    data class UpdatePeripheral(
        val peripheral: PeatPeripheral,
        val timestamp: Long
    ) : DeltaOperation() {
        override fun encode(): ByteArray {
            val peripheralBytes = PeatPeripheral.encode(peripheral)
            val data = ByteArray(1 + 8 + 2 + peripheralBytes.size)  // type(1) + timestamp(8) + len(2) + peripheral
            var offset = 0
            data[offset++] = DeltaOpType.UPDATE_PERIPHERAL
            writeU64LE(data, offset, timestamp); offset += 8
            writeU16LE(data, offset, peripheralBytes.size.toLong()); offset += 2
            peripheralBytes.copyInto(data, offset)
            return data
        }

        companion object {
            fun decode(data: ByteArray, offset: Int): Pair<UpdatePeripheral, Int>? {
                if (data.size < offset + 10) return null
                var pos = offset
                val timestamp = readU64LE(data, pos); pos += 8
                val len = readU16LE(data, pos).toInt(); pos += 2
                if (data.size < pos + len) return null
                val peripheral = PeatPeripheral.decode(data, pos) ?: return null
                pos += len
                return UpdatePeripheral(peripheral, timestamp) to pos
            }
        }
    }

    data class SetEmergency(
        val sourceNode: Long,
        val timestamp: Long,
        val knownPeers: List<Long> = emptyList()
    ) : DeltaOperation() {
        override fun encode(): ByteArray {
            val data = ByteArray(1 + 4 + 8 + 1 + knownPeers.size * 4)  // type(1) + source(4) + ts(8) + count(1) + peers
            var offset = 0
            data[offset++] = DeltaOpType.SET_EMERGENCY
            writeU32LE(data, offset, sourceNode); offset += 4
            writeU64LE(data, offset, timestamp); offset += 8
            data[offset++] = knownPeers.size.toByte()
            for (peer in knownPeers) {
                writeU32LE(data, offset, peer); offset += 4
            }
            return data
        }

        companion object {
            fun decode(data: ByteArray, offset: Int): Pair<SetEmergency, Int>? {
                if (data.size < offset + 13) return null
                var pos = offset
                val sourceNode = readU32LE(data, pos); pos += 4
                val timestamp = readU64LE(data, pos); pos += 8
                val peerCount = data[pos++].toInt() and 0xFF
                if (data.size < pos + peerCount * 4) return null
                val knownPeers = mutableListOf<Long>()
                repeat(peerCount) {
                    knownPeers.add(readU32LE(data, pos)); pos += 4
                }
                return SetEmergency(sourceNode, timestamp, knownPeers) to pos
            }
        }
    }

    data class AckEmergency(
        val nodeId: Long,
        val emergencyTimestamp: Long
    ) : DeltaOperation() {
        override fun encode(): ByteArray {
            val data = ByteArray(13)  // type(1) + nodeId(4) + timestamp(8)
            var offset = 0
            data[offset++] = DeltaOpType.ACK_EMERGENCY
            writeU32LE(data, offset, nodeId); offset += 4
            writeU64LE(data, offset, emergencyTimestamp)
            return data
        }

        companion object {
            fun decode(data: ByteArray, offset: Int): Pair<AckEmergency, Int>? {
                if (data.size < offset + 12) return null
                var pos = offset
                val nodeId = readU32LE(data, pos); pos += 4
                val emergencyTimestamp = readU64LE(data, pos); pos += 8
                return AckEmergency(nodeId, emergencyTimestamp) to pos
            }
        }
    }

    data class ClearEmergency(
        val emergencyTimestamp: Long
    ) : DeltaOperation() {
        override fun encode(): ByteArray {
            val data = ByteArray(9)  // type(1) + timestamp(8)
            var offset = 0
            data[offset++] = DeltaOpType.CLEAR_EMERGENCY
            writeU64LE(data, offset, emergencyTimestamp)
            return data
        }

        companion object {
            fun decode(data: ByteArray, offset: Int): Pair<ClearEmergency, Int>? {
                if (data.size < offset + 8) return null
                val emergencyTimestamp = readU64LE(data, offset)
                return ClearEmergency(emergencyTimestamp) to (offset + 8)
            }
        }
    }

    // =========================================================================
    // FIELD-LEVEL DELTA OPERATIONS (bandwidth efficient)
    // =========================================================================

    /**
     * Update location only - 13 bytes total (type + lat + lon + alt as floats)
     */
    data class UpdateLocation(
        val latitude: Float,
        val longitude: Float,
        val altitude: Float
    ) : DeltaOperation() {
        override fun encode(): ByteArray {
            val data = ByteArray(13)  // type(1) + lat(4) + lon(4) + alt(4)
            var offset = 0
            data[offset++] = DeltaOpType.UPDATE_LOCATION
            writeF32LE(data, offset, latitude); offset += 4
            writeF32LE(data, offset, longitude); offset += 4
            writeF32LE(data, offset, altitude)
            return data
        }

        companion object {
            fun decode(data: ByteArray, offset: Int): Pair<UpdateLocation, Int>? {
                if (data.size < offset + 12) return null
                var pos = offset
                val lat = readF32LE(data, pos); pos += 4
                val lon = readF32LE(data, pos); pos += 4
                val alt = readF32LE(data, pos); pos += 4
                return UpdateLocation(lat, lon, alt) to pos
            }
        }
    }

    /**
     * Update health status only - 5 bytes total (type + battery + hr + activity + alerts)
     */
    data class UpdateHealth(
        val batteryPercent: Int,
        val heartRate: Int?,
        val activityLevel: Int,
        val alerts: Int
    ) : DeltaOperation() {
        override fun encode(): ByteArray {
            val data = ByteArray(5)  // type(1) + battery(1) + hr(1) + activity(1) + alerts(1)
            data[0] = DeltaOpType.UPDATE_HEALTH
            data[1] = batteryPercent.toByte()
            data[2] = (heartRate ?: 0).toByte()
            data[3] = activityLevel.toByte()
            data[4] = alerts.toByte()
            return data
        }

        companion object {
            fun decode(data: ByteArray, offset: Int): Pair<UpdateHealth, Int>? {
                if (data.size < offset + 4) return null
                val battery = data[offset].toInt() and 0xFF
                val hr = data[offset + 1].toInt() and 0xFF
                val activity = data[offset + 2].toInt() and 0xFF
                val alerts = data[offset + 3].toInt() and 0xFF
                return UpdateHealth(battery, if (hr > 0) hr else null, activity, alerts) to (offset + 4)
            }
        }
    }

    /**
     * Update callsign only - 2-14 bytes total (type + len + callsign)
     */
    data class UpdateCallsign(
        val callsign: String
    ) : DeltaOperation() {
        override fun encode(): ByteArray {
            val bytes = callsign.take(12).toByteArray(Charsets.UTF_8)
            val data = ByteArray(2 + bytes.size)  // type(1) + len(1) + callsign
            data[0] = DeltaOpType.UPDATE_CALLSIGN
            data[1] = bytes.size.toByte()
            bytes.copyInto(data, 2)
            return data
        }

        companion object {
            fun decode(data: ByteArray, offset: Int): Pair<UpdateCallsign, Int>? {
                if (data.size < offset + 1) return null
                val len = data[offset].toInt() and 0xFF
                if (data.size < offset + 1 + len) return null
                val callsign = String(data, offset + 1, len, Charsets.UTF_8)
                return UpdateCallsign(callsign) to (offset + 1 + len)
            }
        }
    }

    /**
     * Update event only - 10 bytes total (type + eventType + timestamp)
     */
    data class UpdateEvent(
        val eventType: PeatEventType,
        val timestamp: Long
    ) : DeltaOperation() {
        override fun encode(): ByteArray {
            val data = ByteArray(10)  // type(1) + eventType(1) + timestamp(8)
            var offset = 0
            data[offset++] = DeltaOpType.UPDATE_EVENT
            data[offset++] = eventType.value.toByte()
            writeU64LE(data, offset, timestamp)
            return data
        }

        companion object {
            fun decode(data: ByteArray, offset: Int): Pair<UpdateEvent, Int>? {
                if (data.size < offset + 9) return null
                val eventValue = data[offset].toInt() and 0xFF
                val eventType = PeatEventType.entries.find { it.value == eventValue } ?: PeatEventType.NONE
                val timestamp = readU64LE(data, offset + 1)
                return UpdateEvent(eventType, timestamp) to (offset + 9)
            }
        }
    }
}

// Float read/write helpers for location deltas
private fun readF32LE(data: ByteArray, offset: Int): Float {
    val bits = ((data[offset].toInt() and 0xFF) or
                ((data[offset + 1].toInt() and 0xFF) shl 8) or
                ((data[offset + 2].toInt() and 0xFF) shl 16) or
                ((data[offset + 3].toInt() and 0xFF) shl 24))
    return Float.fromBits(bits)
}

private fun writeF32LE(data: ByteArray, offset: Int, value: Float) {
    val bits = value.toRawBits()
    data[offset] = (bits and 0xFF).toByte()
    data[offset + 1] = ((bits shr 8) and 0xFF).toByte()
    data[offset + 2] = ((bits shr 16) and 0xFF).toByte()
    data[offset + 3] = ((bits shr 24) and 0xFF).toByte()
}

/**
 * Peat Delta Document format for bandwidth-efficient sync.
 *
 * Wire format (0xB2):
 * - marker: 1 byte (0xB2)
 * - flags: 1 byte
 * - origin_node: 4 bytes (LE)
 * - timestamp_ms: 8 bytes (LE)
 * - op_count: 2 bytes (LE)
 * - operations: variable
 */
data class PeatDeltaDocument(
    val originNode: Long,
    val timestampMs: Long,
    val flags: DeltaFlags = DeltaFlags(),
    val operations: List<DeltaOperation>
) {
    companion object {
        private const val TAG = "PeatDeltaDocument"

        /**
         * Check if data is a delta document (starts with 0xB2 marker).
         */
        fun isDeltaDocument(data: ByteArray): Boolean {
            return data.isNotEmpty() && data[0] == DELTA_DOCUMENT_MARKER
        }

        /**
         * Decode a delta document from raw bytes.
         */
        fun decode(data: ByteArray): PeatDeltaDocument? {
            if (data.size < 16) {  // marker(1) + flags(1) + origin(4) + timestamp(8) + opcount(2)
                Log.e(TAG, "Delta document too short: ${data.size} bytes")
                return null
            }

            try {
                var offset = 0

                // Check marker
                if (data[offset++] != DELTA_DOCUMENT_MARKER) {
                    Log.e(TAG, "Invalid delta marker")
                    return null
                }

                // Read flags
                val flags = DeltaFlags.fromByte(data[offset++])

                // Read header
                val originNode = readU32LE(data, offset); offset += 4
                val timestampMs = readU64LE(data, offset); offset += 8

                // Read operation count
                val opCount = readU16LE(data, offset).toInt(); offset += 2

                Log.d(TAG, "Delta: origin=${String.format("%08X", originNode)}, ts=$timestampMs, ops=$opCount")

                // Parse operations
                val operations = mutableListOf<DeltaOperation>()
                for (i in 0 until opCount) {
                    if (offset >= data.size) break
                    val opType = data[offset++]
                    val result: Pair<DeltaOperation, Int>? = when (opType) {
                        DeltaOpType.INCREMENT_COUNTER -> DeltaOperation.IncrementCounter.decode(data, offset)
                        DeltaOpType.UPDATE_PERIPHERAL -> DeltaOperation.UpdatePeripheral.decode(data, offset)
                        DeltaOpType.SET_EMERGENCY -> DeltaOperation.SetEmergency.decode(data, offset)
                        DeltaOpType.ACK_EMERGENCY -> DeltaOperation.AckEmergency.decode(data, offset)
                        DeltaOpType.CLEAR_EMERGENCY -> DeltaOperation.ClearEmergency.decode(data, offset)
                        // Field-level delta operations
                        DeltaOpType.UPDATE_LOCATION -> DeltaOperation.UpdateLocation.decode(data, offset)
                        DeltaOpType.UPDATE_HEALTH -> DeltaOperation.UpdateHealth.decode(data, offset)
                        DeltaOpType.UPDATE_CALLSIGN -> DeltaOperation.UpdateCallsign.decode(data, offset)
                        DeltaOpType.UPDATE_EVENT -> DeltaOperation.UpdateEvent.decode(data, offset)
                        else -> {
                            Log.w(TAG, "Unknown delta op type: $opType")
                            null
                        }
                    }
                    if (result != null) {
                        operations.add(result.first)
                        offset = result.second
                    }
                }

                return PeatDeltaDocument(originNode, timestampMs, flags, operations)

            } catch (e: Exception) {
                Log.e(TAG, "Failed to decode delta document", e)
                return null
            }
        }

        /**
         * Encode a delta document to bytes.
         */
        fun encode(doc: PeatDeltaDocument): ByteArray {
            // Calculate size
            val operationBytes = doc.operations.map { it.encode() }
            val totalOpSize = operationBytes.sumOf { it.size }
            val headerSize = 1 + 1 + 4 + 8 + 2  // marker + flags + origin + timestamp + opcount
            val data = ByteArray(headerSize + totalOpSize)

            var offset = 0
            data[offset++] = DELTA_DOCUMENT_MARKER
            data[offset++] = doc.flags.toByte()
            writeU32LE(data, offset, doc.originNode); offset += 4
            writeU64LE(data, offset, doc.timestampMs); offset += 8
            writeU16LE(data, offset, doc.operations.size.toLong()); offset += 2

            for (opBytes in operationBytes) {
                opBytes.copyInto(data, offset)
                offset += opBytes.size
            }

            return data
        }
    }
}

/**
 * Per-peer sync state for delta tracking.
 */
data class PeerSyncState(
    var lastSentTimestamp: Long = 0,
    var lastSentPeripheral: PeatPeripheral? = null,
    var lastSentCounterValue: Long = 0,
    var syncCount: Int = 0
)

// Helper functions for byte operations (module-level for delta classes)
private fun readU16LE(data: ByteArray, offset: Int): Long {
    return ((data[offset].toLong() and 0xFF) or
            ((data[offset + 1].toLong() and 0xFF) shl 8))
}

private fun readU32LE(data: ByteArray, offset: Int): Long {
    return ((data[offset].toLong() and 0xFF) or
            ((data[offset + 1].toLong() and 0xFF) shl 8) or
            ((data[offset + 2].toLong() and 0xFF) shl 16) or
            ((data[offset + 3].toLong() and 0xFF) shl 24))
}

private fun readU64LE(data: ByteArray, offset: Int): Long {
    return ((data[offset].toLong() and 0xFF) or
            ((data[offset + 1].toLong() and 0xFF) shl 8) or
            ((data[offset + 2].toLong() and 0xFF) shl 16) or
            ((data[offset + 3].toLong() and 0xFF) shl 24) or
            ((data[offset + 4].toLong() and 0xFF) shl 32) or
            ((data[offset + 5].toLong() and 0xFF) shl 40) or
            ((data[offset + 6].toLong() and 0xFF) shl 48) or
            ((data[offset + 7].toLong() and 0xFF) shl 56))
}

private fun writeU16LE(data: ByteArray, offset: Int, value: Long) {
    data[offset] = (value and 0xFF).toByte()
    data[offset + 1] = ((value shr 8) and 0xFF).toByte()
}

private fun writeU32LE(data: ByteArray, offset: Int, value: Long) {
    data[offset] = (value and 0xFF).toByte()
    data[offset + 1] = ((value shr 8) and 0xFF).toByte()
    data[offset + 2] = ((value shr 16) and 0xFF).toByte()
    data[offset + 3] = ((value shr 24) and 0xFF).toByte()
}

private fun writeU64LE(data: ByteArray, offset: Int, value: Long) {
    data[offset] = (value and 0xFF).toByte()
    data[offset + 1] = ((value shr 8) and 0xFF).toByte()
    data[offset + 2] = ((value shr 16) and 0xFF).toByte()
    data[offset + 3] = ((value shr 24) and 0xFF).toByte()
    data[offset + 4] = ((value shr 32) and 0xFF).toByte()
    data[offset + 5] = ((value shr 40) and 0xFF).toByte()
    data[offset + 6] = ((value shr 48) and 0xFF).toByte()
    data[offset + 7] = ((value shr 56) and 0xFF).toByte()
}
