#!/usr/bin/env bash
# Ratchet: cargo test --workspace --lib.
#
# Always `--lib` on macOS to avoid the Keychain popup triggered by integration
# tests that touch real secret stores (feedback_no_keychain_popup).
set -euo pipefail

# Phase 0 guard: cargo errors out on a virtual workspace with no members.
if grep -qE '^\s*members\s*=\s*\[\s*\]\s*$' Cargo.toml; then
  echo "SKIP  workspace has no members yet (Phase 0)"
  exit 0
fi

exec cargo test --workspace --lib
