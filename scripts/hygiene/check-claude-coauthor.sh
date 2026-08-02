#!/usr/bin/env bash
# Vector 13: no "Co-Authored-By: Claude" trailer anywhere on this branch.
#
# Scope, and why it is the WHOLE branch (2026-08-02): this vector used to
# start from `git merge-base nika-diamond main`. That branch was renamed to
# `main` on 2026-05-06 — the merge-base has failed ever since, silently, and
# the `|| git rev-parse HEAD~100` fallback took over. So the check had
# quietly become a ROLLING WINDOW of 100 commits (two days of work at the
# current pace · 5% of the branch) while still reporting "only Nika 🦋
# co-authors on diamond branch" — a claim about 1977 commits made from 100.
#
# There is no merge-base to compute. `main` is an ORPHAN branch (ADR-001):
# it shares zero history with `brouillon`, so its whole history IS the
# diamond history. Scanning all of it costs 0.12s — the window bought
# nothing it did not also cost in truth.
set -uo pipefail

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
found=$(git log --format='%H %B' 2>/dev/null \
  | grep -iE '^Co-Authored-By:[[:space:]]+Claude([^[:alpha:]]|$)' \
  || true)

if [ -n "$found" ]; then
  count=$(printf '%s\n' "$found" | wc -l | tr -d ' ')
  echo "${count} commits with a Claude co-author trailer (use Nika 🦋 only)"
  exit 2
fi

# Say what was actually read. A vector that names its scope cannot drift
# into claiming more than it looked at.
scanned=$(git rev-list --count HEAD 2>/dev/null || echo "?")
echo "OK (only Nika 🦋 co-authors · ${scanned} commits scanned)"
exit 0
