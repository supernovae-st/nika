#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# check-mutation-floor.sh — REAL Gate 5 (mutation ≥90%) enforcement.
#
# The 12-gate admission (ADR-003) lists Gate 5 = "MUTATION ≥90% killed". Until
# now it was honor-system: scripts/ci/check-crate-gates.sh only checked that
# the crate-spec *mentions* "Mutation". This script actually RUNS cargo-mutants
# and enforces a configurable kill-ratio floor — the executable Gate 5.
#
# Kill ratio = caught / viable, where viable = caught + missed + timeout
# (UNVIABLE mutants — those that don't compile — are excluded, per the
# cargo-mutants definition: an unviable mutant can't be a real test gap).
#
# Tier: ADMISSION / CI — NOT pre-commit (cargo-mutants is minutes-slow on big
# crates). Run it when admitting a crate, or in a nightly CI matrix.
#
# Usage: scripts/ci/check-mutation-floor.sh <crate> [floor_pct]   (floor default 90)
# Exit: 0 pass (ratio ≥ floor, or no viable mutants) · 2 fail (ratio < floor)
#       · 3 tooling/usage error.
set -uo pipefail

ENGINE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ENGINE_ROOT" || exit 3

crate="${1:-}"
floor="${2:-90}"
if [ -z "$crate" ]; then
  printf 'usage: %s <crate> [floor_pct]\n' "$0" >&2
  exit 3
fi
if ! command -v cargo-mutants >/dev/null 2>&1; then
  echo "cargo-mutants not installed (run: cargo install cargo-mutants)"
  exit 3
fi
if [ ! -f "crates/${crate}/Cargo.toml" ]; then
  echo "no such crate: crates/${crate}"
  exit 3
fi

out="$(cargo mutants -p "$crate" 2>&1)"
mut_exit=$?

# Parse the canonical summary line, e.g.:
#   "3 mutants tested in 11s: 1 caught, 2 unviable"
#   "20 mutants tested in 2m: 18 caught, 1 missed, 1 timeout"
summary="$(printf '%s\n' "$out" | grep -E '[0-9]+ mutants tested' | tail -1)"
if [ -z "$summary" ]; then
  echo "could not parse cargo-mutants summary (exit $mut_exit):"
  printf '%s\n' "$out" | tail -5
  exit 3
fi

num() { printf '%s\n' "$summary" | grep -oE "[0-9]+ $1" | grep -oE '[0-9]+' | head -1; }
caught="$(num caught)"
caught="${caught:-0}"
missed="$(num missed)"
missed="${missed:-0}"
timeout_n="$(num timeout)"
timeout_n="${timeout_n:-0}"

viable=$((caught + missed + timeout_n))
if [ "$viable" -eq 0 ]; then
  echo "OK (${crate}: no viable mutants — nothing to kill · $summary)"
  exit 0
fi

# Integer kill-ratio percent (no bc dependency).
ratio=$(((caught * 100) / viable))
if [ "$ratio" -lt "$floor" ]; then
  echo "MUTATION FLOOR FAILED: ${crate} killed ${ratio}% (< ${floor}%) · ${caught}/${viable} viable · ${missed} survived, ${timeout_n} timeout"
  printf '%s\n' "$out" | grep -iE 'MISSED|survived' | head -10
  exit 2
fi

echo "OK (${crate}: ${ratio}% killed ≥ ${floor}% floor · ${caught}/${viable} viable)"
exit 0
