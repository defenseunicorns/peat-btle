//! Pure Rust blinky - NO SoftDevice
//!
//! Validates the toolchain works end-to-end without any Nordic binary blobs.
//! This is milestone 1 from RUST-NRF52840.md.

#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_time::{Duration, Timer};
use panic_probe as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    info!("Pure Rust blinky starting (no SoftDevice)...");

    // Default config - no special interrupt priority dance needed without SD
    let p = embassy_nrf::init(Default::default());

    // Feather Sense LED pins (directly from schematic)
    // Red LED: P1.09 (active HIGH)
    // Blue LED: P1.10 (active HIGH)
    let mut red_led = Output::new(p.P1_09, Level::Low, OutputDrive::Standard);
    let mut blue_led = Output::new(p.P1_10, Level::Low, OutputDrive::Standard);

    info!("GPIO initialized, starting blink pattern");

    // Startup pattern: alternate red/blue 3 times
    for i in 0..3 {
        info!("Startup blink {}", i + 1);
        red_led.set_high();
        Timer::after(Duration::from_millis(200)).await;
        red_led.set_low();
        blue_led.set_high();
        Timer::after(Duration::from_millis(200)).await;
        blue_led.set_low();
    }

    info!("Entering main loop - red heartbeat");

    // Main loop: slow red heartbeat
    let mut count: u32 = 0;
    loop {
        red_led.set_high();
        Timer::after(Duration::from_millis(100)).await;
        red_led.set_low();
        Timer::after(Duration::from_millis(900)).await;

        count += 1;
        if count % 10 == 0 {
            info!("Heartbeat count: {}", count);
        }
    }
}
