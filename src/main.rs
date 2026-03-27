//! SPDX-License-Identifier: MIT
//!
//! Copyright (c) 2026 Kevin Thomas
//!
//! # WASM Blinky Firmware for RP2350 (Pico 2)
//!
//! This firmware runs a WebAssembly runtime on the RP2350 bare-metal using
//! wasmtime with the Pulley interpreter. A precompiled WASM module controls
//! GPIO25 (onboard LED) to blink by calling host-provided GPIO and delay
//! functions through the wasmtime runtime.

#![no_std]
#![no_main]

extern crate alloc;

mod led;
mod platform;
mod uart;

use alloc::boxed::Box;
use core::panic::PanicInfo;
use embedded_alloc::LlffHeap as Heap;
use rp235x_hal as hal;
use wasmtime::{Caller, Config, Engine, Linker, Module, Store};

/// Global heap allocator backed by a statically allocated memory region.
///
/// Uses the linked-list first-fit allocation strategy from `embedded-alloc`.
#[global_allocator]
static HEAP: Heap = Heap::empty();

/// External crystal oscillator frequency in Hz.
const XOSC_CRYSTAL_FREQ: u32 = 12_000_000;

/// Heap size in bytes (256 KiB of the available 512 KiB RAM).
const HEAP_SIZE: usize = 262_144;

/// Precompiled Pulley bytecode for the WASM blinky module, embedded at build time.
const WASM_BINARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/blinky.cwasm"));

/// RP2350 boot metadata placed in the `.start_block` section for the Boot ROM.
#[unsafe(link_section = ".start_block")]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

/// Host state shared with WASM guest functions via the wasmtime store.
///
/// Uses boxed closures to abstract over concrete HAL types, keeping the WASM
/// runtime decoupled from hardware-specific pin and timer types.
struct HostState {
    /// Closure to control the LED: `true` sets high, `false` sets low.
    set_led: Box<dyn FnMut(bool)>,
    /// Closure to delay execution for the given number of milliseconds.
    delay_ms: Box<dyn FnMut(u32)>,
}

/// Panic handler that outputs a diagnostic message over UART0.
///
/// Initializes UART0 from scratch (in case it was never set up) and
/// writes the panic location and message to UART0, then halts.
///
/// # Arguments
///
/// * `info` - Panic information containing the location and message.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    uart::panic_init();
    uart::panic_write(b"\n!!! PANIC !!!\n");
    if let Some(location) = info.location() {
        uart::panic_write(b"Location: ");
        uart::panic_write(location.file().as_bytes());
        uart::panic_write(b"\n");
    }
    if let Some(msg) = info.message().as_str() {
        uart::panic_write(b"Message: ");
        uart::panic_write(msg.as_bytes());
        uart::panic_write(b"\n");
    }
    loop {
        cortex_m::asm::wfe();
    }
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
/// # Returns
///
/// The configured clocks manager for peripheral clock access.
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
/// The LED closure writes "GPIO25 On\n" or "GPIO25 Off\n" to UART0
/// each time the LED state changes. Delay is implemented via CPU cycle
/// counting (`cortex_m::asm::delay`) at approximately 150 MHz.
///
/// # Returns
///
/// A `HostState` containing the LED control and delay closures.
fn build_host_state() -> HostState {
    let set_led = Box::new(move |high: bool| {
        if high {
            led::set_high();
            uart::write_msg(b"GPIO25 On\n");
        } else {
            led::set_low();
            uart::write_msg(b"GPIO25 Off\n");
        }
    });
    let delay_ms = Box::new(move |ms: u32| {
        cortex_m::asm::delay(ms * 150_000);
    });
    HostState { set_led, delay_ms }
}

/// Initializes all RP2350 hardware and returns a configured host state.
///
/// Sets up the watchdog, clocks, SIO, GPIO pins, UART0, and timer
/// peripherals. GPIO25 is configured as a push-pull output for the
/// onboard LED. UART0 is configured at 115200 baud on GPIO0 (TX) and
/// GPIO1 (RX) for diagnostic output.
///
/// # Returns
///
/// A `HostState` containing hardware-bound closures for the WASM runtime.
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
    let (uart_dev, led_pin) = uart::init(pac.UART0, &mut pac.RESETS, &clocks, pins);
    uart::store_global(uart_dev);
    led::store_global(led_pin);
    build_host_state()
}

/// Creates a wasmtime engine configured for Pulley on bare-metal.
///
/// Explicitly targets `pulley32` to match the AOT cross-compilation in
/// `build.rs`. All settings must be identical between build-time and
/// runtime engines or `Module::deserialize` will fail. OS-dependent
/// features are disabled and memory limits are tuned for the RP2350's
/// 512 KiB RAM.
///
/// # Returns
///
/// A configured wasmtime `Engine` targeting the Pulley 32-bit interpreter.
///
/// # Panics
///
/// Panics if the engine configuration fails.
fn create_engine() -> Engine {
    let mut config = Config::new();
    config.target("pulley32").expect("set pulley32 target");
    config.signals_based_traps(false);
    config.memory_init_cow(false);
    config.memory_reservation(0);
    config.memory_guard_size(0);
    config.memory_reservation_for_growth(0);
    config.guard_before_linear_memory(false);
    config.max_wasm_stack(16384);
    Engine::new(&config).expect("create Pulley engine")
}

/// Deserializes the precompiled Pulley module from embedded bytes.
///
/// # Safety
///
/// Uses `unsafe` to call `Module::deserialize` which requires that the
/// embedded bytes are a valid serialized wasmtime module. This invariant
/// is upheld because the bytes are produced by our build script.
///
/// # Arguments
///
/// * `engine` - Engine with matching Pulley configuration.
///
/// # Panics
///
/// Panics if the embedded Pulley bytecode is invalid.
fn create_module(engine: &Engine) -> Module {
    unsafe { Module::deserialize(engine, WASM_BINARY) }.expect("valid Pulley module")
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
/// # Returns
///
/// A configured `Linker` with GPIO and delay host functions registered.
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
/// * `module` - Precompiled WASM module to instantiate.
///
/// # Panics
///
/// Panics if instantiation fails or the `run` export is not found.
fn execute_wasm(store: &mut Store<HostState>, linker: &Linker<HostState>, module: &Module) {
    let instance = linker
        .instantiate(&mut *store, module)
        .expect("instantiate WASM module");
    let run = instance
        .get_typed_func::<(), ()>(&mut *store, "run")
        .expect("find run function");
    run.call(&mut *store, ()).expect("execute WASM run");
}

/// Loads and runs the WASM blinky module with the provided host state.
///
/// # Arguments
///
/// * `host_state` - Initialized hardware state with LED and timer closures.
fn run_wasm(host_state: HostState) -> ! {
    let engine = create_engine();
    let module = create_module(&engine);
    let mut store = Store::new(&engine, host_state);
    let linker = build_linker(&engine);
    execute_wasm(&mut store, &linker, &module);
    loop {
        cortex_m::asm::wfe();
    }
}

/// Firmware entry point that initializes hardware and runs the WASM blinky.
#[hal::entry]
fn main() -> ! {
    init_heap();
    let host_state = init_hardware();
    run_wasm(host_state)
}
