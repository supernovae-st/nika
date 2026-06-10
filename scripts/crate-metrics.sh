#!/usr/bin/env bash
# crate-metrics.sh — deterministic per-crate metrics projector.
#
# The SINGLE SOURCE OF TRUTH for the volatile numbers that used to be
# hand-typed (and drift) in docs/crate-specs/*.md gate tables: src LOC,
# largest-file LOC, unit/integration test counts. Everything is DERIVED
# from the code (wc / grep / Cargo metadata) — nothing is hardcoded.
#
# Per PILLAR 1 (projection-by-default) + nika-branch-topology « if you're
# typing a number, STOP and project it ». Specs point HERE for live counts
# instead of carrying numbers that rot; vector 6 (check-crate-specs.sh)
# enforces no grossly-stale number sneaks back into a spec.
#
# Usage:
#   ./scripts/crate-metrics.sh                  # table · all admitted crates
#   ./scripts/crate-metrics.sh --json           # machine-readable (Olympus-consumable)
#   ./scripts/crate-metrics.sh <crate>          # one crate, table row
#   ./scripts/crate-metrics.sh --loc <crate>    # just the src LOC integer (for scripts)
#
# Authored 🦋 for ecosystem self-healing — derive, never hardcode.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 2

# WIP crates (in the workspace but pre-admission) — kept in sync with
# scripts/refresh-status.sh WIP_CRATES. Metrics still compute; the spec
# parity gate skips them (no admission spec table yet).
WIP_CRATES="nika-schema"

# ── derivation helpers (pure · all from the code) ────────────────────

crate_layer() {
  # Layer from the single source: [workspace.metadata.diamond] in Cargo.toml.
  local crate="$1"
  grep -E "^layers\.${crate} = " Cargo.toml 2>/dev/null \
    | sed -E 's/.*= "([^"]+)".*/\1/' | head -1
}

src_loc() {
  # Total non-blank+blank LOC across the crate's src/*.rs (wc -l semantics,
  # matching how the specs + caps speak about LOC).
  local crate="$1"
  find "crates/${crate}/src" -name '*.rs' -print0 2>/dev/null \
    | xargs -0 wc -l 2>/dev/null | awk 'END{print ($1==""?0:$1)}' \
    | tr -d ' '
}

max_file_loc() {
  local crate="$1"
  find "crates/${crate}/src" -name '*.rs' -print0 2>/dev/null \
    | xargs -0 wc -l 2>/dev/null | awk '$2!="total"{if($1>m)m=$1} END{print m+0}'
}

count_tests() {
  # #[test] + #[tokio::test] markers under a directory (0 if absent).
  local dir="$1"
  [ -d "$dir" ] || {
    echo 0
    return
  }
  grep -rho -E '#\[(tokio::)?test\]' "$dir" 2>/dev/null | wc -l | tr -d ' '
}

admitted_crates() {
  # Workspace members that are NOT WIP, sorted, with a src/ dir.
  for cargo in crates/*/Cargo.toml; do
    [ -f "$cargo" ] || continue
    local crate
    crate="$(basename "$(dirname "$cargo")")"
    case " $WIP_CRATES " in *" $crate "*) continue ;; esac
    [ -d "crates/${crate}/src" ] || continue
    echo "$crate"
  done | sort
}

# ── modes ────────────────────────────────────────────────────────────

emit_loc_only() {
  src_loc "$1"
}

emit_row() {
  local crate="$1"
  printf '%-22s %-5s %6s %6s %6s %6s\n' \
    "$crate" "$(crate_layer "$crate")" \
    "$(src_loc "$crate")" "$(max_file_loc "$crate")" \
    "$(count_tests "crates/${crate}/src")" \
    "$(count_tests "crates/${crate}/tests")"
}

emit_table() {
  printf '%-22s %-5s %6s %6s %6s %6s\n' "crate" "layer" "src" "maxf" "unitT" "intgT"
  printf -- '------------------------------------------------------------\n'
  while read -r crate; do emit_row "$crate"; done < <(admitted_crates)
}

emit_json() {
  printf '['
  local first=1
  while read -r crate; do
    [ "$first" -eq 1 ] && first=0 || printf ','
    printf '{"crate":"%s","layer":"%s","src_loc":%s,"max_file_loc":%s,"unit_tests":%s,"integration_tests":%s}' \
      "$crate" "$(crate_layer "$crate")" \
      "$(src_loc "$crate")" "$(max_file_loc "$crate")" \
      "$(count_tests "crates/${crate}/src")" \
      "$(count_tests "crates/${crate}/tests")"
  done < <(admitted_crates)
  printf ']\n'
}

case "${1:-}" in
  --json) emit_json ;;
  --loc)
    [ -n "${2:-}" ] || {
      echo "usage: --loc <crate>" >&2
      exit 2
    }
    emit_loc_only "$2"
    ;;
  "") emit_table ;;
  --*)
    echo "unknown flag: $1" >&2
    exit 2
    ;;
  *) # single crate, table header + row
    printf '%-22s %-5s %6s %6s %6s %6s\n' "crate" "layer" "src" "maxf" "unitT" "intgT"
    emit_row "$1"
    ;;
esac
