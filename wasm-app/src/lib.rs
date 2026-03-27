//! SPDX-License-Identifier: MIT
//!
//! Copyright (c) 2026 Kevin Thomas
//!
//! # WASM Blinky Application
//!
//! A minimal WebAssembly module that blinks the onboard LED on an RP2350 Pico 2
//! by calling host-provided GPIO and delay functions.

#![no_std]

use core::panic::PanicInfo;

// Host-imported functions provided by the firmware WASM runtime.
// The extern block declares WASM imports — no C code is involved.
unsafe extern "C" {
    /// Sets the onboard LED GPIO pin to a high (on) state.
    safe fn gpio_set_high();
    /// Sets the onboard LED GPIO pin to a low (off) state.
    safe fn gpio_set_low();
    /// Delays execution for the specified number of milliseconds.
    ///
    /// # Arguments
    ///
    /// * `ms` - Duration of the delay in milliseconds.
    safe fn delay_ms(ms: u32);
}

/// Panic handler for the WASM environment that halts in an infinite loop.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// Calls the host function to set the LED GPIO pin high.
fn set_led_high() {
    gpio_set_high();
}

/// Calls the host function to delay execution for the given milliseconds.
///
/// # Arguments
///
/// * `ms` - Duration of the delay in milliseconds.
fn delay(ms: u32) {
    delay_ms(ms);
}

/// Calls the host function to set the LED GPIO pin low.
fn set_led_low() {
    gpio_set_low();
}

/// WASM entry point that blinks the onboard LED at 500ms intervals indefinitely.
///
/// Calls host-provided GPIO and delay functions in a continuous loop to toggle
/// the LED on and off with a half-second period.
#[unsafe(no_mangle)]
pub fn run() {
    loop {
        set_led_high();
        delay(500);
        set_led_low();
        delay(500);
    }
}
