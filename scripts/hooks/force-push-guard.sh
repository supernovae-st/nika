#!/usr/bin/env bash
# force-push-guard.sh — Tier 2 pre-push gate
#
# Refuses force-pushes to protected branches unless FORCE_PUSH_OVERRIDE=1.
#
# NOTE: Lefthook v2 does not forward git's native pre-push stdin refspec
# to hooks, so we can't rely on the standard
#     <local-ref> <local-sha> <remote-ref> <remote-sha>
# protocol. Instead we read from stdin when it IS a pipe (direct git hook)
# AND fall back to computing state from the current branch's remote ref
# when stdin is /dev/null (lefthook-invoked).
#
# Protected branches: nika-diamond, main
# Override: FORCE_PUSH_OVERRIDE=1 git push --force
#
# Exit: 0 = allowed | 1 = blocked
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -Eeuo pipefail

readonly PROTECTED_BRANCHES=('nika-diamond' 'main')
readonly ZERO_SHA='0000000000000000000000000000000000000000'

if [[ "${FORCE_PUSH_OVERRIDE:-0}" == '1' ]]; then
  printf '[force-push-guard] FORCE_PUSH_OVERRIDE=1 set — bypass active (USE WITH CARE)\n' >&2
  exit 0
fi

check_force_push() {
  local branch="$1" local_sha="$2" remote_sha="$3"
  local is_protected=0
  for protected in "${PROTECTED_BRANCHES[@]}"; do
    [[ "$branch" == "$protected" ]] && is_protected=1
  done
  [[ "$is_protected" -eq 0 ]] && return 0
  [[ "$remote_sha" == "$ZERO_SHA" || -z "$remote_sha" ]] && return 0

  if ! git merge-base --is-ancestor "$remote_sha" "$local_sha" 2>/dev/null; then
    printf '\n[force-push-guard] BLOCKED — force-push to protected branch "%s"\n' "$branch" >&2
    printf '  remote: %s\n' "${remote_sha:0:12}" >&2
    printf '  local:  %s\n' "${local_sha:0:12}" >&2
    printf '\nTo override (requires explicit approval):\n' >&2
    printf '  FORCE_PUSH_OVERRIDE=1 git push --force\n\n' >&2
    return 1
  fi
  return 0
}

# P1-3 Batch H+: the previous check used `-p /dev/stdin` which tests for a
# named pipe. On macOS, /dev/stdin is a character device (not a pipe), so
# the test always failed even when git provided real stdin data. The correct
# portable test is simply `! -t 0` — "stdin is not a terminal" — which is
# true for both pipes and redirections on all platforms.
if [[ ! -t 0 ]]; then
  while IFS=' ' read -r _local_ref local_sha remote_ref remote_sha; do
    [[ -z "$_local_ref" ]] && continue
    branch="${remote_ref#refs/heads/}"
    check_force_push "$branch" "$local_sha" "$remote_sha" || exit 1
  done
  exit 0
fi

# Lefthook-invoked path (stdin is /dev/null): derive branch + remote sha
# from the current repo state. Only protects the currently-checked-out
# branch when pushed to its tracking remote — the common case.
current_branch="$(git symbolic-ref --short HEAD 2>/dev/null || true)"
[[ -z "$current_branch" ]] && exit 0

local_sha="$(git rev-parse HEAD 2>/dev/null || true)"
remote_sha="$(git rev-parse "origin/${current_branch}" 2>/dev/null || echo "$ZERO_SHA")"

check_force_push "$current_branch" "$local_sha" "$remote_sha" || exit 1
exit 0
