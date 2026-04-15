#!/usr/bin/env bash
# Vector 1: MEMORY.md HEAD SHA vs actual git HEAD.
set -u
MEMORY="$HOME/.claude/projects/-Users-thibaut-dev-supernovae-hq/memory/MEMORY.md"
[ -f "$MEMORY" ] || {
  echo "MEMORY.md not found"
  exit 2
}

actual="$(git rev-parse --short=9 HEAD 2>/dev/null)"
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
