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

/**
 * Cryptographic device identity using Ed25519 signatures.
 *
 * Each device has a unique identity derived from an Ed25519 keypair. The node_id
 * is derived from the public key using BLAKE3 hash, ensuring cryptographic binding
 * between identity and node identifier.
 *
 * ## Usage
 *
 * ```kotlin
 * // Generate new identity
 * val identity = DeviceIdentity.generate()
 *
 * // Get the cryptographically-derived node ID
 * val nodeId = identity.nodeId
 *
 * // Create attestation to prove identity to peers
 * val attestation = identity.createAttestation()
 *
 * // Sign data for authentication
 * val signature = identity.sign(data)
 *
 * // Store private key for persistence
 * val privateKey = identity.privateKey
 * secureStorage.store("identity_key", privateKey)
 *
 * // Restore from stored key
 * val restored = DeviceIdentity.fromPrivateKey(privateKey)
 *
 * // Clean up when done
 * identity.close()
 * ```
 *
 * ## Security Considerations
 *
 * - Private keys should be stored in Android Keystore or encrypted SharedPreferences
 * - The private key bytes are sensitive - zero them after use if possible
 * - Node IDs are derived from public keys, providing identity binding
 */
class DeviceIdentity private constructor(
    private var handle: Long
) : AutoCloseable {

    companion object {
        private const val TAG = "DeviceIdentity"

        init {
            try {
                System.loadLibrary("hive_btle")
                Log.i(TAG, "Loaded hive_btle native library")
            } catch (e: UnsatisfiedLinkError) {
                Log.e(TAG, "Failed to load hive_btle native library", e)
            }
        }

        /**
         * Generate a new random device identity.
         *
         * Creates a new Ed25519 keypair and derives the node_id from the public key.
         *
         * @return New DeviceIdentity instance
         * @throws IllegalStateException if native library fails
         */
        @JvmStatic
        fun generate(): DeviceIdentity {
            val handle = nativeGenerate()
            if (handle == 0L) {
                throw IllegalStateException("Failed to generate DeviceIdentity")
            }
            Log.i(TAG, "Generated new DeviceIdentity")
            return DeviceIdentity(handle)
        }

        /**
         * Restore a device identity from a stored private key.
         *
         * @param privateKey The 32-byte Ed25519 private key
         * @return Restored DeviceIdentity instance
         * @throws IllegalArgumentException if private key is invalid
         */
        @JvmStatic
        fun fromPrivateKey(privateKey: ByteArray): DeviceIdentity {
            require(privateKey.size == 32) { "Private key must be 32 bytes" }
            val handle = nativeFromPrivateKey(privateKey)
            if (handle == 0L) {
                throw IllegalArgumentException("Invalid private key")
            }
            Log.i(TAG, "Restored DeviceIdentity from private key")
            return DeviceIdentity(handle)
        }

        // Native methods
        @JvmStatic
        private external fun nativeGenerate(): Long

        @JvmStatic
        private external fun nativeFromPrivateKey(privateKey: ByteArray): Long

        @JvmStatic
        private external fun nativeDestroy(handle: Long)

        @JvmStatic
        private external fun nativeGetPublicKey(handle: Long): ByteArray

        @JvmStatic
        private external fun nativeGetPrivateKey(handle: Long): ByteArray

        @JvmStatic
        private external fun nativeGetNodeId(handle: Long): Long

        @JvmStatic
        private external fun nativeCreateAttestation(handle: Long, timestampMs: Long): ByteArray

        @JvmStatic
        private external fun nativeSign(handle: Long, data: ByteArray): ByteArray
    }

    private var isDestroyed = false

    /**
     * The Ed25519 public key (32 bytes).
     *
     * This key can be shared freely and is used by peers to verify signatures.
     */
    val publicKey: ByteArray
        get() {
            checkNotDestroyed()
            return nativeGetPublicKey(handle)
        }

    /**
     * The Ed25519 private key (32 bytes).
     *
     * **Security Warning**: This is sensitive data. Store securely and zero after use.
     */
    val privateKey: ByteArray
        get() {
            checkNotDestroyed()
            return nativeGetPrivateKey(handle)
        }

    /**
     * The node ID derived from this identity's public key.
     *
     * This is a 32-bit value derived via BLAKE3 hash of the public key,
     * providing cryptographic binding between identity and node ID.
     */
    val nodeId: Long
        get() {
            checkNotDestroyed()
            return nativeGetNodeId(handle)
        }

    /**
     * Create an identity attestation for peer verification.
     *
     * The attestation contains the public key, node_id, timestamp, and a signature
     * proving possession of the private key. Send this to peers during handshake.
     *
     * @param timestampMs Attestation timestamp (defaults to current time)
     * @return Encoded attestation bytes
     */
    fun createAttestation(timestampMs: Long = System.currentTimeMillis()): ByteArray {
        checkNotDestroyed()
        return nativeCreateAttestation(handle, timestampMs)
    }

    /**
     * Sign data with this identity's private key.
     *
     * Creates an Ed25519 signature over the provided data.
     *
     * @param data The data to sign
     * @return 64-byte Ed25519 signature
     */
    fun sign(data: ByteArray): ByteArray {
        checkNotDestroyed()
        return nativeSign(handle, data)
    }

    /**
     * Get the native handle for use with HiveMesh constructors.
     *
     * **Note**: After passing to HiveMesh.createWithIdentity(), this identity
     * instance should not be used further as ownership is transferred.
     *
     * @return Native handle
     */
    internal fun getHandle(): Long {
        checkNotDestroyed()
        return handle
    }

    /**
     * Mark this identity as consumed (ownership transferred to native code).
     *
     * Called internally when identity is passed to HiveMesh.
     */
    internal fun markConsumed() {
        handle = 0
        isDestroyed = true
    }

    /**
     * Destroy the native identity and release resources.
     */
    fun destroy() {
        if (!isDestroyed && handle != 0L) {
            nativeDestroy(handle)
            isDestroyed = true
            handle = 0
            Log.i(TAG, "DeviceIdentity destroyed")
        }
    }

    override fun close() = destroy()

    private fun checkNotDestroyed() {
        if (isDestroyed) {
            throw IllegalStateException("DeviceIdentity has been destroyed")
        }
    }

    override fun toString(): String {
        return if (isDestroyed) {
            "DeviceIdentity(destroyed)"
        } else {
            "DeviceIdentity(nodeId=${String.format("%08X", nodeId)})"
        }
    }
}
