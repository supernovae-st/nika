#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# check-mutation-floor.sh — REAL Gate 5 (mutation ≥90%) enforcement.
#
# ADR-003 Gate 5 = "MUTATION ≥90% killed (or documented exemption)". Until now
# it was honor-system: check-crate-gates.sh only checked that the crate-spec
# *mentions* "Mutation". This runs cargo-mutants and enforces a configurable
# kill-floor, AND honours the ADR-003 Rule-2 exemption as a VERIFIABLE BUDGET
# (not a skip) — the antifragile reframe.
#
# Two modes:
#   FLOOR mode (no exemption · normal crate): kill ratio = caught / viable
#     (viable = caught + missed + timeout; unviable excluded per cargo-mutants).
#     Pass iff ratio ≥ floor.
#   BUDGET mode (the crate-spec declares `<!-- GATE5-EXEMPT: N -->`): require
#     survivors ≤ N — every uncaught viable mutant must be within a documented
#     budget; a regression adding a survivor beyond N → RED. The count lives
#     ONCE in the spec (SSOT). This MECHANISM is correct, but a cross-platform
#     crate (a11y/screen/ocr) reports MORE survivors than the spec's reachable
#     count, because `cargo mutants -- --lib` mutates the cfg'd-out other-OS
#     arms too (platform-inactive, not real test gaps). So BUDGET mode is only
#     reproducible once the crate ships a `mutants.toml` exclude_re for its
#     cfg'd-out code — a deferred-with-trigger follow-up. Until then those
#     crates carry NO GATE5-EXEMPT marker and run FLOOR mode (where their
#     platform-inactive survivors would false-RED — so do NOT FLOOR-gate a
#     cross-platform crate until its mutants.toml exclude lands).
#
#   Known limitation (count-based, not identity-based): the gate counts
#   survivors, not identities — a dev could "fix" an exempt mutant and let a
#   new one survive (net count unchanged). The crate-spec § lists the exact
#   exempt mutants for human review; the mutants.toml exclude_re is the
#   identity-based upgrade path.
#
# Runs with `-- --lib` (the diamond convention: lib-tests only, matches every
# crate-spec's documented `cargo mutants -p X -- --lib` invocation + the
# CLAUDE.md "always --lib" no-Keychain rule).
#
# Tier: ADMISSION / CI — NOT pre-commit (cargo-mutants is minutes-slow).
#
# Usage: scripts/ci/check-mutation-floor.sh <crate> [floor_pct]   (floor default 90)
# Exit: 0 pass · 2 fail (below floor / over budget) · 3 tooling/usage error.
#
# Test seam: MUTATION_FLOOR_TEST_SUMMARY="<a cargo-mutants summary line>" skips
# the real run (for fast arithmetic TDD).
set -uo pipefail

ENGINE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ENGINE_ROOT" || exit 3

crate="${1:-}"
floor="${2:-90}"
if [ -z "$crate" ]; then
  printf 'usage: %s <crate> [floor_pct]\n' "$0" >&2
  exit 3
fi
if [ ! -f "crates/${crate}/Cargo.toml" ]; then
  echo "no such crate: crates/${crate}"
  exit 3
fi

# --- exemption budget (SSOT: the crate-spec marker) -------------------------
spec="docs/crate-specs/${crate}.md"
exempt=0
if [ -f "$spec" ]; then
  # NB: extract the number AFTER the colon — a naive [0-9]+ would match the
  # "5" in "GATE5-EXEMPT" first.
  m="$(sed -nE 's/.*GATE5-EXEMPT: *([0-9]+).*/\1/p' "$spec" | head -1)"
  exempt="${m:-0}"
fi

# --- obtain the cargo-mutants summary line ---------------------------------
if [ -n "${MUTATION_FLOOR_TEST_SUMMARY:-}" ]; then
  summary="$MUTATION_FLOOR_TEST_SUMMARY"
  out="$summary"
else
  if ! command -v cargo-mutants >/dev/null 2>&1; then
    echo "cargo-mutants not installed (run: cargo install cargo-mutants)"
    exit 3
  fi
  out="$(cargo mutants -p "$crate" -- --lib 2>&1)"
  # Canonical summary, e.g.:
  #   "3 mutants tested in 11s: 1 caught, 2 unviable"
  #   "41 mutants tested in 2m: 34 caught, 7 missed"
  summary="$(printf '%s\n' "$out" | grep -E '[0-9]+ mutants tested' | tail -1)"
fi
if [ -z "$summary" ]; then
  echo "could not parse cargo-mutants summary:"
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

survivors=$((missed + timeout_n))
viable=$((caught + survivors))

if [ "$viable" -eq 0 ]; then
  echo "OK (${crate}: no viable mutants — nothing to kill · $summary)"
  exit 0
fi

# --- BUDGET mode (documented exemption) ------------------------------------
if [ "$exempt" -gt 0 ]; then
  if [ "$survivors" -le "$exempt" ]; then
    echo "OK (${crate}: ${survivors} survivor(s) ≤ ${exempt} documented-exempt budget · ${caught}/${viable} caught · Rule-2 exemption honoured)"
    exit 0
  fi
  over=$((survivors - exempt))
  echo "MUTATION BUDGET FAILED: ${crate} has ${survivors} survivor(s) > ${exempt} documented-exempt (${over} beyond budget) · ${caught}/${viable} caught · a survivor regressed past the Rule-2 exemption"
  printf '%s\n' "$out" | grep -iE 'MISSED|survived' | head -10
  exit 2
fi

# --- FLOOR mode (no exemption) ---------------------------------------------
ratio=$(((caught * 100) / viable))
if [ "$ratio" -lt "$floor" ]; then
  echo "MUTATION FLOOR FAILED: ${crate} killed ${ratio}% (< ${floor}%) · ${caught}/${viable} viable · ${missed} survived, ${timeout_n} timeout"
  printf '%s\n' "$out" | grep -iE 'MISSED|survived' | head -10
  exit 2
fi

echo "OK (${crate}: ${ratio}% killed ≥ ${floor}% floor · ${caught}/${viable} viable)"
exit 0
