# Android Platform Integration Guide

This guide covers integrating `peat-btle` into Android applications using JNI bindings.

## Requirements

| Requirement | Minimum | Recommended |
|-------------|---------|-------------|
| Android API | 23 (6.0) | 26+ (8.0) |
| BLE 5.0 features | API 26 | API 26+ |
| Coded PHY | API 26 | API 26+ |

### Permissions

Add to `AndroidManifest.xml`:

```xml
<!-- BLE permissions -->
<uses-permission android:name="android.permission.BLUETOOTH" />
<uses-permission android:name="android.permission.BLUETOOTH_ADMIN" />
<uses-permission android:name="android.permission.BLUETOOTH_CONNECT" />
<uses-permission android:name="android.permission.BLUETOOTH_SCAN" />
<uses-permission android:name="android.permission.BLUETOOTH_ADVERTISE" />

<!-- Location required for BLE scanning on Android 6-11 -->
<uses-permission android:name="android.permission.ACCESS_FINE_LOCATION" />
<uses-permission android:name="android.permission.ACCESS_COARSE_LOCATION" />

<!-- Declare BLE hardware support -->
<uses-feature android:name="android.hardware.bluetooth_le" android:required="true" />
```

**Permission Notes:**
- Android 12+ (API 31): Use new granular permissions (`BLUETOOTH_SCAN`, `BLUETOOTH_CONNECT`, `BLUETOOTH_ADVERTISE`)
- Android 6-11: Location permission required for BLE scanning
- Android 10+: `ACCESS_BACKGROUND_LOCATION` for background scanning

## Architecture

```
┌─────────────────────────────────────────┐
│        Kotlin/Java Application          │
├─────────────────────────────────────────┤
│           JNI Bridge (native)           │
├─────────────────────────────────────────┤
│         AndroidAdapter (Rust)           │
├─────────────────────────────────────────┤
│  BluetoothAdapter │ BluetoothLeScanner  │
│  BluetoothGatt    │ BluetoothGattServer │
└─────────────────────────────────────────┘
```

## Project Setup

### 1. Configure Cargo.toml

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
peat-btle = { version = "0.1", features = ["android"] }
jni = "0.21"
log = "0.4"
android_logger = "0.13"
```

### 2. Create JNI Library

Create `src/lib.rs`:

```rust
use jni::JNIEnv;
use jni::objects::{JClass, JObject, JString};
use jni::sys::{jlong, jbyteArray, jint};

use peat_btle::{PeatMesh, PeatMeshConfig, NodeId};
use peat_btle::observer::DisconnectReason;

use std::sync::Arc;
use std::panic;

// Store mesh instance pointer
static mut MESH: Option<Arc<PeatMesh>> = None;

#[no_mangle]
pub extern "C" fn Java_com_example_hive_PeatBridge_init(
    mut env: JNIEnv,
    _class: JClass,
    node_id: jlong,
    callsign: JString,
    mesh_id: JString,
) -> jint {
    // Initialize logging
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Debug)
            .with_tag("peat-btle"),
    );

    panic::catch_unwind(|| {
        let callsign: String = env.get_string(&callsign)
            .expect("Invalid callsign")
            .into();
        let mesh_id: String = env.get_string(&mesh_id)
            .expect("Invalid mesh_id")
            .into();

        let config = PeatMeshConfig::new(
            NodeId::new(node_id as u32),
            &callsign,
            &mesh_id,
        );

        let mesh = PeatMesh::new(config);

        unsafe {
            MESH = Some(Arc::new(mesh));
        }

        0 // Success
    }).unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn Java_com_example_hive_PeatBridge_onDiscovered(
    mut env: JNIEnv,
    _class: JClass,
    identifier: JString,
    name: JString,
    rssi: jint,
    mesh_id: JString,
    now_ms: jlong,
) -> jlong {
    let mesh = unsafe {
        match &MESH {
            Some(m) => m.clone(),
            None => return -1,
        }
    };

    let identifier: String = env.get_string(&identifier)
        .unwrap_or_default().into();
    let name: Option<String> = env.get_string(&name).ok().map(|s| s.into());
    let mesh_id: Option<String> = env.get_string(&mesh_id).ok().map(|s| s.into());

    match mesh.on_ble_discovered(
        &identifier,
        name.as_deref(),
        rssi as i8,
        mesh_id.as_deref(),
        now_ms as u64,
    ) {
        Some(peer) => peer.node_id.as_u32() as jlong,
        None => 0,
    }
}

#[no_mangle]
pub extern "C" fn Java_com_example_hive_PeatBridge_onConnected(
    mut env: JNIEnv,
    _class: JClass,
    identifier: JString,
    now_ms: jlong,
) -> jlong {
    let mesh = unsafe {
        match &MESH {
            Some(m) => m.clone(),
            None => return -1,
        }
    };

    let identifier: String = env.get_string(&identifier)
        .unwrap_or_default().into();

    match mesh.on_ble_connected(&identifier, now_ms as u64) {
        Some(node_id) => node_id.as_u32() as jlong,
        None => 0,
    }
}

#[no_mangle]
pub extern "C" fn Java_com_example_hive_PeatBridge_onDisconnected(
    mut env: JNIEnv,
    _class: JClass,
    identifier: JString,
    reason: jint,
) -> jlong {
    let mesh = unsafe {
        match &MESH {
            Some(m) => m.clone(),
            None => return -1,
        }
    };

    let identifier: String = env.get_string(&identifier)
        .unwrap_or_default().into();

    let reason = match reason {
        0 => DisconnectReason::LocalRequest,
        1 => DisconnectReason::RemoteRequest,
        2 => DisconnectReason::Timeout,
        3 => DisconnectReason::LinkLoss,
        _ => DisconnectReason::Unknown,
    };

    match mesh.on_ble_disconnected(&identifier, reason) {
        Some(node_id) => node_id.as_u32() as jlong,
        None => 0,
    }
}

#[no_mangle]
pub extern "C" fn Java_com_example_hive_PeatBridge_onDataReceived(
    mut env: JNIEnv,
    _class: JClass,
    identifier: JString,
    data: jbyteArray,
    now_ms: jlong,
) -> jint {
    let mesh = unsafe {
        match &MESH {
            Some(m) => m.clone(),
            None => return -1,
        }
    };

    let identifier: String = env.get_string(&identifier)
        .unwrap_or_default().into();

    let data = match env.convert_byte_array(data) {
        Ok(d) => d,
        Err(_) => return -2,
    };

    match mesh.on_ble_data_received(&identifier, &data, now_ms as u64) {
        Some(result) => {
            if result.is_emergency { 1 }
            else if result.is_ack { 2 }
            else { 0 }
        }
        None => -3,
    }
}

#[no_mangle]
pub extern "C" fn Java_com_example_hive_PeatBridge_sendEmergency(
    env: JNIEnv,
    _class: JClass,
    timestamp: jlong,
) -> jbyteArray {
    let mesh = unsafe {
        match &MESH {
            Some(m) => m.clone(),
            None => return std::ptr::null_mut(),
        }
    };

    let doc = mesh.send_emergency(timestamp as u64);

    env.byte_array_from_slice(&doc)
        .unwrap_or_else(|_| std::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn Java_com_example_hive_PeatBridge_sendAck(
    env: JNIEnv,
    _class: JClass,
    timestamp: jlong,
) -> jbyteArray {
    let mesh = unsafe {
        match &MESH {
            Some(m) => m.clone(),
            None => return std::ptr::null_mut(),
        }
    };

    let doc = mesh.send_ack(timestamp as u64);

    env.byte_array_from_slice(&doc)
        .unwrap_or_else(|_| std::ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn Java_com_example_hive_PeatBridge_tick(
    env: JNIEnv,
    _class: JClass,
    now_ms: jlong,
) -> jbyteArray {
    let mesh = unsafe {
        match &MESH {
            Some(m) => m.clone(),
            None => return std::ptr::null_mut(),
        }
    };

    match mesh.tick(now_ms as u64) {
        Some(doc) => env.byte_array_from_slice(&doc)
            .unwrap_or_else(|_| std::ptr::null_mut()),
        None => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn Java_com_example_hive_PeatBridge_buildDocument(
    env: JNIEnv,
    _class: JClass,
) -> jbyteArray {
    let mesh = unsafe {
        match &MESH {
            Some(m) => m.clone(),
            None => return std::ptr::null_mut(),
        }
    };

    let doc = mesh.build_document();

    env.byte_array_from_slice(&doc)
        .unwrap_or_else(|_| std::ptr::null_mut())
}
```

### 3. Create Kotlin Bridge Class

```kotlin
package com.example.hive

class PeatBridge {
    companion object {
        init {
            System.loadLibrary("hive_android")
        }

        @JvmStatic
        external fun init(nodeId: Long, callsign: String, meshId: String): Int

        @JvmStatic
        external fun onDiscovered(
            identifier: String,
            name: String?,
            rssi: Int,
            meshId: String?,
            nowMs: Long
        ): Long

        @JvmStatic
        external fun onConnected(identifier: String, nowMs: Long): Long

        @JvmStatic
        external fun onDisconnected(identifier: String, reason: Int): Long

        @JvmStatic
        external fun onDataReceived(
            identifier: String,
            data: ByteArray,
            nowMs: Long
        ): Int

        @JvmStatic
        external fun sendEmergency(timestamp: Long): ByteArray?

        @JvmStatic
        external fun sendAck(timestamp: Long): ByteArray?

        @JvmStatic
        external fun tick(nowMs: Long): ByteArray?

        @JvmStatic
        external fun buildDocument(): ByteArray?
    }
}
```

### 4. Build Script

Create `build-android.sh`:

```bash
#!/bin/bash
set -e

# Ensure NDK is set
if [ -z "$ANDROID_NDK_HOME" ]; then
    echo "Error: ANDROID_NDK_HOME not set"
    exit 1
fi

# Add Android targets
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

# Build for each architecture
TARGETS=(
    "aarch64-linux-android"
    "armv7-linux-androideabi"
    "x86_64-linux-android"
)

for TARGET in "${TARGETS[@]}"; do
    echo "Building for $TARGET..."
    cargo build --release --target $TARGET
done

# Copy libraries to Android project
mkdir -p app/src/main/jniLibs/{arm64-v8a,armeabi-v7a,x86_64}

cp target/aarch64-linux-android/release/libpeat_android.so \
   app/src/main/jniLibs/arm64-v8a/

cp target/armv7-linux-androideabi/release/libpeat_android.so \
   app/src/main/jniLibs/armeabi-v7a/

cp target/x86_64-linux-android/release/libpeat_android.so \
   app/src/main/jniLibs/x86_64/

echo "Done! Libraries copied to app/src/main/jniLibs/"
```

## Android BLE Integration

### BLE Scanner Implementation

```kotlin
class PeatBleManager(private val context: Context) {
    private val bluetoothAdapter: BluetoothAdapter? by lazy {
        (context.getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager).adapter
    }

    private val scanner: BluetoothLeScanner?
        get() = bluetoothAdapter?.bluetoothLeScanner

    private val peatServiceUuid = ParcelUuid.fromString("f47ac10b-58cc-4372-a567-0e02b2c3d479")

    private val scanCallback = object : ScanCallback() {
        override fun onScanResult(callbackType: Int, result: ScanResult) {
            val device = result.device
            val name = result.scanRecord?.deviceName
            val rssi = result.rssi

            // Parse mesh ID from device name (PEAT_MESHID-NODEID)
            val meshId = name?.let {
                if (it.startsWith("PEAT_")) {
                    it.substringAfter("PEAT_").substringBefore("-")
                } else null
            }

            // Notify Rust layer
            PeatBridge.onDiscovered(
                device.address,
                name,
                rssi,
                meshId,
                System.currentTimeMillis()
            )
        }

        override fun onScanFailed(errorCode: Int) {
            Log.e("PeatBLE", "Scan failed: $errorCode")
        }
    }

    fun startScan() {
        val settings = ScanSettings.Builder()
            .setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY)
            .build()

        val filters = listOf(
            ScanFilter.Builder()
                .setServiceUuid(peatServiceUuid)
                .build()
        )

        scanner?.startScan(filters, settings, scanCallback)
    }

    fun stopScan() {
        scanner?.stopScan(scanCallback)
    }
}
```

### GATT Client Implementation

```kotlin
class PeatGattClient(
    private val context: Context,
    private val onDataReceived: (ByteArray) -> Unit
) {
    private var gatt: BluetoothGatt? = null
    private var documentCharacteristic: BluetoothGattCharacteristic? = null

    private val peatServiceUuid = UUID.fromString("f47ac10b-58cc-4372-a567-0e02b2c3d479")
    private val documentCharUuid = UUID.fromString("f47ac10b-58cc-4372-a567-0e02b2c3d479")

    private val gattCallback = object : BluetoothGattCallback() {
        override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) {
            when (newState) {
                BluetoothProfile.STATE_CONNECTED -> {
                    PeatBridge.onConnected(gatt.device.address, System.currentTimeMillis())
                    gatt.discoverServices()
                }
                BluetoothProfile.STATE_DISCONNECTED -> {
                    PeatBridge.onDisconnected(gatt.device.address, 1)
                }
            }
        }

        override fun onServicesDiscovered(gatt: BluetoothGatt, status: Int) {
            if (status == BluetoothGatt.GATT_SUCCESS) {
                val service = gatt.getService(peatServiceUuid)
                documentCharacteristic = service?.getCharacteristic(documentCharUuid)

                // Enable notifications
                documentCharacteristic?.let { char ->
                    gatt.setCharacteristicNotification(char, true)
                    val descriptor = char.getDescriptor(
                        UUID.fromString("00002902-0000-1000-8000-00805f9b34fb")
                    )
                    descriptor?.value = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE
                    gatt.writeDescriptor(descriptor)
                }
            }
        }

        override fun onCharacteristicChanged(
            gatt: BluetoothGatt,
            characteristic: BluetoothGattCharacteristic
        ) {
            if (characteristic.uuid == documentCharUuid) {
                val data = characteristic.value
                PeatBridge.onDataReceived(
                    gatt.device.address,
                    data,
                    System.currentTimeMillis()
                )
                onDataReceived(data)
            }
        }
    }

    fun connect(device: BluetoothDevice) {
        gatt = device.connectGatt(context, false, gattCallback)
    }

    fun disconnect() {
        gatt?.disconnect()
        gatt?.close()
        gatt = null
    }

    fun writeDocument(data: ByteArray) {
        documentCharacteristic?.let { char ->
            char.value = data
            gatt?.writeCharacteristic(char)
        }
    }
}
```

## Encryption Setup

```kotlin
// Generate or load 32-byte encryption secret
val encryptionSecret = ByteArray(32).also {
    SecureRandom().nextBytes(it)
}

// Initialize with encryption
PeatBridge.initWithEncryption(
    nodeId = getNodeId(),
    callsign = "ALPHA-1",
    meshId = "DEMO",
    encryptionSecret = encryptionSecret
)
```

## Lifecycle Integration

```kotlin
class PeatService : Service() {
    private lateinit var bleManager: PeatBleManager

    override fun onCreate() {
        super.onCreate()

        // Initialize Peat
        val nodeId = generateNodeId()
        PeatBridge.init(nodeId, "ANDROID-1", "DEMO")

        bleManager = PeatBleManager(this)

        // Start periodic tick
        handler.postDelayed(tickRunnable, 1000)
    }

    private val tickRunnable = object : Runnable {
        override fun run() {
            PeatBridge.tick(System.currentTimeMillis())?.let { doc ->
                // Broadcast to connected peers
                broadcastDocument(doc)
            }
            handler.postDelayed(this, 1000)
        }
    }

    private fun generateNodeId(): Long {
        // Use last 4 bytes of Bluetooth MAC address
        val btAddress = BluetoothAdapter.getDefaultAdapter()?.address
        return btAddress?.let {
            val bytes = it.split(":").map { b -> b.toInt(16).toByte() }
            ((bytes[2].toLong() and 0xFF) shl 24) or
            ((bytes[3].toLong() and 0xFF) shl 16) or
            ((bytes[4].toLong() and 0xFF) shl 8) or
            (bytes[5].toLong() and 0xFF)
        } ?: System.currentTimeMillis()
    }
}
```

## Testing

### Unit Testing

```kotlin
@Test
fun testPeatBridgeInit() {
    val result = PeatBridge.init(0x12345678, "TEST-1", "TEST")
    assertEquals(0, result)
}

@Test
fun testBuildDocument() {
    PeatBridge.init(0x12345678, "TEST-1", "TEST")
    val doc = PeatBridge.buildDocument()
    assertNotNull(doc)
    assertTrue(doc.isNotEmpty())
}
```

### Integration Testing

Use Android Emulator with BLE support or real devices:

1. Install app on two devices
2. Ensure both are on same mesh ID
3. Trigger emergency on device A
4. Verify device B receives emergency event

## Troubleshooting

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Scan returns no results | Missing permissions | Request runtime permissions |
| Connection fails | Device out of range | Move devices closer |
| Data not syncing | Wrong service UUID | Verify UUID matches PEAT_SERVICE_UUID |
| Library load error | Missing .so file | Check jniLibs directory structure |

### Debug Logging

Enable native logging:

```kotlin
// In Application.onCreate()
android_logger.init_once(
    android_logger.Config.default()
        .with_max_level(log.LevelFilter.Debug)
        .with_tag("peat-btle"),
)
```

View logs:
```bash
adb logcat -s peat-btle
```

## References

- [Android BLE Guide](https://developer.android.com/guide/topics/connectivity/bluetooth/ble-overview)
- [JNI Reference](https://docs.oracle.com/javase/8/docs/technotes/guides/jni/)
- [rust-android-gradle](https://github.com/aspect-build/aspect-workflows/tree/main/rust-android)
