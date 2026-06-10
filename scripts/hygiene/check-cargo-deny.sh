#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# Vector 34: supply-chain policy via `cargo deny check`.
#
# Superset of vector 15 (check-cargo-audit.sh, RustSec advisories ONLY).
# This vector enforces the full deny.toml POLICY:
#   · advisories  — RustSec advisory DB (vulns, yanked, unmaintained)
#   · bans        — banned crates + duplicate-version policy (deny.toml [bans])
#   · licenses    — SPDX allowlist (AGPL-compatible only, deny.toml [licenses])
#   · sources     — only trusted registries/git sources (deny.toml [sources])
#
# Rationale (sovereignty Rule 1 + supply-chain SLSA posture): a forever-v0.x
# sovereign engine must gate its dependency graph against license drift, a
# banned/duplicate dep sneaking in, and untrusted sources — not just CVEs.
# cargo-audit catches CVEs; only `cargo deny check` catches the other three.
#
# Exit: 0 green (policy clean) · 2 red (any deny.toml check failed).
# `unused-wrapper` warnings (a stale ban-allowlist entry) are non-blocking.
set -uo pipefail

ENGINE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ENGINE_ROOT" || exit 2

if ! command -v cargo-deny >/dev/null 2>&1; then
  echo "cargo-deny not installed (run: cargo install cargo-deny)"
  exit 2
fi
if [ ! -f deny.toml ]; then
  echo "deny.toml missing — supply-chain policy unconfigured"
  exit 2
fi

# Run all four checks; capture output for diagnostics. `cargo deny check`
# exits non-zero on any error-level finding (warnings do not fail it unless
# escalated in deny.toml).
out="$(cargo deny check 2>&1)"
status=$?

if [ "$status" -ne 0 ]; then
  # Surface only the error lines, not the full dependency tree.
  echo "cargo deny POLICY violation(s):"
  printf '%s\n' "$out" | grep -iE 'error\[|^error|denied|rejected|not allowed|duplicate' | head -15
  exit 2
fi

# Green. Echo the canonical one-line summary cargo-deny prints.
summary="$(printf '%s\n' "$out" | grep -E 'advisories ok|bans ok|licenses ok|sources ok' | tail -1)"
echo "OK (${summary:-advisories ok, bans ok, licenses ok, sources ok})"
exit 0
