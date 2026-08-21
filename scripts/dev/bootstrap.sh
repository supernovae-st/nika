#!/usr/bin/env bash
# bootstrap.sh — arm THIS clone's local enforcement. One gesture, idempotent.
#
# WHY A GESTURE AT ALL
# --------------------
# `.gitattributes` carried the line « a fix that needs a per-clone gesture is
# not a fix ». The principle is right and the claim was not reachable: git
# offers no way to ship config into a clone, and no hook runs before the merge
# machinery reads `.gitattributes`. A clone therefore cannot arm itself.
#
# What IS reachable is this: make the gesture ONE, make it idempotent, and make
# its absence LOUD (`scripts/hygiene/check-clone-armed.sh`, vector 51). That is
# the honest version of the principle.
#
# WHAT IT ARMS
#   · merge.ours.driver=true  — without it, `/estate.yaml merge=ours` is
#     silently ignored and you get the meaningless aggregate-hash conflict
#   · lefthook                — the local gates declared in lefthook.yml
#
# Safe to re-run. Reports what it changed and what was already in place.
set -uo pipefail

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo .)" || exit 1

armed=0
already=0

# 1 · the estate merge driver
if [ "$(git config --get merge.ours.driver 2>/dev/null || true)" = "true" ]; then
  echo "  = merge.ours.driver     already true"
  already=$((already + 1))
else
  git config merge.ours.driver true
  echo "  + merge.ours.driver     registered (estate.yaml keeps ONE real manifest)"
  armed=$((armed + 1))
fi

# 2 · the local gates
declared=$(grep -cE '^\s+run:' lefthook.yml 2>/dev/null || echo 0)
if ! command -v lefthook >/dev/null 2>&1; then
  echo "  ! lefthook              NOT INSTALLED — ${declared} local gates stay inert"
  echo "                          install it, then re-run this script"
else
  hooks_path=$(git config --get core.hooksPath 2>/dev/null || true)
  if { [ -n "$hooks_path" ] && [ -e "$hooks_path/pre-commit" ]; } || [ -f .git/hooks/pre-commit ]; then
    echo "  = lefthook              already installed (${declared} gates reachable)"
    already=$((already + 1))
  elif lefthook install >/dev/null 2>&1; then
    echo "  + lefthook              installed (${declared} gates now reachable)"
    armed=$((armed + 1))
  else
    echo "  ! lefthook install      FAILED — ${declared} gates stay inert"
  fi
fi

echo
echo "armed ${armed} · already in place ${already}"
echo "verify · bash scripts/hygiene/check-clone-armed.sh"
echo
echo "note · a reachable hook is not a proven run. This script registers the"
echo "       path; only a commit proves the gate fires."
