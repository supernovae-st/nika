#!/usr/bin/env bash
# Vector 8: GitHub milestones exist and are not stale (no v1.x contradicting forever-v0.x).
set -u
if ! command -v gh >/dev/null; then echo "gh not installed"; exit 1; fi

stale="$(gh api repos/supernovae-st/nika/milestones --jq '[.[] | select(.title | test("v1\\.|^Production"))] | length' 2>/dev/null || echo "?")"
if [ "$stale" != "0" ] && [ "$stale" != "" ]; then
  echo "$stale legacy milestones present"; exit 2
fi
count="$(gh api repos/supernovae-st/nika/milestones --jq '. | length' 2>/dev/null || echo 0)"
echo "OK ($count milestones)"; exit 0
