#!/usr/bin/env bash
# adr-seal-check.sh — Tier 4 post-merge/post-rewrite warning
#
# Sealed ADRs (docs/adr/ADR-001 through ADR-015) are read-only after acceptance.
# A rebase or merge that modifies them is almost always a mistake.
#
# This script warns loudly (stderr) but does NOT block (exit 0 always) because
# post-merge/post-rewrite hooks cannot veto the operation after it completes.
#
# Usage:
#   adr-seal-check.sh post-merge
#   adr-seal-check.sh post-rewrite
#
# Context detection:
#   post-merge:   checks ORIG_HEAD..HEAD for ADR modifications
#   post-rewrite: reads rewritten-list from stdin (git format)
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -uo pipefail

readonly CONTEXT="${1:-post-merge}"

# The scope DERIVES, it is never typed.
#
# Until 2026-08-13 this read:
#   ADR_PATTERN='docs/adr/ADR-0(0[1-9]|1[0-5])'   SEALED_COUNT=15
# Two hand-typed constants, and the guard had never warned once. Measured:
#   the uppercase pattern matched  0  of the 73 tracked ADR files
#   the same pattern in lowercase  15 — exactly SEALED_COUNT
# The files are named `adr-001-…md`. The guard was written for a spelling
# that does not exist here, so it protected nothing, ever — a gate whose
# own prose claimed a seal it could not enforce.
#
# The range was wrong too, independently: 59 ADRs carry `Accepted` today,
# not 15. Both defects have one cause — a scope typed once instead of read
# from the thing it describes. So the pattern now matches ANY ADR file
# (either spelling), and `sealed_only` keeps the ones whose own Status says
# Accepted. Adding an ADR, accepting one, or renaming the directory's case
# cannot silence this guard again.
readonly ADR_PATTERN='docs/adr/[Aa][Dd][Rr]-[0-9]{3}-'

# Read the seal from each file rather than from a number kept in this script.
sealed_only() {
  local f
  while IFS= read -r f; do
    [[ -z "$f" ]] && continue
    # The file may be gone (a deletion is still a modification worth naming).
    [[ -f "$f" ]] || {
      printf '%s\n' "$f"
      continue
    }
    grep -qiE '^[[:space:]]*(\*\*)?status(\*\*)?[[:space:]]*:?.*accepted' "$f" \
      && printf '%s\n' "$f"
  done
  return 0
}

warn_adr_modified() {
  local files="$1"
  printf '\n' >&2
  printf '╔══════════════════════════════════════════════════════════╗\n' >&2
  printf '║  [adr-seal-check] WARNING: SEALED ADR MODIFIED          ║\n' >&2
  printf '╚══════════════════════════════════════════════════════════╝\n' >&2
  printf '\nThese ADRs carry Status: Accepted — they are SEALED. Modified:\n' >&2
  printf '  %s\n' "$files" >&2
  printf '\nIf this was intentional (rare — architecture reversal):\n' >&2
  printf '  1. Create a new ADR-NNN superseding the old one\n' >&2
  printf '  2. Update old ADR status to "Superseded by ADR-NNN"\n' >&2
  printf '  3. Never edit the decision text of a sealed ADR\n' >&2
  printf '\n' >&2
}

case "$CONTEXT" in
  post-merge)
    if ! git rev-parse ORIG_HEAD >/dev/null 2>&1; then
      exit 0 # No ORIG_HEAD (first commit, initial clone, etc.)
    fi
    MODIFIED="$(git diff --name-only ORIG_HEAD..HEAD 2>/dev/null \
      | grep -E "$ADR_PATTERN" | sealed_only || true)"
    if [[ -n "$MODIFIED" ]]; then
      warn_adr_modified "$MODIFIED"
    fi
    ;;

  post-rewrite)
    # git passes rewritten pairs via stdin: old-sha new-sha
    MODIFIED=''
    while IFS=' ' read -r old_sha new_sha _rest; do
      [[ -z "$old_sha" ]] && continue
      if git diff --name-only "${old_sha}..${new_sha}" 2>/dev/null \
        | grep -qE "$ADR_PATTERN"; then
        MODIFIED="${MODIFIED}$(git diff --name-only "${old_sha}..${new_sha}" 2>/dev/null \
          | grep -E "$ADR_PATTERN" | sealed_only)"$'\n'
      fi
    done
    if [[ -n "$MODIFIED" ]]; then
      warn_adr_modified "$MODIFIED"
    fi
    ;;
esac

exit 0 # always exit 0 — post-hook cannot block completed operation
