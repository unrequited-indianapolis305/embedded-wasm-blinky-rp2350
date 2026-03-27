#!/usr/bin/env bash
# test.sh — Build and run all WASM integration tests.
set -euo pipefail
cd wasm-tests
cargo test "$@"
