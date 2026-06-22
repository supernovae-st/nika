#!/usr/bin/env bash
# Ratchet: cargo clippy --workspace --all-targets -- -D warnings, across every
# workspace feature EXCEPT the macOS-only `metal`.
set -euo pipefail

# Phase 0 guard: cargo errors out on a virtual workspace with no members.
if grep -qE '^\s*members\s*=\s*\[\s*\]\s*$' Cargo.toml; then
  echo "SKIP  workspace has no members yet (Phase 0)"
  exit 0
fi

# `--all-features` would enable `metal` (nika-infer-local → candle-core/metal →
# objc2, which `compile_error!`s off Apple platforms), so it can't run on the
# Linux CI runner. Enable every other workspace feature instead — same coverage
# as `--all-features` minus the one macOS-only feature. The metal/feature-powerset
# surface is the cargo-hack job's domain (where metal is likewise excluded).
features=$(cargo metadata --no-deps --format-version 1 \
  | jq -r '[.packages[] as $p | ($p.features | keys[]) | select(. != "metal") | "\($p.name)/\(.)"] | unique | join(",")')

if [ -z "$features" ]; then
  # Fallback (no jq / no features): plain --all-features.
  exec cargo clippy --workspace --all-targets --all-features -- -D warnings
fi

exec cargo clippy --workspace --all-targets --features "$features" -- -D warnings
