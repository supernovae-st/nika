#!/usr/bin/env bash
# Vector 24: per-crate LOC ratchet (≤ 15,000 LOC of tracked .rs sources).
#
# Two tiers, escalating consequence (the file-loc vector 12 pattern):
#   12,000 LOC — YELLOW warning (the descent window, no block)
#   15,000 LOC — RED hard fail (cannot push)
#
# Why a band: the red used to be discovered AT the push, mid-merge-train,
# and the descent then got improvised by whichever session held the crate
# (empirical: 7 reactive splits in 22 days on nika-cli, two of them the
# same day by two parallel sessions — 2026-07-31). file-loc has warned
# early since ADR-023 ([800,1500) yellow); this vector now speaks the same
# three-tier language. 12k = 80% of the cap: at the growth velocity the
# trust-experience arc showed (~5k prod LOC in one day), 3k of headroom
# is one planning window — enough to draw the boundary calmly, with an
# ADR, instead of at the gate.
#
# Both tiers consume the ONE prod-LOC counter in
# scripts/ci/check-crate-size.sh (CRATE_SIZE_MAX lowers only the ceiling —
# never a second implementation of the measure, the drift class this
# dashboard exists to kill). The wrapper translates to hygiene exit codes.
#
# This invariant is in nika-invariants.md ("Max LOC per crate: 15,000")
# but until now lived only in CI (`scripts/ci/`), not in the pre-push
# hygiene dashboard. Wired as vector 24 per Audit-2 P0 (2026-04-16).
#
# Exit codes:
#   0 — GREEN  (all crates < 12k LOC)
#   1 — YELLOW (one or more crates in [12k, 15k) — descent window, informational)
#   2 — RED    (at least one crate over budget — split required)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

WARN_AT=12000

OUTPUT=$(bash scripts/ci/check-crate-size.sh 2>&1)
STATUS=$?

if [ "$STATUS" -ne 0 ]; then
  # The CI script exits 1 on violation; promote to RED (2) for hygiene.
  echo "$OUTPUT" | head -10
  echo ""
  echo "Hint: split the over-budget crate into focused sub-crates per"
  echo "ADR-022 / ADR-024. The 15k LOC cap is a Diamond invariant"
  echo "(.claude/rules/nika-invariants.md)."
  exit 2
fi

# Red is silent — now probe the band with the same counter, lower ceiling.
BAND_OUTPUT=$(CRATE_SIZE_MAX="$WARN_AT" bash scripts/ci/check-crate-size.sh 2>&1)
BAND_STATUS=$?

if [ "$BAND_STATUS" -ne 0 ]; then
  # The dashboard (check-all.sh) shows ONE line per vector — the FIRST one.
  # A generic band header therefore printed 12001 and 14995 identically, and
  # the band's whole purpose is that the red is not discovered AT the push.
  # Measured 2026-08-18 · nika-check sat at 14995/15000 (5 LOC of headroom)
  # and read, on the dashboard, exactly like a crate with 3000 to spare.
  # So the header carries the TIGHTEST crate and its remaining headroom.
  BAND_ROWS=$(echo "$BAND_OUTPUT" | grep '^FAIL' \
    | sed -E 's/^FAIL  ([^ ]+)  ([0-9]+) LOC .*/\2 \1/' | LC_ALL=C sort -rn)
  BAND_N=$(printf '%s\n' "$BAND_ROWS" | grep -c .)
  TIGHT_LOC=$(printf '%s\n' "$BAND_ROWS" | head -1 | cut -d' ' -f1)
  TIGHT_CRATE=$(printf '%s\n' "$BAND_ROWS" | head -1 | cut -d' ' -f2)
  printf "YELLOW: %d crate(s) in the descent window · tightest %s at %d/15000 — %d LOC of headroom\n" \
    "$BAND_N" "$TIGHT_CRATE" "$TIGHT_LOC" "$((15000 - TIGHT_LOC))"
  printf '%s\n' "$BAND_ROWS" \
    | awk '{ printf "  %s: %s LOC · %s left before the wall\n", $2, $1, 15000-$1 }'
  echo ""
  echo "Hint: the wall is where a push BLOCKS — plan the descent in a calm"
  echo "window instead: one unit, two members, boundary drawn with an ADR"
  echo "(ADR-110 · D-2026-07-09-N1). Not blocking."
  exit 1
fi

echo "OK: all admitted crates under ${WARN_AT} LOC (15,000 budget)"
exit 0
