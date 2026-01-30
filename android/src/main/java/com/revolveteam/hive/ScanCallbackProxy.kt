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

import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanResult
import android.bluetooth.le.ScanSettings
import android.util.Log

/**
 * Proxy class that forwards BLE scan results to the HiveBtle layer.
 *
 * This class extends Android's ScanCallback and parses scan events into
 * DiscoveredDevice objects. The actual mesh processing happens in HiveBtle
 * via UniFFI bindings to the Rust HiveMesh.
 *
 * Usage:
 * ```kotlin
 * val proxy = ScanCallbackProxy { device -> hiveBtle.onDeviceDiscovered(device) }
 * bluetoothLeScanner.startScan(filters, settings, proxy)
 * ```
 */
class ScanCallbackProxy(
    private val onDeviceFound: ((DiscoveredDevice) -> Unit)? = null
) : ScanCallback() {

    companion object {
        private const val TAG = "HiveBtle.ScanCallback"
    }

    /**
     * Called when a BLE device is discovered during scanning.
     *
     * Extracts device information from the ScanResult and invokes the
     * onDeviceFound callback for HIVE protocol processing.
     *
     * @param callbackType Type of callback (CALLBACK_TYPE_ALL_MATCHES, etc.)
     * @param result The scan result containing device information
     */
    override fun onScanResult(callbackType: Int, result: ScanResult) {
        try {
            val device = result.device
            val scanRecord = result.scanRecord

            // Extract device info
            val address = device.address
            val name = scanRecord?.deviceName ?: device.name ?: ""
            val rssi = result.rssi

            // Extract service UUIDs (look for HIVE service)
            val serviceUuids = scanRecord?.serviceUuids?.map { it.toString() } ?: emptyList()

            // Extract service data for HIVE service UUID
            // Try both canonical 128-bit UUID and 16-bit alias 0xF47A used by ESP32/Core2
            val hiveServiceData = scanRecord?.getServiceData(
                android.os.ParcelUuid.fromString(HiveBtle.HIVE_SERVICE_UUID.toString())
            ) ?: scanRecord?.getServiceData(
                android.os.ParcelUuid.fromString(HiveBtle.HIVE_SERVICE_UUID_16.toString())
            )

            // Check if this is a HIVE device (by name prefix, WearTAK pattern, or service UUID)
            // Look for canonical 128-bit UUID "f47ac10b" or 16-bit alias 0xF47A (expands to 0000f47a-0000-1000-8000-00805f9b34fb)
            // WearTAK devices (WT-WEAROS-XXXX) are accepted by name pattern to handle BLE address rotation
            // (WearOS rotates BLE addresses for privacy, and not all advertisements include service data)
            val isWearTakDevice = name.startsWith("WT-WEAROS-") || name.startsWith("WEAROS-")
            val isHiveDevice = name.startsWith(HiveBtle.HIVE_MESH_PREFIX) ||
                name.startsWith(HiveBtle.HIVE_NAME_PREFIX) ||
                isWearTakDevice ||  // Accept WearTAK by name (handle address rotation)
                serviceUuids.any {
                    it.contains("f47ac10b", ignoreCase = true) ||  // Full 128-bit HIVE service UUID
                    it.startsWith("0000f47a-0000-1000", ignoreCase = true)  // 16-bit alias (0xF47A) used by ESP32/Core2
                } ||
                hiveServiceData != null

            // Parse mesh ID and node ID from device name
            // Supports both new format (HIVE_MESHID-NODEID) and legacy format (HIVE-NODEID)
            val parsed = HiveBtle.parseDeviceName(name)
            var meshId: String? = parsed?.first
            var nodeId: Long? = parsed?.second

            // If name parsing failed, try service data
            // Format: [nodeId:4 bytes BE][meshId: up to 8 chars UTF-8]
            if (hiveServiceData != null && hiveServiceData.size >= 4) {
                if (nodeId == null) {
                    nodeId = ((hiveServiceData[0].toLong() and 0xFF) shl 24) or
                        ((hiveServiceData[1].toLong() and 0xFF) shl 16) or
                        ((hiveServiceData[2].toLong() and 0xFF) shl 8) or
                        (hiveServiceData[3].toLong() and 0xFF)
                }
                // Extract mesh ID from service data (bytes 4+)
                if (meshId == null && hiveServiceData.size > 4) {
                    meshId = String(hiveServiceData, 4, hiveServiceData.size - 4, Charsets.UTF_8)
                }
                Log.i(TAG, "HIVE device found via service data: nodeId=${nodeId?.let { String.format("%08X", it) }}, meshId=$meshId")
            }

            // For WearTAK devices, derive a stable nodeId from the name suffix
            // Format: WT-WEAROS-XXXX or WEAROS-XXXX where XXXX is a 4-digit suffix
            // IMPORTANT: Use name suffix (not BLE address) for stability across address rotation.
            // WearOS rotates BLE addresses for privacy, but name suffix stays constant.
            // The correct mesh ID comes from service data when available.
            if (isWearTakDevice && nodeId == null) {
                val suffix = name.substringAfterLast("-", "")
                if (suffix.isNotEmpty() && suffix.all { it.isDigit() }) {
                    // Use name suffix as base for stable nodeId across address rotations
                    // Combine suffix with a hash of the full name to reduce collision risk
                    val nameHash = name.hashCode().toLong() and 0xFFFF0000L
                    nodeId = nameHash or (suffix.toLongOrNull() ?: 0L)
                }
                // Don't default to WEARTAK mesh - let HiveBtle use service data mesh
                // meshId stays null until we get proper service data
                Log.i(TAG, "WearTAK device found via name pattern: $name -> nodeId=${nodeId?.let { String.format("%08X", it) }}, meshId=${meshId ?: "unknown (waiting for service data)"}")
            }

            // Debug: log service data if present
            if (hiveServiceData != null) {
                Log.d(TAG, "HIVE service data (${hiveServiceData.size} bytes): ${hiveServiceData.joinToString(" ") { String.format("%02X", it) }}")
            }

            Log.d(TAG, "Scan result: $address ($name) RSSI=$rssi, isHive=$isHiveDevice, meshId=$meshId, nodeId=${nodeId?.let { String.format("%08X", it) }}")

            // Create discovered device and invoke callback
            val discoveredDevice = DiscoveredDevice(
                address = address,
                name = name,
                rssi = rssi,
                nodeId = nodeId,
                meshId = meshId,
                timestampNanos = result.timestampNanos,
                isHiveDevice = isHiveDevice
            )

            // Invoke Kotlin callback for processing
            onDeviceFound?.invoke(discoveredDevice)

        } catch (e: Exception) {
            Log.e(TAG, "Error processing scan result", e)
        }
    }

    /**
     * Called when batch scan results are available.
     *
     * Processes each result individually through onScanResult.
     *
     * @param results List of scan results
     */
    override fun onBatchScanResults(results: MutableList<ScanResult>) {
        Log.d(TAG, "Batch scan results: ${results.size} devices")
        for (result in results) {
            onScanResult(ScanSettings.CALLBACK_TYPE_ALL_MATCHES, result)
        }
    }

    /**
     * Called when scanning fails.
     *
     * @param errorCode Error code indicating the failure reason
     */
    override fun onScanFailed(errorCode: Int) {
        val errorMsg = when (errorCode) {
            SCAN_FAILED_ALREADY_STARTED -> "Scan already started"
            SCAN_FAILED_APPLICATION_REGISTRATION_FAILED -> "App registration failed"
            SCAN_FAILED_INTERNAL_ERROR -> "Internal error"
            SCAN_FAILED_FEATURE_UNSUPPORTED -> "Feature unsupported"
            else -> "Unknown error"
        }
        Log.e(TAG, "Scan failed: $errorMsg (code=$errorCode)")
    }
}
