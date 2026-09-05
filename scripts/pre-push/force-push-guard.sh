#!/usr/bin/env bash
# force-push-guard.sh — Tier 2 pre-push gate
#
# Refuses force-pushes to protected branches unless FORCE_PUSH_OVERRIDE=1.
#
# The Lefthook script job forwards git's native pre-push stdin refspec via
# use_stdin, so the actual proposed refs take precedence over local state:
#     <local-ref> <local-sha> <remote-ref> <remote-sha>
# Direct invocation without refs falls back to the current branch's remote
# ref only; that narrower fallback cannot judge another branch's proposal.
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
#
# `! -t 0` says "stdin is not a terminal". That is true of git's refspec
# pipe, of /dev/null, AND of an inherited socket, and those three want
# three different behaviours. Two consequences were live:
#   · /dev/null took this branch, read EOF at once and `exit 0` — the
#     lefthook path never reached the derive-from-state check below, so
#     the guard has been guarding NOTHING whenever lefthook ran it.
#   · an inherited socket (a non-TTY push, e.g. an agent shell) takes it
#     too and never sees EOF, so the loop blocks forever and no push can
#     leave that machine.
# Bounding the read separates them: real refspec lines arrive at once,
# and a silent stdin falls through to the state-derived check instead of
# hanging or waving the push past unexamined.
if [[ ! -t 0 ]]; then
  saw_refspec=0
  while IFS=' ' read -r -t 5 _local_ref local_sha remote_ref remote_sha; do
    [[ -z "$_local_ref" ]] && continue
    saw_refspec=1
    branch="${remote_ref#refs/heads/}"
    check_force_push "$branch" "$local_sha" "$remote_sha" || exit 1
  done
  [[ "$saw_refspec" -eq 1 ]] && exit 0
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
