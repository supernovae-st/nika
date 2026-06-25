#!/usr/bin/env bash
# Ratchet: cargo clippy --workspace --all-targets -- -D warnings, across every
# workspace feature EXCEPT the system-lib features (`metal` · `xcap`).
set -euo pipefail

# Phase 0 guard: cargo errors out on a virtual workspace with no members.
if grep -qE '^\s*members\s*=\s*\[\s*\]\s*$' Cargo.toml; then
  echo "SKIP  workspace has no members yet (Phase 0)"
  exit 0
fi

# `--all-features` would enable two features that need platform system libs the
# default Linux runner lacks: `metal` (nika-infer-local → candle-core/metal →
# objc2, which `compile_error!`s off Apple platforms) and `xcap` (nika-screen →
# Wayland/X11/PipeWire/DRM/EGL). Enable every OTHER workspace feature instead —
# same coverage minus those two. Their surface is the cargo-hack job's domain
# (it installs the desktop libs + exercises `xcap`; `metal` stays excluded there).
features=$(cargo metadata --no-deps --format-version 1 \
  | jq -r '[.packages[] as $p | ($p.features | keys[]) | select(. != "metal" and . != "xcap") | "\($p.name)/\(.)"] | unique | join(",")')

if [ -z "$features" ]; then
  # Fallback (no jq / no features): plain --all-features.
  exec cargo clippy --workspace --all-targets --all-features -- -D warnings
fi

exec cargo clippy --workspace --all-targets --features "$features" -- -D warnings
