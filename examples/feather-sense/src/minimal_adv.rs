//! Minimal BLE advertising - Feather nRF52840 Sense
//!
//! Clean implementation from scratch.

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

/// Run the SoftDevice event loop
#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Minimal ADV starting");

    // Default embassy config - no custom priorities
    let p = embassy_nrf::init(Default::default());

    // Red LED for status
    let mut led = Output::new(p.P1_09, Level::Low, OutputDrive::Standard);

    // Blink to show we started
    for _ in 0..2 {
        led.set_high();
        Timer::after(Duration::from_millis(100)).await;
        led.set_low();
        Timer::after(Duration::from_millis(100)).await;
    }

    info!("Enabling SoftDevice");

    // SoftDevice config - MUST set gap_role_count for advertising to work
    let sd = Softdevice::enable(&nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16,
            rc_temp_ctiv: 2,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        // Enable advertising capability
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: 1,
            periph_role_count: 1,
            central_role_count: 0,
            central_sec_count: 0,
            _bitfield_1: raw::ble_gap_cfg_role_count_t::new_bitfield_1(0),
        }),
        ..Default::default()
    });

    info!("SoftDevice enabled");

    // Blink 3 times = SD enabled
    for _ in 0..3 {
        led.set_high();
        Timer::after(Duration::from_millis(100)).await;
        led.set_low();
        Timer::after(Duration::from_millis(100)).await;
    }

    // Spawn SD task
    spawner.spawn(softdevice_task(sd).unwrap());

    Timer::after(Duration::from_millis(500)).await;

    info!("Starting advertising");

    // Raw advertising data - match working Eche devices
    #[rustfmt::skip]
    let adv_data: &[u8] = &[
        // Flags (3 bytes)
        0x02, 0x01, 0x06,
        // Complete List of 16-bit Service UUIDs (4 bytes)
        0x03, 0x03, 0x7A, 0xF4,  // UUID 0xF47A little-endian
        // Service Data (8 bytes): len, type, UUID, data
        0x07, 0x16, 0x7A, 0xF4,  // len=7, type=0x16, UUID=F47A
        0xDE, 0xAD, 0xBE, 0xEF,  // beacon data
        // Short Name (6 bytes)
        0x05, 0x08, b'H', b'I', b'V', b'E',
    ];

    #[rustfmt::skip]
    let scan_data: &[u8] = &[
        // Full name in scan response
        0x09, 0x09, b'B', b'F', b'S', b'-', b'H', b'I', b'V', b'E',
    ];

    // Blink 4 times = about to advertise
    for _ in 0..4 {
        led.set_high();
        Timer::after(Duration::from_millis(100)).await;
        led.set_low();
        Timer::after(Duration::from_millis(100)).await;
    }

    // Use non-connectable advertising (like nrf-softdevice examples)
    let adv = peripheral::NonconnectableAdvertisement::ScannableUndirected {
        adv_data,
        scan_data,
    };

    let config = peripheral::Config {
        interval: 160, // 100ms (160 * 0.625ms)
        ..Default::default()
    };

    info!("Calling advertise");

    // Start advertising (runs forever for non-connectable, or until error)
    let result = peripheral::advertise(sd, adv, &config).await;

    // If we get here, advertising stopped/failed
    match result {
        Ok(()) => info!("Advertise returned Ok (unexpected for non-connectable)"),
        Err(e) => info!("Advertise error: {:?}", defmt::Debug2Format(&e)),
    }

    // Fast blink = advertise returned (error case)
    loop {
        led.set_high();
        Timer::after(Duration::from_millis(100)).await;
        led.set_low();
        Timer::after(Duration::from_millis(100)).await;
    }
}
