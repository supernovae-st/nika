#!/usr/bin/env bash
# build-wasm.sh — the browser lot for nika-tui-core, the workspace pattern
# (the nika-check-wasm precedent): cargo build on the wasm target under the
# workspace's `wasm-release` profile, then the wasm-bindgen CLI, then the
# current binaryen's strip passes.
#
# The two --remap-path-prefix flags keep host paths OUT of the public
# artifact (the distribution review's blocking find on the sibling crate —
# 266 `/Users/…` strings measured, never again).
set -euo pipefail
cd "$(dirname "$0")"

command -v cargo >/dev/null || {
  echo "cargo missing"
  exit 2
}

cargo test --locked -p nika-tui-core

workspace_root="$(cd ../.. && pwd)"
RUSTFLAGS="--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo --remap-path-prefix=${workspace_root}=/build" \
  cargo build --locked -p nika-tui-core --features wasm --target wasm32-unknown-unknown --profile wasm-release

# the artifact path is DERIVED, never assumed (CARGO_TARGET_DIR moves it)
target_root="$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')"
artifact="$target_root/wasm32-unknown-unknown/wasm-release/nika_tui_core.wasm"

if command -v wasm-bindgen >/dev/null; then
  wasm-bindgen --target web --out-dir pkg "$artifact"
  if command -v wasm-opt >/dev/null; then
    wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
      --strip-producers --strip-target-features \
      -o pkg/nika_tui_core_bg.wasm.opt pkg/nika_tui_core_bg.wasm
    mv pkg/nika_tui_core_bg.wasm.opt pkg/nika_tui_core_bg.wasm
  fi
  raw=$(stat -f%z pkg/nika_tui_core_bg.wasm)
  gz=$(gzip -c pkg/nika_tui_core_bg.wasm | wc -c | tr -d ' ')
  echo "pkg/ written · raw ${raw} B · gzip ${gz} B"
else
  echo "wasm-bindgen-cli absent — the .wasm is built, bindings skipped"
  echo "install: cargo install wasm-bindgen-cli (match Cargo.lock's minor)"
fi

# the Node harness judges the built artifact on the real fixtures (law ⑧)
wasm-bindgen --target nodejs --out-dir pkg-node "$artifact"
node test.mjs
