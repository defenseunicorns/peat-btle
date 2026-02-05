//! Raw blinky - no embassy, no timers, just GPIO and busy-wait
//!
//! This avoids ALL peripherals except GPIO to test if the basic
//! cortex-m-rt startup works with the Adafruit bootloader.

#![no_std]
#![no_main]

use core::panic::PanicInfo;

// Pull in embassy-nrf to provide interrupt vectors
use embassy_nrf as _;

// GPIO P1 registers (nRF52840)
// P0 base = 0x50000000, P1 base = 0x50000300
const P1_BASE: u32 = 0x5000_0300;
const P1_OUTSET: *mut u32 = (P1_BASE + 0x508) as *mut u32;
const P1_OUTCLR: *mut u32 = (P1_BASE + 0x50C) as *mut u32;
const P1_DIRSET: *mut u32 = (P1_BASE + 0x518) as *mut u32;

const RED_LED: u32 = 9;   // P1.09
const BLUE_LED: u32 = 10; // P1.10

#[cortex_m_rt::entry]
fn main() -> ! {
    // Configure LEDs as outputs
    unsafe {
        core::ptr::write_volatile(P1_DIRSET, (1 << RED_LED) | (1 << BLUE_LED));
    }

    // Turn on red LED immediately to show we're running
    unsafe {
        core::ptr::write_volatile(P1_OUTSET, 1 << RED_LED);
    }

    // Busy-wait blink loop
    loop {
        // Red on
        unsafe {
            core::ptr::write_volatile(P1_OUTSET, 1 << RED_LED);
            core::ptr::write_volatile(P1_OUTCLR, 1 << BLUE_LED);
        }
        busy_wait(2_000_000);

        // Blue on
        unsafe {
            core::ptr::write_volatile(P1_OUTCLR, 1 << RED_LED);
            core::ptr::write_volatile(P1_OUTSET, 1 << BLUE_LED);
        }
        busy_wait(2_000_000);
    }
}

#[inline(never)]
fn busy_wait(cycles: u32) {
    for _ in 0..cycles {
        cortex_m::asm::nop();
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    // On panic, turn on both LEDs
    unsafe {
        core::ptr::write_volatile(P1_OUTSET, (1 << RED_LED) | (1 << BLUE_LED));
    }
    loop {
        cortex_m::asm::wfi();
    }
}
