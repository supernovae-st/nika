#!/usr/bin/env bash
# check-layering.sh — enforce L0 → L5 dependency discipline per ADR-006.
#
# Validates that every admitted crate declared in [workspace.metadata.diamond.layers]
# respects DOWNWARD-ONLY dependency flow:
#
#   L0  → nothing (zero deps on workspace members)
#   L0.5 → L0 only
#   L1  → L0, L0.5
#   L1.5 → L0, L0.5, L1
#   L2  → L0, L0.5, L1, L1.5
#   L3  → L0, L0.5, L1, L2
#   L4  → L0, L0.5, L1, L2, L3
#   L5  → L0, L0.5, L1, L2, L3, L4
#
# Usage:
#   bash scripts/ci/check-layering.sh
#
# Exit:
#   0 — all layer constraints respected
#   2 — at least one upward dep (RED)
#
# Source of truth:
#   - Cargo.toml [workspace.metadata.diamond.layers]
#   - docs/architecture/crate-layer-registry.md

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

# Layer numeric rank (for comparison).
layer_rank() {
  case "$1" in
    "L0") echo 0 ;;
    "L0.5") echo 1 ;;
    "L1") echo 2 ;;
    "L1.5") echo 3 ;;
    "L2") echo 4 ;;
    "L3") echo 5 ;;
    "L4") echo 6 ;;
    "L5") echo 7 ;;
    *) echo -1 ;;
  esac
}

# Parse [workspace.metadata.diamond.layers] from Cargo.toml.
# Output: one "crate=layer" per line.
parse_layers() {
  awk '
    /^\[workspace\.metadata\.diamond\]/ { in_section = 1; next }
    /^\[/ && in_section { in_section = 0 }
    in_section && /^layers\./ {
      gsub(/^layers\./, "")
      gsub(/ /, "")
      gsub(/"/, "")
      print
    }
  ' Cargo.toml
}

# Build an associative array: crate → layer.
#
# The `=()` is load-bearing. `declare -A CRATE_LAYER` alone leaves the
# variable UNSET, and under `set -u` (with no `set -e`) the guard below
# expands `${#CRATE_LAYER[@]}` on an unset name: bash writes "unbound
# variable" to stderr, the `if` test itself fails, and the protective
# `exit 2` is SKIPPED. Execution then walks past the loop — which cannot
# iterate an empty map — straight to `exit 0`. Measured with a real
# upward L0→L4 dep planted and the metadata section renamed: "OK: all 0
# admitted crates respect layer discipline", rc=0. Assigning once makes
# the name bound, so the guard can actually fire.
declare -A CRATE_LAYER=()
while IFS='=' read -r crate layer; do
  [ -z "$crate" ] && continue
  CRATE_LAYER["$crate"]="$layer"
done < <(parse_layers)

if [ ${#CRATE_LAYER[@]} -eq 0 ]; then
  echo "❌ No [workspace.metadata.diamond.layers] section found in Cargo.toml" >&2
  exit 2
fi

# Every declared layer must be one this script can rank. `layer_rank`
# answers -1 for anything else, and -1 is never `-gt` a real rank, so an
# unrecognised layer made a crate INVISIBLE as a dependency target: any
# crate could depend upward on it and pass. The default arm has to fail
# closed, and it cannot do so from inside `layer_rank` — that runs in a
# command substitution, where `exit` ends only the subshell. So validate
# here, once, before any comparison is trusted.
for _crate in "${!CRATE_LAYER[@]}"; do
  if [ "$(layer_rank "${CRATE_LAYER[$_crate]}")" -lt 0 ]; then
    echo "❌ ${_crate} declares layer '${CRATE_LAYER[$_crate]}', which this script cannot rank." >&2
    echo "   Add it to layer_rank() or fix Cargo.toml — an unrankable layer is unenforceable." >&2
    exit 2
  fi
done

violations=0

# For each admitted crate, check its Cargo.toml dependencies.
for crate in "${!CRATE_LAYER[@]}"; do
  crate_toml="crates/${crate}/Cargo.toml"
  if [ ! -f "$crate_toml" ]; then
    echo "⚠️  ${crate}: Cargo.toml not found (skipping)" >&2
    continue
  fi

  current_layer="${CRATE_LAYER[$crate]}"
  current_rank=$(layer_rank "$current_layer")

  # Extract workspace-member deps (lines like `nika-xxx = { ... path = "../..." ... }`
  # or `nika-xxx = { workspace = true }` or `nika-xxx.workspace = true`).
  # Matches the crate name (LHS) before `=`.
  #
  # The class allows DIGITS (2026-08-02). It read `[a-z-]+`, so every
  # crate whose name carries one was invisible to this check:
  # `nika-bm25` and `nika-a11y`. Two real dependency lines went unread
  # (nika-onboard and nika-verb-agent on nika-bm25 · both downward, so
  # nothing was hiding today) — but an UPWARD dep on either would have
  # passed in silence, which is the only thing this check exists to stop.
  deps=$(awk '
    /^\[dependencies\]/ || /^\[dev-dependencies\]/ || /^\[build-dependencies\]/ { in_deps = 1; next }
    /^\[/ && in_deps { in_deps = 0 }
    in_deps && /^nika-[a-z0-9-]+[[:space:]]*=/ {
      split($0, parts, "=")
      gsub(/[[:space:]]/, "", parts[1])
      print parts[1]
    }
    in_deps && /^nika-[a-z0-9-]+\.workspace[[:space:]]*=/ {
      split($0, parts, ".")
      gsub(/[[:space:]]/, "", parts[1])
      print parts[1]
    }
  ' "$crate_toml" | sort -u)

  for dep in $deps; do
    # Only check workspace-member deps
    if [ -z "${CRATE_LAYER[$dep]:-}" ]; then
      continue
    fi
    dep_layer="${CRATE_LAYER[$dep]}"
    dep_rank=$(layer_rank "$dep_layer")

    # Strict-upward only: dep must be at SAME or LOWER layer than consumer.
    # (Same-layer OK: L0 can use L0, L0.5 mock can use L0.5 kernel, etc.)
    if [ "$dep_rank" -gt "$current_rank" ]; then
      echo "❌ UPWARD DEP: ${crate} (${current_layer}) depends on ${dep} (${dep_layer})" >&2
      violations=$((violations + 1))
    fi
  done
done

if [ "$violations" -gt 0 ]; then
  echo "" >&2
  echo "Found $violations layering violation(s). See docs/architecture/crate-layer-registry.md." >&2
  exit 2
fi

echo "OK: all ${#CRATE_LAYER[@]} admitted crates respect layer discipline"
exit 0
