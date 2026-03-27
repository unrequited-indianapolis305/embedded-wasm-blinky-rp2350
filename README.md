# Embedded WASM Blinky
## WebAssembly Blinky on RP2350 Pico 2

A pure Embedded Rust project that runs a **WebAssembly runtime** (wasmtime + Pulley interpreter) directly on the RP2350 (Raspberry Pi Pico 2) bare-metal. A WASM module is AOT-compiled to Pulley bytecode on the host and executed on the device to control the onboard LED — no operating system and no standard library.

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Project Structure](#project-structure)
- [Source Files](#source-files)
- [Prerequisites](#prerequisites)
- [Building](#building)
- [Flashing](#flashing)
- [Testing](#testing)
- [How It Works](#how-it-works)
- [Host Function Interface](#host-function-interface)
- [Memory Layout](#memory-layout)
- [Extending the Project](#extending-the-project)
- [Troubleshooting](#troubleshooting)
- [License](#license)

## Overview

This project demonstrates that WebAssembly is not just for browsers — it can run on a microcontroller with 512 KB of RAM. The firmware uses [wasmtime](https://github.com/bytecodealliance/wasmtime) with the **Pulley interpreter** (a portable, `no_std`-compatible WebAssembly runtime) and executes a precompiled WASM module that blinks GPIO25 at 500ms intervals.

**Key properties:**

- **Pure Rust** — zero C code, zero C bindings, zero FFI
- **Minimal unsafe** — only unavoidable sites (heap init, boot metadata, module deserialize, panic handler UART)
- **Tiny WASM binary** — 191 bytes for the blinky module
- **AOT compilation** — WASM is compiled to Pulley bytecode on the host, no compilation on device
- **Industry-standard runtime** — wasmtime is the reference WebAssembly implementation
- **UART diagnostics** — LED state changes are logged to UART0, panics output file/message over serial

## Architecture

```
┌─────────────────────────────────────────────────┐
│                 RP2350 (Pico 2)                 │
│                                                 │
│  ┌───────────────────────────────────────────┐  │
│  │            Firmware (src/main.rs)         │  │
│  │                                           │  │
│  │  ┌─────────┐  ┌────────┐  ┌───────────┐   │  │
│  │  │  Heap   │  │wasmtime│  │ Host Fns  │   │  │
│  │  │ 256 KiB │  │ Pulley │  │ LED/Delay │   │  │
│  │  └─────────┘  └───┬────┘  └─────┬─────┘   │  │
│  │                   │             │         │  │
│  │  ┌────────┐  ┌────┴─────────────┴──────┐  │  │
│  │  │ led.rs │  │ Pulley Bytecode(.cwasm) │  │  │
│  │  │uart.rs │  │                         │  │  │
│  │  └────────┘  │  imports:               │  │  │
│  │              │    env.gpio_set_high()  │  │  │
│  │              │    env.gpio_set_low()   │  │  │
│  │              │    env.delay_ms(u32)    │  │  │
│  │              │                         │  │  │
│  │              │  exports:               │  │  │
│  │              │    run()                │  │  │
│  │              └─────────────────────────┘  │  │
│  └───────────────────────────────────────────┘  │
│                                                 │
│  GPIO25 (Onboard LED) ◄── led::set_high/set_low │
│  GPIO0/1 (UART0) ◄── uart::write_msg (diag)     │
└─────────────────────────────────────────────────┘
```

## Project Structure

```
embedded-wasm-blinky/
├── .cargo/
│   └── config.toml           # ARM Cortex-M33 target, linker flags, picotool runner
├── .vscode/
│   ├── extensions.json       # Recommended VS Code extensions
│   └── settings.json         # Rust-analyzer target configuration
├── wasm-app/                 # WASM blinky module (compiled to .wasm)
│   ├── .cargo/
│   │   └── config.toml       # WASM linker flags (minimal memory)
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs            # Blinky logic: imports host GPIO/delay, exports run()
├── wasm-tests/               # Integration tests for the WASM module
│   ├── Cargo.toml
│   ├── build.rs              # Compiles WASM app before tests
│   └── tests/
│       └── integration.rs    # Tests: loading, imports, blink sequence, timing, fuel
├── src/
│   ├── main.rs               # Firmware: hardware init, wasmtime runtime, host functions
│   ├── led.rs                # GPIO25 LED driver (shared plug-and-play module)
│   ├── uart.rs               # UART0 driver (shared plug-and-play module)
│   └── platform.rs           # Platform TLS glue for wasmtime no_std
├── build.rs                  # Compiles WASM app, AOT-compiles to Pulley bytecode
├── Cargo.toml                # Firmware dependencies
├── rp2350.x                  # RP2350 memory layout linker script
├── SKILLS.md                 # Project conventions and lessons learned
└── README.md                 # This file
```

## Source Files

### `wasm-app/src/lib.rs` — WASM Guest Module

The WASM module compiled to `wasm32-unknown-unknown`. Declares host imports (`gpio_set_high`, `gpio_set_low`, `delay_ms`) and exports a `run()` function that blinks the LED in an infinite loop at 500ms intervals. Helper functions (`set_led_high`, `set_led_low`, `delay`) wrap the raw extern calls.

### `src/main.rs` — Firmware Entry Point

Orchestrates everything: initializes the heap (256 KiB), clocks, and hardware peripherals, then boots the wasmtime Pulley engine. Registers host functions that bridge WASM imports to the `led` and `uart` driver modules, deserializes the embedded `.cwasm` bytecode, and calls the WASM `run()` export. The panic handler uses `uart::panic_init()` and `uart::panic_write()` to output diagnostics over UART0 via raw register writes.

### `src/led.rs` — GPIO25 LED Driver (Shared Module)

Controls the onboard LED via a `critical_section::Mutex`. `init()` (in `uart.rs`) configures GPIO25 as push-pull output and returns the pin. `led::store_global()` stores it in a mutex. `led::set_high()` and `led::set_low()` toggle the LED. Marked `#![allow(dead_code)]` because this is a shared plug-and-play module — not every repo uses every function.

### `src/uart.rs` — UART0 Driver (Shared Module)

Provides both HAL-based and raw-register UART0 access. `uart::init()` configures UART0 at 115200 baud on GPIO0 (TX) / GPIO1 (RX) and returns the peripheral plus the GPIO25 pin. `uart::store_global()` stores the UART in a `critical_section::Mutex`. HAL functions: `write_msg()`, `read_byte()`, `write_byte()`. Panic functions (raw registers, no HAL): `panic_init()`, `panic_write()`. Marked `#![allow(dead_code)]` — shared module, identical across repos.

### `src/platform.rs` — wasmtime TLS Glue

Implements `wasmtime_tls_get()` and `wasmtime_tls_set()` using a global `AtomicPtr`. Required by wasmtime on `no_std` platforms. On this single-threaded MCU, TLS is just a single atomic pointer.

### `build.rs` — AOT Build Script

Copies the linker script (`rp2350.x` → `memory.x`), spawns a child `cargo build` to compile `wasm-app/` to `.wasm`, then AOT-compiles it to Pulley bytecode via Cranelift. Strips `CARGO_ENCODED_RUSTFLAGS` from the child build to prevent ARM linker flags from leaking into the WASM compilation.

## Prerequisites

### Toolchain

```bash
# Rust (stable)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Required compilation targets
rustup target add thumbv8m.main-none-eabihf   # RP2350 ARM Cortex-M33
rustup target add wasm32-unknown-unknown      # WebAssembly
```

### Flashing Tool

```bash
# macOS
brew install picotool

# Linux (build from source)
# See https://github.com/raspberrypi/picotool
```

### Serial Terminal (for UART diagnostics)

```bash
# macOS
screen /dev/tty.usbserial* 115200

# Linux
minicom -D /dev/ttyACM0 -b 115200
```

### Optional (Debugging)

```bash
cargo install probe-rs-tools
```

## Building

```bash
cargo build --release
```

This single command does everything:

1. `build.rs` compiles `wasm-app/` to `wasm32-unknown-unknown` → produces `wasm_app.wasm` (191 bytes)
2. `build.rs` AOT-compiles the WASM binary to Pulley bytecode via Cranelift → produces `blinky.cwasm`
3. The firmware compiles for `thumbv8m.main-none-eabihf`, embedding the Pulley bytecode via `include_bytes!`
4. The result is an ELF at `target/thumbv8m.main-none-eabihf/release/t-wasm`

## Flashing

```bash
cargo run --release
```

This builds the firmware and flashes it to the Pico 2 via `picotool` (configured as the cargo runner in `.cargo/config.toml`).

> **Note:** Hold the **BOOTSEL** button on the Pico 2 while plugging in the USB cable to enter bootloader mode. Release once connected.

After flashing, the LED on GPIO25 will begin blinking at 500ms intervals. If a USB-to-serial adapter is connected to GPIO0/GPIO1, you will see `GPIO25 On` and `GPIO25 Off` messages at 115200 baud.

## Testing

```bash
cd wasm-tests && cargo test
```

Runs all 9 integration tests validating module loading, import/export contracts, blink sequencing, timing, and fuel-based execution limits.

## How It Works

### 1. The WASM Application (`wasm-app/src/lib.rs`)

The WASM module is a `#![no_std]` Rust library compiled to `wasm32-unknown-unknown`. It declares three host imports and one export:

```rust
unsafe extern "C" {
    safe fn gpio_set_high();
    safe fn gpio_set_low();
    safe fn delay_ms(ms: u32);
}

#[unsafe(no_mangle)]
pub fn run() {
    loop {
        set_led_high();    // → calls gpio_set_high()
        delay(500);        // → calls delay_ms(500)
        set_led_low();     // → calls gpio_set_low()
        delay(500);        // → calls delay_ms(500)
    }
}
```

The `safe fn` declarations inside `unsafe extern` mean calling these functions requires no `unsafe` block — the safety invariant is upheld by the firmware.

The compiled WASM binary is only **191 bytes** because:
- No standard library (`#![no_std]`)
- Stack limited to 4 KB via linker flags
- Linear memory limited to 1 page (64 KB)
- LTO + size optimization (`opt-level = "s"`)

### 2. The Firmware Runtime (`src/main.rs`)

The firmware boots in this sequence:

1. **`init_heap()`** — 256 KiB heap for wasmtime via `embedded-alloc`.
2. **`init_hardware()`** — Clocks, SIO, GPIO, UART0, LED:
   - `uart::init()` → configures UART0 at 115200 baud, returns UART + LED pin
   - `uart::store_global()` → stores UART in mutex
   - `led::store_global()` → stores LED pin in mutex
   - `build_host_state()` → wraps `led::set_high/low` + `uart::write_msg` + `cortex_m::asm::delay` into boxed closures
3. **`run_wasm(host_state)`** — Boots the WASM runtime:
   ```
   create_engine()    → Config::target("pulley32"), bare-metal settings
   create_module()    → Module::deserialize(embedded .cwasm bytes)
   Store::new()       → Holds HostState with LED/delay closures
   build_linker()     → Registers env.gpio_set_high, env.gpio_set_low, env.delay_ms
   execute_wasm()     → linker.instantiate() → instance.get("run") → run.call()
   ```

### 3. The Call Chain

```
WASM run()
  → gpio_set_high()           [WASM import]
    → linker callback          [wasmtime dispatch]
      → (host_state.set_led)(true)  [boxed closure]
        → led::set_high()     [led.rs — HAL pin.set_high()]
        → uart::write_msg("GPIO25 On\n")  [uart.rs — serial output]
  → delay_ms(500)             [WASM import]
    → linker callback
      → (host_state.delay_ms)(500)
        → cortex_m::asm::delay(75_000_000)  [CPU cycle spin]
  → gpio_set_low()            [WASM import]
    → ... same pattern ...
```

### 4. The Build Pipeline (`build.rs`)

```
cargo build --release
       │
       ▼
   build.rs runs:
       │
       ├── 1. Copy rp2350.x → OUT_DIR/memory.x (linker script)
       │
       ├── 2. Spawn: cargo build --release --target wasm32-unknown-unknown
       │         └── wasm-app/ compiles → wasm_app.wasm (191 B)
       │
       ├── 3. AOT-compile to Pulley bytecode via Cranelift:
       │         └── engine.precompile_module(&wasm_bytes) → blinky.cwasm
       │
       └── 4. Main firmware compiles:
               └── include_bytes!("blinky.cwasm") embeds the Pulley bytecode
               └── Links against memory.x for RP2350 memory layout
```

Critical detail: `CARGO_ENCODED_RUSTFLAGS` (ARM flags like `--nmagic`, `-Tlink.x`) must be stripped from the child WASM build via `.env_remove("CARGO_ENCODED_RUSTFLAGS")`.

### 5. Creating a New Project from This Template

1. Copy the repo and rename it.
2. Drop in `uart.rs` and `platform.rs` unchanged — they are plug-and-play.
3. Drop in `led.rs` if your project uses GPIO25.
4. Edit `wasm-app/src/lib.rs`:
   - Add your host imports as `safe fn` inside the existing `unsafe extern "C"` block
     (the `unsafe extern` wrapper is a Rust 2024 language requirement, not optional)
   - Write your logic in `pub fn run() { ... }`
5. Edit `src/main.rs`:
   - Define `HostState` with closures matching your WASM imports
   - Register each import with `linker.func_wrap("env", "name", ...)`
   - Call `uart::init()`, `uart::store_global()`, etc. in `init_hardware()`
6. `build.rs` and `Cargo.toml` need no changes unless you rename the `.cwasm` output.
7. `cargo build --release` → `cargo run --release` to flash.

## Host Function Interface

| Import Name         | Signature    | Description                                                  |
| ------------------- | ------------ | ------------------------------------------------------------ |
| `env.gpio_set_high` | `() → ()`    | Sets GPIO25 high (on) and logs "GPIO25 On" to UART0          |
| `env.gpio_set_low`  | `() → ()`    | Sets GPIO25 low (off) and logs "GPIO25 Off" to UART0         |
| `env.delay_ms`      | `(i32) → ()` | Blocks execution for N milliseconds (via CPU cycle counting) |

## Memory Layout

| Region             | Address      | Size            | Usage                                              |
| ------------------ | ------------ | --------------- | -------------------------------------------------- |
| Flash              | `0x10000000` | 2 MiB           | Firmware code + embedded WASM binary               |
| RAM (striped)      | `0x20000000` | 512 KiB         | Stack + heap + data                                |
| Heap (allocated)   | —            | 256 KiB         | wasmtime engine, store, module, WASM linear memory |
| WASM linear memory | —            | 64 KiB (1 page) | WASM module's addressable memory                   |
| WASM stack         | —            | 4 KiB           | WASM call stack                                    |

> **Important:** The default WASM linker allocates 1 MB of linear memory (16 pages). This exceeds the RP2350's total RAM. The `wasm-app/.cargo/config.toml` explicitly sets `--initial-memory=65536` (1 page) and `stack-size=4096`.

## Extending the Project

### Adding New Host Functions

1. Add the import declaration in `wasm-app/src/lib.rs` (Rust 2024 requires the
   `unsafe extern` block, but individual functions are declared `safe fn` so
   callers need no `unsafe`):
   ```rust
   // Inside the existing unsafe extern "C" block:
   safe fn my_new_function(arg: u32);
   ```

2. Register the host function in `src/main.rs`:
   ```rust
   linker.func_wrap("env", "my_new_function", |mut caller: Caller<'_, HostState>, arg: i32| {
       // Your implementation here
   }).expect("register my_new_function");
   ```

3. Add the corresponding field/closure to `HostState` if hardware access is needed.

### Changing Blink Speed

Edit the delay values in `wasm-app/src/lib.rs`:

```rust
pub fn run() {
    loop {
        set_led_high();
        delay(100);     // 100ms on
        set_led_low();
        delay(900);     // 900ms off
    }
}
```

Rebuild and reflash — only the 191-byte WASM binary changes.

## Troubleshooting

| Symptom                                         | Cause                                  | Fix                                                                              |
| ----------------------------------------------- | -------------------------------------- | -------------------------------------------------------------------------------- |
| LED not blinking after flash                    | WASM linear memory too large for heap  | Ensure `wasm-app/.cargo/config.toml` has `--initial-memory=65536`                |
| No UART output                                  | Wiring or baud rate wrong              | GPIO0→adapter RX, GPIO1→adapter TX, 115200 8N1                                   |
| `Module::deserialize` panics                    | Config mismatch build vs device        | Both engines must have identical `Config` settings                               |
| `Module::deserialize` panics                    | `default-features` mismatch            | Both `[dependencies]` and `[build-dependencies]` need `default-features = false` |
| Build fails with `unknown argument: --nmagic`   | Parent rustflags leaking to WASM build | Ensure `build.rs` has `.env_remove("CARGO_ENCODED_RUSTFLAGS")`                   |
| Build fails with `extern blocks must be unsafe` | Rust 2024 edition                      | Use `unsafe extern { ... }` with `safe fn` declarations                          |
| `picotool` can't find device                    | Not in bootloader mode                 | Hold BOOTSEL while plugging in USB                                               |
| `cargo build` doesn't pick up WASM changes      | Cached build artifacts                 | Run `cargo clean && cargo build --release`                                       |

## License

- [MIT License](LICENSE)
