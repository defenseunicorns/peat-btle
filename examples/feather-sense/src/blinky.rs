//! Test: embassy init with SoftDevice-safe interrupt priorities

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
    info!("Starting...");

    // Configure embassy with SoftDevice-safe interrupt priorities
    // SoftDevice reserves P0, P1, P4 - we use P2 for time driver
    let mut config = embassy_nrf::config::Config::default();
    config.gpiote_interrupt_priority = Priority::P2;
    config.time_interrupt_priority = Priority::P2;

    let p = embassy_nrf::init(config);
    info!("Embassy initialized with safe priorities");

    let mut led = Output::new(p.P1_09, Level::Low, OutputDrive::Standard);

    // 1 blink = embassy init OK
    led.set_high();
    Timer::after(Duration::from_millis(200)).await;
    led.set_low();
    Timer::after(Duration::from_millis(200)).await;

    info!("Enabling SoftDevice...");

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

    // 2 blinks = SD enabled
    for _ in 0..2 {
        led.set_high();
        Timer::after(Duration::from_millis(200)).await;
        led.set_low();
        Timer::after(Duration::from_millis(200)).await;
    }

    spawner.spawn(softdevice_task(sd).unwrap());
    info!("SD task spawned");

    // Continuous blink = all working!
    loop {
        led.set_high();
        Timer::after(Duration::from_millis(500)).await;
        led.set_low();
        Timer::after(Duration::from_millis(500)).await;
    }
}
