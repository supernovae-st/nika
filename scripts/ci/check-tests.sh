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

# The environment and the tool are judged SEPARATELY. They used to share
# one `&&`, which made a missing cargo-nextest indistinguishable from "not
# CI": in CI, with the tool absent, the whole condition went false and the
# script fell through to `cargo test --workspace --lib` — silently
# reinstating the exact regression the SCOPE note above says was closed on
# 2026-07-06, with the tests/ suites running nowhere again. `command -v`
# failing has to read as "I cannot judge", never as "take the narrower
# path".
if [ -n "${CI:-}" ]; then
  if ! command -v cargo-nextest >/dev/null 2>&1; then
    echo "FAIL  CI requires cargo-nextest for the FULL battery (lib AND tests/)." >&2
    echo "      Refusing to fall back to --lib: that scope is the 2026-07-06" >&2
    echo "      regression, and a narrower run is not this gate passing." >&2
    exit 1
  fi
  # CI: the FULL battery — lib AND integration targets (tests/).
  exec cargo nextest run --workspace
fi

if command -v cargo-nextest >/dev/null 2>&1; then
  exec cargo nextest run --workspace --lib
fi

# Local, no nextest: the fallback serializes tests within each process.
# CLI fixtures borrow the process-global cwd; readers cannot safely run beside
# a test that enters an invalid project. Nextest isolates these by process.
# A reduced scope that announces itself is a choice; one that does not is a
# false green.
echo "WARN  cargo-nextest absent — running 'cargo test --workspace --lib -- --test-threads=1'." >&2
echo "      LIB TARGETS ONLY: the tests/ integration suites are NOT executed." >&2
echo "      SERIAL TESTS: isolate process-global cwd fixtures from concurrent readers." >&2
exec cargo test --workspace --lib -- --test-threads=1
