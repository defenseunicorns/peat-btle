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

/**
 * Utility class for working with identity attestations.
 *
 * Identity attestations are cryptographic proofs of device identity. They contain:
 * - Public key (32 bytes)
 * - Node ID derived from public key (4 bytes)
 * - Timestamp (8 bytes)
 * - Ed25519 signature over the above (64 bytes)
 *
 * Attestations are used during peer handshake to establish identity before
 * trusting any data from a peer. The TOFU (Trust On First Use) model registers
 * the public key on first contact and rejects any future key changes.
 *
 * ## Usage
 *
 * ```kotlin
 * // Receive attestation bytes from peer
 * val attestationBytes = receivedFromPeer()
 *
 * // Verify cryptographic signature
 * if (IdentityAttestation.verify(attestationBytes)) {
 *     // Get peer's node ID
 *     val peerNodeId = IdentityAttestation.getNodeId(attestationBytes)
 *
 *     // Register/verify with mesh (TOFU)
 *     val result = mesh.verifyPeerIdentity(attestationBytes)
 *     when (result) {
 *         VerifyResult.REGISTERED -> Log.i(TAG, "New peer registered")
 *         VerifyResult.VERIFIED -> Log.i(TAG, "Known peer verified")
 *         VerifyResult.INVALID_SIGNATURE -> Log.w(TAG, "Bad signature!")
 *         VerifyResult.KEY_MISMATCH -> Log.e(TAG, "Impersonation attempt!")
 *     }
 * }
 * ```
 */
object IdentityAttestation {
    private const val TAG = "IdentityAttestation"

    init {
        try {
            System.loadLibrary("hive_btle")
            Log.i(TAG, "Loaded hive_btle native library")
        } catch (e: UnsatisfiedLinkError) {
            Log.e(TAG, "Failed to load hive_btle native library", e)
        }
    }

    /**
     * Verify the cryptographic signature of an attestation.
     *
     * This checks that:
     * 1. The attestation format is valid
     * 2. The Ed25519 signature is valid for the public key
     * 3. The node_id matches the public key (BLAKE3 derivation)
     *
     * @param attestationBytes Encoded attestation bytes
     * @return true if signature is valid, false otherwise
     */
    @JvmStatic
    fun verify(attestationBytes: ByteArray): Boolean {
        return nativeVerify(attestationBytes)
    }

    /**
     * Extract the node ID from attestation bytes.
     *
     * @param attestationBytes Encoded attestation bytes
     * @return Node ID (32-bit value as Long), or 0 if invalid
     */
    @JvmStatic
    fun getNodeId(attestationBytes: ByteArray): Long {
        return nativeGetNodeId(attestationBytes)
    }

    /**
     * Extract the public key from attestation bytes.
     *
     * @param attestationBytes Encoded attestation bytes
     * @return 32-byte public key, or empty array if invalid
     */
    @JvmStatic
    fun getPublicKey(attestationBytes: ByteArray): ByteArray {
        return nativeGetPublicKey(attestationBytes)
    }

    // Native methods
    @JvmStatic
    private external fun nativeVerify(attestationBytes: ByteArray): Boolean

    @JvmStatic
    private external fun nativeGetNodeId(attestationBytes: ByteArray): Long

    @JvmStatic
    private external fun nativeGetPublicKey(attestationBytes: ByteArray): ByteArray
}

/**
 * Result of identity verification via TOFU registry.
 */
@Retention(AnnotationRetention.SOURCE)
@IntDef(
    VerifyResult.REGISTERED,
    VerifyResult.VERIFIED,
    VerifyResult.INVALID_SIGNATURE,
    VerifyResult.KEY_MISMATCH,
    VerifyResult.ERROR
)
annotation class VerifyResult {
    companion object {
        /**
         * Identity was newly registered (first contact with this node).
         *
         * The public key has been stored in the TOFU registry.
         */
        const val REGISTERED = 0

        /**
         * Identity was verified against existing record.
         *
         * The public key matches what was previously registered.
         */
        const val VERIFIED = 1

        /**
         * Cryptographic signature verification failed.
         *
         * The attestation is corrupted or forged.
         */
        const val INVALID_SIGNATURE = 2

        /**
         * Public key doesn't match previously registered key.
         *
         * **Security Alert**: This indicates a potential impersonation attempt!
         * A different device is claiming the same node_id with a different key.
         */
        const val KEY_MISMATCH = 3

        /**
         * Error during verification (e.g., mesh not initialized).
         */
        const val ERROR = -1

        /**
         * Check if the result indicates a trusted identity.
         */
        fun isTrusted(result: Int): Boolean = result == REGISTERED || result == VERIFIED

        /**
         * Check if the result indicates a security violation.
         */
        fun isViolation(result: Int): Boolean = result == INVALID_SIGNATURE || result == KEY_MISMATCH
    }
}
