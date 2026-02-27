# Windows Platform Integration Guide

This guide covers integrating `peat-btle` into Windows applications using the WinRT Bluetooth APIs.

## Requirements

| Feature | Minimum Windows Version |
|---------|-------------------------|
| BLE Scanning | Windows 10 1703 (Creators Update) |
| BLE Advertising | Windows 10 1703 |
| GATT Client | Windows 10 1703 |
| GATT Server | Windows 10 1803 (April 2018 Update) |
| Extended Advertising | Windows 10 1903 |
| Coded PHY | Windows 10 2004 |

### Hardware Requirements

- Bluetooth 4.0+ adapter (built-in or USB dongle)
- For BLE 5.0 features: Bluetooth 5.0+ adapter

## Architecture

```
┌─────────────────────────────────────────┐
│       WinRtBleAdapter (Rust)            │
├─────────────────────────────────────────┤
│     Watcher      │      Publisher       │
│   (scanning)     │    (advertising)     │
├─────────────────────────────────────────┤
│  GattClient      │    GattServer        │
│  (connecting,    │   (hosting Peat      │
│   reading)       │    service)          │
├─────────────────────────────────────────┤
│           WinRT Bluetooth APIs          │
└─────────────────────────────────────────┘
```

## Project Setup

### Cargo.toml

```toml
[dependencies]
peat-btle = { version = "0.1", features = ["windows"] }
windows = { version = "0.54", features = [
    "Devices_Bluetooth",
    "Devices_Bluetooth_Advertisement",
    "Devices_Bluetooth_GenericAttributeProfile",
    "Devices_Enumeration",
    "Foundation",
    "Foundation_Collections",
    "Storage_Streams",
]}
tokio = { version = "1", features = ["full"] }
log = "0.4"
```

### Basic Usage

```rust
use peat_btle::platform::windows::WinRtBleAdapter;
use peat_btle::{BleConfig, NodeId, MeshTransport};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create configuration
    let config = BleConfig::new(NodeId::new(0x12345678));

    // Create and initialize adapter
    let mut adapter = WinRtBleAdapter::new()?;
    adapter.init(&config).await?;

    // Start operations
    adapter.start().await?;

    println!("Peat BLE running on Windows...");
    println!("Address: {:?}", adapter.address());

    // Keep running
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
```

## WinRT API Mapping

### Advertisement Watcher (Scanning)

```rust
use windows::Devices::Bluetooth::Advertisement::*;

// Create watcher
let watcher = BluetoothLEAdvertisementWatcher::new()?;

// Configure scan settings
watcher.SetScanningMode(BluetoothLEScanningMode::Active)?;

// Set filter for Peat service UUID
let filter = BluetoothLEAdvertisementFilter::new()?;
let advertisement = BluetoothLEAdvertisement::new()?;
let service_uuids = advertisement.ServiceUuids()?;
service_uuids.Append(PEAT_SERVICE_GUID)?;
filter.SetAdvertisement(&advertisement)?;
watcher.SetAdvertisementFilter(&filter)?;

// Handle received advertisements
watcher.Received(&TypedEventHandler::new(|_, args: &Option<_>| {
    if let Some(args) = args {
        let address = args.BluetoothAddress()?;
        let rssi = args.RawSignalStrengthInDBm()?;
        let advertisement = args.Advertisement()?;
        let name = advertisement.LocalName()?.to_string();

        // Process discovered device
        println!("Found: {} RSSI: {}", name, rssi);
    }
    Ok(())
}))?;

// Start scanning
watcher.Start()?;
```

### Advertisement Publisher (Advertising)

```rust
use windows::Devices::Bluetooth::Advertisement::*;
use windows::Storage::Streams::*;

// Create publisher
let publisher = BluetoothLEAdvertisementPublisher::new()?;

// Build advertisement data
let advertisement = BluetoothLEAdvertisement::new()?;
advertisement.SetLocalName(&HSTRING::from("PEAT_DEMO-12345678"))?;

// Add service UUID
let service_uuids = advertisement.ServiceUuids()?;
service_uuids.Append(PEAT_SERVICE_GUID)?;

// Add service data
let data_section = BluetoothLEAdvertisementDataSection::new()?;
data_section.SetDataType(0x16)?; // Service Data - 16-bit UUID

let writer = DataWriter::new()?;
writer.WriteUInt16(PEAT_SERVICE_UUID_16BIT)?;
writer.WriteBytes(&beacon_data)?;
data_section.SetData(&writer.DetachBuffer()?)?;

let data_sections = advertisement.DataSections()?;
data_sections.Append(&data_section)?;

publisher.SetAdvertisement(&advertisement)?;

// Start advertising
publisher.Start()?;
```

### GATT Client (Connecting)

```rust
use windows::Devices::Bluetooth::*;
use windows::Devices::Bluetooth::GenericAttributeProfile::*;

async fn connect_to_device(address: u64) -> Result<(), Box<dyn std::error::Error>> {
    // Get device from address
    let device = BluetoothLEDevice::FromBluetoothAddressAsync(address)?.await?;

    // Get Peat service
    let services = device.GetGattServicesForUuidAsync(PEAT_SERVICE_GUID)?.await?;
    let service = services.Services()?.GetAt(0)?;

    // Get document characteristic
    let chars = service.GetCharacteristicsForUuidAsync(DOC_CHAR_GUID)?.await?;
    let characteristic = chars.Characteristics()?.GetAt(0)?;

    // Enable notifications
    characteristic.WriteClientCharacteristicConfigurationDescriptorAsync(
        GattClientCharacteristicConfigurationDescriptorValue::Notify
    )?.await?;

    // Handle value changes
    characteristic.ValueChanged(&TypedEventHandler::new(|_, args: &Option<_>| {
        if let Some(args) = args {
            let reader = DataReader::FromBuffer(&args.CharacteristicValue()?)?;
            let len = reader.UnconsumedBufferLength()? as usize;
            let mut data = vec![0u8; len];
            reader.ReadBytes(&mut data)?;

            // Process received data
            println!("Received {} bytes", data.len());
        }
        Ok(())
    }))?;

    Ok(())
}
```

### GATT Server (Hosting Service)

```rust
use windows::Devices::Bluetooth::GenericAttributeProfile::*;

async fn create_gatt_server() -> Result<(), Box<dyn std::error::Error>> {
    // Create service parameters
    let service_params = GattLocalCharacteristicParameters::new()?;
    service_params.SetCharacteristicProperties(
        GattCharacteristicProperties::Read |
        GattCharacteristicProperties::Write |
        GattCharacteristicProperties::Notify
    )?;
    service_params.SetReadProtectionLevel(GattProtectionLevel::Plain)?;
    service_params.SetWriteProtectionLevel(GattProtectionLevel::Plain)?;

    // Create service provider
    let result = GattServiceProvider::CreateAsync(PEAT_SERVICE_GUID)?.await?;
    let provider = result.ServiceProvider()?;

    // Add document characteristic
    let char_result = provider.Service()?.CreateCharacteristicAsync(
        DOC_CHAR_GUID,
        &service_params
    )?.await?;
    let characteristic = char_result.Characteristic()?;

    // Handle read requests
    characteristic.ReadRequested(&TypedEventHandler::new(|_, args: &Option<_>| {
        if let Some(args) = args {
            let deferral = args.GetDeferral()?;
            let request = args.GetRequestAsync()?.get()?;

            let writer = DataWriter::new()?;
            writer.WriteBytes(&get_current_document())?;
            request.RespondWithValue(&writer.DetachBuffer()?)?;

            deferral.Complete()?;
        }
        Ok(())
    }))?;

    // Handle write requests
    characteristic.WriteRequested(&TypedEventHandler::new(|_, args: &Option<_>| {
        if let Some(args) = args {
            let deferral = args.GetDeferral()?;
            let request = args.GetRequestAsync()?.get()?;

            let reader = DataReader::FromBuffer(&request.Value()?)?;
            let len = reader.UnconsumedBufferLength()? as usize;
            let mut data = vec![0u8; len];
            reader.ReadBytes(&mut data)?;

            // Process received document
            process_document(&data);

            request.Respond()?;
            deferral.Complete()?;
        }
        Ok(())
    }))?;

    // Start advertising
    let adv_params = GattServiceProviderAdvertisingParameters::new()?;
    adv_params.SetIsDiscoverable(true)?;
    adv_params.SetIsConnectable(true)?;
    provider.StartAdvertising(&adv_params)?;

    Ok(())
}
```

## High-Level Integration with PeatMesh

```rust
use peat_btle::{PeatMesh, PeatMeshConfig, NodeId};
use peat_btle::observer::{PeatEvent, PeatObserver};
use std::sync::Arc;

struct WindowsObserver;

impl PeatObserver for WindowsObserver {
    fn on_event(&self, event: PeatEvent) {
        match event {
            PeatEvent::EmergencyReceived { from_node } => {
                println!("EMERGENCY from {:08X}!", from_node.as_u32());
                // Show Windows notification
            }
            PeatEvent::PeerDiscovered { peer } => {
                println!("Discovered: {}", peer.display_name());
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create mesh
    let config = PeatMeshConfig::new(
        NodeId::new(0x12345678),
        "WIN-1",
        "DEMO",
    );
    let mesh = Arc::new(PeatMesh::new(config));

    // Add observer
    mesh.add_observer(Arc::new(WindowsObserver));

    // Create WinRT adapter and integrate with mesh
    let adapter = WinRtBleAdapter::new()?;

    // ... set up callbacks to call mesh.on_ble_* methods

    // Run tick loop
    loop {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as u64;

        if let Some(doc) = mesh.tick(now_ms) {
            // Broadcast document to connected peers
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
```

## UWP App Integration

For UWP apps, add capabilities to `Package.appxmanifest`:

```xml
<Capabilities>
    <DeviceCapability Name="bluetooth" />
    <DeviceCapability Name="bluetoothAdapter" />
</Capabilities>
```

## Desktop App (Win32)

For Win32 desktop apps, no special capabilities are needed, but ensure:

1. App runs with appropriate permissions
2. Bluetooth adapter is enabled
3. User has granted Bluetooth access in Windows Settings

## Troubleshooting

### Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| `E_ACCESSDENIED` | Missing capability | Add bluetooth capability |
| No devices found | Filter too strict | Check service UUID filter |
| GATT server fails | Wrong Windows version | Requires 1803+ |
| Connection drops | Range issues | Move devices closer |

### Debug Logging

```rust
use log::LevelFilter;
use env_logger::Builder;

fn setup_logging() {
    Builder::new()
        .filter_level(LevelFilter::Debug)
        .filter_module("peat_btle", LevelFilter::Trace)
        .init();
}
```

### Check Bluetooth State

```rust
use windows::Devices::Radios::*;

async fn check_bluetooth_state() -> Result<bool, Box<dyn std::error::Error>> {
    let radios = Radio::GetRadiosAsync()?.await?;

    for i in 0..radios.Size()? {
        let radio = radios.GetAt(i)?;
        if radio.Kind()? == RadioKind::Bluetooth {
            return Ok(radio.State()? == RadioState::On);
        }
    }

    Ok(false)
}
```

## Performance Considerations

1. **Scanning**: Use filters to reduce callback frequency
2. **Advertising**: Use 500ms+ intervals for battery efficiency
3. **GATT**: Request appropriate MTU for document size
4. **Threading**: WinRT callbacks run on thread pool - sync to UI thread as needed

## References

- [Windows.Devices.Bluetooth Namespace](https://docs.microsoft.com/en-us/uwp/api/windows.devices.bluetooth)
- [Bluetooth GATT Server](https://docs.microsoft.com/en-us/windows/uwp/devices-sensors/gatt-server)
- [windows-rs crate](https://github.com/microsoft/windows-rs)
