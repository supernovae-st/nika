#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# Vector 38: public-API surface coverage ratchet.
#
# .github/workflows/public-api.yml locks each covered crate's public API
# (diffs `cargo public-api` vs crates/<crate>/public-api.txt) and
# semver-checks.yml classifies drift as breaking-vs-additive. But both run on a
# HARDCODED 5-crate list — a newly-admitted crate gets NO API lock + NO semver
# coverage, and its public surface can break silently. That is the ADR-090
# anti-pattern: a hardcoded list that should be a projection of the workspace.
#
# This gate makes coverage a MONOTONIC ratchet, derived from the workspace:
#   - the floor is scripts/ci/public-api-coverage-baseline.txt (covered crates);
#   - every crate in the floor MUST still have its public-api.txt → else RED
#     (a regression that dropped an API lock);
#   - every admitted crate WITH a lib target (src/lib.rs) but NOT covered is a
#     YELLOW ratchet target (generate the baseline, add it to the floor, the
#     coverage rises and can't fall back).
#
# Generate a baseline:
#   cargo public-api -p <crate> --omit auto-trait-impls > crates/<crate>/public-api.txt
# (DEFAULT features + ubuntu-canonical: --all-features breaks on platform-
#  exclusive features (metal · xcap) and macOS renders platform-cfg'd crates
#  differently — regenerate from the public-api-actual CI artifact when in doubt)
#
# Exit: 0 green (all lib crates covered) · 1 yellow (uncovered lib crates remain
# · informational, ratcheting) · 2 red (a baseline-floor crate lost its lock,
# or the baseline names a non-existent crate).
set -uo pipefail

ENGINE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ENGINE_ROOT" || exit 2

# shellcheck source=scripts/ci/_lib.sh
. "$ENGINE_ROOT/scripts/ci/_lib.sh"

FLOOR="scripts/ci/public-api-coverage-baseline.txt"
[ -f "$FLOOR" ] || {
  echo "coverage baseline missing: $FLOOR"
  exit 2
}

# admitted crates with a lib target (src/lib.rs) — the should-be-covered set.
members="$(workspace_members)"

lib_crates=""
while IFS= read -r c; do
  [ -z "$c" ] && continue
  [ -f "crates/$c/src/lib.rs" ] && lib_crates="$lib_crates $c"
done <<EOF
$members
EOF

# floor crates (the ratchet floor, comment-stripped).
floor_crates="$(grep -vE '^[[:space:]]*#' "$FLOOR" | grep -E '^nika-' | sort -u)"

red=0
RED_MSGS=()

# --- regression: every floor crate must still have its public-api.txt ---------
while IFS= read -r c; do
  [ -z "$c" ] && continue
  if ! printf '%s\n' "$lib_crates" | tr ' ' '\n' | grep -qx "$c"; then
    RED_MSGS+=("baseline crate '$c' is no longer an admitted lib crate — update $FLOOR.")
    red=1
    continue
  fi
  if [ ! -f "crates/$c/public-api.txt" ]; then
    RED_MSGS+=("baseline crate '$c' lost its public-api.txt lock — regression. Regenerate it or it is a real API-coverage loss.")
    red=1
  fi
done <<EOF
$floor_crates
EOF

if [ "$red" -ne 0 ]; then
  echo "public-API coverage FAILED:"
  for m in "${RED_MSGS[@]}"; do echo "  RED: $m"; done
  exit 2
fi

# --- ratchet: list uncovered lib crates (yellow, informational) --------------
uncovered=""
covered_n=0
for c in $lib_crates; do
  if [ -f "crates/$c/public-api.txt" ]; then
    covered_n=$((covered_n + 1))
  else
    uncovered="$uncovered $c"
  fi
done

total_n="$(printf '%s' "$lib_crates" | wc -w | tr -d ' ')"
if [ -n "${uncovered# }" ]; then
  echo "RATCHET (${covered_n}/${total_n} lib crates API-locked) · uncovered (generate baselines + add to floor):"
  for c in $uncovered; do echo "  · $c"; done
  exit 1
fi

echo "OK (all ${total_n} admitted lib crates carry a public-api.txt surface lock)"
exit 0
