#!/usr/bin/env bash
# Ratchet: cargo check with default features disabled.
# See S12-G3 — catches feature-gate rot that the default build hides.
set -euo pipefail

# Phase 0 guard: cargo errors out on a virtual workspace with no members.
if grep -qE '^\s*members\s*=\s*\[\s*\]\s*$' Cargo.toml; then
  echo "SKIP  workspace has no members yet (Phase 0)"
  exit 0
fi

exec cargo check --workspace --no-default-features
