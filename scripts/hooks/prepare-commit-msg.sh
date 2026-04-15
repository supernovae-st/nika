#!/usr/bin/env bash
# prepare-commit-msg.sh — auto-inject Nika co-author trailer
#
# Appends the mandatory Co-Authored-By trailer to every commit message
# that doesn't already contain it. Prevents validation failures from
# validate-conventional-commit.sh (gate 7) and removes the need for
# manual --trailer flags or commit templates.
#
# Skips: merge commits, fixup/squash, messages that already contain it.
#
# Usage: lefthook prepare-commit-msg hook, or:
#   prepare-commit-msg.sh <commit-msg-file> [<commit-source>] [<sha>]
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -uo pipefail

MSG_FILE="${1:-}"
COMMIT_SOURCE="${2:-}"

[[ -n "$MSG_FILE" && -f "$MSG_FILE" ]] || exit 0

# Skip merge/squash/fixup/amend commits — git populates their messages
# from a template and the user may not want auto-injection.
case "$COMMIT_SOURCE" in
  merge | squash) exit 0 ;;
esac

readonly TRAILER='Co-Authored-By: Nika 🦋 <nika@supernovae.studio>'

# Already present — nothing to do.
if grep -qF "$TRAILER" "$MSG_FILE" 2>/dev/null; then
  exit 0
fi

# Append trailer with blank line separator if needed.
# Check if file ends with a newline already.
if [[ -s "$MSG_FILE" ]]; then
  last_char="$(tail -c1 "$MSG_FILE" | xxd -p)"
  if [[ "$last_char" != "0a" ]]; then
    printf '\n' >>"$MSG_FILE"
  fi
  # Ensure blank line before trailer (git convention)
  last_line="$(tail -1 "$MSG_FILE")"
  if [[ -n "$last_line" ]]; then
    printf '\n' >>"$MSG_FILE"
  fi
fi

printf '%s\n' "$TRAILER" >>"$MSG_FILE"
