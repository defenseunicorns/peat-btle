//! Test Embassy without SoftDevice

#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_nrf as _;
use embassy_time::{Duration, Timer};
use panic_probe as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    info!("Embassy test starting...");

    let p = embassy_nrf::init(Default::default());

    // Red LED = P1.09 (active HIGH)
    let mut red_led = Output::new(p.P1_09, Level::Low, OutputDrive::Standard);

    info!("Blinking LED...");

    loop {
        red_led.set_high();
        Timer::after(Duration::from_millis(200)).await;
        red_led.set_low();
        Timer::after(Duration::from_millis(200)).await;
    }
}
