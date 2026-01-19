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
 * Mesh genesis block - cryptographic seed for a new mesh network.
 *
 * The genesis block establishes the cryptographic foundation for a mesh:
 * - Unique mesh_id derived from creator's identity and timestamp
 * - Shared encryption secret for mesh-wide encrypted communication
 * - Membership policy defining access control rules
 *
 * ## Usage
 *
 * ```kotlin
 * // Create identity first
 * val identity = DeviceIdentity.generate()
 *
 * // Create new mesh with Controlled access
 * val genesis = MeshGenesis.create(
 *     meshName = "ALPHA-TEAM",
 *     identity = identity,
 *     policy = MembershipPolicy.CONTROLLED
 * )
 *
 * // Get mesh ID for display/sharing
 * val meshId = genesis.meshId  // e.g., "ALPH-A7B3"
 *
 * // Encode for persistence or sharing
 * val encoded = genesis.encode()
 * storage.store("genesis", encoded)
 *
 * // Create mesh from genesis
 * val newIdentity = DeviceIdentity.generate()
 * val mesh = HiveMesh.createFromGenesis(genesis, newIdentity, "OPERATOR-1")
 *
 * // Restore genesis from storage
 * val restored = MeshGenesis.decode(encoded)
 * ```
 *
 * ## Membership Policies
 *
 * - **Open**: Any node can join (good for testing/demos)
 * - **Controlled**: Creator can approve new members (default)
 * - **Strict**: Only pre-registered identities allowed
 */
class MeshGenesis private constructor(
    private var handle: Long
) : AutoCloseable {

    companion object {
        private const val TAG = "MeshGenesis"

        init {
            try {
                System.loadLibrary("hive_btle")
                Log.i(TAG, "Loaded hive_btle native library")
            } catch (e: UnsatisfiedLinkError) {
                Log.e(TAG, "Failed to load hive_btle native library", e)
            }
        }

        /**
         * Create a new mesh genesis block.
         *
         * This establishes the cryptographic foundation for a new mesh network.
         *
         * @param meshName Human-readable mesh name (used in mesh_id derivation)
         * @param identity The creator's identity (used for signing and key derivation)
         * @param policy Membership access control policy
         * @return New MeshGenesis instance
         * @throws IllegalStateException if creation fails
         */
        @JvmStatic
        fun create(
            meshName: String,
            identity: DeviceIdentity,
            @MembershipPolicy policy: Int = MembershipPolicy.CONTROLLED
        ): MeshGenesis {
            val handle = nativeCreate(meshName, identity.getHandle(), policy)
            if (handle == 0L) {
                throw IllegalStateException("Failed to create MeshGenesis")
            }
            Log.i(TAG, "Created MeshGenesis: $meshName")
            return MeshGenesis(handle)
        }

        /**
         * Decode a mesh genesis from stored bytes.
         *
         * @param data Previously encoded genesis bytes
         * @return Decoded MeshGenesis, or null if invalid
         */
        @JvmStatic
        fun decode(data: ByteArray): MeshGenesis? {
            val handle = nativeDecode(data)
            return if (handle != 0L) {
                Log.i(TAG, "Decoded MeshGenesis from ${data.size} bytes")
                MeshGenesis(handle)
            } else {
                Log.e(TAG, "Failed to decode MeshGenesis")
                null
            }
        }

        // Native methods
        @JvmStatic
        private external fun nativeCreate(meshName: String, identityHandle: Long, policy: Int): Long

        @JvmStatic
        private external fun nativeDestroy(handle: Long)

        @JvmStatic
        private external fun nativeGetMeshId(handle: Long): String

        @JvmStatic
        private external fun nativeGetEncryptionSecret(handle: Long): ByteArray

        @JvmStatic
        private external fun nativeEncode(handle: Long): ByteArray

        @JvmStatic
        private external fun nativeDecode(data: ByteArray): Long
    }

    private var isDestroyed = false

    /**
     * The unique mesh identifier.
     *
     * Format: `<NAME>-<HASH>` (e.g., "ALPHA-A7B3")
     * Derived from mesh name, creator identity, and creation timestamp.
     */
    val meshId: String
        get() {
            checkNotDestroyed()
            return nativeGetMeshId(handle)
        }

    /**
     * The shared encryption secret for mesh-wide encryption.
     *
     * This 32-byte secret is used to derive:
     * - Beacon encryption keys
     * - Broadcast encryption keys
     * - Other mesh-wide cryptographic operations
     *
     * **Security Warning**: This is sensitive data. Handle securely.
     */
    val encryptionSecret: ByteArray
        get() {
            checkNotDestroyed()
            return nativeGetEncryptionSecret(handle)
        }

    /**
     * Encode the genesis block for persistence or sharing.
     *
     * The encoded format includes all necessary data to recreate the mesh:
     * - Mesh name and ID
     * - Creator's public key
     * - Creation timestamp
     * - Encryption secret
     * - Membership policy
     * - Digital signature
     *
     * @return Encoded genesis bytes
     */
    fun encode(): ByteArray {
        checkNotDestroyed()
        return nativeEncode(handle)
    }

    /**
     * Get the native handle for use with HiveMesh constructors.
     *
     * @return Native handle
     */
    internal fun getHandle(): Long {
        checkNotDestroyed()
        return handle
    }

    /**
     * Destroy the native genesis and release resources.
     */
    fun destroy() {
        if (!isDestroyed && handle != 0L) {
            nativeDestroy(handle)
            isDestroyed = true
            handle = 0
            Log.i(TAG, "MeshGenesis destroyed")
        }
    }

    override fun close() = destroy()

    private fun checkNotDestroyed() {
        if (isDestroyed) {
            throw IllegalStateException("MeshGenesis has been destroyed")
        }
    }

    override fun toString(): String {
        return if (isDestroyed) {
            "MeshGenesis(destroyed)"
        } else {
            "MeshGenesis(meshId=$meshId)"
        }
    }
}

/**
 * Membership policy defining access control for mesh networks.
 */
@Retention(AnnotationRetention.SOURCE)
@IntDef(
    MembershipPolicy.OPEN,
    MembershipPolicy.CONTROLLED,
    MembershipPolicy.STRICT
)
annotation class MembershipPolicy {
    companion object {
        /**
         * Open access - any node can join the mesh.
         *
         * Use for testing, demos, or low-security scenarios.
         */
        const val OPEN = 0

        /**
         * Controlled access - mesh creator can approve new members.
         *
         * Default policy. Balances security with operational flexibility.
         */
        const val CONTROLLED = 1

        /**
         * Strict access - only pre-registered identities can join.
         *
         * Highest security. Requires out-of-band identity distribution.
         */
        const val STRICT = 2
    }
}
