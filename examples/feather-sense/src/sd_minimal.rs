//! Minimal SoftDevice blinky with correct init sequence
//!
//! Based on embassy-rs nrf-softdevice examples

#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf::interrupt::Priority;
use embassy_time::{Duration, Timer};
use nrf_softdevice::{raw, Softdevice};
use panic_probe as _;

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // CRITICAL: Configure embassy with SoftDevice-safe interrupt priorities FIRST
    // SoftDevice reserves priorities 0, 1, 4 (highest)
    // We must use 2, 3, 5, 6, 7 for our interrupts
    let mut config = embassy_nrf::config::Config::default();
    config.gpiote_interrupt_priority = Priority::P2;
    config.time_interrupt_priority = Priority::P2;

    let p = embassy_nrf::init(config);

    // Set up LED immediately (before SoftDevice)
    let mut led = Output::new(p.P1_09, Level::High, OutputDrive::Standard);

    // Quick blink to show we got this far
    Timer::after(Duration::from_millis(100)).await;
    led.set_low();
    Timer::after(Duration::from_millis(100)).await;
    led.set_high();
    Timer::after(Duration::from_millis(100)).await;
    led.set_low();

    info!("Embassy initialized, enabling SoftDevice...");

    // Minimal SoftDevice config - just clock
    let sd_config = nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16,
            rc_temp_ctiv: 2,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        ..Default::default()
    };

    let sd = Softdevice::enable(&sd_config);
    info!("SoftDevice enabled!");

    // Two blinks = SD OK
    for _ in 0..2 {
        led.set_high();
        Timer::after(Duration::from_millis(200)).await;
        led.set_low();
        Timer::after(Duration::from_millis(200)).await;
    }

    spawner.spawn(softdevice_task(sd).expect("sd task"));
    info!("SD task spawned, entering main loop");

    // Continuous slow blink
    loop {
        led.set_high();
        Timer::after(Duration::from_millis(500)).await;
        led.set_low();
        Timer::after(Duration::from_millis(500)).await;
    }
}
