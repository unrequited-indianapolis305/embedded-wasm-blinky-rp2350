//! SPDX-License-Identifier: MIT
//!
//! Copyright (c) 2026 Kevin Thomas
//!
//! # WASM Blinky Component
//!
//! A minimal WebAssembly component that blinks the onboard LED on an RP2350
//! Pico 2 by calling host-provided GPIO and delay functions through typed
//! WIT interfaces. GPIO pins are addressed by their hardware pin number
//! (e.g., 25 for the onboard LED).

#![no_std]

// Enable the global allocator for heap-backed collections.
extern crate alloc;

use core::panic::PanicInfo; // Panic handler signature type.

/// Global heap allocator required by the canonical ABI's `cabi_realloc`.
#[global_allocator]
static ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

use embedded::platform::{gpio, timing}; // Host-provided GPIO and timing imports.

// Generate guest-side bindings for the `blinky` WIT world.
wit_bindgen::generate!({
    world: "blinky",
    path: "../wit",
});

/// WASM guest component implementing the `blinky` world.
struct BlinkyApp;

// Register `BlinkyApp` as the component's exported implementation.
export!(BlinkyApp);

impl Guest for BlinkyApp {
    /// Blinks the onboard LED at 500ms intervals in an infinite loop.
    fn run() {
        /// Hardware GPIO pin number for the onboard LED.
        const LED_PIN: u32 = 25;
        loop {
            gpio::set_high(LED_PIN);
            timing::delay_ms(500);
            gpio::set_low(LED_PIN);
            timing::delay_ms(500);
        }
    }
}

/// Panic handler for the WASM environment that halts in an infinite loop.
///
/// # Arguments
///
/// * `_info` - Panic information (unused in the WASM environment).
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
