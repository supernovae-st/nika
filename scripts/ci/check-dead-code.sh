#!/usr/bin/env bash
# Ratchet: zero `#[allow(dead_code)]` in src/.
# Dead code must be deleted, not hidden. See deletion-first.md.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"

total=0
found_any=0
while IFS= read -r f; do
  [ -z "$f" ] && continue
  hits=$(grep -nE '#\[allow\(dead_code\)\]' "$f" || true)
  if [ -n "$hits" ]; then
    found_any=1
    printf '%s\n' "$hits" | sed "s|^|$f:|"
    total=$((total + $(printf '%s\n' "$hits" | wc -l | tr -d ' ')))
  fi
done < <(rs_src_files)

if [ "$found_any" -eq 1 ]; then
  printf '\nFAIL  %d #[allow(dead_code)] in src/ (delete the code instead)\n' "$total" >&2
  exit 1
fi

echo "OK  zero #[allow(dead_code)] in src/"
