#!/usr/bin/env bash
# Vector 8: the milestones exist and none carries a retired framing.
#
# Three repairs, 2026-08-02, all the same class — a check reporting a
# verdict it could not justify:
#
# 1. PAGE. The API answers OPEN milestones, thirty per page, by default.
#    The check read 5 of this repo's 11, so a closed milestone was
#    invisible to the one thing looking for it. `?state=all&per_page=100`.
#
# 2. FAILURE. `|| echo "?"` fed a question mark into a numeric test,
#    `"?" != "0"` held, and a network blip filed a drift issue naming
#    milestones nobody had read. Three outcomes now, named apart: none
#    present · some present · could not ask.
#
# 3. PREMISE. The old rule was « no v1.x, it contradicts forever-v0.x ».
#    That doctrine was RETIRED (D-2026-06-20-N1 · real semver toward
#    1.0), and 1.0.0 is now the target the open milestones name. The
#    check would have gone red on the roadmap's own destination. What is
#    retired is the forever-v0.x framing itself, so that is what it
#    looks for.
set -u

if ! command -v gh >/dev/null; then
  echo "gh not installed — cannot ask"
  exit 1
fi

if ! titles="$(gh api 'repos/supernovae-st/nika/milestones?state=all&per_page=100' \
  --jq '.[].title' 2>/dev/null)"; then
  echo "YELLOW: the milestones API could not be reached — this is not a clean bill"
  exit 1
fi

count="$(printf '%s\n' "$titles" | grep -c . || true)"
if [ "$count" -eq 0 ]; then
  echo "YELLOW: no milestones at all — the roadmap has no gates on GitHub"
  exit 1
fi

# The retired framings. `forever v0.x` and its spellings; NOT `v1.x`,
# which is where the roadmap is going.
stale="$(printf '%s\n' "$titles" | grep -iE 'forever[ -]?v?0|no v1\.0|v0\.x forever' || true)"
if [ -n "$stale" ]; then
  printf 'RED: milestone(s) carrying the retired forever-v0.x framing:\n'
  printf '%s\n' "$stale" | sed 's/^/    /'
  printf '  (superseded by D-2026-06-20-N1 · real semver toward 1.0)\n'
  exit 2
fi

echo "OK ($count milestone(s), open and closed, no retired framing)"
exit 0
