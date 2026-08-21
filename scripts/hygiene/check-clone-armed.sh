#!/usr/bin/env bash
# check-clone-armed.sh — is THIS clone's local enforcement actually armed?
#
# WHY THIS EXISTS
# ---------------
# `.gitattributes` marks `/estate.yaml merge=ours` because a textual merge of
# two aggregate manifests describes a tree that never existed. That mark only
# does anything when `merge.ours.driver` is registered in the clone's config —
# and git IGNORES AN UNREGISTERED DRIVER IN SILENCE. It falls back to a plain
# three-way merge and hands you the meaningless conflict the mark exists to
# prevent, with no message naming the cause.
#
# The driver is registered by `scripts/hooks/estate-gate.sh`, which runs from
# `lefthook.yml`, which requires `lefthook install`. In a clone where that
# gesture was never made, every local gate is inert and NOTHING says so:
# measured 2026-08-21 on a working loose clone — 20 gates declared, zero
# installed, `.git/hooks/` holding only samples.
#
# So this vector does the one thing the silent path cannot: it makes the
# unarmed state VISIBLE, where a contributor already looks.
#
# WHAT IT LOOKS AT (per gate-honesty Mandate 5 — an instrument says what it
# examined, what it skipped, and how it goes red)
#   · merge.ours.driver in the effective git config
#   · whether a real pre-commit hook is reachable (core.hooksPath, or a
#     non-sample .git/hooks/pre-commit)
# WHAT IT CANNOT SEE
#   · whether the hook, once installed, actually FIRES. A registered path is
#     not a proven run. That is the arming-edge limit, and it is stated rather
#     than papered over.
#
# EXIT · 0 armed · 1 unarmed (yellow — local state, never a repo defect)
set -uo pipefail

cd "$(git rev-parse --show-toplevel 2>/dev/null || echo .)" || exit 1

missing=()

driver=$(git config --get merge.ours.driver 2>/dev/null || true)
[ "$driver" = "true" ] || missing+=("merge.ours.driver")

hooks_path=$(git config --get core.hooksPath 2>/dev/null || true)
hook_ok=0
if [ -n "$hooks_path" ] && [ -e "$hooks_path/pre-commit" ]; then
  hook_ok=1
elif [ -f .git/hooks/pre-commit ]; then
  # a `.sample` is git's stock file; only a real one counts
  hook_ok=1
fi
[ "$hook_ok" -eq 1 ] || missing+=("pre-commit hook")

declared=$(grep -cE '^\s+run:' lefthook.yml 2>/dev/null || echo 0)

if [ ${#missing[@]} -eq 0 ]; then
  echo "armed · merge.ours.driver + pre-commit reachable (${declared} local gates declared · a reachable hook is not a proven run)"
  exit 0
fi

printf 'UNARMED · %s absent — ' "$(
  IFS=', '
  echo "${missing[*]}"
)"
printf '%s local gates in lefthook.yml are inert here, and an unregistered ' "$declared"
printf 'merge driver fails SILENTLY on estate.yaml. One gesture: bash scripts/dev/bootstrap.sh\n'
exit 1
