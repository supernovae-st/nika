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
readonly ADR_PATTERN='docs/adr/ADR-0(0[1-9]|1[0-5])'
readonly SEALED_COUNT=15

warn_adr_modified() {
  local files="$1"
  printf '\n' >&2
  printf '╔══════════════════════════════════════════════════════════╗\n' >&2
  printf '║  [adr-seal-check] WARNING: SEALED ADR MODIFIED          ║\n' >&2
  printf '╚══════════════════════════════════════════════════════════╝\n' >&2
  printf '\nADR-001..%03d are SEALED (Accepted status). Modified:\n' "$SEALED_COUNT" >&2
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
      | grep -E "$ADR_PATTERN" || true)"
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
          | grep -E "$ADR_PATTERN")"$'\n'
      fi
    done
    if [[ -n "$MODIFIED" ]]; then
      warn_adr_modified "$MODIFIED"
    fi
    ;;
esac

exit 0 # always exit 0 — post-hook cannot block completed operation
