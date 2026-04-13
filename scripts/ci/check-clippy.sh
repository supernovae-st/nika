#!/usr/bin/env bash
# Ratchet: cargo clippy --workspace --all-targets -- -D warnings
set -euo pipefail

# Phase 0 guard: cargo errors out on a virtual workspace with no members.
if grep -qE '^\s*members\s*=\s*\[\s*\]\s*$' Cargo.toml; then
  echo "SKIP  workspace has no members yet (Phase 0)"
  exit 0
fi

exec cargo clippy --workspace --all-targets --all-features -- -D warnings
