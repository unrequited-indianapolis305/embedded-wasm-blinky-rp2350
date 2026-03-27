# Embedded WASM Blinky — WebAssembly Blinky on RP2350 Pico 2

A pure Embedded Rust project that runs a **WebAssembly interpreter** directly on the RP2350 (Raspberry Pi Pico 2) bare-metal. A compiled WASM module controls the onboard LED — no operating system and no standard library.

## Table of Contents

- [Embedded WASM Blinky — WebAssembly Blinky on RP2350 Pico 2](#embedded-wasm-blinky--webassembly-blinky-on-rp2350-pico-2)
  - [Table of Contents](#table-of-contents)
  - [Overview](#overview)
  - [Architecture](#architecture)
  - [Project Structure](#project-structure)
  - [Prerequisites](#prerequisites)
    - [Toolchain](#toolchain)
    - [Flashing Tool](#flashing-tool)
    - [Optional (Debugging)](#optional-debugging)
  - [Building](#building)
  - [Flashing](#flashing)
    - [Option 1: Script](#option-1-script)
    - [Option 2: Manual](#option-2-manual)
  - [How It Works](#how-it-works)
    - [1. The WASM Application](#1-the-wasm-application)
    - [2. The Firmware Runtime](#2-the-firmware-runtime)
    - [3. The Build Pipeline](#3-the-build-pipeline)
  - [Host Function Interface](#host-function-interface)
  - [Memory Layout](#memory-layout)
  - [Extending the Project](#extending-the-project)
    - [Adding New Host Functions](#adding-new-host-functions)
    - [Changing Blink Speed](#changing-blink-speed)
  - [Troubleshooting](#troubleshooting)
  - [License](#license)

## Overview

This project demonstrates that WebAssembly is not just for browsers — it can run on a microcontroller with 512 KB of RAM. The firmware embeds the [wasmi](https://github.com/wasmi-labs/wasmi) interpreter (a pure Rust, `no_std`-compatible WASM runtime) and executes a 191-byte WASM module that blinks GPIO25 at 500ms intervals.

**Key properties:**

- **Pure Rust** — zero C code, zero C bindings, zero FFI
- **Minimal unsafe** — only two unavoidable sites (heap init, boot metadata)
- **Tiny WASM binary** — 191 bytes for the blinky module
- **Audited runtime** — wasmi has been security-audited twice (SRLabs, Runtime Verification Inc.)

## Architecture

```
┌─────────────────────────────────────────────────┐
│                 RP2350 (Pico 2)                 │
│                                                 │
│  ┌───────────────────────────────────────────┐  │
│  │            Firmware (src/main.rs)         │  │
│  │                                           │  │
│  │  ┌─────────┐  ┌────────┐  ┌───────────┐   │  │
│  │  │  Heap   │  │ wasmi  │  │ Host Fns  │   │  │
│  │  │ 256 KiB │  │ Engine │  │ GPIO/Timer│   │  │
│  │  └─────────┘  └───┬────┘  └─────┬─────┘   │  │
│  │                    │             │        │  │
│  │              ┌─────┴─────────────┴─────┐  │  │
│  │              │   WASM Module (191 B)   │  │  │
│  │              │                         │  │  │
│  │              │  imports:               │  │  │
│  │              │    env.gpio_set_high()  │  │  │
│  │              │    env.gpio_set_low()   │  │  │
│  │              │    env.delay_ms(u32)    │  │  │
│  │              │                         │  │  │
│  │              │  exports:               │  │  │
│  │              │    run()                │  │  │
│  │              └─────────────────────────┘  │  │
│  └───────────────────────────────────────────┘  │
│                                                 │
│  GPIO25 (Onboard LED) ◄── set_high / set_low    │
└─────────────────────────────────────────────────┘
```

## Project Structure

```
t-wasm/
├── .cargo/
│   └── config.toml          # ARM Cortex-M33 target, linker flags, picotool runner
├── .vscode/
│   ├── extensions.json      # Recommended VS Code extensions
│   └── settings.json        # Rust-analyzer target configuration
├── wasm-app/                # WASM blinky module (compiled to .wasm)
│   ├── .cargo/
│   │   └── config.toml      # WASM linker flags (minimal memory)
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs            # Blinky logic: imports host GPIO/delay, exports run()
├── src/
│   └── main.rs               # Firmware: hardware init, wasmi runtime, host functions
├── build.rs                   # Compiles WASM app, sets up linker scripts
├── Cargo.toml                 # Firmware dependencies
├── flash.sh                   # One-command build + flash script
├── rp2350.x                   # RP2350 memory layout linker script
├── SKILLS.md                   # Project conventions and lessons learned
└── README.md                   # This file
```

## Prerequisites

### Toolchain

```bash
# Rust (stable)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Required compilation targets
rustup target add thumbv8m.main-none-eabihf   # RP2350 ARM Cortex-M33
rustup target add wasm32-unknown-unknown        # WebAssembly
```

### Flashing Tool

```bash
# macOS
brew install picotool

# Linux (build from source)
# See https://github.com/raspberrypi/picotool
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
2. The WASM binary is copied into the build output directory
3. The firmware compiles for `thumbv8m.main-none-eabihf`, embedding the WASM binary via `include_bytes!`
4. The result is an ELF at `target/thumbv8m.main-none-eabihf/release/t-wasm`

## Flashing

### Option 1: Script

```bash
./flash.sh
```

### Option 2: Manual

```bash
cargo build --release
picotool load -u -v -x -t elf target/thumbv8m.main-none-eabihf/release/t-wasm
```

> **Note:** Hold the **BOOTSEL** button on the Pico 2 while plugging in the USB cable to enter bootloader mode. Release once connected.

After flashing, the LED on GPIO25 will begin blinking at 500ms intervals.

## How It Works

### 1. The WASM Application

**File:** `wasm-app/src/lib.rs`

The WASM module is a `#![no_std]` Rust library compiled to `wasm32-unknown-unknown`. It declares three host imports and one export:

```rust
// Host-imported functions — these are provided by the firmware at runtime.
unsafe extern {
    safe fn gpio_set_high();
    safe fn gpio_set_low();
    safe fn delay_ms(ms: u32);
}

// Exported entry point called by the firmware.
#[unsafe(no_mangle)]
pub fn run() {
    loop {
        set_led_high();
        delay(500);
        set_led_low();
        delay(500);
    }
}
```

The `safe fn` declarations inside `unsafe extern` mean that calling these functions from Rust requires no `unsafe` block — the safety invariant is upheld by the firmware implementation.

The compiled WASM binary is only **191 bytes** because:
- No standard library (`#![no_std]`)
- Stack limited to 4 KB via linker flags
- Linear memory limited to 1 page (64 KB)
- LTO + size optimization (`opt-level = "s"`)

### 2. The Firmware Runtime

**File:** `src/main.rs`

The firmware performs these steps at boot:

1. **Initialize heap** — 256 KiB of the RP2350's 512 KiB RAM is allocated as a heap for the wasmi runtime using `embedded-alloc`'s linked-list first-fit allocator.

2. **Initialize hardware** — Configures the external 12 MHz crystal oscillator, system clocks/PLLs, watchdog, SIO, GPIO25 (push-pull output), and Timer0.

3. **Create host state** — Wraps the LED pin and timer in boxed closures (`Box<dyn FnMut>`) so the WASM runtime doesn't need to know concrete HAL types.

4. **Boot the WASM runtime:**
   ```
   Engine::default()        → Create the wasmi interpreter engine
   Module::new(wasm_bytes)  → Parse and compile the embedded WASM binary
   Store::new(host_state)   → Create a store holding our GPIO/timer closures
   Linker::new()            → Register host functions:
                                env.gpio_set_high → (set_led)(true)
                                env.gpio_set_low  → (set_led)(false)
                                env.delay_ms      → timer.delay_ms(ms)
   linker.instantiate()     → Link imports, create WASM instance
   instance.get("run")      → Look up the exported run() function
   run.call()               → Execute — blinks forever
   ```

### 3. The Build Pipeline

**File:** `build.rs`

The build script orchestrates two compilations in sequence:

```
cargo build --release
       │
       ▼
   build.rs runs:
       │
       ├── 1. Write rp2350.x → OUT_DIR/memory.x (linker script)
       │
       ├── 2. Spawn: cargo build --release --target wasm32-unknown-unknown
       │         └── wasm-app/ compiles → wasm_app.wasm (191 B)
       │         └── Copy to OUT_DIR/blinky.wasm
       │
       └── 3. Main firmware compiles:
               └── include_bytes!("blinky.wasm") embeds the WASM binary
               └── Links against memory.x for RP2350 memory layout
               └── Produces ELF binary (1.5 MB)
```

A critical detail: the parent build's `CARGO_ENCODED_RUSTFLAGS` (containing ARM-specific flags like `--nmagic` and `-Tlink.x`) must be stripped from the child WASM build via `.env_remove("CARGO_ENCODED_RUSTFLAGS")`, otherwise the WASM linker will fail on unrecognized arguments.

## Host Function Interface

The WASM module communicates with hardware through three host functions registered under the `"env"` namespace:

| Import Name         | Signature    | Description                            |
| ------------------- | ------------ | -------------------------------------- |
| `env.gpio_set_high` | `() → ()`    | Sets GPIO25 (onboard LED) to high (on) |
| `env.gpio_set_low`  | `() → ()`    | Sets GPIO25 (onboard LED) to low (off) |
| `env.delay_ms`      | `(i32) → ()` | Blocks execution for N milliseconds    |

These are registered with the wasmi `Linker` via `func_wrap()`, which wraps Rust closures as WASM-callable functions.

## Memory Layout

| Region             | Address      | Size            | Usage                                           |
| ------------------ | ------------ | --------------- | ----------------------------------------------- |
| Flash              | `0x10000000` | 2 MiB           | Firmware code + embedded WASM binary            |
| RAM (striped)      | `0x20000000` | 512 KiB         | Stack + heap + data                             |
| Heap (allocated)   | —            | 256 KiB         | wasmi engine, store, module, WASM linear memory |
| WASM linear memory | —            | 64 KiB (1 page) | WASM module's addressable memory                |
| WASM stack         | —            | 4 KiB           | WASM call stack                                 |

> **Important:** The default WASM linker allocates 1 MB of linear memory (16 pages). This exceeds the RP2350's total RAM. The `wasm-app/.cargo/config.toml` explicitly sets `--initial-memory=65536` (1 page) and `stack-size=4096`.

## Extending the Project

### Adding New Host Functions

1. Add the import declaration in `wasm-app/src/lib.rs`:
   ```rust
   unsafe extern {
       safe fn my_new_function(arg: u32);
   }
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

| Symptom                                         | Cause                                  | Fix                                                               |
| ----------------------------------------------- | -------------------------------------- | ----------------------------------------------------------------- |
| LED not blinking after flash                    | WASM linear memory too large for heap  | Ensure `wasm-app/.cargo/config.toml` has `--initial-memory=65536` |
| Build fails with `unknown argument: --nmagic`   | Parent rustflags leaking to WASM build | Ensure `build.rs` has `.env_remove("CARGO_ENCODED_RUSTFLAGS")`    |
| Build fails with `extern blocks must be unsafe` | Rust 2024 edition                      | Use `unsafe extern { ... }` with `safe fn` declarations           |
| `picotool` can't find device                    | Not in bootloader mode                 | Hold BOOTSEL while plugging in USB                                |
| `cargo build` doesn't pick up WASM changes      | Cached build artifacts                 | Run `cargo clean && cargo build --release`                        |

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.
