#!/usr/bin/env bash
# Vector 6: crate-spec presence + metrics freshness.
#
# (a) docs/crate-specs/nika-X.md exists for every admitted crate.
# (b) If a spec states a `~NNN LOC src` anchor, that number must track the
#     LIVE src LOC (scripts/crate-metrics.sh --loc) within ±15% — the
#     gross-staleness band. This is the structural fix for the drift class
#     that hand-typed gate-evidence numbers fall into (PILLAR 2 ·
#     hygiene-at-commit-time). The number is a cached projection; the
#     projector is the source of truth; this gate keeps them honest.
#     A spec that carries NO hardcoded number (pointing only to the
#     projector) is fine — nothing to drift.
#
# Why ±15% not exact: src LOC moves with every doc-comment edit; an exact
# parity gate would false-fail constantly. 15% catches real staleness
# (the blob 300-vs-393 = 31% and http 515-vs-1007 = 95% drifts this vector
# was created to prevent) while absorbing churn. Fix on breach is
# mechanical: `scripts/crate-metrics.sh <crate>` → update the IMPL row.
#
# Exit codes: 0 GREEN · 2 RED (missing spec OR stale LOC anchor).
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

METRICS="scripts/crate-metrics.sh"
TOLERANCE_PCT=15

missing=""
stale=""

# Admitted-crate list comes from the projector (single source · derives the
# WIP split from [workspace.metadata.diamond] wip = [...] in Cargo.toml).
while read -r crate; do
  [ -n "$crate" ] || continue
  spec="docs/crate-specs/${crate}.md"
  if [ ! -f "$spec" ]; then
    missing="${missing}${crate} "
    continue
  fi

  # (b) freshness — only for numbers that OPT IN to the live-anchor
  # convention: `~NNN LOC src (… live …)`. The `(…live…)` marker is what
  # distinguishes a freshness-gated live anchor from a design-target
  # estimate (e.g. « Target: ~500 LOC src ») which is NOT a live claim.
  anchor="$(grep -oE '~[0-9]+ LOC src \([^)]*live' "$spec" 2>/dev/null | head -1)"
  [ -n "$anchor" ] || continue
  stated="$(printf '%s' "$anchor" | grep -oE '[0-9]+' | head -1)"
  [ -n "$stated" ] || continue

  live="$(bash "$METRICS" --loc "$crate" 2>/dev/null)"
  [ -n "$live" ] && [ "$live" -gt 0 ] 2>/dev/null || continue

  diff=$((stated - live))
  [ "$diff" -lt 0 ] && diff=$((-diff))
  drift_pct=$((diff * 100 / live))
  if [ "$drift_pct" -gt "$TOLERANCE_PCT" ]; then
    stale="${stale}${crate}(spec ~${stated} vs live ${live} = ${drift_pct}% off) "
  fi
done < <(bash "$METRICS" --admitted)

rc=0
if [ -n "$missing" ]; then
  echo "RED: missing specs: ${missing}"
  rc=2
fi
if [ -n "$stale" ]; then
  echo "RED: stale LOC anchor (>${TOLERANCE_PCT}% drift) — run \`scripts/crate-metrics.sh <crate>\` and update the spec IMPL row:"
  echo "     ${stale}"
  rc=2
fi
[ "$rc" -eq 0 ] && echo "OK (all specs present · LOC anchors fresh within ±${TOLERANCE_PCT}%)"
exit "$rc"
