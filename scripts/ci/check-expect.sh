#!/usr/bin/env bash
# Ratchet: zero `.expect(` calls in production src/.
#
# "Production" = lines outside `#[cfg(test)]` / `#[test]` items, in any
# `*/src/*.rs` file whose basename is not `tests.rs`. Mirrors clippy's
# `expect_used` lint scoping. This script promotes the workspace-level
# `expect_used = "warn"` to a hard block on this branch.
#
# ONE RULE, ONE READER (#1207). Until 2026-08-25 this gate carried a private
# python re-implementation of the test-item scope. It read `#[cfg(test)]` as
# an attribute waiting for a brace, so a declaration —
#
#     #[cfg(test)]
#     mod tests;
#
# — left the flag pending and the NEXT block became the "test" region. A
# production `.expect(` inside that block was then invisible. Verified on two
# fixtures differing only in the module's shape: the shipped reader saw 0
# where the shared filter saw 1. Silent direction: it does not raise a false
# finding, it stops reporting real ones.
#
# The delta over the tree at the time of the change was ZERO (782 files, 0 vs
# 0), so nothing was leaking. It was a latent hole in a load-bearing ratchet,
# and the remedy is not a second patched copy — it is to stop having copies.
# `_lib.sh::strip_test_items` already ends a pending item at `;` and already
# blanks string, raw-string and char literals before counting braces, lessons
# the private copies never inherited. `scripts/ci/check-unwrap.sh` has read
# through it all along; this file now does the same, and its behaviour is
# pinned by `test-strip-test-items.sh`, which the shared filter's readers run
# before they judge anything.
#
# Exit codes: 0 — OK · 1 — RED (production .expect( found)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"

# Pre-existing violations (Batch H+ 2026-04-15). Each entry names a file the
# ratchet skips until it is fixed; the reasons live beside the entries. Note
# that several exist because `strip_test_items` cannot parse
# `#[cfg(all(test, ...))]` — a documented limitation of the shared filter, so
# reading through it does not change what those entries are for.
ALLOWLIST_FILE="$HERE/allowlist-expect.conf"
_allowed_files=""
if [ -f "$ALLOWLIST_FILE" ]; then
  _allowed_files="$(grep -v '^#' "$ALLOWLIST_FILE" | grep -v '^$' || true)"
fi

total=0
found_any=0
while IFS= read -r f; do
  [ -z "$f" ] && continue
  if [ -n "$_allowed_files" ] && printf '%s\n' "$_allowed_files" | grep -qF "$f"; then
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
