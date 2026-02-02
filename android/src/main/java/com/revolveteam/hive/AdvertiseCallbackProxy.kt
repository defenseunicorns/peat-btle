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

import android.bluetooth.le.AdvertiseCallback
import android.bluetooth.le.AdvertiseSettings
import android.util.Log

/**
 * Proxy class that handles BLE advertising events.
 *
 * This class extends Android's AdvertiseCallback and logs advertising
 * start/stop events. The actual mesh state management happens in HiveBtle
 * via UniFFI bindings to the Rust HiveMesh.
 *
 * Usage:
 * ```kotlin
 * val proxy = AdvertiseCallbackProxy()
 * bluetoothLeAdvertiser.startAdvertising(settings, data, proxy)
 * ```
 */
class AdvertiseCallbackProxy : AdvertiseCallback() {

    companion object {
        private const val TAG = "HiveBtle.AdvertiseCb"
    }

    /**
     * Called when advertising starts successfully.
     *
     * @param settingsInEffect The actual settings used for advertising
     */
    override fun onStartSuccess(settingsInEffect: AdvertiseSettings) {
        val mode = when (settingsInEffect.mode) {
            AdvertiseSettings.ADVERTISE_MODE_LOW_POWER -> "LOW_POWER"
            AdvertiseSettings.ADVERTISE_MODE_BALANCED -> "BALANCED"
            AdvertiseSettings.ADVERTISE_MODE_LOW_LATENCY -> "LOW_LATENCY"
            else -> "UNKNOWN"
        }
        val txPower = when (settingsInEffect.txPowerLevel) {
            AdvertiseSettings.ADVERTISE_TX_POWER_ULTRA_LOW -> "ULTRA_LOW"
            AdvertiseSettings.ADVERTISE_TX_POWER_LOW -> "LOW"
            AdvertiseSettings.ADVERTISE_TX_POWER_MEDIUM -> "MEDIUM"
            AdvertiseSettings.ADVERTISE_TX_POWER_HIGH -> "HIGH"
            else -> "UNKNOWN"
        }
        Log.i(TAG, "Advertising started: mode=$mode, txPower=$txPower, connectable=${settingsInEffect.isConnectable}")
    }

    /**
     * Called when advertising fails to start.
     *
     * @param errorCode Error code indicating the failure reason
     */
    override fun onStartFailure(errorCode: Int) {
        val errorMsg = when (errorCode) {
            ADVERTISE_FAILED_DATA_TOO_LARGE -> "Data too large"
            ADVERTISE_FAILED_TOO_MANY_ADVERTISERS -> "Too many advertisers"
            ADVERTISE_FAILED_ALREADY_STARTED -> "Already started"
            ADVERTISE_FAILED_INTERNAL_ERROR -> "Internal error"
            ADVERTISE_FAILED_FEATURE_UNSUPPORTED -> "Feature unsupported"
            else -> "Unknown error"
        }
        Log.e(TAG, "Advertising failed: $errorMsg (code=$errorCode)")
    }
}
