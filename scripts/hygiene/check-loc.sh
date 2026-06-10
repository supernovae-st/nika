#!/usr/bin/env bash
# Vector 3: workspace src LOC — live, derived report.
#
# History: this vector used to compare live src LOC against a number
# hand-recorded in MEMORY.md (« Total engine src LOC: ~13.5k »). That was
# the exact drift class the crate-spec-metrics ratchet kills: gating a
# fully-derivable number against hand-typed prose — and worse, via a STALE
# MEMORY.md path (`-Users-thibaut-dev-supernovae-hq`, pre repo-move) that
# never resolved, so the vector sat permanently YELLOW (« cannot determine
# LOC »). LOC is derivable; there is no second source to drift against.
#
# Now: report the live total (sum of per-crate src from the projector,
# the single LOC method) — GREEN/informational. The per-crate 15k cap is
# vector 24, the per-file 1500 cap is vector 12; this vector is the
# at-a-glance workspace total, always honest because always derived.
#
# Exit codes: 0 GREEN (reported) · 2 RED (projector unavailable).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

METRICS="scripts/crate-metrics.sh"
if [ ! -x "$METRICS" ]; then
  echo "RED: $METRICS missing (LOC projector is the source of truth)"
  exit 2
fi

# Sum per-crate src LOC from the projector (one method · same numbers the
# crate-spec anchors + the per-crate cap vectors speak).
total="$(bash "$METRICS" --json | grep -oE '"src_loc":[0-9]+' | grep -oE '[0-9]+' | awk '{s += $1} END {print s + 0}')"

if [ -z "$total" ] || [ "$total" -le 0 ] 2>/dev/null; then
  echo "RED: projector returned no LOC"
  exit 2
fi

echo "OK (${total} workspace src LOC · live · scripts/crate-metrics.sh)"
exit 0
