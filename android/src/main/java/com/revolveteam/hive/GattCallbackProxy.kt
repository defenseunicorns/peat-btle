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

import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCallback
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothGattDescriptor
import android.bluetooth.BluetoothProfile
import android.util.Log

/**
 * Listener interface for HIVE document events.
 */
interface HiveDocumentListener {
    /**
     * Called when document data is received (via read or notification).
     */
    fun onDocumentReceived(data: ByteArray)

    /**
     * Called when services are discovered.
     */
    fun onServicesDiscovered() {}

    /**
     * Called when connection state changes.
     */
    fun onConnectionStateChanged(connected: Boolean) {}

    /**
     * Called when a characteristic write operation completes.
     * Used for write queue management.
     */
    fun onWriteComplete(success: Boolean) {}

    /**
     * Called when RSSI is read from a connected peer.
     * Used for realtime signal strength updates.
     */
    fun onRssiRead(rssi: Int) {}
}

/**
 * Proxy class that forwards GATT events to the HiveBtle layer.
 *
 * This class extends Android's BluetoothGattCallback and bridges all GATT
 * events via the HiveDocumentListener interface. The actual mesh processing
 * happens in HiveBtle via UniFFI bindings to the Rust HiveMesh.
 *
 * ## MTU Negotiation
 *
 * BLE has a default ATT MTU of 23 bytes (~20 byte payload). HiveDocument
 * structures can exceed this limit, so MTU negotiation is required.
 * This callback automatically requests a larger MTU (185 bytes) after
 * connecting, then triggers service discovery after MTU is negotiated.
 *
 * Usage:
 * ```kotlin
 * val proxy = GattCallbackProxy(connectionId)
 * proxy.documentListener = myListener
 * device.connectGatt(context, false, proxy, BluetoothDevice.TRANSPORT_LE)
 * ```
 *
 * @param connectionId Unique identifier for this connection (for logging)
 */
class GattCallbackProxy(private val connectionId: Long) : BluetoothGattCallback() {

    /**
     * Optional listener for document events.
     */
    var documentListener: HiveDocumentListener? = null

    /**
     * Current negotiated MTU for this connection.
     * Default BLE ATT MTU is 23 bytes (20 byte payload).
     */
    var negotiatedMtu: Int = 23
        private set

    companion object {
        private const val TAG = "HiveBtle.GattCallback"

        // GATT status codes
        const val GATT_SUCCESS = BluetoothGatt.GATT_SUCCESS

        // Connection states
        const val STATE_DISCONNECTED = BluetoothProfile.STATE_DISCONNECTED
        const val STATE_CONNECTING = BluetoothProfile.STATE_CONNECTING
        const val STATE_CONNECTED = BluetoothProfile.STATE_CONNECTED
        const val STATE_DISCONNECTING = BluetoothProfile.STATE_DISCONNECTING

        /**
         * Requested MTU size for HIVE connections.
         *
         * HiveDocument minimum size: 12 bytes (header only, no counters)
         * HiveDocument with 1 GCounter entry: 24 bytes
         * HiveDocument with location data: ~40-60 bytes
         *
         * 185 bytes provides good headroom while maintaining compatibility
         * with most BLE devices. Maximum supported is 517 bytes (BLE 5.0).
         */
        const val REQUESTED_MTU = 185
    }

    /**
     * Called when the connection state changes.
     *
     * @param gatt The GATT client
     * @param status Status of the operation (GATT_SUCCESS if successful)
     * @param newState The new connection state
     */
    override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) {
        val stateStr = when (newState) {
            STATE_DISCONNECTED -> "DISCONNECTED"
            STATE_CONNECTING -> "CONNECTING"
            STATE_CONNECTED -> "CONNECTED"
            STATE_DISCONNECTING -> "DISCONNECTING"
            else -> "UNKNOWN($newState)"
        }
        Log.i(TAG, "[$connectionId] Connection state changed: $stateStr (status=$status)")

        // Notify listener
        documentListener?.onConnectionStateChanged(newState == STATE_CONNECTED)

        // On connect: request larger MTU first (service discovery happens in onMtuChanged)
        // This is required because HiveDocument can exceed the default 23-byte BLE MTU
        if (newState == STATE_CONNECTED && status == GATT_SUCCESS) {
            Log.d(TAG, "[$connectionId] Requesting MTU: $REQUESTED_MTU")
            val mtuRequested = gatt.requestMtu(REQUESTED_MTU)
            if (!mtuRequested) {
                // MTU request failed, fall back to immediate service discovery
                Log.w(TAG, "[$connectionId] MTU request failed, proceeding with default MTU")
                gatt.discoverServices()
            }
        }
    }

    /**
     * Called when GATT services have been discovered.
     *
     * @param gatt The GATT client
     * @param status Status of the discovery operation
     */
    override fun onServicesDiscovered(gatt: BluetoothGatt, status: Int) {
        Log.i(TAG, "[$connectionId] Services discovered (status=$status)")

        if (status == GATT_SUCCESS) {
            // Log discovered services
            for (service in gatt.services) {
                Log.d(TAG, "[$connectionId]   Service: ${service.uuid}")
                for (char in service.characteristics) {
                    Log.d(TAG, "[$connectionId]     Char: ${char.uuid} (props=${char.properties})")
                }
            }
        }

        // Notify listener
        if (status == GATT_SUCCESS) {
            documentListener?.onServicesDiscovered()
        }
    }

    /**
     * Called when a characteristic read operation completes.
     *
     * @param gatt The GATT client
     * @param characteristic The characteristic that was read
     * @param status Status of the read operation
     */
    @Deprecated("Deprecated in API 33")
    override fun onCharacteristicRead(
        gatt: BluetoothGatt,
        characteristic: BluetoothGattCharacteristic,
        status: Int
    ) {
        val value = characteristic.value ?: ByteArray(0)
        Log.d(TAG, "[$connectionId] Characteristic read: ${characteristic.uuid} (${value.size} bytes, status=$status)")

        // Notify listener if this is the HIVE document characteristic
        if (status == GATT_SUCCESS && isHiveDocumentCharacteristic(characteristic)) {
            documentListener?.onDocumentReceived(value)
        }
    }

    private fun isHiveDocumentCharacteristic(characteristic: BluetoothGattCharacteristic): Boolean {
        // Check against canonical HIVE document characteristic UUID (CHAR_SYNC_DATA)
        return characteristic.uuid == HiveBtle.HIVE_CHAR_DOCUMENT
    }

    /**
     * Called when a characteristic read operation completes (API 33+).
     */
    override fun onCharacteristicRead(
        gatt: BluetoothGatt,
        characteristic: BluetoothGattCharacteristic,
        value: ByteArray,
        status: Int
    ) {
        Log.d(TAG, "[$connectionId] Characteristic read: ${characteristic.uuid} (${value.size} bytes, status=$status)")

        // Notify listener if this is the HIVE document characteristic
        if (status == GATT_SUCCESS && isHiveDocumentCharacteristic(characteristic)) {
            documentListener?.onDocumentReceived(value)
        }
    }

    /**
     * Called when a characteristic write operation completes.
     *
     * @param gatt The GATT client
     * @param characteristic The characteristic that was written
     * @param status Status of the write operation
     */
    override fun onCharacteristicWrite(
        gatt: BluetoothGatt,
        characteristic: BluetoothGattCharacteristic,
        status: Int
    ) {
        Log.d(TAG, "[$connectionId] Characteristic write: ${characteristic.uuid} (status=$status)")

        // Notify listener for write queue management
        documentListener?.onWriteComplete(status == BluetoothGatt.GATT_SUCCESS)
    }

    /**
     * Called when a characteristic value changes (notification/indication).
     *
     * @param gatt The GATT client
     * @param characteristic The characteristic whose value changed
     */
    @Deprecated("Deprecated in API 33")
    override fun onCharacteristicChanged(
        gatt: BluetoothGatt,
        characteristic: BluetoothGattCharacteristic
    ) {
        val value = characteristic.value ?: ByteArray(0)
        Log.d(TAG, "[$connectionId] Characteristic changed: ${characteristic.uuid} (${value.size} bytes)")

        // Notify listener if this is the HIVE document characteristic
        if (isHiveDocumentCharacteristic(characteristic)) {
            documentListener?.onDocumentReceived(value)
        }
    }

    /**
     * Called when a characteristic value changes (API 33+).
     */
    override fun onCharacteristicChanged(
        gatt: BluetoothGatt,
        characteristic: BluetoothGattCharacteristic,
        value: ByteArray
    ) {
        Log.d(TAG, "[$connectionId] Characteristic changed: ${characteristic.uuid} (${value.size} bytes)")

        // Notify listener if this is the HIVE document characteristic
        if (isHiveDocumentCharacteristic(characteristic)) {
            documentListener?.onDocumentReceived(value)
        }
    }

    /**
     * Called when a descriptor write operation completes.
     *
     * @param gatt The GATT client
     * @param descriptor The descriptor that was written
     * @param status Status of the write operation
     */
    override fun onDescriptorWrite(
        gatt: BluetoothGatt,
        descriptor: BluetoothGattDescriptor,
        status: Int
    ) {
        Log.d(TAG, "[$connectionId] Descriptor write: ${descriptor.uuid} (status=$status)")
    }

    /**
     * Called when the MTU for a connection changes.
     *
     * After MTU negotiation completes, service discovery is triggered.
     * This ensures we have a larger MTU before exchanging HiveDocument data.
     *
     * @param gatt The GATT client
     * @param mtu The new MTU size
     * @param status Status of the MTU request
     */
    override fun onMtuChanged(gatt: BluetoothGatt, mtu: Int, status: Int) {
        if (status == GATT_SUCCESS) {
            negotiatedMtu = mtu
            Log.i(TAG, "[$connectionId] MTU negotiated: $mtu bytes")
        } else {
            Log.w(TAG, "[$connectionId] MTU negotiation failed (status=$status), using default: $negotiatedMtu")
        }

        // Proceed with service discovery now that MTU is negotiated
        Log.d(TAG, "[$connectionId] Starting service discovery (MTU=$negotiatedMtu)")
        gatt.discoverServices()
    }

    /**
     * Called when the PHY is updated.
     *
     * @param gatt The GATT client
     * @param txPhy The new TX PHY
     * @param rxPhy The new RX PHY
     * @param status Status of the PHY update
     */
    override fun onPhyUpdate(gatt: BluetoothGatt, txPhy: Int, rxPhy: Int, status: Int) {
        Log.i(TAG, "[$connectionId] PHY updated: tx=$txPhy, rx=$rxPhy (status=$status)")
    }

    /**
     * Called when the RSSI is read.
     *
     * @param gatt The GATT client
     * @param rssi The RSSI value
     * @param status Status of the RSSI read
     */
    override fun onReadRemoteRssi(gatt: BluetoothGatt, rssi: Int, status: Int) {
        Log.d(TAG, "[$connectionId] RSSI read: $rssi dBm (status=$status)")
        if (status == GATT_SUCCESS) {
            documentListener?.onRssiRead(rssi)
        }
    }
}
