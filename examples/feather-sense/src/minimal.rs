//! Ultra minimal - just turn on ALL possible LEDs, no blinking

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use cortex_m_rt::entry;

use embassy_nrf as _;
use nrf_softdevice as _;

// GPIO bases
const P0_BASE: u32 = 0x5000_0000;
const P1_BASE: u32 = 0x5000_0300;

// Register offsets
const OUTSET: u32 = 0x508;
const OUTCLR: u32 = 0x50C;
const DIRSET: u32 = 0x518;
const PIN_CNF_BASE: u32 = 0x700;

fn pin_cnf(base: u32, pin: u32) -> *mut u32 {
    (base + PIN_CNF_BASE + pin * 4) as *mut u32
}

fn configure_output(base: u32, pin: u32) {
    unsafe {
        // Configure pin: DIR=output, INPUT=disconnected
        core::ptr::write_volatile(pin_cnf(base, pin), 0x3);
        // Set direction
        core::ptr::write_volatile((base + DIRSET) as *mut u32, 1 << pin);
    }
}

fn set_low(base: u32, pin: u32) {
    unsafe {
        core::ptr::write_volatile((base + OUTCLR) as *mut u32, 1 << pin);
    }
}

fn set_high(base: u32, pin: u32) {
    unsafe {
        core::ptr::write_volatile((base + OUTSET) as *mut u32, 1 << pin);
    }
}

#[entry]
fn main() -> ! {
    // Red LED = P1.09 (active HIGH)
    configure_output(P1_BASE, 9);
    set_high(P1_BASE, 9);

    loop {
        cortex_m::asm::nop();
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop { cortex_m::asm::wfi(); }
}
