#!/usr/bin/env bash
# Ratchet: the workspace lib-test battery.
#
# Runner: cargo-nextest when installed (process-per-test isolation + parallel
# scheduling — the 2026 flagship standard; CI installs it), plain `cargo test`
# as the everywhere-fallback so a machine without nextest still gates.
# Always `--lib` on macOS to avoid the Keychain popup triggered by integration
# tests that touch real secret stores (feedback_no_keychain_popup).
set -euo pipefail

# Phase 0 guard: cargo errors out on a virtual workspace with no members.
if grep -qE '^\s*members\s*=\s*\[\s*\]\s*$' Cargo.toml; then
  echo "SKIP  workspace has no members yet (Phase 0)"
  exit 0
fi

if command -v cargo-nextest >/dev/null 2>&1; then
  exec cargo nextest run --workspace --lib
fi
exec cargo test --workspace --lib
