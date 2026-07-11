#!/usr/bin/env bash
# Vector 1: MEMORY.md HEAD SHA vs actual git HEAD.
set -u
MEMORY_NEW="$HOME/.claude/projects/-Users-thibaut-supernovae/memory/MEMORY.md"
MEMORY_LEGACY="$HOME/.claude/projects/-Users-thibaut-supernovae-hq/memory/MEMORY.md"
if [ -f "$MEMORY_NEW" ]; then
  MEMORY="$MEMORY_NEW"
elif [ -f "$MEMORY_LEGACY" ]; then
  MEMORY="$MEMORY_LEGACY"
else
  MEMORY="$MEMORY_NEW" # will trigger the not-found error below
fi
[ -f "$MEMORY" ] || {
  # Local-only vector: MEMORY.md lives in the developer's ~/.claude tree, never
  # in the repo. Absent ⇒ running in CI or a fresh clone, not a real drift —
  # skip rather than false-RED (this was the dominant cause of the nightly
  # hygiene-drift issue spam · 2026-06).
  echo "SKIP (local-only check · MEMORY.md absent — run locally to verify the HEAD pointer)"
  exit 0
}

# The recorded pointer tracks MAIN, so the comparison must too: HEAD is
# the CHECKOUT's tip, and a worktree on a feature branch false-REDs every
# push (2026-07-11 ×6 — the class that forced a whole arc onto the
# api-commit route). `main` resolves fine from linked worktrees (shared
# common-dir); HEAD stays the fallback for clones with another default.
actual="$(git rev-parse --short=9 main 2>/dev/null || git rev-parse --short=9 HEAD 2>/dev/null)"
# shellcheck disable=SC2016  # literal backtick regex — no expansion needed
recorded="$(grep -oE 'nika-diamond\):\*\* `[a-f0-9]+`' "$MEMORY" | head -1 | grep -oE '[a-f0-9]+' | tail -1)"

if [ -z "$recorded" ]; then
  echo "no HEAD recorded in MEMORY.md"
  exit 1
fi

# Use safe comparison (no glob expansion via unquoted vars).
# Compare first 7 chars (short SHA) at minimum.
actual_prefix="${actual:0:7}"
recorded_prefix="${recorded:0:7}"
if [ "$actual_prefix" = "$recorded_prefix" ]; then
  echo "OK ($recorded)"
  exit 0
else
  echo "stale: MEMORY=$recorded actual=$actual"
  exit 2
fi
