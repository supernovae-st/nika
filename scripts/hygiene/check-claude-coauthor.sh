#!/usr/bin/env bash
# Vector 13: no "Co-Authored-By: Claude" in commits on nika-diamond branch
# (since its divergence from main). Legacy main commits predate the Nika 🦋
# convention and are outside our scope.
set -uo pipefail

# Find merge-base with main — scope check to diamond-only commits
base=$(git merge-base nika-diamond main 2>/dev/null || git rev-parse HEAD~100 2>/dev/null || echo "HEAD~50")

found=$(git log "${base}..HEAD" --format='%H %B' 2>/dev/null \
        | grep -iE 'Co-Authored-By:[[:space:]]+Claude' \
        | grep -v 'Nika' || true)

if [ -n "$found" ]; then
  count=$(printf '%s\n' "$found" | wc -l | tr -d ' ')
  echo "${count} diamond-branch commits with Claude co-author (use Nika 🦋 only)"; exit 2
fi
echo "OK (only Nika 🦋 co-authors on diamond branch)"; exit 0
