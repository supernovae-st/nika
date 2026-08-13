#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# test-mutation-floor.sh — the regression proofs for check-mutation-floor.sh.
#
# The gate under test ships a documented test seam
# (MUTATION_FLOOR_TEST_SUMMARY) "for fast arithmetic TDD" and had, until
# 2026-08-13, ZERO tests of its own. An adversarial review found that its
# zero-caught guard returned the tooling code for BOTH of its two named
# causes, so a vacuous-but-reachable suite was classified as a broken
# instrument. That is the exact shape of green this gate exists to refuse,
# and nothing would have caught it, because nothing was watching the gate.
#
# Every case below pins an exit code, because the exit code is the whole
# product of this script. A message is a courtesy; the code is the verdict.
#
# Usage: bash scripts/test/test-mutation-floor.sh
# Exit: 0 all pass · 1 a case failed
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -uo pipefail

ENGINE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ENGINE_ROOT" || exit 1

GATE="scripts/ci/check-mutation-floor.sh"
FAILURES=0

# A crate whose unit tests live INLINE in src/ — fully reachable under the
# `-- --lib` invocation, so any ratio it reports is a real measurement.
INLINE_CRATE="nika-cadence"
# A crate whose tests live ONLY in tests/ — unreachable under `-- --lib`,
# so any ratio it reports measures nothing.
SPLIT_CRATE="nika-tui-core"

# Guard the fixtures themselves: this file's whole argument rests on those
# two crates having the shapes named above. If a crate moves its tests, the
# cases below silently stop testing what they claim to.
check_fixture() {
  local crate="$1" want="$2" inline
  inline="$(grep -rl 'cfg(test)' "crates/${crate}/src" 2>/dev/null | wc -l | tr -d ' ')"
  case "$want" in
    inline)
      if [ "${inline:-0}" -eq 0 ]; then
        echo "FIXTURE STALE: ${crate} no longer has inline tests — the reachable-crate cases below test nothing"
        FAILURES=$((FAILURES + 1))
      fi
      ;;
    split)
      if [ "${inline:-0}" -ne 0 ] || [ ! -d "crates/${crate}/tests" ]; then
        echo "FIXTURE STALE: ${crate} is no longer tests/-only — the scope-hole case below tests nothing"
        FAILURES=$((FAILURES + 1))
      fi
      ;;
  esac
}

# case <name> <crate> <summary> <expected-exit>
case_is() {
  local name="$1" crate="$2" summary="$3" want="$4" got
  MUTATION_FLOOR_TEST_SUMMARY="$summary" bash "$GATE" "$crate" >/dev/null 2>&1
  got=$?
  if [ "$got" -ne "$want" ]; then
    echo "FAIL: ${name} — expected exit ${want}, got ${got}"
    echo "      crate=${crate} summary='${summary}'"
    FAILURES=$((FAILURES + 1))
  else
    echo "ok: ${name} (exit ${got})"
  fi
}

check_fixture "$INLINE_CRATE" inline
check_fixture "$SPLIT_CRATE" split

# ── the zero-caught fork · the whole point of the 2026-08-13 fix ────────
# Same summary, same zero, TWO different verdicts, decided by whether the
# instrument could see the crate at all.
case_is "zero caught on a tests/-only crate is a TOOLING fault" \
  "$SPLIT_CRATE" "165 mutants tested in 3m: 0 caught, 165 missed" 3

case_is "zero caught on an inline-test crate is a REAL floor failure" \
  "$INLINE_CRATE" "165 mutants tested in 3m: 0 caught, 165 missed" 2

# ── the ordinary verdicts must survive the guard ───────────────────────
case_is "a genuine below-floor ratio still fails" \
  "$INLINE_CRATE" "209 mutants tested in 4m: 186 caught, 23 missed" 2

case_is "a ratio at the floor passes" \
  "$INLINE_CRATE" "100 mutants tested in 1m: 90 caught, 10 missed" 0

case_is "a comfortable ratio passes" \
  "$INLINE_CRATE" "215 mutants tested in 5m: 200 caught, 15 missed" 0

# ── the edges ──────────────────────────────────────────────────────────
# No viable mutant is not a failure: there was nothing to kill. This arm
# runs BEFORE the zero-caught guard and must keep doing so, or an all-
# unviable crate would be read as a vacuous suite.
case_is "no viable mutants is not a failure" \
  "$INLINE_CRATE" "8 mutants tested in 9s: 8 unviable" 0

# A timeout is a survivor, not a kill.
case_is "a timeout counts against the ratio" \
  "$INLINE_CRATE" "100 mutants tested in 1m: 85 caught, 10 missed, 5 timeout" 2

# An unparseable summary is a tooling fault, never a verdict.
case_is "an unreadable summary refuses to score" \
  "$INLINE_CRATE" "cargo-mutants exploded" 3

# An unknown crate is a usage error.
case_is "an unknown crate is a usage error" \
  "nika-does-not-exist" "100 mutants tested: 100 caught" 3

if [ "$FAILURES" -gt 0 ]; then
  echo
  echo "check-mutation-floor.sh: ${FAILURES} case(s) failed"
  exit 1
fi

echo
echo "check-mutation-floor.sh: all cases pass"
exit 0
