#!/usr/bin/env bash
# Ratchet: zero `.unwrap()` calls in production src/.
# See CONSTELLATION_PLAN §7 criterion 6, feedback_zero_unwrap_policy.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"

total=0
found_any=0
while IFS= read -r f; do
  [ -z "$f" ] && continue
  hits=$(grep -nE '\.unwrap\(\)' "$f" || true)
  if [ -n "$hits" ]; then
    found_any=1
    printf '%s\n' "$hits" | sed "s|^|$f:|"
    total=$((total + $(printf '%s\n' "$hits" | wc -l | tr -d ' ')))
  fi
done < <(rs_src_files)

if [ "$found_any" -eq 1 ]; then
  printf '\nFAIL  %d .unwrap() call(s) in src/\n' "$total" >&2
  exit 1
fi

echo "OK  zero .unwrap() in src/"
