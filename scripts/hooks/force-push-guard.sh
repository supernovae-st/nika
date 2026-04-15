#!/usr/bin/env bash
# force-push-guard.sh — Tier 2 pre-push gate
#
# Reads the push refspec from stdin (standard pre-push protocol).
# Refuses force-pushes to protected branches unless FORCE_PUSH_OVERRIDE=1.
#
# Protected branches: nika-diamond, main
# Override: FORCE_PUSH_OVERRIDE=1 git push --force (requires explicit set)
#
# Format from git pre-push hook stdin:
#   <local-ref> <local-sha> <remote-ref> <remote-sha>
#   (remote-sha is 0000000... when remote branch doesn't exist yet)
#
# Exit: 0 = allowed | 1 = blocked
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -Eeuo pipefail

readonly PROTECTED_BRANCHES=('nika-diamond' 'main')
readonly ZERO_SHA='0000000000000000000000000000000000000000'

# Check override first
if [[ "${FORCE_PUSH_OVERRIDE:-0}" == '1' ]]; then
  printf '[force-push-guard] FORCE_PUSH_OVERRIDE=1 set — bypass active (USE WITH CARE)\n' >&2
  exit 0
fi

# Read pre-push stdin
while IFS=' ' read -r local_ref local_sha remote_ref remote_sha; do
  [[ -z "$local_ref" ]] && continue

  # Extract branch name from remote ref (refs/heads/name)
  branch="${remote_ref#refs/heads/}"

  # Check if target is a protected branch
  for protected in "${PROTECTED_BRANCHES[@]}"; do
    if [[ "$branch" == "$protected" ]]; then
      # Detect force-push: remote has a SHA that is NOT an ancestor of local
      # (git will only show non-zero remote SHA when the branch exists)
      if [[ "$remote_sha" != "$ZERO_SHA" && "$remote_sha" != '' ]]; then
        # Check if remote SHA is ancestor of local SHA
        if ! git merge-base --is-ancestor "$remote_sha" "$local_sha" 2>/dev/null; then
          printf '\n[force-push-guard] BLOCKED — force-push to protected branch "%s"\n' "$branch" >&2
          printf '  remote: %s\n' "${remote_sha:0:12}" >&2
          printf '  local:  %s\n' "${local_sha:0:12}" >&2
          printf '\nTo override (requires explicit approval):\n' >&2
          printf '  FORCE_PUSH_OVERRIDE=1 git push --force\n\n' >&2
          exit 1
        fi
      fi
      break
    fi
  done
done

exit 0
