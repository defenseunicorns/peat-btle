# Apple Platform Integration Guide (iOS/macOS)

This guide covers integrating `eche-btle` into iOS and macOS applications using CoreBluetooth.

## Requirements

| Platform | Minimum Version |
|----------|-----------------|
| iOS | 13.0 |
| macOS | 10.15 (Catalina) |
| Xcode | 15.0+ |

### Hardware Requirements

- iPhone 8 or newer for best BLE 5.0 support
- Any modern Mac with Bluetooth

## Architecture

```
┌─────────────────────────────────────────┐
│       SwiftUI / UIKit Application       │
├─────────────────────────────────────────┤
│         UniFFI Swift Bindings           │
├─────────────────────────────────────────┤
│      CoreBluetoothAdapter (Rust)        │
├─────────────────────────────────────────┤
│  CentralManager    │  PeripheralManager │
│   (scanning,       │   (advertising,    │
│    connecting)     │    GATT server)    │
├─────────────────────────────────────────┤
│           Objective-C Delegates          │
├─────────────────────────────────────────┤
│           CoreBluetooth Framework        │
└─────────────────────────────────────────┘
```

## Project Setup

### Option 1: Pure Swift with Native BLE

For simpler integration, use Swift's CoreBluetooth directly and call Rust for mesh logic only.

### Option 2: Full Rust Integration (UniFFI)

Use UniFFI to expose the entire Rust API to Swift.

---

## Info.plist Configuration

Add required permissions:

```xml
<!-- Bluetooth usage description -->
<key>NSBluetoothAlwaysUsageDescription</key>
<string>Eche uses Bluetooth to sync data with nearby devices</string>

<!-- For iOS 13+ -->
<key>NSBluetoothPeripheralUsageDescription</key>
<string>Eche uses Bluetooth to sync data with nearby devices</string>

<!-- Background modes (iOS) -->
<key>UIBackgroundModes</key>
<array>
    <string>bluetooth-central</string>
    <string>bluetooth-peripheral</string>
</array>
```

### macOS Sandbox Entitlements

For sandboxed Mac apps, add to `*.entitlements`:

```xml
<key>com.apple.security.device.bluetooth</key>
<true/>
```

## Swift Implementation

### EcheManager Class

```swift
import CoreBluetooth
import Combine

class EcheManager: NSObject, ObservableObject {
    // Published state
    @Published var peers: [EchePeer] = []
    @Published var isScanning = false
    @Published var isAdvertising = false
    @Published var emergencyActive = false

    // CoreBluetooth managers
    private var centralManager: CBCentralManager!
    private var peripheralManager: CBPeripheralManager!

    // Eche UUIDs
    private let echeServiceUUID = CBUUID(string: "F47AC10B-58CC-4372-A567-0E02B2C3D479")
    private let documentCharUUID = CBUUID(string: "F47AC10B-58CC-4372-A567-0E02B2C30003")

    // Connections
    private var connectedPeripherals: [UUID: CBPeripheral] = [:]
    private var documentCharacteristics: [UUID: CBCharacteristic] = [:]

    // Rust bridge
    private var meshBridge: EcheMeshBridge?

    override init() {
        super.init()
        centralManager = CBCentralManager(delegate: self, queue: nil)
        peripheralManager = CBPeripheralManager(delegate: self, queue: nil)

        // Initialize Rust mesh
        initializeMesh()
    }

    private func initializeMesh() {
        let nodeId = generateNodeId()
        meshBridge = EcheMeshBridge(
            nodeId: nodeId,
            callsign: "IOS-\(UIDevice.current.name.prefix(4))",
            meshId: "DEMO"
        )
    }

    private func generateNodeId() -> UInt32 {
        // Use a stable identifier derived from device
        let id = UIDevice.current.identifierForVendor ?? UUID()
        let bytes = id.uuid
        return UInt32(bytes.12) << 24 |
               UInt32(bytes.13) << 16 |
               UInt32(bytes.14) << 8 |
               UInt32(bytes.15)
    }

    // MARK: - Public API

    func startScanning() {
        guard centralManager.state == .poweredOn else { return }
        centralManager.scanForPeripherals(
            withServices: [echeServiceUUID],
            options: [CBCentralManagerScanOptionAllowDuplicatesKey: false]
        )
        isScanning = true
    }

    func stopScanning() {
        centralManager.stopScan()
        isScanning = false
    }

    func startAdvertising() {
        guard peripheralManager.state == .poweredOn else { return }

        let advertisementData: [String: Any] = [
            CBAdvertisementDataServiceUUIDsKey: [echeServiceUUID],
            CBAdvertisementDataLocalNameKey: meshBridge?.deviceName ?? "Eche"
        ]

        peripheralManager.startAdvertising(advertisementData)
        isAdvertising = true
    }

    func stopAdvertising() {
        peripheralManager.stopAdvertising()
        isAdvertising = false
    }

    func sendEmergency() {
        guard let data = meshBridge?.sendEmergency() else { return }
        broadcastToAllPeers(data: data)
        emergencyActive = true
    }

    func sendAck() {
        guard let data = meshBridge?.sendAck() else { return }
        broadcastToAllPeers(data: data)
    }

    func clearEmergency() {
        meshBridge?.clearEvent()
        emergencyActive = false
    }

    private func broadcastToAllPeers(data: Data) {
        for (uuid, char) in documentCharacteristics {
            if let peripheral = connectedPeripherals[uuid] {
                peripheral.writeValue(data, for: char, type: .withResponse)
            }
        }
    }

    func tick() {
        if let data = meshBridge?.tick() {
            broadcastToAllPeers(data: data)
        }
    }
}

// MARK: - CBCentralManagerDelegate

extension EcheManager: CBCentralManagerDelegate {
    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        switch central.state {
        case .poweredOn:
            startScanning()
            setupGattService()
        case .poweredOff:
            isScanning = false
            isAdvertising = false
        default:
            break
        }
    }

    func centralManager(_ central: CBCentralManager,
                        didDiscover peripheral: CBPeripheral,
                        advertisementData: [String: Any],
                        rssi RSSI: NSNumber) {
        let name = advertisementData[CBAdvertisementDataLocalNameKey] as? String

        // Parse mesh ID from name
        var meshId: String?
        if let name = name, name.hasPrefix("ECHE_") {
            let parts = name.dropFirst(5).split(separator: "-")
            if parts.count >= 1 {
                meshId = String(parts[0])
            }
        }

        // Notify Rust layer
        if let nodeId = meshBridge?.onDiscovered(
            identifier: peripheral.identifier.uuidString,
            name: name,
            rssi: RSSI.int8Value,
            meshId: meshId
        ) {
            // Add to peers list
            let peer = EchePeer(
                nodeId: nodeId,
                name: name ?? "Unknown",
                rssi: RSSI.intValue
            )

            if !peers.contains(where: { $0.nodeId == nodeId }) {
                DispatchQueue.main.async {
                    self.peers.append(peer)
                }
            }

            // Connect if not already
            if connectedPeripherals[peripheral.identifier] == nil {
                central.connect(peripheral, options: nil)
            }
        }
    }

    func centralManager(_ central: CBCentralManager,
                        didConnect peripheral: CBPeripheral) {
        connectedPeripherals[peripheral.identifier] = peripheral
        peripheral.delegate = self
        peripheral.discoverServices([echeServiceUUID])

        meshBridge?.onConnected(identifier: peripheral.identifier.uuidString)
    }

    func centralManager(_ central: CBCentralManager,
                        didDisconnectPeripheral peripheral: CBPeripheral,
                        error: Error?) {
        connectedPeripherals.removeValue(forKey: peripheral.identifier)
        documentCharacteristics.removeValue(forKey: peripheral.identifier)

        meshBridge?.onDisconnected(identifier: peripheral.identifier.uuidString)

        // Reconnect
        central.connect(peripheral, options: nil)
    }
}

// MARK: - CBPeripheralDelegate

extension EcheManager: CBPeripheralDelegate {
    func peripheral(_ peripheral: CBPeripheral,
                    didDiscoverServices error: Error?) {
        guard let services = peripheral.services else { return }

        for service in services {
            if service.uuid == echeServiceUUID {
                peripheral.discoverCharacteristics([documentCharUUID], for: service)
            }
        }
    }

    func peripheral(_ peripheral: CBPeripheral,
                    didDiscoverCharacteristicsFor service: CBService,
                    error: Error?) {
        guard let characteristics = service.characteristics else { return }

        for char in characteristics {
            if char.uuid == documentCharUUID {
                documentCharacteristics[peripheral.identifier] = char

                // Enable notifications
                peripheral.setNotifyValue(true, for: char)

                // Initial read
                peripheral.readValue(for: char)
            }
        }
    }

    func peripheral(_ peripheral: CBPeripheral,
                    didUpdateValueFor characteristic: CBCharacteristic,
                    error: Error?) {
        guard let data = characteristic.value else { return }

        if let result = meshBridge?.onDataReceived(
            identifier: peripheral.identifier.uuidString,
            data: data
        ) {
            if result.isEmergency {
                DispatchQueue.main.async {
                    self.emergencyActive = true
                    self.triggerHapticFeedback()
                }
            }
        }
    }

    private func triggerHapticFeedback() {
        #if os(iOS)
        let generator = UINotificationFeedbackGenerator()
        generator.notificationOccurred(.warning)
        #endif
    }
}

// MARK: - CBPeripheralManagerDelegate

extension EcheManager: CBPeripheralManagerDelegate {
    func peripheralManagerDidUpdateState(_ peripheral: CBPeripheralManager) {
        if peripheral.state == .poweredOn {
            setupGattService()
            startAdvertising()
        }
    }

    private func setupGattService() {
        let characteristic = CBMutableCharacteristic(
            type: documentCharUUID,
            properties: [.read, .write, .notify],
            value: nil,
            permissions: [.readable, .writeable]
        )

        let service = CBMutableService(type: echeServiceUUID, primary: true)
        service.characteristics = [characteristic]

        peripheralManager.add(service)
    }

    func peripheralManager(_ peripheral: CBPeripheralManager,
                           didReceiveRead request: CBATTRequest) {
        if request.characteristic.uuid == documentCharUUID {
            if let data = meshBridge?.buildDocument() {
                request.value = data
                peripheral.respond(to: request, withResult: .success)
            } else {
                peripheral.respond(to: request, withResult: .attributeNotFound)
            }
        }
    }

    func peripheralManager(_ peripheral: CBPeripheralManager,
                           didReceiveWrite requests: [CBATTRequest]) {
        for request in requests {
            if request.characteristic.uuid == documentCharUUID,
               let data = request.value {
                meshBridge?.onDataReceived(
                    identifier: request.central.identifier.uuidString,
                    data: data
                )
            }
        }

        if let first = requests.first {
            peripheral.respond(to: first, withResult: .success)
        }
    }
}
```

### SwiftUI View

```swift
import SwiftUI

struct ContentView: View {
    @StateObject private var echeManager = EcheManager()

    var body: some View {
        NavigationView {
            VStack(spacing: 20) {
                // Status
                HStack {
                    StatusIndicator(
                        label: "Scanning",
                        active: echeManager.isScanning
                    )
                    StatusIndicator(
                        label: "Advertising",
                        active: echeManager.isAdvertising
                    )
                }

                // Peer list
                List(echeManager.peers) { peer in
                    PeerRow(peer: peer)
                }

                // Action buttons
                HStack(spacing: 20) {
                    Button("EMERGENCY") {
                        echeManager.sendEmergency()
                    }
                    .buttonStyle(EmergencyButtonStyle())

                    Button("ACK") {
                        echeManager.sendAck()
                    }
                    .buttonStyle(AckButtonStyle())

                    Button("RESET") {
                        echeManager.clearEmergency()
                    }
                    .buttonStyle(ResetButtonStyle())
                }
            }
            .navigationTitle("Eche Mesh")
            .onAppear {
                // Start tick timer
                Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { _ in
                    echeManager.tick()
                }
            }
        }
    }
}
```

## UniFFI Integration (Optional)

For full Rust API exposure, use UniFFI bindings.

### 1. Create FFI Crate

```toml
# eche-apple-ffi/Cargo.toml
[package]
name = "eche-apple-ffi"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["staticlib", "cdylib"]
name = "eche_apple_ffi"

[dependencies]
eche-btle = { path = ".." }
uniffi = "0.25"

[build-dependencies]
uniffi = { version = "0.25", features = ["build"] }
```

### 2. Define UDL Interface

```udl
// eche-apple-ffi/src/eche.udl
namespace eche_apple_ffi {
    EcheMeshBridge create_mesh(u32 node_id, string callsign, string mesh_id);
};

interface EcheMeshBridge {
    constructor(u32 node_id, string callsign, string mesh_id);

    string device_name();

    u32? on_discovered(string identifier, string? name, i8 rssi, string? mesh_id);
    u32? on_connected(string identifier);
    void on_disconnected(string identifier);

    DataResult? on_data_received(string identifier, bytes data);

    bytes send_emergency();
    bytes send_ack();
    void clear_event();

    bytes? tick();
    bytes build_document();
};

dictionary DataResult {
    u32 source_node;
    boolean is_emergency;
    boolean is_ack;
};
```

### 3. Build Script

```bash
#!/bin/bash
# build-apple.sh

set -e

# Build for all Apple platforms
for TARGET in \
    aarch64-apple-ios \
    aarch64-apple-ios-sim \
    x86_64-apple-ios \
    aarch64-apple-darwin \
    x86_64-apple-darwin
do
    echo "Building for $TARGET..."
    cargo build --release --target $TARGET
done

# Create XCFramework
mkdir -p build

# Generate Swift bindings
cargo run --bin uniffi-bindgen generate \
    src/eche.udl --language swift --out-dir build/

# Create fat libraries
lipo -create \
    target/aarch64-apple-ios-sim/release/libeche_apple_ffi.a \
    target/x86_64-apple-ios/release/libeche_apple_ffi.a \
    -output build/libeche_apple_ffi_sim.a

# Create XCFramework
xcodebuild -create-xcframework \
    -library target/aarch64-apple-ios/release/libeche_apple_ffi.a \
    -headers build/ \
    -library build/libeche_apple_ffi_sim.a \
    -headers build/ \
    -library target/aarch64-apple-darwin/release/libeche_apple_ffi.a \
    -headers build/ \
    -output build/EcheFFI.xcframework

echo "XCFramework created at build/EcheFFI.xcframework"
```

## Background Execution

### iOS Background Handling

```swift
class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        // Check if launched from BLE event
        if let centralOptions = launchOptions?[.bluetoothCentrals] as? [String] {
            // Restore central manager state
        }
        if let peripheralOptions = launchOptions?[.bluetoothPeripherals] as? [String] {
            // Restore peripheral manager state
        }
        return true
    }
}
```

### State Restoration

```swift
// In EcheManager init
centralManager = CBCentralManager(
    delegate: self,
    queue: nil,
    options: [CBCentralManagerOptionRestoreIdentifierKey: "EcheCentral"]
)

peripheralManager = CBPeripheralManager(
    delegate: self,
    queue: nil,
    options: [CBPeripheralManagerOptionRestoreIdentifierKey: "EchePeripheral"]
)

// Handle restoration
func centralManager(_ central: CBCentralManager,
                    willRestoreState dict: [String: Any]) {
    if let peripherals = dict[CBCentralManagerRestoredStatePeripheralsKey] as? [CBPeripheral] {
        for peripheral in peripherals {
            connectedPeripherals[peripheral.identifier] = peripheral
            peripheral.delegate = self
        }
    }
}
```

## Troubleshooting

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Scan returns nothing | No BLE permission | Check Info.plist |
| Background stops | Missing background mode | Add UIBackgroundModes |
| Mac sandbox error | Missing entitlement | Add bluetooth entitlement |
| Discovery fails | Wrong UUID format | Use uppercase UUID |

### Debug Logging

```swift
// Enable CoreBluetooth debug logging
// Add to scheme environment variables:
// CBUUID_DEBUG=1
```

## References

- [CoreBluetooth Programming Guide](https://developer.apple.com/library/archive/documentation/NetworkingInternetWeb/Conceptual/CoreBluetooth_concepts/)
- [WWDC: What's New in Core Bluetooth](https://developer.apple.com/videos/play/wwdc2019/901/)
- [UniFFI Swift Bindings](https://mozilla.github.io/uniffi-rs/swift/overview.html)
- [Background Execution](https://developer.apple.com/documentation/corebluetooth/cbcentralmanager/1518696-restoredstate)
