#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-private-leaks.sh
# Mutation proof for vector 14 — the FULL-TREE half of the privacy boundary.
#
# Why this test exists, measured 2026-08-15. Vector 14 guarded ONE hardcoded
# pattern while the pre-commit hook guarded TEN plus the venture shape. The
# sweep that looks EVERYWHERE searched for almost nothing, so 18 occurrences
# of a private monorepo path — one of them in a user-facing error hint of the
# PUBLIC engine — slept in this tree until a merge happened to drop them into
# someone's staged diff. Widening it without a mutation proof would just move
# the trust, not earn it.
#
# The properties, and each one is a defect this vector actually had:
#
#   1. IT LOOKS. A private path anywhere in the tracked tree turns it red.
#      With one pattern, nine families were invisible.
#   2. IT DOES NOT OVER-LOOK. The public repos tier is of the same SHAPE as
#      a private pole; the first cut of the widening reported every file
#      citing the public tier as a leak — it named a clean handoff doc guilty.
#      A gate that names a file it never proved guilty teaches the reader to
#      disbelieve it.
#   3. THE WORD BOUNDARY HOLDS. Unanchored `studio/` matched `lmstudio/` and
#      `~/.cache/lm-studio/` — real, public provider paths — so the gate
#      blocked legitimate work.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
VECTOR="$ROOT/scripts/hygiene/check-private-leaks.sh"

# A tracked file the vector reads, restored after every case.
PROBE="$ROOT/crates/nika-error/src/lib.rs"
BACKUP="$(mktemp)"
cp "$PROBE" "$BACKUP"
restore() { cp "$BACKUP" "$PROBE"; }
trap 'restore; rm -f "$BACKUP"' EXIT

pass=0
fail=0

expect() { # label, expected_rc, injected_line ("" = clean tree)
  local label="$1" want="$2" line="${3:-}"
  [ -n "$line" ] && printf '\n// %s\n' "$line" >>"$PROBE"
  (cd "$ROOT" && bash "$VECTOR") >/dev/null 2>&1
  local got=$?
  restore
  if [ "$got" = "$want" ]; then
    printf '  ok    %s (rc=%s)\n' "$label" "$got"
    pass=$((pass + 1))
  else
    printf '  FAIL  %s — expected rc=%s, got rc=%s\n' "$label" "$want" "$got"
    fail=$((fail + 1))
  fi
}

expect "clean tree is green" 0 ""
expect "a dx/ path turns it red" 2 "see dx/journal/ for the studio chronicle"
expect "a studio/ path turns it red" 2 "see studio/04-identity/brand/ for the palette"
# A SYNTHETIC pole · the shape is what turns it red, not the document. Naming a
# real private file here published its path in a public repo, which is the very
# thing this gate exists to stop (measured 2026-08-20 by the monorepo's
# vocabulary screen, whose private-PM-pole pattern matched the old line).
#
# That screen then matched THIS comment, because the first draft quoted the
# offending token while explaining it. An explanation of a screened string is
# not exempt from the screen · describe the pattern, never reproduce it.
expect "a private venture pole turns it red" 2 "ventures/example/05-growth/campaign-notes.md"
expect "a pre-migration spelling turns it red" 2 "nika/hq/strategy.md"
expect "the PUBLIC venture tier stays green" 0 "ventures/nika/02-engineering/repos/engine/README.md"
expect "lmstudio keeps the word boundary green" 0 "/home/u/.cache/lm-studio/models and the lmstudio/ provider"

# The gate and its full-tree twin must read the SAME list, or they drift back
# apart — which is the whole reason the leak slept.
SHARED="$ROOT/scripts/lib/private-patterns.sh"
for consumer in "$VECTOR" "$ROOT/scripts/hooks/block-private-paths.sh"; do
  if grep -q 'private-patterns.sh' "$consumer"; then
    printf '  ok    %s sources the shared pattern list\n' "$(basename "$consumer")"
    pass=$((pass + 1))
  else
    printf '  FAIL  %s does NOT source %s\n' "$(basename "$consumer")" "$SHARED"
    fail=$((fail + 1))
  fi
done

printf '\n%s/%s cases correct.\n' "$pass" "$((pass + fail))"
[ "$fail" -eq 0 ]
