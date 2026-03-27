//! Integration tests for the WASM blinky module.
//!
//! Validates that the compiled WASM binary loads correctly, exports the
//! expected `run` function, imports the correct host functions, and calls
//! them in the proper blink sequence with the correct delay values.

use wasmi::{Caller, Config, Engine, Linker, Module, Store};

/// Compiled WASM blinky module embedded at build time.
const WASM_BINARY: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/blinky.wasm"));

/// Represents a single host function call recorded during WASM execution.
#[derive(Debug, PartialEq)]
enum HostCall {
    /// The `gpio_set_high` host function was called.
    GpioSetHigh,
    /// The `gpio_set_low` host function was called.
    GpioSetLow,
    /// The `delay_ms` host function was called with the given value.
    DelayMs(u32),
}

/// Host state that records all function calls made by the WASM guest.
struct TestHostState {
    /// Ordered log of every host function call.
    calls: Vec<HostCall>,
}

/// Creates a wasmi engine with fuel metering enabled.
fn create_fuel_engine() -> Engine {
    let mut config = Config::default();
    config.consume_fuel(true);
    Engine::new(&config)
}

/// Creates a default wasmi engine without fuel metering.
fn create_default_engine() -> Engine {
    Engine::default()
}

/// Compiles the embedded WASM binary into a wasmi module.
///
/// # Panics
///
/// Panics if the WASM binary is invalid.
fn compile_module(engine: &Engine) -> Module {
    Module::new(engine, WASM_BINARY).expect("valid WASM module")
}

/// Registers the `gpio_set_high` test host function on the linker.
///
/// # Panics
///
/// Panics if registration fails.
fn register_gpio_set_high(linker: &mut Linker<TestHostState>) {
    linker
        .func_wrap(
            "env",
            "gpio_set_high",
            |mut caller: Caller<'_, TestHostState>| {
                caller.data_mut().calls.push(HostCall::GpioSetHigh);
            },
        )
        .expect("register gpio_set_high");
}

/// Registers the `gpio_set_low` test host function on the linker.
///
/// # Panics
///
/// Panics if registration fails.
fn register_gpio_set_low(linker: &mut Linker<TestHostState>) {
    linker
        .func_wrap(
            "env",
            "gpio_set_low",
            |mut caller: Caller<'_, TestHostState>| {
                caller.data_mut().calls.push(HostCall::GpioSetLow);
            },
        )
        .expect("register gpio_set_low");
}

/// Registers the `delay_ms` test host function on the linker.
///
/// # Panics
///
/// Panics if registration fails.
fn register_delay_ms(linker: &mut Linker<TestHostState>) {
    linker
        .func_wrap(
            "env",
            "delay_ms",
            |mut caller: Caller<'_, TestHostState>, ms: i32| {
                caller.data_mut().calls.push(HostCall::DelayMs(ms as u32));
            },
        )
        .expect("register delay_ms");
}

/// Builds a fully configured test linker with all host functions registered.
///
/// # Arguments
///
/// * `engine` - The wasmi engine to associate the linker with.
fn build_test_linker(engine: &Engine) -> Linker<TestHostState> {
    let mut linker = <Linker<TestHostState>>::new(engine);
    register_gpio_set_high(&mut linker);
    register_gpio_set_low(&mut linker);
    register_delay_ms(&mut linker);
    linker
}

/// Creates a store with an empty call log and the given fuel budget.
///
/// # Arguments
///
/// * `engine` - The wasmi engine to create the store for.
/// * `fuel` - The amount of fuel to allocate for execution.
fn create_fueled_store(engine: &Engine, fuel: u64) -> Store<TestHostState> {
    let mut store = Store::new(engine, TestHostState { calls: Vec::new() });
    store.set_fuel(fuel).expect("set fuel");
    store
}

/// Runs the WASM `run` function until fuel is exhausted, then returns the store.
///
/// # Arguments
///
/// * `store` - The wasmi store with fuel and host state.
/// * `linker` - The linker with host functions registered.
/// * `module` - The compiled WASM module.
fn run_until_out_of_fuel(
    store: &mut Store<TestHostState>,
    linker: &Linker<TestHostState>,
    module: &Module,
) {
    let instance = linker
        .instantiate_and_start(&mut *store, module)
        .expect("instantiate");
    let run = instance
        .get_typed_func::<(), ()>(&*store, "run")
        .expect("find run");
    let _ = run.call(&mut *store, ());
}

#[test]
fn test_wasm_module_loads() {
    let engine = create_default_engine();
    let _module = compile_module(&engine);
}

#[test]
fn test_wasm_exports_run_function() {
    let engine = create_default_engine();
    let module = compile_module(&engine);
    let linker = build_test_linker(&engine);
    let mut store = Store::new(&engine, TestHostState { calls: Vec::new() });
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .expect("instantiate");
    let run = instance.get_typed_func::<(), ()>(&store, "run");
    assert!(run.is_ok(), "module must export a `run` function");
}

#[test]
fn test_wasm_imports_match_expected() {
    let engine = create_default_engine();
    let module = compile_module(&engine);
    let imports: Vec<_> = module.imports().collect();
    let names: Vec<_> = imports.iter().map(|i| i.name()).collect();
    assert!(names.contains(&"gpio_set_high"), "missing gpio_set_high");
    assert!(names.contains(&"gpio_set_low"), "missing gpio_set_low");
    assert!(names.contains(&"delay_ms"), "missing delay_ms");
}

#[test]
fn test_all_imports_from_env_module() {
    let engine = create_default_engine();
    let module = compile_module(&engine);
    for import in module.imports() {
        assert_eq!(import.module(), "env", "all imports must be from env");
    }
}

#[test]
fn test_blink_sequence_order() {
    let engine = create_fuel_engine();
    let module = compile_module(&engine);
    let linker = build_test_linker(&engine);
    let mut store = create_fueled_store(&engine, 100_000);
    run_until_out_of_fuel(&mut store, &linker, &module);
    let calls = &store.data().calls;
    assert!(calls.len() >= 4, "need at least one full blink cycle");
    assert_eq!(calls[0], HostCall::GpioSetHigh);
    assert_eq!(calls[1], HostCall::DelayMs(500));
    assert_eq!(calls[2], HostCall::GpioSetLow);
    assert_eq!(calls[3], HostCall::DelayMs(500));
}

#[test]
fn test_blink_pattern_repeats() {
    let engine = create_fuel_engine();
    let module = compile_module(&engine);
    let linker = build_test_linker(&engine);
    let mut store = create_fueled_store(&engine, 500_000);
    run_until_out_of_fuel(&mut store, &linker, &module);
    let calls = &store.data().calls;
    assert!(calls.len() >= 8, "need at least two full blink cycles");
    for chunk in calls.chunks_exact(4) {
        assert_eq!(chunk[0], HostCall::GpioSetHigh);
        assert_eq!(chunk[1], HostCall::DelayMs(500));
        assert_eq!(chunk[2], HostCall::GpioSetLow);
        assert_eq!(chunk[3], HostCall::DelayMs(500));
    }
}

#[test]
fn test_delay_value_is_500ms() {
    let engine = create_fuel_engine();
    let module = compile_module(&engine);
    let linker = build_test_linker(&engine);
    let mut store = create_fueled_store(&engine, 100_000);
    run_until_out_of_fuel(&mut store, &linker, &module);
    let calls = &store.data().calls;
    for call in calls {
        if let HostCall::DelayMs(ms) = call {
            assert_eq!(*ms, 500, "delay must always be 500ms");
        }
    }
}

#[test]
fn test_no_unexpected_host_calls() {
    let engine = create_fuel_engine();
    let module = compile_module(&engine);
    let linker = build_test_linker(&engine);
    let mut store = create_fueled_store(&engine, 100_000);
    run_until_out_of_fuel(&mut store, &linker, &module);
    let calls = &store.data().calls;
    for call in calls {
        match call {
            HostCall::GpioSetHigh | HostCall::GpioSetLow | HostCall::DelayMs(_) => {}
        }
    }
}

#[test]
fn test_fuel_metering_halts_infinite_loop() {
    let engine = create_fuel_engine();
    let module = compile_module(&engine);
    let linker = build_test_linker(&engine);
    let mut store = create_fueled_store(&engine, 1_000);
    run_until_out_of_fuel(&mut store, &linker, &module);
    let remaining = store.get_fuel().expect("get fuel");
    assert!(
        remaining < 10,
        "fuel must be nearly exhausted, got {remaining}"
    );
}
