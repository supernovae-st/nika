#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# Vector 35: ADR-081 computer-use guard-presence admission gate.
#
# ADR-081 (docs/adr/adr-081-l1-effect-crate-guard-contract.md) declares a
# per-crate Guard Ownership Matrix: each L1 computer-use effect crate
# (screen, ocr, a11y, input, browser, vision-local) owns one or more MANDATORY
# security guards (secure-field redaction, capture-LED, consent UX, password
# redaction, ConsentProof-TTL, prompt-injection sanitization, clickjacking).
# The ADR says each guard is "MANDATORY-at-admission" — yet nothing structural
# enforced it. This gate closes that gap.
#
# INVARIANT (the security forcing-function):
#   For every matrix row with Mode = MANDATORY whose owner-crate is a
#   workspace MEMBER, an impl-binding row MUST exist in
#   scripts/ci/adr-081-guard-manifest.tsv AND its impl_regex + test_regex MUST
#   match in that crate's src/. A guard-owner admitted without its guard ⇒ RED.
#
# This is DECLARATIVE + EVOLUTIVE (zero hardcode): the matrix is the source of
# what-is-owed; the manifest binds impls of crates that exist; the gate
# computes owed ∩ present. When nika-input (M2.4) lands, Guards 1+2 become
# owed-and-present ⇒ the gate auto-requires a binding ⇒ forces the guard.
#
# Exit: 0 green (all owed-present guards verified) · 1 yellow (owed guards for
# crates not yet in workspace — informational) · 2 red (a workspace
# guard-owner is missing its binding/impl/test).
set -uo pipefail

ENGINE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ENGINE_ROOT" || exit 2

ADR="docs/adr/adr-081-l1-effect-crate-guard-contract.md"
MANIFEST="scripts/ci/adr-081-guard-manifest.tsv"

[ -f "$ADR" ] || {
  echo "ADR-081 missing: $ADR"
  exit 2
}
[ -f "$MANIFEST" ] || {
  echo "guard manifest missing: $MANIFEST"
  exit 2
}

# --- workspace members (the `members = [...]` array, NOT `exclude`) ----------
# Extract crate basenames from the members array line(s).
members="$(awk '
  /^members[[:space:]]*=/ { grab=1 }
  grab { line = line $0 }
  grab && /\]/ { print line; grab=0 }
' Cargo.toml | grep -oE 'crates/nika-[a-z0-9-]+' | sed 's#crates/##' | sort -u)"

is_member() { printf '%s\n' "$members" | grep -qx "$1"; }

# --- parse the ADR matrix → "guard_id<TAB>crate<TAB>mode" --------------------
# Matrix rows look like: | 6 · Capture LED indicator | `nika-screen` | MANDATORY-at-admission | M2.1 |
# awk (NOT sed) so the TAB separator is a real \t on BSD sed (macOS) too —
# BSD sed does not expand \t in the replacement (it would emit a literal 't').
matrix="$(grep -E '^\|[[:space:]]*[0-9]+[[:space:]]*·' "$ADR" \
  | awk -F'|' '{
      gid=$2; sub(/^[^0-9]*/, "", gid); sub(/[^0-9].*/, "", gid);
      crate=$3; gsub(/[`[:space:]]/, "", crate);
      mode=$4; gsub(/^[[:space:]]+|[[:space:]]+$/, "", mode);
      if (gid != "" && crate != "") printf "%s\t%s\t%s\n", gid, crate, mode;
    }')"

[ -n "$matrix" ] || {
  echo "ADR-081 matrix parse yielded 0 rows — format drift?"
  exit 2
}

# --- manifest lookup (bash-3.2 safe · TSV, comments stripped) ----------------
manifest_lookup() { # $1=guard_id $2=crate → prints "impl_regex<TAB>test_regex" or empty
  grep -vE '^[[:space:]]*#' "$MANIFEST" | awk -F'\t' -v g="$1" -v c="$2" \
    'NF>=4 && $1==g && $2==c { print $3"\t"$4; exit }'
}

red=0
verified=0
owed_future=()
RED_MSGS=()

while IFS=$'\t' read -r gid crate mode; do
  [ -z "$gid" ] && continue
  case "$mode" in MANDATORY*) : ;; *) continue ;; esac

  if ! is_member "$crate"; then
    owed_future+=("Guard $gid → $crate (owed at admission)")
    continue
  fi

  # owed AND present → binding is mandatory.
  binding="$(manifest_lookup "$gid" "$crate")"
  if [ -z "$binding" ]; then
    RED_MSGS+=("Guard $gid for workspace crate '$crate' is MANDATORY (ADR-081 matrix) but has NO impl-binding in $MANIFEST — add a row + implement the guard.")
    red=1
    continue
  fi
  impl_re="$(printf '%s' "$binding" | cut -f1)"
  test_re="$(printf '%s' "$binding" | cut -f2)"

  src_glob="crates/$crate/src"
  if ! grep -rqE -- "$impl_re" "$src_glob" 2>/dev/null; then
    RED_MSGS+=("Guard $gid ($crate): impl marker /$impl_re/ NOT found in $src_glob/ — guard not implemented.")
    red=1
    continue
  fi
  if ! grep -rqE -- "$test_re" "$src_glob" 2>/dev/null; then
    RED_MSGS+=("Guard $gid ($crate): test marker /$test_re/ NOT found in $src_glob/ — guard not tested.")
    red=1
    continue
  fi
  verified=$((verified + 1))
done <<EOF
$matrix
EOF

# --- orphan-manifest check: every binding row maps to a real matrix row ------
while IFS=$'\t' read -r gid crate _impl _test; do
  [ -z "${gid:-}" ] && continue
  case "$gid" in \#*) continue ;; esac
  if ! printf '%s\n' "$matrix" | grep -qE "^${gid}	${crate}	"; then
    RED_MSGS+=("Manifest row (guard $gid, $crate) has NO matching ADR-081 matrix row — orphan binding.")
    red=1
  fi
done < <(grep -vE '^[[:space:]]*#' "$MANIFEST")

if [ "$red" -ne 0 ]; then
  echo "ADR-081 guard enforcement FAILED:"
  for m in "${RED_MSGS[@]}"; do echo "  RED: $m"; done
  exit 2
fi

if [ "${#owed_future[@]}" -gt 0 ]; then
  echo "OK ($verified guard(s) verified) · ${#owed_future[@]} owed at future admission:"
  for m in "${owed_future[@]}"; do echo "  · $m"; done
  exit 1
fi

echo "OK ($verified MANDATORY guard(s) verified · all workspace guard-owners covered)"
exit 0
