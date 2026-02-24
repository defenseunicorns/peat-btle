//! Test: SoftDevice init + continuous blink (no advertising)

#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_nrf::gpio::{Level, Output, OutputDrive};
use embassy_time::{Duration, Timer};
use nrf_softdevice::{raw, Softdevice};
use panic_probe as _;

// Busy-wait delay (doesn't use interrupts)
fn busy_delay_ms(ms: u32) {
    for _ in 0..ms {
        for _ in 0..8000 {
            cortex_m::asm::nop();
        }
    }
}

#[embassy_executor::task]
async fn softdevice_task(sd: &'static Softdevice) -> ! {
    sd.run().await
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("SD_BLINK starting");

    let p = embassy_nrf::init(Default::default());
    let mut led = Output::new(p.P1_09, Level::Low, OutputDrive::Standard);

    // STAGE 1: 3 blinks (busy-wait) = embassy started
    info!("Stage 1: Embassy started");
    for _ in 0..3 {
        led.set_high();
        busy_delay_ms(500);
        led.set_low();
        busy_delay_ms(500);
    }

    info!("Enabling SoftDevice...");

    let sd = Softdevice::enable(&nrf_softdevice::Config {
        clock: Some(raw::nrf_clock_lf_cfg_t {
            source: raw::NRF_CLOCK_LF_SRC_RC as u8,
            rc_ctiv: 16,
            rc_temp_ctiv: 2,
            accuracy: raw::NRF_CLOCK_LF_ACCURACY_500_PPM as u8,
        }),
        gap_role_count: Some(raw::ble_gap_cfg_role_count_t {
            adv_set_count: 1,
            periph_role_count: 1,
            central_role_count: 0,
            central_sec_count: 0,
            _bitfield_1: raw::ble_gap_cfg_role_count_t::new_bitfield_1(0),
        }),
        ..Default::default()
    });

    info!("SoftDevice enabled!");

    // STAGE 2: 4 blinks (busy-wait) = SD enabled
    info!("Stage 2: SD enabled");
    for _ in 0..4 {
        led.set_high();
        busy_delay_ms(500);
        led.set_low();
        busy_delay_ms(500);
    }

    // SOLID ON = about to create spawn token
    led.set_high();
    busy_delay_ms(1000);

    info!("Creating spawn token...");
    let token = softdevice_task(sd).unwrap();

    // SOLID OFF = token created OK
    led.set_low();
    busy_delay_ms(1000);

    // SOLID ON = about to spawn
    led.set_high();
    busy_delay_ms(1000);

    info!("Calling spawner.spawn...");
    spawner.spawn(token);

    // SOLID OFF = spawn returned
    led.set_low();
    busy_delay_ms(1000);

    info!("SD task spawned!");

    // STAGE 3: 5 blinks (busy-wait) = task spawned successfully
    info!("Stage 3: Task spawned");
    for _ in 0..5 {
        led.set_high();
        busy_delay_ms(500);
        led.set_low();
        busy_delay_ms(500);
    }

    // STAGE 4: Fast blink using Timer::after = SUCCESS
    info!("Stage 4: Testing Timer::after...");
    loop {
        led.set_high();
        Timer::after(Duration::from_millis(100)).await;
        led.set_low();
        Timer::after(Duration::from_millis(100)).await;
    }
}
