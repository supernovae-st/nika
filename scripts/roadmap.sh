#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# roadmap.sh — the live Diamond roadmap, in one screen.
#
# Projection-by-default: the layer map is DERIVED from the authoritative
# `[workspace.metadata.diamond] layers.*` in Cargo.toml + the WIP list in
# refresh-status.sh. It CANNOT drift the way a hand-maintained snapshot does
# (the `crate-admission-order.md` snapshot went stale in days — this replaces
# the need to eyeball it). Companion: `scripts/refresh-status.sh` (the
# canonical status block · parity-enforced by hygiene vector 23).
#
# Usage: bash scripts/roadmap.sh

set -euo pipefail

ENGINE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ENGINE_ROOT"

TARGET=42 # v0.90 crate target (cap 100) · per nika-invariants.md + POST_AUDIT 2026-04-14

# ── WIP list · single source = refresh-status.sh (stay DRY) ──────────
WIP_CSV="$(grep -m1 '^WIP_CRATES=' scripts/refresh-status.sh | sed -E 's/.*"([^"]*)".*/\1/')"
is_wip() { case ",$WIP_CSV," in *",$1,"*) return 0 ;; *) return 1 ;; esac }

# ── parse layer assignments (authoritative · same source cargo-deny uses) ──
declare -A CRATES_IN # layer -> space-separated crate list
ADMITTED=0
WIP_COUNT=0
while IFS= read -r line; do
  crate="$(echo "$line" | sed -E 's/^layers\.(nika-[a-z0-9-]+) = .*/\1/')"
  layer="$(echo "$line" | sed -E 's/.*= "([^"]+)".*/\1/')"
  if is_wip "$crate"; then
    mark="$crate(WIP)"
    WIP_COUNT=$((WIP_COUNT + 1))
  else
    mark="$crate"
    ADMITTED=$((ADMITTED + 1))
  fi
  CRATES_IN["$layer"]="${CRATES_IN["$layer"]:-} $mark"
done < <(grep -E '^layers\.nika-[a-z0-9-]+ = "' Cargo.toml)

# ── progress bar (admitted / target) ─────────────────────────────────
pct=$((ADMITTED * 100 / TARGET))
filled=$((ADMITTED * 24 / TARGET))
bar=""
for ((i = 0; i < 24; i++)); do
  if [ "$i" -lt "$filled" ]; then bar="${bar}█"; else bar="${bar}░"; fi
done

HEAD_SHORT="$(git rev-parse --short=9 HEAD)"
BRANCH="$(git branch --show-current)"

# ── render · top-down (goal at top → foundation at bottom) ───────────
printf '\n  🦋 NIKA DIAMOND · forever-v0.x · branch %s · %s\n' "$BRANCH" "$HEAD_SHORT"
printf '  %s  %d / %d crates admitted (%d%%) · %d WIP\n\n' "$bar" "$ADMITTED" "$TARGET" "$pct" "$WIP_COUNT"

# layer · one-line role · the crates currently in the workspace at that layer
render_layer() {
  local layer="$1" role="$2"
  local crates="${CRATES_IN["$layer"]:-}"
  local count
  count="$(echo "$crates" | wc -w | tr -d ' ')"
  printf '  %-5s %-14s %2d  %s\n' "$layer" "$role" "$count" "${crates# }"
}

render_layer "L5" "binary" # the `nika` CLI composition root
render_layer "L4" "interfaces"
render_layer "L3" "orchestration"
render_layer "L2" "domain·verbs"
render_layer "L1" "effects"
render_layer "L0.5" "kernel"
render_layer "L0" "foundation"

# ── the frontier · current computer-use M2 arc (canonical 6-crate sequence) ──
cat <<'FRONTIER'

  ── FRONTIER · Phase 2 M2 · computer-use L1 effects ──────────────────
     M2.1 screen   ✅   M2.2 ocr   ✅   M2.3 a11y   ✅
     M2.4 input    ⬜ NEXT (2 MANDATORY guards · write-side)
     M2.5 browser  ⬜      M2.6 vision-local ⬜

  ── THE CLIMB AHEAD ──────────────────────────────────────────────────
     finish M2 effects → L2 verbs (infer·exec·invoke·agent) + Connectome
     → L3 runtime+daemon (runs the DAG) → L4 cli·serve·sdk → L5 binary
     → v0.90 (42 crates · 12 gates each · 7 shadow zones green)

FRONTIER
