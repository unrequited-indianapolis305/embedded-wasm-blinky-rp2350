//! SPDX-License-Identifier: MIT
//!
//! Copyright (c) 2026 Kevin Thomas
//!
//! # Integration Tests for Wasm Blinky Component
//!
//! Validates that the compiled Wasm component loads correctly through the
//! Component Model, implements the expected WIT interfaces
//! (`embedded:platform/gpio` and `embedded:platform/timing`), exports the
//! `run` function, and calls host functions in the proper blink sequence
//! with the correct delay values and pin targeting.

use wasmtime::component::{Component, HasSelf};
use wasmtime::{Config, Engine, Store};

wasmtime::component::bindgen!({
    world: "blinky",
    path: "../wit",
});

/// Compiled Wasm blinky component embedded at build time.
const WASM_BINARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/blinky.wasm"));

/// Represents a single host function call recorded during Wasm execution.
#[derive(Debug, PartialEq)]
enum HostCall {
    /// The `gpio.set-high` WIT function was called with the given pin.
    GpioSetHigh(u32),
    /// The `gpio.set-low` WIT function was called with the given pin.
    GpioSetLow(u32),
    /// The `timing.delay-ms` WIT function was called with the given value.
    DelayMs(u32),
}

/// Host state that records all function calls made by the Wasm guest.
struct TestHostState {
    /// Ordered log of every host function call.
    calls: Vec<HostCall>,
}

impl embedded::platform::gpio::Host for TestHostState {
    /// Records a `set-high` call with the given pin number.
    ///
    /// # Arguments
    ///
    /// * `pin` - GPIO pin number passed by the Wasm guest.
    fn set_high(&mut self, pin: u32) {
        self.calls.push(HostCall::GpioSetHigh(pin));
    }

    /// Records a `set-low` call with the given pin number.
    ///
    /// # Arguments
    ///
    /// * `pin` - GPIO pin number passed by the Wasm guest.
    fn set_low(&mut self, pin: u32) {
        self.calls.push(HostCall::GpioSetLow(pin));
    }
}

impl embedded::platform::timing::Host for TestHostState {
    /// Records a `delay-ms` call with the given duration.
    ///
    /// # Arguments
    ///
    /// * `ms` - Delay duration in milliseconds passed by the Wasm guest.
    fn delay_ms(&mut self, ms: u32) {
        self.calls.push(HostCall::DelayMs(ms));
    }
}

/// Creates a wasmtime engine with fuel metering enabled.
///
/// # Returns
///
/// A wasmtime `Engine` with fuel consumption enabled.
///
/// # Panics
///
/// Panics if engine creation fails.
fn create_fuel_engine() -> Engine {
    let mut config = Config::default();
    config.consume_fuel(true);
    Engine::new(&config).expect("create fuel engine")
}

/// Creates a default wasmtime engine without fuel metering.
///
/// # Returns
///
/// A wasmtime `Engine` with default configuration.
fn create_default_engine() -> Engine {
    Engine::default()
}

/// Compiles the embedded Wasm binary into a wasmtime component.
///
/// # Arguments
///
/// * `engine` - The wasmtime engine to compile with.
///
/// # Returns
///
/// The compiled Wasm `Component`.
///
/// # Panics
///
/// Panics if the Wasm binary is invalid.
fn compile_component(engine: &Engine) -> Component {
    Component::new(engine, WASM_BINARY).expect("valid Wasm component")
}

/// Builds a fully configured test linker with all WIT interfaces registered.
///
/// # Arguments
///
/// * `engine` - The wasmtime engine to associate the linker with.
///
/// # Returns
///
/// A component `Linker` with `gpio::Host` and `timing::Host` registered.
///
/// # Panics
///
/// Panics if WIT interface registration fails.
fn build_test_linker(engine: &Engine) -> wasmtime::component::Linker<TestHostState> {
    let mut linker = wasmtime::component::Linker::new(engine);
    Blinky::add_to_linker::<TestHostState, HasSelf<TestHostState>>(
        &mut linker,
        |state: &mut TestHostState| state,
    )
    .expect("register WIT interfaces");
    linker
}

/// Creates a store with an empty call log and the given fuel budget.
///
/// # Arguments
///
/// * `engine` - The wasmtime engine to create the store for.
/// * `fuel` - The amount of fuel to allocate for execution.
///
/// # Returns
///
/// A `Store` containing an empty `TestHostState` with the fuel budget set.
///
/// # Panics
///
/// Panics if fuel allocation fails.
fn create_fueled_store(engine: &Engine, fuel: u64) -> Store<TestHostState> {
    let mut store = Store::new(engine, TestHostState { calls: Vec::new() });
    store.set_fuel(fuel).expect("set fuel");
    store
}

/// Runs the Wasm `run` function until fuel is exhausted.
///
/// # Arguments
///
/// * `store` - The wasmtime store with fuel and host state.
/// * `linker` - The component linker with WIT interfaces registered.
/// * `component` - The compiled Wasm component.
///
/// # Panics
///
/// Panics if component instantiation fails.
fn run_until_out_of_fuel(
    store: &mut Store<TestHostState>,
    linker: &wasmtime::component::Linker<TestHostState>,
    component: &Component,
) {
    let blinky =
        Blinky::instantiate(&mut *store, component, linker).expect("instantiate component");
    let _ = blinky.call_run(&mut *store);
}

/// Verifies that the Wasm component binary loads without error.
///
/// # Panics
///
/// Panics if the Wasm component binary fails to compile.
#[test]
fn test_wasm_component_loads() {
    let engine = create_default_engine();
    let _component = compile_component(&engine);
}

/// Verifies that the component instantiates and exports the `run` function.
///
/// # Panics
///
/// Panics if the component fails to instantiate.
#[test]
fn test_wasm_exports_run_function() {
    let engine = create_default_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let mut store = Store::new(&engine, TestHostState { calls: Vec::new() });
    let blinky = Blinky::instantiate(&mut store, &component, &linker);
    assert!(blinky.is_ok(), "component must instantiate with run export");
}

/// Verifies that the component imports both `gpio` and `timing` interfaces.
///
/// # Panics
///
/// Panics if a required interface import is missing.
#[test]
fn test_wasm_imports_match_expected() {
    let engine = create_default_engine();
    let component = compile_component(&engine);
    let ty = component.component_type();
    let import_names: Vec<_> = ty
        .imports(&engine)
        .map(|(name, _)| name.to_string())
        .collect();
    assert!(
        import_names.iter().any(|n| n.contains("gpio")),
        "missing gpio interface"
    );
    assert!(
        import_names.iter().any(|n| n.contains("timing")),
        "missing timing interface"
    );
}

/// Verifies that all imports originate from the `embedded:platform` package.
///
/// # Panics
///
/// Panics if any import is not from the `embedded:platform` package.
#[test]
fn test_all_imports_from_embedded_platform() {
    let engine = create_default_engine();
    let component = compile_component(&engine);
    let ty = component.component_type();
    for (name, _) in ty.imports(&engine) {
        assert!(
            name.starts_with("embedded:platform/"),
            "import '{name}' must be from embedded:platform"
        );
    }
}

/// Verifies that the component has exactly 2 interface imports.
///
/// # Panics
///
/// Panics if the import count is not exactly 2.
#[test]
fn test_import_count_is_exactly_two() {
    let engine = create_default_engine();
    let component = compile_component(&engine);
    let ty = component.component_type();
    let count = ty.imports(&engine).count();
    assert_eq!(
        count, 2,
        "component must have exactly 2 interface imports, got {count}"
    );
}

/// Verifies the first blink cycle follows the high-delay-low-delay pattern.
///
/// # Panics
///
/// Panics if the blink cycle does not match the expected sequence.
#[test]
fn test_blink_sequence_order() {
    let engine = create_fuel_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let mut store = create_fueled_store(&engine, 100_000);
    run_until_out_of_fuel(&mut store, &linker, &component);
    let calls = &store.data().calls;
    assert!(calls.len() >= 4, "need at least one full blink cycle");
    assert_eq!(calls[0], HostCall::GpioSetHigh(25));
    assert_eq!(calls[1], HostCall::DelayMs(500));
    assert_eq!(calls[2], HostCall::GpioSetLow(25));
    assert_eq!(calls[3], HostCall::DelayMs(500));
}

/// Verifies that the blink pattern repeats consistently across cycles.
///
/// # Panics
///
/// Panics if any blink cycle deviates from the expected pattern.
#[test]
fn test_blink_pattern_repeats() {
    let engine = create_fuel_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let mut store = create_fueled_store(&engine, 500_000);
    run_until_out_of_fuel(&mut store, &linker, &component);
    let calls = &store.data().calls;
    assert!(calls.len() >= 8, "need at least two full blink cycles");
    for chunk in calls.chunks_exact(4) {
        assert_eq!(chunk[0], HostCall::GpioSetHigh(25));
        assert_eq!(chunk[1], HostCall::DelayMs(500));
        assert_eq!(chunk[2], HostCall::GpioSetLow(25));
        assert_eq!(chunk[3], HostCall::DelayMs(500));
    }
}

/// Verifies that all delay calls use the expected 500ms value.
///
/// # Panics
///
/// Panics if any delay call does not use 500ms.
#[test]
fn test_delay_value_is_500ms() {
    let engine = create_fuel_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let mut store = create_fueled_store(&engine, 100_000);
    run_until_out_of_fuel(&mut store, &linker, &component);
    let calls = &store.data().calls;
    for call in calls {
        if let HostCall::DelayMs(ms) = call {
            assert_eq!(*ms, 500, "delay must always be 500ms");
        }
    }
}

/// Verifies that no unknown host call variants are recorded.
///
/// # Panics
///
/// Panics if an unrecognized host call variant is encountered.
#[test]
fn test_no_unexpected_host_calls() {
    let engine = create_fuel_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let mut store = create_fueled_store(&engine, 100_000);
    run_until_out_of_fuel(&mut store, &linker, &component);
    let calls = &store.data().calls;
    for call in calls {
        match call {
            HostCall::GpioSetHigh(_) | HostCall::GpioSetLow(_) | HostCall::DelayMs(_) => {}
        }
    }
}

/// Verifies that fuel metering halts the infinite blink loop.
///
/// # Panics
///
/// Panics if fuel retrieval fails or fuel is not nearly exhausted.
#[test]
fn test_fuel_metering_halts_infinite_loop() {
    let engine = create_fuel_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let mut store = create_fueled_store(&engine, 1_000);
    run_until_out_of_fuel(&mut store, &linker, &component);
    let remaining = store.get_fuel().expect("get fuel");
    assert!(
        remaining < 10,
        "fuel must be nearly exhausted, got {remaining}"
    );
}

/// Verifies that all GPIO calls target pin 25 exclusively.
///
/// # Panics
///
/// Panics if any GPIO call targets a pin other than 25.
#[test]
fn test_gpio_pin_is_always_25() {
    let engine = create_fuel_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let mut store = create_fueled_store(&engine, 500_000);
    run_until_out_of_fuel(&mut store, &linker, &component);
    let calls = &store.data().calls;
    for call in calls {
        match call {
            HostCall::GpioSetHigh(pin) | HostCall::GpioSetLow(pin) => {
                assert_eq!(*pin, 25, "GPIO pin must always be 25");
            }
            _ => {}
        }
    }
}

/// Verifies that `set_high` and `set_low` are called an equal number of times.
///
/// # Panics
///
/// Panics if the high and low call counts are not equal.
#[test]
fn test_equal_high_low_calls() {
    let engine = create_fuel_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let mut store = create_fueled_store(&engine, 500_000);
    run_until_out_of_fuel(&mut store, &linker, &component);
    let calls = &store.data().calls;
    let highs = calls
        .iter()
        .filter(|c| matches!(c, HostCall::GpioSetHigh(_)))
        .count();
    let lows = calls
        .iter()
        .filter(|c| matches!(c, HostCall::GpioSetLow(_)))
        .count();
    assert_eq!(highs, lows, "set_high and set_low must be called equally");
}

/// Verifies that the Wasm component binary is under 16 KB.
///
/// # Panics
///
/// Panics if the component binary is 16 KB or larger.
#[test]
fn test_wasm_component_size_under_16kb() {
    assert!(
        WASM_BINARY.len() < 16_384,
        "Wasm component must be under 16 KB, got {} bytes",
        WASM_BINARY.len()
    );
}

/// Verifies that the component has exactly 1 export (`run`).
///
/// # Panics
///
/// Panics if the export count is not exactly 1.
#[test]
fn test_component_exports_exactly_one() {
    let engine = create_default_engine();
    let component = compile_component(&engine);
    let ty = component.component_type();
    let count = ty.exports(&engine).count();
    assert_eq!(
        count, 1,
        "component must have exactly 1 export (run), got {count}"
    );
}

/// Verifies that the `embedded:platform/gpio` import is present.
///
/// # Panics
///
/// Panics if the `embedded:platform/gpio` import is missing.
#[test]
fn test_gpio_import_name_is_correct() {
    let engine = create_default_engine();
    let component = compile_component(&engine);
    let ty = component.component_type();
    let import_names: Vec<_> = ty
        .imports(&engine)
        .map(|(name, _)| name.to_string())
        .collect();
    assert!(
        import_names.iter().any(|n| n == "embedded:platform/gpio"),
        "missing embedded:platform/gpio import, got {import_names:?}"
    );
}

/// Verifies that the `embedded:platform/timing` import is present.
///
/// # Panics
///
/// Panics if the `embedded:platform/timing` import is missing.
#[test]
fn test_timing_import_name_is_correct() {
    let engine = create_default_engine();
    let component = compile_component(&engine);
    let ty = component.component_type();
    let import_names: Vec<_> = ty
        .imports(&engine)
        .map(|(name, _)| name.to_string())
        .collect();
    assert!(
        import_names.iter().any(|n| n == "embedded:platform/timing"),
        "missing embedded:platform/timing import, got {import_names:?}"
    );
}

/// Verifies that the first host call is always `set_high(25)`.
///
/// # Panics
///
/// Panics if the first call is not `set_high(25)`.
#[test]
fn test_first_call_is_always_set_high() {
    let engine = create_fuel_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let mut store = create_fueled_store(&engine, 100_000);
    run_until_out_of_fuel(&mut store, &linker, &component);
    let calls = &store.data().calls;
    assert!(!calls.is_empty(), "must have at least one call");
    assert_eq!(
        calls[0],
        HostCall::GpioSetHigh(25),
        "first call must be set_high(25)"
    );
}

/// Verifies that every GPIO call is paired with a corresponding delay call.
///
/// # Panics
///
/// Panics if GPIO and delay call counts are not equal.
#[test]
fn test_delay_count_equals_gpio_count() {
    let engine = create_fuel_engine();
    let component = compile_component(&engine);
    let linker = build_test_linker(&engine);
    let mut store = create_fueled_store(&engine, 500_000);
    run_until_out_of_fuel(&mut store, &linker, &component);
    let calls = &store.data().calls;
    let gpio_count = calls
        .iter()
        .filter(|c| matches!(c, HostCall::GpioSetHigh(_) | HostCall::GpioSetLow(_)))
        .count();
    let delay_count = calls
        .iter()
        .filter(|c| matches!(c, HostCall::DelayMs(_)))
        .count();
    assert_eq!(
        gpio_count, delay_count,
        "each GPIO call must be followed by a delay"
    );
}

/// Verifies that instantiation fails when WIT imports are not registered.
///
/// # Panics
///
/// Panics if instantiation succeeds without WIT imports.
#[test]
fn test_instantiate_with_missing_imports_fails() {
    let engine = create_default_engine();
    let component = compile_component(&engine);
    let linker = wasmtime::component::Linker::<TestHostState>::new(&engine);
    let mut store = Store::new(&engine, TestHostState { calls: Vec::new() });
    let result = Blinky::instantiate(&mut store, &component, &linker);
    assert!(
        result.is_err(),
        "instantiation must fail without WIT imports registered"
    );
}
