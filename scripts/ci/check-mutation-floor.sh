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

# --- ACCOUNTING: every mutant lands in exactly one bucket ------------------
# Found 2026-08-13 by the regression suite this gate never had. The empty
# check above catches an ABSENT summary; it cannot catch a summary that is
# present but unreadable. A truncated line ("12 mutants tested in 3s:") or an
# upstream format change yields caught=missed=timeout=0, which reads as
# "no viable mutants · nothing to kill" and scores as a PASS. Garbage in,
# green out — for every crate at once, silently, the day the format moves.
#
# So the totals must reconcile: caught + missed + timeout + unviable has to
# equal the mutants the run says it tested. When it does not, the script has
# no verdict to give and says so (exit 3) rather than inventing one.
total="$(printf '%s\n' "$summary" | grep -oE '[0-9]+ mutants tested' | grep -oE '[0-9]+' | head -1)"
unviable="$(num unviable)"
unviable="${unviable:-0}"
if [ -z "$total" ]; then
  echo "could not read a mutant total from the cargo-mutants summary:"
  printf '  %s\n' "$summary"
  exit 3
fi
if [ $((caught + missed + timeout_n + unviable)) -ne "$total" ]; then
  echo "could not account for every mutant in the cargo-mutants summary:"
  printf '  %s\n' "$summary"
  echo "  ${caught} caught + ${missed} missed + ${timeout_n} timeout + ${unviable} unviable != ${total} tested"
  echo "  The summary format moved or the line is truncated. No ratio is trustworthy here."
  exit 3
fi

if [ "$viable" -eq 0 ]; then
  echo "OK (${crate}: no viable mutants — nothing to kill · $summary)"
  exit 0
fi

# --- HARNESS GUARD: zero caught over N viable is an INSTRUMENT fault --------
# A suite that kills NONE of N viable mutants has not run. A weak suite still
# catches something; a ratio of exactly 0% is the signature of a test command
# that executed nothing. Printing it as a FLOOR failure sends a human to repair
# a crate that is not broken, and that is the more expensive lie: a red that
# accuses the wrong subject.
#
# The dominant cause is SCOPE. This script runs `-- --lib` (the diamond
# convention + the macOS no-Keychain rule), so a crate whose tests live ONLY in
# tests/ has zero test executed. Measured 2026-08-13: nika-tui-core, 0 caught of
# 165 viable, with 1047 LOC of tests sitting in crates/nika-tui-core/tests/.
# Exactly two crates in the workspace sit in that hole (nika-acp · nika-tui-core);
# the other 61 keep their unit tests inline and are measured correctly.
#
# Exit 3 (tooling/usage), never 2 (below floor): the run produced no verdict.
if [ "$caught" -eq 0 ]; then
  # WHICH cause decides the EXIT CODE, not merely the message. The first
  # version of this guard printed two different causes and returned 3 for
  # both, so the detection was decoration: an adversarial review named it
  # "diagnostic theatre, not diagnostic logic" and it was right. A crate
  # whose tests are INLINE is fully reachable under `-- --lib`, so 0 caught
  # there is the textbook signature of a vacuous suite — a DESERVED floor
  # failure. Classifying it as tooling would buy exactly the green this
  # whole file exists to refuse.
  inline_tests="$(grep -rl 'cfg(test)' "crates/${crate}/src" 2>/dev/null | wc -l | tr -d ' ')"
  if [ "${inline_tests:-0}" -eq 0 ] && [ -d "crates/${crate}/tests" ]; then
    echo "MUTATION HARNESS FAULT: ${crate} caught 0 of ${viable} viable mutants."
    echo "  The test run executed nothing. This is NOT a kill ratio and must not be read as one."
    echo "  cause: crates/${crate}/src carries no #[cfg(test)] and crates/${crate}/tests/ exists,"
    echo "         so the '-- --lib' invocation cannot reach this crate's tests."
    echo "  fix:   measure this crate without the --lib restriction (mind the macOS Keychain"
    echo "         rule in .claude/CLAUDE.md), or move its unit tests inline into src/."
    exit 3
  fi
  echo "MUTATION FLOOR FAILED: ${crate} killed 0% · 0/${viable} viable · ${missed} survived, ${timeout_n} timeout"
  echo "  This crate's tests are reachable under '-- --lib' and killed NOTHING."
  echo "  That is a vacuous suite, not a broken instrument: a real Gate 5 failure."
  printf '%s\n' "$out" | grep -iE 'MISSED|survived' | head -10
  exit 2
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
