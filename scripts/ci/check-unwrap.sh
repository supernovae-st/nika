#!/usr/bin/env bash
# Ratchet: zero `.unwrap()` calls in production src/.
#
# "Production" = lines outside `#[cfg(test)]` / `#[test]` items, in any
# `*/src/*.rs` file whose basename is not `tests.rs`. This mirrors clippy's
# `unwrap_used` lint scoping. The cargo-driven `check-clippy.sh` is the
# canonical enforcer once the workspace has members; this script gives the
# same answer faster, and works during the early scaffold phase.
#
# See CONSTELLATION_PLAN §7 criterion 6, feedback_zero_unwrap_policy.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"

total=0
found_any=0
while IFS= read -r f; do
  [ -z "$f" ] && continue
  hits=$(strip_test_items "$f" | grep -nE '\.unwrap\(\)' || true)
  if [ -n "$hits" ]; then
    found_any=1
    printf '%s\n' "$hits" | sed "s|^|$f:|"
    total=$((total + $(printf '%s\n' "$hits" | wc -l | tr -d ' ')))
  fi
done < <(rs_prod_files)

if [ "$found_any" -eq 1 ]; then
  printf '\nFAIL  %d .unwrap() call(s) in production src/\n' "$total" >&2
  exit 1
fi

echo "OK  zero .unwrap() in production src/"
