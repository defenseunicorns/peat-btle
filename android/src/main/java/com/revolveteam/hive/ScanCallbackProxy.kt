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
 * Proxy class that forwards BLE scan results to the EcheBtle layer.
 *
 * This class extends Android's ScanCallback and parses scan events into
 * DiscoveredDevice objects. The actual mesh processing happens in EcheBtle
 * via UniFFI bindings to the Rust EcheMesh.
 *
 * Usage:
 * ```kotlin
 * val proxy = ScanCallbackProxy { device -> echeBtle.onDeviceDiscovered(device) }
 * bluetoothLeScanner.startScan(filters, settings, proxy)
 * ```
 */
class ScanCallbackProxy(
    private val onDeviceFound: ((DiscoveredDevice) -> Unit)? = null
) : ScanCallback() {

    companion object {
        private const val TAG = "EcheBtle.ScanCallback"
    }

    /**
     * Called when a BLE device is discovered during scanning.
     *
     * Extracts device information from the ScanResult and invokes the
     * onDeviceFound callback for Eche protocol processing.
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

            // Extract service UUIDs (look for Eche service)
            val serviceUuids = scanRecord?.serviceUuids?.map { it.toString() } ?: emptyList()

            // Extract service data for Eche service UUID
            // Try both canonical 128-bit UUID and 16-bit alias 0xF47A used by ESP32/Core2
            val echeServiceData = scanRecord?.getServiceData(
                android.os.ParcelUuid.fromString(EcheBtle.ECHE_SERVICE_UUID.toString())
            ) ?: scanRecord?.getServiceData(
                android.os.ParcelUuid.fromString(EcheBtle.ECHE_SERVICE_UUID_16.toString())
            )

            // Check if this is a Eche device (by name prefix, WearTAK pattern, or service UUID)
            // Look for canonical 128-bit UUID "f47ac10b" or 16-bit alias 0xF47A (expands to 0000f47a-0000-1000-8000-00805f9b34fb)
            // WearTAK devices (WT-WEAROS-XXXX) are accepted by name pattern to handle BLE address rotation
            // (WearOS rotates BLE addresses for privacy, and not all advertisements include service data)
            val isWearTakDevice = name.startsWith("WT-WEAROS-") || name.startsWith("WEAROS-")
            val isEcheDevice = name.startsWith(EcheBtle.ECHE_MESH_PREFIX) ||
                name.startsWith(EcheBtle.ECHE_NAME_PREFIX) ||
                isWearTakDevice ||  // Accept WearTAK by name (handle address rotation)
                serviceUuids.any {
                    it.contains("f47ac10b", ignoreCase = true) ||  // Full 128-bit Eche service UUID
                    it.startsWith("0000f47a-0000-1000", ignoreCase = true)  // 16-bit alias (0xF47A) used by ESP32/Core2
                } ||
                echeServiceData != null

            // Parse mesh ID and node ID from device name
            // Supports both new format (ECHE_MESHID-NODEID) and legacy format (ECHE-NODEID)
            val parsed = EcheBtle.parseDeviceName(name)
            var meshId: String? = parsed?.first
            var nodeId: Long? = parsed?.second

            // If name parsing failed, try service data
            // Format: [nodeId:4 bytes BE][meshId: up to 8 chars UTF-8]
            if (echeServiceData != null && echeServiceData.size >= 4) {
                if (nodeId == null) {
                    nodeId = ((echeServiceData[0].toLong() and 0xFF) shl 24) or
                        ((echeServiceData[1].toLong() and 0xFF) shl 16) or
                        ((echeServiceData[2].toLong() and 0xFF) shl 8) or
                        (echeServiceData[3].toLong() and 0xFF)
                }
                // Extract mesh ID from service data (bytes 4+)
                if (meshId == null && echeServiceData.size > 4) {
                    meshId = String(echeServiceData, 4, echeServiceData.size - 4, Charsets.UTF_8)
                }
                Log.i(TAG, "Eche device found via service data: nodeId=${nodeId?.let { String.format("%08X", it) }}, meshId=$meshId")
            }

            // For devices matching WearTAK name pattern (WEAROS-* or WT-WEAROS-*):
            // These could be WearOS system advertisements OR our Eche advertisements.
            // Only process as Eche device if we have service data (nodeId from Eche advertisement).
            // Don't log "waiting for service data" spam - just silently skip system advertisements.
            if (isWearTakDevice && nodeId == null) {
                // No service data = WearOS system advertisement, not our Eche advertisement
                // Silently skip - don't spam logs since system advertises frequently
                return
            }
            if (isWearTakDevice && nodeId != null) {
                Log.i(TAG, "Eche device (WearTAK): $name -> nodeId=${String.format("%08X", nodeId)}, meshId=$meshId")
            }

            // Debug: log service data if present
            if (echeServiceData != null) {
                Log.d(TAG, "Eche service data (${echeServiceData.size} bytes): ${echeServiceData.joinToString(" ") { String.format("%02X", it) }}")
            }

            Log.d(TAG, "Scan result: $address ($name) RSSI=$rssi, isHive=$isEcheDevice, meshId=$meshId, nodeId=${nodeId?.let { String.format("%08X", it) }}")

            // Create discovered device and invoke callback
            val discoveredDevice = DiscoveredDevice(
                address = address,
                name = name,
                rssi = rssi,
                nodeId = nodeId,
                meshId = meshId,
                timestampNanos = result.timestampNanos,
                isEcheDevice = isEcheDevice
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
