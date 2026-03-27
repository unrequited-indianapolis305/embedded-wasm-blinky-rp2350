//! SPDX-License-Identifier: MIT
//!
//! Copyright (c) 2026 Kevin Thomas
//!
//! # GPIO25 LED Driver for RP2350 (Pico 2)
//!
//! Provides control of the onboard LED (GPIO25) via a critical-section
//! mutex. Designed as a shared plug-and-play module identical across repos.

#![allow(dead_code)]

use core::cell::RefCell;
use critical_section::Mutex;
use embedded_hal::digital::OutputPin;
use rp235x_hal as hal;

/// Type alias for the GPIO25 LED pin configured as push-pull output.
pub type LedPin = hal::gpio::Pin<
    hal::gpio::bank0::Gpio25,
    hal::gpio::FunctionSio<hal::gpio::SioOutput>,
    hal::gpio::PullDown,
>;

/// Global LED pin behind a critical-section mutex for safe shared access.
static LED: Mutex<RefCell<Option<LedPin>>> = Mutex::new(RefCell::new(None));

/// Stores the LED pin in a global mutex for shared access.
///
/// Must be called exactly once during firmware initialization.
///
/// # Arguments
///
/// * `pin` - GPIO25 pin configured as push-pull output.
pub fn store_global(pin: LedPin) {
    critical_section::with(|cs| {
        LED.borrow(cs).replace(Some(pin));
    });
}

/// Sets the LED high (on).
///
/// # Panics
///
/// Panics if called before `store_global`.
pub fn set_high() {
    critical_section::with(|cs| {
        let cell = LED.borrow(cs);
        let mut pin = cell.borrow_mut();
        let _ = pin.as_mut().unwrap().set_high();
    });
}

/// Sets the LED low (off).
///
/// # Panics
///
/// Panics if called before `store_global`.
pub fn set_low() {
    critical_section::with(|cs| {
        let cell = LED.borrow(cs);
        let mut pin = cell.borrow_mut();
        let _ = pin.as_mut().unwrap().set_low();
    });
}
