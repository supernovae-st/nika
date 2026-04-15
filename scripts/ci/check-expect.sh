#!/usr/bin/env bash
# shellcheck disable=SC1091  # _lib.sh sourced at runtime
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

# Load allowlist (P1-2 Batch H+: pre-existing violations surfaced by errexit fix)
ALLOWLIST_FILE="$HERE/allowlist-expect.conf"
_allowed_files=""
if [[ -f "$ALLOWLIST_FILE" ]]; then
  _allowed_files="$(grep -v '^#' "$ALLOWLIST_FILE" | grep -v '^$' || true)"
fi

total=0
found_any=0
while IFS= read -r f; do
  [ -z "$f" ] && continue
  # Skip allowlisted files
  if [ -n "$_allowed_files" ] && echo "$_allowed_files" | grep -qF "$f"; then
    continue
  fi
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
