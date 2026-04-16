#!/usr/bin/env bash
# Vector 2: workspace member count parity with layer assignments.
#
# Every crate listed in `[workspace] members = [...]` MUST have a layer
# assignment in `[workspace.metadata.diamond.layers.*]`. This catches new
# crates added without the layer discipline + crates removed from the
# workspace but still tagged.
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
members_count=$(grep -m1 '^members' Cargo.toml | grep -oE '"crates/[^"]+"' | wc -l | tr -d ' ')

# Count layer assignments (one per crate, e.g. layers.nika-types = "L0").
layer_count=$(grep -cE '^layers\.[a-z0-9-]+ = "L[0-9.]+"$' Cargo.toml)

if [ "$members_count" -eq 0 ]; then
  echo "RED: no workspace members detected (parser drift?)"
  exit 2
fi

if [ "$members_count" != "$layer_count" ]; then
  echo "RED: $members_count workspace members vs $layer_count layer assignments"
  echo ""
  echo "Hint: add a 'layers.nika-X = \"L?\"' entry under"
  echo "[workspace.metadata.diamond] for every member crate, OR remove"
  echo "stale layer assignments for crates no longer in the workspace."
  exit 2
fi

echo "OK ($members_count workspace members, all layer-classified)"
exit 0
