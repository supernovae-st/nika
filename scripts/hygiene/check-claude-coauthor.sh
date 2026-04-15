#!/usr/bin/env bash
# Vector 13: no "Co-Authored-By: Claude" in commits on nika-diamond branch
# (since its divergence from main). Legacy main commits predate the Nika 🦋
# convention and are outside our scope.
set -uo pipefail

# Find merge-base with main — scope check to diamond-only commits
base=$(git merge-base nika-diamond main 2>/dev/null || git rev-parse HEAD~100 2>/dev/null || echo "HEAD~50")

# Detect Claude co-author trailers with two safeguards (P0-1 Batch H+):
#   1. ^ anchor restricts to trailer lines (subject line has SHA prefix
#      from %H so ^ cannot match there).
#   2. Word-boundary check `([^[:alpha:]]|$)` after "Claude" prevents
#      false-positives on legitimate contributors like "Claudia" while
#      still catching bypass attempts like "Claude-via-Nika-relay" or
#      "Claude.AI" (any non-alpha char / EOL confirms it is the Claude
#      token, not a name starting with the same letters).
#   The previous `grep -v 'Nika'` filter was both redundant and harmful —
#   it removed trailer lines that happened to mention 'Nika', creating a
#   silent bypass.
found=$(git log "${base}..HEAD" --format='%H %B' 2>/dev/null \
  | grep -iE '^Co-Authored-By:[[:space:]]+Claude([^[:alpha:]]|$)' \
  || true)

if [ -n "$found" ]; then
  count=$(printf '%s\n' "$found" | wc -l | tr -d ' ')
  echo "${count} diamond-branch commits with Claude co-author (use Nika 🦋 only)"
  exit 2
fi
echo "OK (only Nika 🦋 co-authors on diamond branch)"
exit 0
