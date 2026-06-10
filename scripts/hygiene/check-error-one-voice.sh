#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# Vector 37: error one-voice enforcement.
#
# The "error one-voice" arc made every workspace error enum speak the single
# canonical NikaErrorCode trait (nika_error / nika_kernel::prelude) — each
# variant maps to a central registry code (NIKA-XXXX). The completeness record
# is docs/architecture/error-trait-completeness-2026-06-10.md. Until now it was
# hand-maintained: a NEW error enum could ship without the trait and nothing
# caught it. This gate closes that — the doctrine is now structural.
#
# INVARIANT: every thiserror error enum (one with #[error(...)] variant attrs)
# in an admitted crate's NON-TEST src MUST impl NikaErrorCode — UNLESS it is in
# the documented exemption allowlist (scripts/ci/error-one-voice-allowlist.tsv,
# the SSOT projected from the audit table's exemption classes: build-time tool,
# wrapped intermediate, deferred-with-trigger).
#
# A new un-allowlisted error enum without the trait → RED. Orphan-allowlist
# check: every allowlist row must name a real enum that genuinely lacks the
# trait (so the allowlist can't silently rot).
#
# Exit: 0 green · 2 red (an enum violates the invariant, or a stale allowlist).
set -uo pipefail

ENGINE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ENGINE_ROOT" || exit 2

# shellcheck source=../ci/_lib.sh
. "$ENGINE_ROOT/scripts/ci/_lib.sh"

ALLOWLIST="scripts/ci/error-one-voice-allowlist.tsv"
[ -f "$ALLOWLIST" ] || {
  echo "error-one-voice allowlist missing: $ALLOWLIST"
  exit 2
}

# allowlisted <enum> <crate> → 0 if a row exists.
is_allowlisted() {
  grep -vE '^[[:space:]]*#' "$ALLOWLIST" \
    | awk -F'\t' -v e="$1" -v c="$2" 'NF>=2 && $1==e && $2==c { found=1 } END { exit found?0:1 }'
}

# workspace members (basenames).
members="$(workspace_members)"

# Does crate <c> impl NikaErrorCode for enum <e> (anywhere in its src)?
impls_trait() {
  grep -rqE "NikaErrorCode for $1\b" "crates/$2/src" 2>/dev/null
}

red=0
RED_MSGS=()
checked=0

# --- forward: every thiserror enum in admitted-crate non-test src is covered -
while IFS= read -r crate; do
  [ -z "$crate" ] && continue
  while IFS= read -r f; do
    [ -f "$f" ] || continue
    # thiserror enums = enums that carry at least one #[error(...)] variant
    # attribute, in the non-test region (strip_test_items blanks test items).
    enums="$(strip_test_items "$f" | awk '
      /pub enum [A-Z][A-Za-z0-9]*/ { name=$0; sub(/.*pub enum /,"",name); sub(/[^A-Za-z0-9].*/,"",name); inenum=name }
      /#\[error\(/ { if (inenum != "") { print inenum; inenum="" } }
    ')"
    for e in $enums; do
      checked=$((checked + 1))
      if impls_trait "$e" "$crate"; then
        continue
      fi
      if is_allowlisted "$e" "$crate"; then
        continue
      fi
      RED_MSGS+=("$crate::$e ($f) is a thiserror error enum but does NOT impl NikaErrorCode and is NOT in $ALLOWLIST — implement the trait (error one-voice) or add a documented exemption row.")
      red=1
    done
  done < <(find "crates/$crate/src" -name '*.rs' 2>/dev/null)
done <<EOF
$members
EOF

# --- orphan-allowlist: every row names a real enum that lacks the trait ------
while IFS=$'\t' read -r e crate _class _reason; do
  [ -z "${e:-}" ] && continue
  case "$e" in \#*) continue ;; esac
  if ! grep -rqE "pub enum $e\b" "crates/$crate/src" 2>/dev/null; then
    RED_MSGS+=("allowlist row '$e' ($crate) names no enum in crates/$crate/src — stale exemption.")
    red=1
    continue
  fi
  if impls_trait "$e" "$crate"; then
    RED_MSGS+=("allowlist row '$e' ($crate) DOES impl NikaErrorCode — exemption no longer needed, remove the row.")
    red=1
  fi
done < <(grep -vE '^[[:space:]]*#' "$ALLOWLIST")

if [ "$red" -ne 0 ]; then
  echo "error one-voice enforcement FAILED:"
  for m in "${RED_MSGS[@]}"; do echo "  RED: $m"; done
  exit 2
fi

exempt_n="$(grep -cvE '^[[:space:]]*#' "$ALLOWLIST")"
echo "OK ($checked thiserror enum(s) checked · all speak NikaErrorCode or are 1 of $exempt_n documented exemptions)"
exit 0
