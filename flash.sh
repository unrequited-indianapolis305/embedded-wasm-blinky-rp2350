#!/bin/bash

# Build the firmware in release mode (also compiles the WASM app via build.rs)
cargo build --release || exit 1

# Flash the ELF binary to the RP2350 Pico 2 via picotool
picotool load -u -v -x -t elf target/thumbv8m.main-none-eabihf/release/t-wasm
