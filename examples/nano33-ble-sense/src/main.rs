//! Eche Sensor Beacon for Arduino Nano 33 BLE Sense
//!
//! Transmit-only Eche node that:
//! - Advertises Eche beacon
//! - Serves sensor data via GATT
//!
//! ## Building
//!
//! ```bash
//! cargo build --release
//! cargo run --release
//! ```

#![no_std]
#![no_main]

use core::mem;

use defmt::{info, warn, unwrap, Debug2Format};
use defmt_rtt as _;
use embassy_nrf as _; // Force link of time driver (RTC1)
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use nrf_softdevice::ble::advertisement_builder::{
    Flag, LegacyAdvertisementBuilder, ServiceList, ServiceUuid16,
};
use nrf_softdevice::ble::gatt_server;
use nrf_softdevice::ble::peripheral;
use nrf_softdevice::{raw, Softdevice};
use panic_probe as _;

/// Eche Service UUID (16-bit short form for advertising)
const ECHE_SERVICE_UUID_16: u16 = 0xF47A;

/// Device name in advertisements
const DEVICE_NAME: &str = "ECHE-SENSE";

// GATT Server definition
#[nrf_softdevice::gatt_server]
struct EcheServer {
    eche: EcheService,
}

// Eche GATT Service
#[nrf_softdevice::gatt_service(uuid = "f47ac10b-58cc-4372-a567-0e02b2c3d479")]
struct EcheService {
    /// Document characteristic - CRDT state
    #[characteristic(uuid = "f47a0003-58cc-4372-a567-0e02b2c3d479", read, write, notify)]
    document: [u8; 128],

    /// Sensor data characteristic - latest readings
    #[characteristic(uuid = "f47a0010-58cc-4372-a567-0e02b2c3d479", read, notify)]
    sensor_data: [u8; 32],
}

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Eche Sensor Beacon starting...");

    // Configure SoftDevice
    let config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16,
            rc_temp_ctiv: 2,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: 1,
            event_length: 24,
        }),
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t { att_mtu: 128 }),
        gatts_attr_tab_size: Some(raw::ble_gatts_cfg_attr_tab_size_t {
            attr_tab_size: 1024,
        }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: 1,
            periph_role_count: 1,
            central_role_count: 0,
            central_sec_count: 0,
            _bitfield_1: raw::ble_gap_cfg_role_count_t::new_bitfield_1(0),
        }),
        gap_device_name: Some(raw::ble_gap_cfg_device_name_t {
            p_value: DEVICE_NAME.as_ptr() as *const u8 as *mut u8,
            current_len: DEVICE_NAME.len() as u16,
            max_len: DEVICE_NAME.len() as u16,
            write_perm: unsafe { mem::zeroed() },
            _bitfield_1: raw::ble_gap_cfg_device_name_t::new_bitfield_1(
                raw::BLE_GATTS_VLOC_STACK as u8,
            ),
        }),
        ..Default::default()
    };

    // Enable SoftDevice
    let sd = Softdevice::enable(&config);

    // Create GATT server
    let server = unwrap!(EcheServer::new(sd));

    // Spawn softdevice task
    spawner.spawn(unwrap!(softdevice_task(sd)));

    info!("BLE initialized, starting advertising...");

    // Main advertising loop
    loop {
        // Build advertisement data
        let adv_data = LegacyAdvertisementBuilder::new()
            .flags(&[Flag::GeneralDiscovery, Flag::LE_Only])
            .services_16(
                ServiceList::Complete,
                &[ServiceUuid16::from_u16(ECHE_SERVICE_UUID_16)],
            )
            .full_name(DEVICE_NAME)
            .build();

        let scan_data = LegacyAdvertisementBuilder::new().build();

        let config = peripheral::Config::default();

        // Advertise and wait for connection
        info!("Advertising as {}...", DEVICE_NAME);
        let adv = peripheral::ConnectableAdvertisement::ScannableUndirected {
            adv_data: &adv_data,
            scan_data: &scan_data,
        };

        match peripheral::advertise_connectable(sd, adv, &config).await {
            Ok(conn) => {
                info!("Connected!");

                // Run GATT server until disconnect
                let res = gatt_server::run(&conn, &server, |event| match event {
                    EcheServerEvent::Eche(e) => match e {
                        EcheServiceEvent::DocumentWrite(data) => {
                            info!("Received document: {} bytes", data.len());
                            // TODO: Merge into local CRDT
                        }
                        EcheServiceEvent::SensorDataCccdWrite { notifications } => {
                            info!("Sensor notifications: {}", notifications);
                        }
                        EcheServiceEvent::DocumentCccdWrite { notifications } => {
                            info!("Document notifications: {}", notifications);
                        }
                    },
                })
                .await;

                info!("Disconnected: {:?}", Debug2Format(&res));
            }
            Err(e) => {
                warn!("Advertise error: {:?}", Debug2Format(&e));
            }
        }

        // Small delay before re-advertising
        Timer::after(Duration::from_millis(100)).await;
    }
}
