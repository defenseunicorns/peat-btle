//! Absolute minimum blinky - no embassy, no async, just raw hardware
//!
//! If this doesn't blink, the problem is cortex-m-rt or memory layout.

#![no_std]
#![no_main]

use core::panic::PanicInfo;

// Red LED on Feather Sense: P1.09
const P1_OUTSET: *mut u32 = 0x5000_0508 as *mut u32;  // GPIO P1 OUTSET register
const P1_OUTCLR: *mut u32 = 0x5000_050C as *mut u32;  // GPIO P1 OUTCLR register
const P1_DIRSET: *mut u32 = 0x5000_0518 as *mut u32;  // GPIO P1 DIR SET register

const LED_PIN: u32 = 9; // P1.09

// Provide empty interrupt vector table to satisfy cortex-m-rt
// This works because nrf-pac provides the actual vectors via its device.x
#[allow(non_upper_case_globals)]
#[no_mangle]
pub static __INTERRUPTS: [unsafe extern "C" fn(); 48] = [default_handler; 48];

#[no_mangle]
unsafe extern "C" fn default_handler() {
    loop {}
}

#[cortex_m_rt::entry]
fn main() -> ! {
    // Configure P1.09 as output
    unsafe {
        core::ptr::write_volatile(P1_DIRSET, 1 << LED_PIN);
    }

    loop {
        // LED on
        unsafe {
            core::ptr::write_volatile(P1_OUTSET, 1 << LED_PIN);
        }
        delay(500_000);

        // LED off
        unsafe {
            core::ptr::write_volatile(P1_OUTCLR, 1 << LED_PIN);
        }
        delay(500_000);
    }
}

#[inline(never)]
fn delay(count: u32) {
    for _ in 0..count {
        cortex_m::asm::nop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        cortex_m::asm::wfi();
    }
}
