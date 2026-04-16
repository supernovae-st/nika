#!/usr/bin/env bash
# Vector 25: max 3 sibling L0 deps per L0 crate (ADR-027 §"Hard rule").
#
# L0 crates are the foundation. Each L0 crate having too many `nika-*`
# sibling dependencies fragments the compile DAG and creates implicit
# coupling. ADR-027 caps at 3.
#
# Source of truth: each L0 crate's `crates/<crate>/Cargo.toml`
# `[dependencies]` section. Counts entries matching `^nika-[a-z0-9-]+`
# (excluding self). The future `docs/workspace.toml` SSOT (ADR-026) will
# eventually centralize this; until then, parse Cargo.toml directly.
#
# Thresholds (ADR-027):
#   ≤ 2 deps : GREEN (comfortable)
#   = 3 deps : GREEN at the cap
#   > 3 deps : RED   (violation — split, OR justify in admission spec)
#
# Per-crate spec exemption: a crate may exceed 3 with explicit rationale
# in `docs/crate-specs/<crate>.md`. To exempt, add a comment line to
# Cargo.toml like:
#   # L0-DEP-FANOUT-EXEMPT: <reason> (e.g., "schema fan-in is intentional")
#
# Exit codes:
#   0 — GREEN (all L0 crates ≤ 3 sibling deps OR exempted)
#   2 — RED   (at least one L0 crate > 3 without exempt marker)

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

MAX=3

# Extract L0 crate names from workspace metadata.
mapfile -t L0_CRATES < <(grep -E '^layers\.[a-z0-9-]+ = "L0"$' Cargo.toml \
  | sed -E 's/^layers\.([a-z0-9-]+) = "L0"$/\1/')

if [ "${#L0_CRATES[@]}" -eq 0 ]; then
  echo "WARN: no L0 crates found in [workspace.metadata.diamond]"
  exit 1
fi

violations=0
log=""

for crate in "${L0_CRATES[@]}"; do
  manifest="crates/$crate/Cargo.toml"
  [ -f "$manifest" ] || continue

  # Extract [dependencies] block (until next [section] or EOF).
  # Then count lines starting with `nika-` (the dependency name as key).
  # Excludes self-reference (shouldnt happen but defensive).
  count=$(awk '
    /^\[/ { in_deps = ($0 == "[dependencies]") ? 1 : 0; next }
    in_deps && /^nika-[a-z0-9-]+[[:space:]]*=/ { print }
  ' "$manifest" | grep -cvE "^${crate}[[:space:]]*=" || true)

  # Check for exempt marker
  exempt=0
  if grep -qE '^#[[:space:]]*L0-DEP-FANOUT-EXEMPT:' "$manifest"; then
    exempt=1
  fi

  if [ "$count" -gt "$MAX" ]; then
    if [ "$exempt" -eq 1 ]; then
      reason=$(grep -E '^#[[:space:]]*L0-DEP-FANOUT-EXEMPT:' "$manifest" | head -1 | sed -E 's/^#[[:space:]]*L0-DEP-FANOUT-EXEMPT:[[:space:]]*//')
      log+="\n  $crate: $count nika-* deps (>$MAX) — EXEMPT: $reason"
    else
      violations=$((violations + 1))
      log+="\n  $crate: $count nika-* deps (>$MAX, no exempt marker)"
    fi
  fi
done

if [ "$violations" -gt 0 ]; then
  printf "RED: %d L0 crate(s) over %d sibling-dep cap:" "$violations" "$MAX"
  printf "%b\n" "$log"
  echo ""
  echo "Hint: split the over-budget crate, OR add to Cargo.toml:"
  echo "  # L0-DEP-FANOUT-EXEMPT: <reason>"
  echo "Per ADR-027 the cap forces conscious DAG design."
  exit 2
fi

echo "OK: ${#L0_CRATES[@]} L0 crate(s) within ≤${MAX} sibling-dep cap (${L0_CRATES[*]})"
exit 0
