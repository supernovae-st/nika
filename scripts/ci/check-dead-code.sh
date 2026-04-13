#!/usr/bin/env bash
# Ratchet: zero `#[allow(dead_code)]` in production src/.
# Dead code must be deleted, not hidden. See deletion-first.md.
#
# "Production" = lines outside `#[cfg(test)]` / `#[test]` items, in any
# `*/src/*.rs` file whose basename is not `tests.rs`. A `#[allow(dead_code)]`
# inside `#[cfg(test)]` is harmless and intentionally permitted.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"

total=0
found_any=0
while IFS= read -r f; do
  [ -z "$f" ] && continue
  hits=$(strip_test_items "$f" | grep -nE '#\[allow\(dead_code\)\]' || true)
  if [ -n "$hits" ]; then
    found_any=1
    printf '%s\n' "$hits" | sed "s|^|$f:|"
    total=$((total + $(printf '%s\n' "$hits" | wc -l | tr -d ' ')))
  fi
done < <(rs_prod_files)

if [ "$found_any" -eq 1 ]; then
  printf '\nFAIL  %d #[allow(dead_code)] in production src/ (delete the code instead)\n' "$total" >&2
  exit 1
fi

echo "OK  zero #[allow(dead_code)] in production src/"
