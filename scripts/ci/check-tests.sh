#!/usr/bin/env bash
# Ratchet: the workspace test battery.
#
# Runner: cargo-nextest when installed (process-per-test isolation + parallel
# scheduling — the 2026 flagship standard; CI installs it), plain `cargo test`
# as the everywhere-fallback so a machine without nextest still gates.
#
# SCOPE (the 2026-07-06 discovery): `--lib` everywhere meant the tests/
# suites (floor · output_fidelity · conformance · …) ran NOWHERE — not CI,
# not pre-push, not the canonical local command. floor_deep was born red
# and never executed. The fix: CI (linux · no Keychain) runs the FULL
# nextest battery — every test target; LOCAL keeps `--lib` (the macOS
# Keychain-popup law · feedback_no_keychain_popup). A mac dev with nextest
# installed still gets `--lib`: the gate keys on $CI, not on the tool.
set -euo pipefail

# Phase 0 guard: cargo errors out on a virtual workspace with no members.
if grep -qE '^\s*members\s*=\s*\[\s*\]\s*$' Cargo.toml; then
  echo "SKIP  workspace has no members yet (Phase 0)"
  exit 0
fi

if [ -n "${CI:-}" ] && command -v cargo-nextest >/dev/null 2>&1; then
  # CI: the FULL battery — lib AND integration targets (tests/).
  exec cargo nextest run --workspace
fi
if command -v cargo-nextest >/dev/null 2>&1; then
  exec cargo nextest run --workspace --lib
fi
exec cargo test --workspace --lib
