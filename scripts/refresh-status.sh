#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# refresh-status.sh — single source of truth for status claims.
#
# Reads the engine state from the canonical sources (git, Cargo.toml,
# cargo test, cargo clippy) and prints a markdown block that every
# status doc (MEMORY.md, STATE.md, CLAUDE.md, ROADMAP.md current-state
# section, etc.) MUST quote verbatim.
#
# Usage:
#   ./scripts/refresh-status.sh           # print canonical block
#   ./scripts/refresh-status.sh --quick   # skip cargo test (fast preview)
#   ./scripts/refresh-status.sh --write   # PRINT *and* write it into every
#                                         # doc carrying the marker
#
# ⚠️ `--write` exists because the loop had no closing move. The doctrine
# says «run refresh-status.sh to regenerate the canonical block» — and the
# script only ever PRINTED it, waiting for a human to paste. Measured
# 2026-08-14: both status docs carried a HEAD from eleven commits earlier
# and two `(skipped)` placeholders where numbers belong, while vector 23
# read GREEN — it compares STRUCTURAL fields only, never HEAD or the
# measurements. A regeneration nobody can run is a regeneration nobody
# runs.
#
# ⚠️ The `HEAD` field can NEVER equal the current HEAD of a committed tree:
# the commit that carries the block does not exist when the block is
# generated. It means «the HEAD the block was generated against», one
# commit behind by construction — not a drift to chase. What IS a drift is
# a HEAD from eleven commits back, which is what --write exists to close.
#
# Companion: scripts/hygiene/check-status-claims-sync.sh (Phase B vector
# 26) greps the status docs and fails if their claims don't match this
# script's output.

set -euo pipefail

ENGINE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ENGINE_ROOT"

# flags are scanned, not positional — `--quick --write` and `--write --quick`
# must mean the same thing, and the old single-positional form still works
QUICK=""
WRITE=""
for arg in "$@"; do
  case "$arg" in
    --quick) QUICK="--quick" ;;
    --write) WRITE=1 ;;
    *) ;;
  esac
done

# ── git ────────────────────────────────────────────────────────────
HEAD_SHA_FULL="$(git rev-parse HEAD)"
HEAD_SHA_SHORT="$(git rev-parse --short=9 HEAD)"
# No BRANCH field (#1240): the row stamped the GENERATION branch — under
# PR flow always a feature branch deleted after squash — so it could
# never be right on main. Provenance rides the HEAD row.

# ── workspace ──────────────────────────────────────────────────────
WORKSPACE_VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/^version[ \t]+=[ \t]+"([^"]+)".*/\1/')"

# Workspace members — exact `members = [...]` list, not exclude list.
# Grab the first `members` assignment after the `[workspace]` header.
MEMBERS_RAW="$(grep -m1 '^members' Cargo.toml)"
WORKSPACE_MEMBERS=$(echo "$MEMBERS_RAW" | grep -oE '"crates/[^"]+"' | wc -l | tr -d ' ')

# Per-layer distribution from [workspace.metadata.diamond].
# `grep -c` exits 1 on zero matches under `set -e`, so we use the
# `wc -l`-on-grep-output pattern uniformly to keep the script honest.
count_layer() {
  # `pipefail` would propagate grep's exit code 1 on zero matches —
  # absorb that with `|| true` so the empty-layer case returns 0
  # instead of failing the whole script.
  # NOTE `[a-z0-9-]` (not `[a-z-]`) · crate names carry digits (nika-bm25 ·
  # nika-a11y) · the digit-blind regex undercounted L1 (showed 3 · was 5).
  { grep -E "layers\\.nika-[a-z0-9-]+ = \"$1\"\$" Cargo.toml || true; } \
    | wc -l | tr -d ' '
}
L0_COUNT=$(count_layer "L0")
L05_COUNT=$(count_layer "L0\\.5")
L1_COUNT=$(count_layer "L1")
L15_COUNT=$(count_layer "L1\\.5")
L2_COUNT=$(count_layer "L2")
L3_COUNT=$(count_layer "L3")
L4_COUNT=$(count_layer "L4")

# Identify WIP crates (admitted = passed all 12 gates; WIP = in
# workspace but pre-admission). Heuristic: nika-schema (L0 parser
# scaffolding · no admission commit yet). nika-screen + nika-ocr + nika-a11y
# ALL ADMITTED 2026-05-25 (M2.1 capture / M2.2 OCR / M2.3 a11y · ADR-003
# canonical 12 gates · mutation + Rule-2 OS-FFI/model/AXUIElement-walk
# exemptions + Foreman-direct 3-lens review per PE-5.1). Refine when a
# per-crate admission ledger lands (Phase B vector 27 candidate).
# WIP list DERIVED from the projector (single source · [workspace.metadata.diamond]
# wip = [...] in Cargo.toml) — no re-hardcoded list (per crate-spec-metrics ratchet).
WIP_CRATES="$(bash scripts/crate-metrics.sh --wip | xargs)"
# `grep -c` exits 1 on zero matches under `set -e` — absorb with `|| true`
# (same as count_layer) so an EMPTY wip array (every crate admitted) returns
# 0 instead of failing the whole script.
WIP_COUNT=$(bash scripts/crate-metrics.sh --wip | grep -c . || true)
ADMITTED_COUNT=$((WORKSPACE_MEMBERS - WIP_COUNT))

# ── tests ──────────────────────────────────────────────────────────
if [[ "$QUICK" == "--quick" ]]; then
  LIB_TESTS="(skipped — pass --no-quick to compute)"
  CLIPPY_STATUS="(skipped)"
else
  LIB_TESTS=$(cargo test --workspace --lib 2>&1 | grep "test result" | awk '{sum+=$4; fail+=$6} END {print sum" passed, "fail" failed"}')
  if cargo clippy --workspace --all-targets -- -D warnings >/dev/null 2>&1; then
    CLIPPY_STATUS="0 warnings"
  else
    CLIPPY_STATUS="WARNINGS PRESENT — run cargo clippy"
  fi
fi

# ── output canonical block ─────────────────────────────────────────
BLOCK="$(
  cat <<EOF
<!-- AUTO-GENERATED by scripts/refresh-status.sh — do not edit by hand -->
<!-- Status drift between this block and any quoting doc is caught by
     scripts/hygiene/check-status-claims-sync.sh (vector 23). No branch
     row since 2026-08-26 (#1240): it stamped the generation branch —
     always a deleted feature branch under PR flow — never main. -->

| field            | value                                          |
|------------------|------------------------------------------------|
| HEAD             | \`$HEAD_SHA_SHORT\` (\`$HEAD_SHA_FULL\`)             |
| workspace        | v$WORKSPACE_VERSION                                  |
| crates (workspace)| $WORKSPACE_MEMBERS                                              |
| crates (admitted)| $ADMITTED_COUNT                                             |
| crates (WIP)     | $WIP_COUNT — $WIP_CRATES                                  |
| L0               | $L0_COUNT                                              |
| L0.5             | $L05_COUNT                                              |
| L1               | $L1_COUNT                                              |
| L1.5             | $L15_COUNT                                              |
| L2               | $L2_COUNT                                              |
| L3               | $L3_COUNT                                              |
| L4               | $L4_COUNT                                              |
| lib tests        | $LIB_TESTS                              |
| clippy           | $CLIPPY_STATUS                              |
EOF
)"

printf '%s\n' "$BLOCK"

[[ -n "$WRITE" ]] || exit 0

# ── --write · replace the block in every doc that carries the marker ──
MARK='<!-- AUTO-GENERATED by scripts/refresh-status.sh'
# anchored at line START — `estate.yaml` CITES the marker inside a string
# without carrying the block, and a loose match would have handed it to the
# rewriter. The python guard caught it (it refused rather than corrupting a
# file), but a selector that hands the wrong subject to a guard is the
# selector's bug, not the guard's win.
mapfile -t DOCS < <(
  find . \
    -type d \( -name target -o -name .git \) -prune -o \
    -path './scripts' -prune -o \
    -type f -name '*.md' -exec grep -l "^$MARK" {} + 2>/dev/null
)

if [[ ${#DOCS[@]} -eq 0 ]]; then
  echo "refresh-status --write: NO doc carries the marker — nothing written." >&2
  echo "  (a write that finds zero subjects is a failure, not a success)" >&2
  exit 3
fi

BLOCK="$BLOCK" python3 - "${DOCS[@]}" <<'PY'
import io, os, sys

block = os.environ['BLOCK'].rstrip('\n').split('\n')
mark = '<!-- AUTO-GENERATED by scripts/refresh-status.sh'
changed = 0

for path in sys.argv[1:]:
    lines = io.open(path, encoding='utf-8').read().split('\n')
    starts = [i for i, l in enumerate(lines) if l.startswith(mark)]
    if len(starts) != 1:
        sys.exit('refresh-status --write: %s carries %d markers, expected 1'
                 % (path, len(starts)))
    start = starts[0]
    ends = [i for i, l in enumerate(lines)
            if i > start and l.startswith('| clippy')]
    if not ends:
        sys.exit('refresh-status --write: %s has a marker but no `| clippy` '
                 'row — the block is malformed' % path)
    end = ends[0]
    if lines[start:end + 1] == block:
        print('  = %s (already current)' % path)
        continue
    lines[start:end + 1] = block
    io.open(path, 'w', encoding='utf-8').write('\n'.join(lines))
    print('  ✎ %s' % path)
    changed += 1

print('refresh-status --write: %d/%d doc(s) rewritten'
      % (changed, len(sys.argv) - 1))
PY
