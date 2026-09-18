#!/usr/bin/env bash
# Vector 52: live on-disk Nika programs are `*.nika`. Retired
# `.nika.yaml` / `.nika.yml` must not reappear in tracked pathnames or
# live text. Exceptions are explicit: path prefix + category + reason.
#
# Exit: 0 green · 2 red.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

ALLOW="$REPO_ROOT/scripts/hygiene/source-naming-allowlist.tsv"
PATTERN='\.nika\.ya?ml'

is_allowed() {
  local rel="$1"
  [ -f "$ALLOW" ] || return 1
  while IFS=$'\t' read -r prefix category reason owner; do
    [ -z "${prefix:-}" ] && continue
    case "$prefix" in
      \#*) continue ;;
    esac
    case "$rel" in
      "$prefix"|"$prefix"/*|"$prefix"*)
        return 0
        ;;
    esac
  done <"$ALLOW"
  return 1
}

fail=0
hits=()

while IFS= read -r rel; do
  [ -z "$rel" ] && continue
  case "$rel" in
    *.nika.yaml|*.nika.yml)
      if ! is_allowed "$rel"; then
        hits+=("path $rel")
        fail=1
      fi
      ;;
  esac
done < <(git ls-files)

while IFS= read -r rel; do
  [ -z "$rel" ] && continue
  is_allowed "$rel" && continue
  hits+=("text $rel")
  fail=1
done < <(git grep -l -I -E "$PATTERN" -- . 2>/dev/null || true)

if [ "$fail" -ne 0 ]; then
  echo "RED source-naming: retired .nika.yaml/.nika.yml on the live surface:"
  for h in "${hits[@]}"; do
    echo "  $h"
  done
  echo "  add a bounded allowlist row (path + category + reason) only for historical, frozen, negative-test or ratchet data"
  exit 2
fi

echo "OK source-naming: no unallowlisted retired program suffix"
exit 0
