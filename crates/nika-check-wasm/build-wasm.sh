#!/usr/bin/env bash
# build-wasm.sh — the three steps from README.md, guarded.
# The getrandom 0.3 backend is selected at BUILD time via RUSTFLAGS; forgetting
# it reproduces the exact compile_error this crate's Cargo.toml documents.
set -euo pipefail
cd "$(dirname "$0")"

command -v cargo >/dev/null || {
  echo "cargo missing"
  exit 2
}

cargo test -p nika-check-wasm --lib

RUSTFLAGS='--cfg getrandom_backend="wasm_js"' \
  cargo build -p nika-check-wasm --target wasm32-unknown-unknown --release

if command -v wasm-bindgen >/dev/null; then
  wasm-bindgen --target web --out-dir pkg \
    ../../target/wasm32-unknown-unknown/release/nika_check_wasm.wasm
  echo "pkg/ written · $(du -h pkg/nika_check_wasm_bg.wasm | cut -f1) raw"
else
  echo "wasm-bindgen-cli absent — the .wasm is built, bindings skipped"
  echo "install: cargo install wasm-bindgen-cli (match Cargo.lock's minor)"
fi
