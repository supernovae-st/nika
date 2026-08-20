#!/usr/bin/env bash
# The estate gate, split by what each verdict MEANS.
#
# WHY THIS SPLIT EXISTS · `estate.yaml` is a WHOLE-TREE projection. Any
# commit that touches any tracked file rewrites it, so two branches that
# share no source file still collide there. Measured 2026-08-20: four
# pull requests, four conflicts, every one of them on this file and only
# this file — `git merge-tree` reported nine source files with zero
# collisions and one "changed in both", the projection. The pre-commit
# hook REQUIRED the delta that guaranteed the collision, while the CI job
# that would have caught real drift ran `continue-on-error: true`. The
# refusal and the enforcement had swapped places.
#
# Freshness (exit 5) is therefore no longer a commit-time refusal. It is
# enforced where it is the actual guarantee: at TAG time, in
# `.github/workflows/release.yml`, beside the version-match guard. A
# released artifact carries a true manifest; an intermediate commit
# never needed to.
#
# Coverage (exit 3 · a path no rule classifies, or a rule matching
# nothing) STAYS blocking, here and in CI. It is the half a stale
# manifest cannot hide, it is author-fixable at the moment it appears,
# and it is the reason the manifest exists at all.
#
# Exit: 0 the commit may proceed · 1 a coverage hole the author must fix.
set -uo pipefail

out="$(python3 scripts/estate.py --check 2>&1)"
rc=$?

case "$rc" in
  0)
    exit 0
    ;;
  5)
    # Freshness only. Say so, do not refuse: the tag gate owns this.
    printf 'estate: the manifest is behind the tree (freshness only).\n' >&2
    printf '  Nothing to do — it is regenerated and PROVEN at tag time\n' >&2
    printf '  (RELEASING.md step 2 · release.yml refuses a tag whose\n' >&2
    printf '  manifest does not match). Run `python3 scripts/estate.py\n' >&2
    printf '  --write` yourself only if you WANT this commit to carry it.\n' >&2
    exit 0
    ;;
  3)
    printf 'estate: COVERAGE HOLE — a path no rule classifies, or a rule\n' >&2
    printf 'that now matches nothing. This is not freshness; the manifest\n' >&2
    printf 'cannot describe the tree until a rule covers it.\n\n' >&2
    printf '%s\n\n' "$out" >&2
    printf 'Fix: add or repair the glob in scripts/estate_rules.py.\n' >&2
    exit 1
    ;;
  *)
    printf 'estate: the tool exited %s — an unrecognised verdict is not a\n' "$rc" >&2
    printf 'pass. Refusing rather than guessing which half failed.\n\n' >&2
    printf '%s\n' "$out" >&2
    exit 1
    ;;
esac
