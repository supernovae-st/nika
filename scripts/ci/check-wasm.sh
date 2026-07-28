#!/usr/bin/env bash
# The wasm leg — nika-check-wasm's own reason to exist, machine-verified.
# The Gate-11 admission swarm found the gap that mandates this file: the
# crate ships for wasm32-unknown-unknown and NO CI job had ever built that
# target — Gate 3 (compiles) and Gate 4 (zero clippy) were only ever checked
# by hand via build-wasm.sh. This leg also runs differential Leg B
# (NIKA_DIFF_CLI=1 · rows vs the SAME TREE's CLI over the conformance
# corpus), which otherwise no-ops unless a human remembers the env var —
# the exact "checked only by presence" shape ADR-003's 2026-06-10 amendment
# closed for Gate 5.
set -euo pipefail

if grep -qE '^\s*members\s*=\s*\[\s*\]\s*$' Cargo.toml; then
  echo "SKIP  workspace has no members yet (Phase 0)"
  exit 0
fi

rustup target add wasm32-unknown-unknown

# the getrandom 0.3 backend is selected at build time; forgetting the cfg
# reproduces the exact compile_error the crate's Cargo.toml documents
export RUSTFLAGS='--cfg getrandom_backend="wasm_js"'

cargo clippy -p nika-check-wasm --lib --target wasm32-unknown-unknown -- -D warnings
cargo build -p nika-check-wasm --lib --target wasm32-unknown-unknown --profile wasm-release

# Leg B needs the native CLI, not the wasm flags
unset RUSTFLAGS
NIKA_DIFF_CLI=1 cargo test -p nika-check-wasm --test differential
