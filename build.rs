//! Build script for the t-wasm firmware.
//!
//! Sets up the RP2350 linker script and compiles the WASM blinky application
//! for embedding into the firmware binary.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

/// Creates the output directory path and registers it for linker search.
///
/// # Panics
///
/// Panics if the `OUT_DIR` environment variable is not set.
fn setup_output_dir() -> PathBuf {
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    println!("cargo:rustc-link-search={}", out.display());
    out
}

/// Writes the RP2350 memory layout linker script to the output directory.
///
/// # Arguments
///
/// * `out` - Output directory path where `memory.x` will be created.
///
/// # Panics
///
/// Panics if the file cannot be created or written.
fn write_linker_script(out: &PathBuf) {
    let memory_x = include_bytes!("rp2350.x");
    let mut f = File::create(out.join("memory.x")).unwrap();
    f.write_all(memory_x).unwrap();
}

/// Compiles the WASM blinky application and copies the binary to the output directory.
///
/// # Arguments
///
/// * `out` - Output directory path where `blinky.wasm` will be placed.
///
/// # Panics
///
/// Panics if the WASM compilation fails or the binary cannot be copied.
fn compile_wasm_app(out: &PathBuf) {
    let status = Command::new("cargo")
        .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
        .current_dir("wasm-app")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status()
        .expect("failed to build WASM app");
    assert!(status.success(), "WASM app compilation failed");
    std::fs::copy(
        "wasm-app/target/wasm32-unknown-unknown/release/wasm_app.wasm",
        out.join("blinky.wasm"),
    )
    .expect("copy WASM binary");
}

/// Registers file change triggers for incremental rebuilds.
fn print_rerun_triggers() {
    println!("cargo:rerun-if-changed=rp2350.x");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wasm-app/src/lib.rs");
    println!("cargo:rerun-if-changed=wasm-app/Cargo.toml");
}

/// Build script entry point that sets up linker scripts and compiles the WASM app.
fn main() {
    let out = setup_output_dir();
    write_linker_script(&out);
    compile_wasm_app(&out);
    print_rerun_triggers();
}
