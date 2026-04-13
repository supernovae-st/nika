#!/usr/bin/env bash
# Ratchet: zero `.expect(` calls in production src/.
#
# "Production" = lines outside `#[cfg(test)]` / `#[test]` items, in any
# `*/src/*.rs` file whose basename is not `tests.rs`. Mirrors clippy's
# `expect_used` lint scoping. This script promotes the workspace-level
# `expect_used = "warn"` to a hard block on the `nika-diamond` branch.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"

total=0
found_any=0
while IFS= read -r f; do
  [ -z "$f" ] && continue
  hits=$(strip_test_items "$f" | grep -nE '\.expect\(' || true)
  if [ -n "$hits" ]; then
    found_any=1
    printf '%s\n' "$hits" | sed "s|^|$f:|"
    total=$((total + $(printf '%s\n' "$hits" | wc -l | tr -d ' ')))
  fi
done < <(rs_prod_files)

if [ "$found_any" -eq 1 ]; then
  printf '\nFAIL  %d .expect( call(s) in production src/\n' "$total" >&2
  exit 1
fi

echo "OK  zero .expect( in production src/"
