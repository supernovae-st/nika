#!/usr/bin/env bash
# Vector 24: per-crate LOC ratchet (≤ 25,000 LOC of tracked Rust + embedded jq).
#
# Two tiers, escalating consequence (the file-loc vector 12 pattern):
#   15,000 LOC — YELLOW warning from this size (the descent window, never blocks)
#   25,000 LOC — RED above this size (cannot push)
# ADR-154 set these values: the former 15,000 wall became the warning, and one
# 25,000 ceiling replaced it for every crate (scripts/ci/crate-size-policy.sh).
#
# Why a band: the red used to be discovered AT the push, mid-merge-train,
# and the descent then got improvised by whichever session held the crate
# (empirical: 7 reactive splits in 22 days on nika-cli, two of them the
# same day by two parallel sessions — 2026-07-31). file-loc has warned
# early since ADR-023 (its yellow tier); this vector speaks the same
# three-tier language. The warning sits at the former 15k wall: at the growth
# velocity the trust-experience arc showed (~5k prod LOC in one day), the 10k
# of headroom it leaves is two planning windows — enough to draw a boundary
# calmly, with an ADR, instead of at the gate. (Under the 15k wall this band
# was [12k, 15k), 3k wide.)
#
# Both tiers consume the ONE prod-LOC counter in
# scripts/ci/check-crate-size.sh (CRATE_SIZE_MAX lowers only the ceiling —
# never a second implementation of the measure, the drift class this
# dashboard exists to kill). The wrapper translates to hygiene exit codes.
#
# This invariant is in nika-invariants.md ("Max LOC per crate: 25,000")
# but until now lived only in CI (`scripts/ci/`), not in the pre-push
# hygiene dashboard. Wired as vector 24 per Audit-2 P0 (2026-04-16).
#
# Exit codes:
#   0 — GREEN  (all crates < 15k LOC)
#   1 — YELLOW (one or more crates in [15k, 25k] — descent window, informational)
#   2 — RED    (at least one crate over the 25k ceiling — split required)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2
# shellcheck source=scripts/ci/crate-size-policy.sh
. "$REPO_ROOT/scripts/ci/crate-size-policy.sh"

WARN_AT=15000
CAP=$(crate_size_limit crates/any) || exit 2

OUTPUT=$(bash scripts/ci/check-crate-size.sh 2>&1)
STATUS=$?

if [ "$STATUS" -ne 0 ]; then
  # The CI script exits 1 on violation; promote to RED (2) for hygiene.
  echo "$OUTPUT" | head -10
  echo ""
  echo "Hint: review the over-budget crate's module boundaries per ADR-022 / ADR-024."
  echo "The ceiling is ${CAP} production lines for every crate (ADR-154)."
  exit 2
fi

# Red is silent — now probe the band with the same counter, lower ceiling.
BAND_OUTPUT=$(CRATE_SIZE_MAX="$WARN_AT" bash scripts/ci/check-crate-size.sh 2>&1)
BAND_STATUS=$?

if [ "$BAND_STATUS" -ne 0 ]; then
  # The dashboard (check-all.sh) shows ONE line per vector — the FIRST one.
  # A generic band header therefore printed 12001 and 14995 identically, and
  # the band's whole purpose is that the red is not discovered AT the push.
  # Measured 2026-08-18, under the former 15k wall · nika-check sat at
  # 14995/15000 (5 LOC of headroom) and read, on the dashboard, exactly like a
  # crate with 3000 to spare.
  # So the header carries the TIGHTEST crate and its remaining headroom.
  BAND_ROWS=$(echo "$BAND_OUTPUT" | grep '^FAIL' \
    | while read -r _ crate loc _; do
      limit=$(crate_size_limit "$crate") || exit 2
      printf '%d %s %d %d\n' "$((limit - loc))" "$crate" "$loc" "$limit"
    done | LC_ALL=C sort -n)
  BAND_N=$(printf '%s\n' "$BAND_ROWS" | grep -c .)
  TIGHT_LEFT=$(printf '%s\n' "$BAND_ROWS" | head -1 | cut -d' ' -f1)
  TIGHT_CRATE=$(printf '%s\n' "$BAND_ROWS" | head -1 | cut -d' ' -f2)
  TIGHT_LOC=$(printf '%s\n' "$BAND_ROWS" | head -1 | cut -d' ' -f3)
  TIGHT_LIMIT=$(printf '%s\n' "$BAND_ROWS" | head -1 | cut -d' ' -f4)
  printf "YELLOW: %d crate(s) in the descent window · tightest %s at %d/%d — %d LOC of headroom\n" \
    "$BAND_N" "$TIGHT_CRATE" "$TIGHT_LOC" "$TIGHT_LIMIT" "$TIGHT_LEFT"
  printf '%s\n' "$BAND_ROWS" \
    | awk '{ printf "  %s: %s/%s LOC · %s left before the wall\n", $2, $3, $4, $1 }'
  echo ""
  echo "Hint: the wall is where a push BLOCKS — plan the descent in a calm"
  echo "window instead: one unit, two members, boundary drawn with an ADR"
  echo "(ADR-110 · D-2026-07-09-N1). Not blocking."
  exit 1
fi

echo "OK: all admitted crates under ${WARN_AT} LOC (${CAP} budget)"
exit 0
