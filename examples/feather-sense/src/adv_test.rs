//! Minimal advertising test - raw AD bytes like eche-btle library

#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_time::{Duration, Timer};
use nrf_softdevice::ble::peripheral;
use nrf_softdevice::{raw, Softdevice};
use panic_probe as _;

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("ADV_TEST starting!");

    let p = embassy_nrf::init(Default::default());

    let mut led = Output::new(p.P1_09, Level::Low, OutputDrive::Standard);

    // 1 blink = started
    led.set_high();
    Timer::after(Duration::from_millis(200)).await;
    led.set_low();
    Timer::after(Duration::from_millis(200)).await;

    info!("Enabling SoftDevice...");

    // More complete SD config - match exact_example
    let config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16,
            rc_temp_ctiv: 2,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        conn_gap: Some(raw::ble_gap_conn_cfg_t {
            conn_count: 6,
            event_length: 24,
        }),
        conn_gatt: Some(raw::ble_gatt_conn_cfg_t { att_mtu: 256 }),
        gatts_attr_tab_size: Some(raw::ble_gatts_cfg_attr_tab_size_t {
            attr_tab_size: raw::BLE_GATTS_ATTR_TAB_SIZE_DEFAULT,
        }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: 1,
            periph_role_count: 3,
            central_role_count: 3,
            central_sec_count: 0,
            _bitfield_1: raw::ble_gap_cfg_role_count_t::new_bitfield_1(0),
        }),
        ..Default::default()
    };

    let sd = Softdevice::enable(&config);
    info!("SoftDevice enabled!");

    // 2 blinks = SD enabled
    for _ in 0..2 {
        led.set_high();
        Timer::after(Duration::from_millis(200)).await;
        led.set_low();
        Timer::after(Duration::from_millis(200)).await;
    }

    spawner.spawn(softdevice_task(sd).unwrap());
    info!("SD task spawned");

    // 3 blinks = SD task spawned
    for _ in 0..3 {
        led.set_high();
        Timer::after(Duration::from_millis(200)).await;
        led.set_low();
        Timer::after(Duration::from_millis(200)).await;
    }

    // Build raw advertising data like eche-btle library does
    // Format: Flags(3) + ServiceUUID(4) + ServiceData(14) + ShortName(6) = 27 bytes
    let node_id: u32 = unsafe { core::ptr::read_volatile(0x10000060 as *const u32) };

    // Raw AD bytes
    let adv_data: [u8; 27] = [
        // Flags (3 bytes)
        0x02, 0x01, 0x06,
        // Complete List of 16-bit Service UUIDs (4 bytes)
        0x03, 0x03, 0x7A, 0xF4,  // UUID 0xF47A little-endian
        // Service Data - 16-bit UUID (14 bytes: 1+1+2+10)
        0x0D, 0x16, 0x7A, 0xF4,  // Length=13, Type=0x16, UUID=0xF47A
        // Beacon data (10 bytes): version|caps, caps_lo, node_id[4], hierarchy, battery, seq[2]
        0x10, 0x00,  // Version 1, caps=0
        (node_id >> 24) as u8, (node_id >> 16) as u8, (node_id >> 8) as u8, node_id as u8,
        0x00,  // Hierarchy: Platform
        0x55,  // Battery: 85%
        0x00, 0x01,  // Sequence: 1
        // Shortened Local Name (6 bytes)
        0x05, 0x08, b'H', b'I', b'V', b'E',
    ];

    // Scan response with full name
    let scan_data: [u8; 10] = [
        // Complete Local Name (length=9, type=0x09, then 8 chars)
        0x09, 0x09, b'B', b'F', b'S', b'-', b'H', b'I', b'V', b'E',
    ];

    info!("NodeID: {:08X}", node_id);
    info!("Adv data: {} bytes", adv_data.len());

    // 4 blinks = about to advertise
    for _ in 0..4 {
        led.set_high();
        Timer::after(Duration::from_millis(200)).await;
        led.set_low();
        Timer::after(Duration::from_millis(200)).await;
    }

    info!("Starting advertising loop...");

    // Advertising loop
    loop {
        led.set_high();

        let adv = peripheral::NonconnectableAdvertisement::ScannableUndirected {
            adv_data: &adv_data,
            scan_data: &scan_data,
        };

        let mut adv_config = peripheral::Config::default();
        adv_config.interval = 160;  // 100ms (160 * 0.625ms)

        info!("Calling advertise...");

        match peripheral::advertise(sd, adv, &adv_config).await {
            Ok(()) => info!("Advertise returned Ok"),
            Err(e) => info!("Advertise error: {:?}", defmt::Debug2Format(&e)),
        }

        led.set_low();
        Timer::after(Duration::from_millis(500)).await;
    }
}
