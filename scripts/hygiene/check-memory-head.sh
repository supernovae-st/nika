#!/usr/bin/env bash
# Vector 1: MEMORY.md HEAD SHA vs actual git HEAD.
set -u
MEMORY="$HOME/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/MEMORY.md"
[ -f "$MEMORY" ] || { echo "MEMORY.md not found"; exit 2; }

actual="$(git rev-parse --short=9 HEAD 2>/dev/null)"
recorded="$(grep -oE 'HEAD \(nika-diamond\): `[a-f0-9]+`' "$MEMORY" | head -1 | grep -oE '[a-f0-9]+' | tail -1)"

if [ -z "$recorded" ]; then
  echo "no HEAD recorded in MEMORY.md"; exit 1
fi

# Compare common prefix
if [ "${actual#$recorded}" != "$actual" ] || [ "${recorded#$actual}" != "$recorded" ]; then
  echo "OK ($recorded)"; exit 0
else
  echo "stale: MEMORY=$recorded actual=$actual"; exit 2
fi
