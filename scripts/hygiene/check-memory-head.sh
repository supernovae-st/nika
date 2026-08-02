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
# The anchor, and why there are two (2026-08-02): this vector matched the
# literal `nika-diamond):** <sha>` prose. That branch was renamed to `main`
# on 2026-05-06, so the pattern has matched nothing for three months — and
# the empty result printed "no HEAD recorded in MEMORY.md", which reads as
# "you forgot to write it down" when it meant "I am looking for a branch
# that no longer exists". A vector that cannot find its own anchor must
# say so in those words, or the next reader spends the same three months.
#
# The current prose is `engine main <sha>`; the legacy form is kept so an
# older MEMORY.md still resolves. Both are tried, in order, and the failure
# branch below names them rather than blaming the file.
# shellcheck disable=SC2016  # literal backtick regexes — no expansion wanted
PATTERNS='engine[^`]{0,40}`[0-9a-f]{7,40}`|nika-diamond\):\*\* `[0-9a-f]{7,40}`'
recorded="$(grep -oE "$PATTERNS" "$MEMORY" | head -1 | grep -oE '[0-9a-f]{7,40}' | tail -1)"

if [ -z "$recorded" ]; then
  # Two different problems wear this face. Say which is which, and name
  # what was tried, so neither one can hide behind the other again.
  # shellcheck disable=SC2016
  if grep -qE '`[0-9a-f]{7,40}`' "$MEMORY"; then
    echo "MEMORY.md records SHAs but none under an engine anchor · tried: ${PATTERNS}"
  else
    echo "no HEAD recorded in MEMORY.md (no backticked SHA anywhere in the file)"
  fi
  exit 1
fi

# Use safe comparison (no glob expansion via unquoted vars).
# Compare first 7 chars (short SHA) at minimum.
actual_prefix="${actual:0:7}"
recorded_prefix="${recorded:0:7}"
if [ "$actual_prefix" = "$recorded_prefix" ]; then
  echo "OK ($recorded)"
  exit 0
fi

# Behind is not the same as wrong (2026-08-02). This vector ran RED on any
# mismatch, which at PRE-PUSH time is red by construction: MEMORY.md is
# written before the commit being pushed exists, so it is always at least
# one behind. A gate that cannot be green when everything is correct is the
# false-RED class the runner's own header calls social noise. It only
# escaped notice because the vector had been blind since May.
#
# So the two cases split. A recorded SHA the repo knows AND that is an
# ancestor of main is a LAG — yellow, carrying its distance, so a pointer
# drifting far behind still says so loudly. A recorded SHA the repo does
# not know, or that sits on another lineage, is a WRONG pointer — red.
if ! git cat-file -e "${recorded}^{commit}" 2>/dev/null; then
  echo "wrong: MEMORY=$recorded is not a commit in this repo (actual=$actual)"
  exit 2
fi
if ! git merge-base --is-ancestor "$recorded" "$actual" 2>/dev/null; then
  echo "wrong: MEMORY=$recorded is not an ancestor of $actual (diverged pointer)"
  exit 2
fi
behind="$(git rev-list --count "${recorded}..${actual}" 2>/dev/null || echo '?')"
echo "behind by ${behind}: MEMORY=$recorded actual=$actual"
exit 1
