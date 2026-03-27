//! Build script for the wasm-tests crate.
//!
//! Compiles the WASM blinky application before tests run so the binary
//! is available for integration tests via `include_bytes!`.

use std::process::Command;

/// Compiles the WASM blinky application for the `wasm32-unknown-unknown` target.
///
/// # Panics
///
/// Panics if the cargo build command fails to execute or returns a non-zero exit code.
fn compile_wasm_app() {
    let status = Command::new("cargo")
        .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
        .current_dir("../wasm-app")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .status()
        .expect("failed to build WASM app");
    assert!(status.success(), "WASM app compilation failed");
}

/// Copies the compiled WASM binary into the `OUT_DIR` for `include_bytes!`.
///
/// # Panics
///
/// Panics if the copy operation fails.
fn copy_wasm_binary() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let src = "../wasm-app/target/wasm32-unknown-unknown/release/wasm_app.wasm";
    let dst = format!("{out_dir}/blinky.wasm");
    std::fs::copy(src, &dst).expect("failed to copy WASM binary");
}

/// Registers file change triggers for incremental test rebuilds.
fn print_rerun_triggers() {
    println!("cargo:rerun-if-changed=../wasm-app/src/lib.rs");
    println!("cargo:rerun-if-changed=../wasm-app/Cargo.toml");
    println!("cargo:rerun-if-changed=build.rs");
}

/// Build script entry point that compiles the WASM app for testing.
fn main() {
    compile_wasm_app();
    copy_wasm_binary();
    print_rerun_triggers();
}
