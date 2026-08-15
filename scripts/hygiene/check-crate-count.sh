#!/usr/bin/env bash
# Vector 2: workspace member count parity with layer assignments.
#
# Every crate listed in `[workspace] members = [...]` MUST have a layer
# assignment in `[workspace.metadata.diamond.layers.*]`. This catches new
# crates added without the layer discipline + crates removed from the
# workspace but still tagged.
#
# It compares SETS, not counts. Counting was the original form and it was
# green on the realistic case: add `nika-b` without a layer, drop
# `nika-old` while its layer stays, and 2 members vs 2 layers matches
# while BOTH halves of the invariant are broken. Measured 2026-08-15 —
# `OK (2 workspace members, all layer-classified)`, rc=0, with one crate
# unclassified and one layer pointing at nothing. Each fault alone
# reddened; only together did they cancel. A parity that two errors can
# satisfy is arithmetic, not enforcement.
#
# Replaces the old MEMORY.md-text-grep approach (which broke when MEMORY
# format changed and the path had a typo `-supernovae-nika` vs `-supernovae-hq`).
# Status drift between MEMORY and reality is now covered by vector 23
# (status-claims-sync) which compares against scripts/refresh-status.sh.
#
# Exit codes:
#   0 — GREEN (counts match)
#   2 — RED  (drift)

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../.." || exit 2

# Count crates in the workspace `members = [...]` array. Read the first
# `^members` line and count `"crates/X"` entries. Brittle to multi-line
# arrays, but the canonical Cargo.toml uses a single-line members list.
members=$(grep -m1 '^members' Cargo.toml | grep -oE '"crates/[^"]+"' \
  | tr -d '"' | sed 's|^crates/||' | sort -u)

# Layer assignments, one per crate (e.g. layers.nika-types = "L0").
layers=$(grep -oE '^layers\.[a-z0-9-]+ = "L[0-9.]+"$' Cargo.toml \
  | sed -e 's|^layers\.||' -e 's| = .*||' | sort -u)

members_count=$(printf '%s\n' "$members" | grep -c . || true)

if [ "${members_count:-0}" -eq 0 ]; then
  echo "RED: no workspace members detected (parser drift?)"
  exit 2
fi

# Both directions, named separately: a member with no layer is undisciplined,
# a layer with no member is a zombie. Counting could not tell them apart, and
# one of each cancelled out.
unclassified=$(comm -23 <(printf '%s\n' "$members") <(printf '%s\n' "$layers"))
zombies=$(comm -13 <(printf '%s\n' "$members") <(printf '%s\n' "$layers"))

if [ -n "$unclassified" ] || [ -n "$zombies" ]; then
  echo "RED: workspace members and layer assignments disagree"
  if [ -n "$unclassified" ]; then
    echo ""
    echo "  member(s) with NO layer:"
    printf '%s\n' "$unclassified" | sed 's/^/    · /'
  fi
  if [ -n "$zombies" ]; then
    echo ""
    echo "  layer(s) naming no member:"
    printf '%s\n' "$zombies" | sed 's/^/    · /'
  fi
  echo ""
  echo "Hint: add a 'layers.nika-X = \"L?\"' entry under"
  echo "[workspace.metadata.diamond] for every member crate, OR remove"
  echo "stale layer assignments for crates no longer in the workspace."
  exit 2
fi

echo "OK ($members_count workspace members, all layer-classified)"
exit 0
