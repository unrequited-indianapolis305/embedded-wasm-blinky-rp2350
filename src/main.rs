//! SPDX-License-Identifier: MIT
//!
//! Copyright (c) 2026 Kevin Thomas
//!
//! # WASM Blinky Firmware for RP2350 (Pico 2)
//!
//! This firmware runs a WebAssembly interpreter on the RP2350 bare-metal.
//! A compiled WASM module controls GPIO25 (onboard LED) to blink by calling
//! host-provided GPIO and delay functions through the wasmi runtime.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use core::panic::PanicInfo;
use embedded_alloc::LlffHeap as Heap;
use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;
use rp235x_hal as hal;
use wasmi::{Caller, Engine, Linker, Module, Store};

/// Global heap allocator backed by a statically allocated memory region.
///
/// Uses the linked-list first-fit allocation strategy from `embedded-alloc`.
#[global_allocator]
static HEAP: Heap = Heap::empty();

/// External crystal oscillator frequency in Hz.
const XOSC_CRYSTAL_FREQ: u32 = 12_000_000;

/// Heap size in bytes (256 KiB of the available 512 KiB RAM).
const HEAP_SIZE: usize = 262_144;

/// Compiled WASM blinky module embedded at build time.
const WASM_BINARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/blinky.wasm"));

/// RP2350 boot metadata placed in the `.start_block` section for the Boot ROM.
#[unsafe(link_section = ".start_block")]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

/// Host state shared with WASM guest functions via the wasmi store.
///
/// Uses boxed closures to abstract over concrete HAL types, keeping the WASM
/// runtime decoupled from hardware-specific pin and timer types.
struct HostState {
    /// Closure to control the LED: `true` sets high, `false` sets low.
    set_led: Box<dyn FnMut(bool)>,
    /// Closure to delay execution for the given number of milliseconds.
    delay_ms: Box<dyn FnMut(u32)>,
}

/// Custom panic handler that halts execution in an infinite loop.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

/// Initializes the global heap allocator from a static memory region.
///
/// # Safety
///
/// Must be called exactly once before any heap allocations occur.
/// Uses `unsafe` to initialize the allocator with a raw pointer to static memory.
fn init_heap() {
    use core::mem::MaybeUninit;
    static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
    unsafe { HEAP.init(&raw mut HEAP_MEM as usize, HEAP_SIZE) }
}

/// Initializes system clocks and PLLs from the external crystal oscillator.
///
/// # Arguments
///
/// * `xosc` - External oscillator peripheral.
/// * `clocks` - Clocks peripheral.
/// * `pll_sys` - System PLL peripheral.
/// * `pll_usb` - USB PLL peripheral.
/// * `resets` - Resets peripheral for subsystem reset control.
/// * `watchdog` - Watchdog timer used as the clock reference.
///
/// # Panics
///
/// Panics if clock initialization fails.
fn init_clocks(
    xosc: hal::pac::XOSC,
    clocks: hal::pac::CLOCKS,
    pll_sys: hal::pac::PLL_SYS,
    pll_usb: hal::pac::PLL_USB,
    resets: &mut hal::pac::RESETS,
    watchdog: &mut hal::Watchdog,
) -> hal::clocks::ClocksManager {
    hal::clocks::init_clocks_and_plls(
        XOSC_CRYSTAL_FREQ,
        xosc,
        clocks,
        pll_sys,
        pll_usb,
        resets,
        watchdog,
    )
    .ok()
    .unwrap()
}

/// Wraps hardware peripherals into boxed closures for the WASM host state.
///
/// # Arguments
///
/// * `led_pin` - GPIO pin configured as a push-pull output for the LED.
/// * `timer` - Hardware timer implementing nanosecond-resolution delays.
fn build_host_state(
    mut led_pin: impl OutputPin + 'static,
    mut timer: impl DelayNs + 'static,
) -> HostState {
    let set_led = Box::new(move |high: bool| {
        if high {
            let _ = led_pin.set_high();
        } else {
            let _ = led_pin.set_low();
        }
    });
    let delay_ms = Box::new(move |ms: u32| timer.delay_ms(ms));
    HostState { set_led, delay_ms }
}

/// Initializes all RP2350 hardware and returns a configured host state.
///
/// Sets up the watchdog, clocks, SIO, GPIO pins, and timer peripherals.
/// GPIO25 is configured as a push-pull output for the onboard LED.
///
/// # Panics
///
/// Panics if the hardware peripherals have already been taken.
fn init_hardware() -> HostState {
    let mut pac = hal::pac::Peripherals::take().unwrap();
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);
    let clocks = init_clocks(
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    );
    let sio = hal::Sio::new(pac.SIO);
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );
    let timer = hal::Timer::new_timer0(pac.TIMER0, &mut pac.RESETS, &clocks);
    build_host_state(pins.gpio25.into_push_pull_output(), timer)
}

/// Creates a WASM engine and compiles the embedded WASM module.
///
/// # Panics
///
/// Panics if the embedded WASM binary is invalid.
fn create_engine_and_module() -> (Engine, Module) {
    let engine = Engine::default();
    let module = Module::new(&engine, WASM_BINARY).expect("valid WASM module");
    (engine, module)
}

/// Registers the `gpio_set_high` host function that turns the LED on.
///
/// # Arguments
///
/// * `linker` - WASM linker to register the function with.
///
/// # Panics
///
/// Panics if the function cannot be registered.
fn register_gpio_set_high(linker: &mut Linker<HostState>) {
    linker
        .func_wrap(
            "env",
            "gpio_set_high",
            |mut caller: Caller<'_, HostState>| {
                (caller.data_mut().set_led)(true);
            },
        )
        .expect("register gpio_set_high");
}

/// Registers the `gpio_set_low` host function that turns the LED off.
///
/// # Arguments
///
/// * `linker` - WASM linker to register the function with.
///
/// # Panics
///
/// Panics if the function cannot be registered.
fn register_gpio_set_low(linker: &mut Linker<HostState>) {
    linker
        .func_wrap(
            "env",
            "gpio_set_low",
            |mut caller: Caller<'_, HostState>| {
                (caller.data_mut().set_led)(false);
            },
        )
        .expect("register gpio_set_low");
}

/// Registers the `delay_ms` host function for millisecond delays.
///
/// # Arguments
///
/// * `linker` - WASM linker to register the function with.
///
/// # Panics
///
/// Panics if the function cannot be registered.
fn register_delay_ms(linker: &mut Linker<HostState>) {
    linker
        .func_wrap(
            "env",
            "delay_ms",
            |mut caller: Caller<'_, HostState>, ms: i32| {
                (caller.data_mut().delay_ms)(ms as u32);
            },
        )
        .expect("register delay_ms");
}

/// Builds the WASM linker with all host function bindings registered.
///
/// # Arguments
///
/// * `engine` - WASM engine that the linker is associated with.
///
/// # Panics
///
/// Panics if any host function fails to register.
fn build_linker(engine: &Engine) -> Linker<HostState> {
    let mut linker = <Linker<HostState>>::new(engine);
    register_gpio_set_high(&mut linker);
    register_gpio_set_low(&mut linker);
    register_delay_ms(&mut linker);
    linker
}

/// Instantiates the WASM module and executes the exported `run` function.
///
/// # Arguments
///
/// * `store` - WASM store holding the host state.
/// * `linker` - WASM linker with host functions registered.
/// * `module` - Compiled WASM module to instantiate.
///
/// # Panics
///
/// Panics if instantiation fails or the `run` export is not found.
fn execute_wasm(store: &mut Store<HostState>, linker: &Linker<HostState>, module: &Module) {
    let instance = linker
        .instantiate_and_start(&mut *store, module)
        .expect("instantiate WASM module");
    let run = instance
        .get_typed_func::<(), ()>(&*store, "run")
        .expect("find run function");
    run.call(&mut *store, ()).expect("execute WASM run");
}

/// Loads and runs the WASM blinky module with the provided host state.
///
/// # Arguments
///
/// * `host_state` - Initialized hardware state with LED and timer closures.
fn run_wasm(host_state: HostState) -> ! {
    let (engine, module) = create_engine_and_module();
    let mut store = Store::new(&engine, host_state);
    let linker = build_linker(&engine);
    execute_wasm(&mut store, &linker, &module);
    loop {}
}

/// Firmware entry point that initializes hardware and runs the WASM blinky.
#[hal::entry]
fn main() -> ! {
    init_heap();
    let host_state = init_hardware();
    run_wasm(host_state)
}
