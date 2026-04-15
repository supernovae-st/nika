#!/usr/bin/env bash
# coauthor-squash-detect.sh — Tier 4 post-merge warning
#
# Detects if a merge-squash operation dropped Co-Authored-By: Nika 🦋 trailers.
# GitHub squash-merges produce a single commit from N PR commits; if the
# squash message was auto-generated without trailers, attribution is lost.
#
# Checks: HEAD commit has Nika 🦋 co-author in footer.
# Runs only if HEAD is a squash-merge (single parent, came from post-merge context).
#
# Non-blocking: warns to stderr, exits 0.
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -uo pipefail

readonly COAUTHOR_PATTERN='Co-Authored-By: Nika'

HEAD_MSG="$(git log -1 --format='%B' HEAD 2>/dev/null)" || exit 0
HEAD_SHA="$(git rev-parse --short HEAD 2>/dev/null)" || exit 0

# Only warn if HEAD is a single-parent commit (squash-merge indicator)
PARENT_COUNT="$(git log -1 --format='%P' HEAD 2>/dev/null | wc -w | tr -d ' ')"
if ((PARENT_COUNT != 1)); then
  exit 0 # merge commit (2 parents) or initial — not a squash
fi

# Skip if ORIG_HEAD doesn't exist (not a merge context)
if ! git rev-parse ORIG_HEAD >/dev/null 2>&1; then
  exit 0
fi

# Check if HEAD co-author is present
if printf '%s' "$HEAD_MSG" | grep -q "$COAUTHOR_PATTERN"; then
  exit 0 # co-author preserved
fi

# P0-5 Batch H+: the previous logic tried `git log "${ORIG_SHA}..HEAD~1"` to
# confirm the squashed commits HAD the co-author before warning. But after a
# squash, HEAD~1 is the pre-squash parent (not a squashed commit), so that
# range was always empty → warning never fired.
#
# Simplified: if we're in post-merge context (ORIG_HEAD exists), HEAD is a
# single-parent commit (squash indicator), and HEAD lacks the co-author
# trailer, that's enough to warn. We don't need to prove the squashed
# commits had it — the convention is ALL commits must have it.
printf '\n' >&2
printf '[coauthor-squash-detect] WARNING: squash at %s dropped Co-Authored-By: Nika 🦋\n' "$HEAD_SHA" >&2
printf '  The squash message does not contain Nika attribution.\n' >&2
printf '  Fix: git commit --amend (if unpublished) and add:\n' >&2
printf '    Co-Authored-By: Nika 🦋 <nika@supernovae.studio>\n\n' >&2

exit 0
