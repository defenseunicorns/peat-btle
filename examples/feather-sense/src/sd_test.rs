//! Minimal SoftDevice test - no tasks

#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf as _;
use embassy_time::{Duration, Timer};
use nrf_softdevice::raw;
use panic_probe as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    info!("SD test starting...");

    let p = embassy_nrf::init(Default::default());
    let mut led = Output::new(p.P1_09, Level::Low, OutputDrive::Standard);

    // Blink once - got here
    led.set_high();
    Timer::after(Duration::from_millis(200)).await;
    led.set_low();
    Timer::after(Duration::from_millis(200)).await;

    info!("Enabling SoftDevice with minimal config...");

    // Minimal SoftDevice config
    let config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16,
            rc_temp_ctiv: 2,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        ..Default::default()
    };

    let _sd = nrf_softdevice::Softdevice::enable(&config);

    // Blink twice - SoftDevice enabled!
    for _ in 0..2 {
        led.set_high();
        Timer::after(Duration::from_millis(200)).await;
        led.set_low();
        Timer::after(Duration::from_millis(200)).await;
    }

    info!("SoftDevice enabled!");

    // Continuous blink - all good
    loop {
        led.set_high();
        Timer::after(Duration::from_millis(100)).await;
        led.set_low();
        Timer::after(Duration::from_millis(900)).await;
    }
}
