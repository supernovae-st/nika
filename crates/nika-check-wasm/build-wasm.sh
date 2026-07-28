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

cargo test --locked -p nika-check-wasm

# --remap-path-prefix ×2: without them the artifact's panic-Location strings
# embed absolute host paths — measured 266 `/Users/…` strings including the
# full private monorepo layout, in a public artifact (the distribution
# review's blocking find). Remapping also removes the machine variable from
# reproducibility: every host builds `/cargo/…` + `/build/…`. Verdict rows
# proven byte-identical across the remap (Location strings never reach one).
workspace_root="$(cd ../.. && pwd)"
RUSTFLAGS="--cfg getrandom_backend=\"wasm_js\" --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo --remap-path-prefix=${workspace_root}=/build" \
  cargo build --locked -p nika-check-wasm --target wasm32-unknown-unknown --profile wasm-release

# the artifact path is DERIVED, never assumed: with CARGO_TARGET_DIR set the
# hardcoded ../../target/... points at whatever stale file sits there, and
# wasm-bindgen would silently package a wasm this run did not build
# (Gate-11 security finding F4)
target_root="$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
artifact="$target_root/wasm32-unknown-unknown/wasm-release/nika_check_wasm.wasm"

if command -v wasm-bindgen >/dev/null; then
  wasm-bindgen --target web --out-dir pkg \
    "$artifact"
  if command -v wasm-opt >/dev/null; then
    # the two strip passes drop the only custom sections the artifact
    # carries (producers 112 B · target_features 157 B — toolchain
    # disclosure, not size); --converge was measured at −39 bytes for
    # +14 s wall and declined. Keep the --enable-* flags: once stripped,
    # a future re-optimization has no feature section left to read.
    wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
      --strip-producers --strip-target-features \
      -o pkg/nika_check_wasm_bg.wasm.opt pkg/nika_check_wasm_bg.wasm
    mv pkg/nika_check_wasm_bg.wasm.opt pkg/nika_check_wasm_bg.wasm
  fi
  echo "pkg/ written · $(du -h pkg/nika_check_wasm_bg.wasm | cut -f1) raw"
else
  echo "wasm-bindgen-cli absent — the .wasm is built, bindings skipped"
  echo "install: cargo install wasm-bindgen-cli (match Cargo.lock's minor)"
fi
