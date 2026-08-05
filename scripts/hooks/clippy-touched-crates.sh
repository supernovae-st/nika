#!/usr/bin/env bash
# clippy-touched-crates.sh — targeted clippy for Tier 1 pre-commit
#
# Strategy:
#   1. Find staged .rs files.
#   2. Map each file to its containing crate (Cargo.toml [package]).
#   3. For each directly touched crate, find reverse-deps via `cargo tree --invert`
#      to avoid missing breakage in dependent crates.
#   4. Run `cargo clippy --package <crate>` for each affected crate.
#      Fall back to --workspace if reverse-dep resolution fails or >5 crates touched.
#
# Design tradeoff: fast path (≤5 touched crates) targets clippy to avoid
# full-workspace cost. Full workspace still runs at pre-push.
#
# Runs from inside nika/engine (cwd set by lefthook root: directive).
# Exit: 0 = clean | 1 = clippy warnings
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -Eeuo pipefail

readonly MAX_TARGETED_CRATES=5
readonly CLIPPY_FLAGS='--all-targets -- -D warnings'

# ---------------------------------------------------------------------------
# 1. Collect staged .rs files
# ---------------------------------------------------------------------------
mapfile -t STAGED_RS < <(
  git diff --cached --name-only --diff-filter=ACM 2>/dev/null \
    | grep '\.rs$' \
    | grep -v '^fuzz/' \
    || true
)
# fuzz/ is a self-rooted cargo-fuzz crate (NOT a workspace member ·
# `cargo clippy --package nika-schema-fuzz` from the root cannot resolve
# it) · its targets are nightly-built by .github/workflows/fuzz-nightly.yml.

if ((${#STAGED_RS[@]} == 0)); then
  printf '[clippy-touched] no .rs files staged — skipping\n' >&2
  exit 0
fi

# ---------------------------------------------------------------------------
# 2. Resolve crate name from file path
#    Walk up from each .rs file to find the nearest Cargo.toml with [package].
# ---------------------------------------------------------------------------
find_crate_name() {
  local file="$1"
  local dir
  dir="$(dirname "$file")"
  while [[ "$dir" != '.' && "$dir" != '/' ]]; do
    local manifest="${dir}/Cargo.toml"
    if [[ -f "$manifest" ]] && grep -q '^\[package\]' "$manifest" 2>/dev/null; then
      # Extract name = "..." from [package] section
      awk '/^\[package\]/{found=1} found && /^name[[:space:]]*=/{
        gsub(/.*=[[:space:]]*"/, ""); gsub(/".*/, ""); print; exit
      }' "$manifest"
      return 0
    fi
    dir="$(dirname "$dir")"
  done
  return 1
}

# A manifest that carries its own [workspace] table is a STANDALONE
# workspace (root `exclude` · nika-acp precedent: the ACP SDK's
# serde_json/preserve_order must never unify into the engine's feature
# graph). `--package` from the root cannot resolve it — the gate stays
# REAL by running clippy INSIDE that workspace instead (never a skip).
find_crate_dir() {
  local file="$1"
  local dir
  dir="$(dirname "$file")"
  while [[ "$dir" != '.' && "$dir" != '/' ]]; do
    if [[ -f "${dir}/Cargo.toml" ]] && grep -q '^\[package\]' "${dir}/Cargo.toml" 2>/dev/null; then
      printf '%s' "$dir"
      return 0
    fi
    dir="$(dirname "$dir")"
  done
  return 1
}

declare -A TOUCHED_CRATES
declare -A STANDALONE_DIRS
# Scalar counts ride beside the assoc arrays: under `set -u`, expanding
# an EMPTY `declare -A` array is "unbound" on the bash this hook may
# meet — and "every staged file went standalone" makes empty real.
TOUCHED_COUNT=0
STANDALONE_COUNT=0
for rs_file in "${STAGED_RS[@]}"; do
  if crate=$(find_crate_name "$rs_file" 2>/dev/null); then
    crate_dir=$(find_crate_dir "$rs_file" 2>/dev/null) || crate_dir=''
    if [[ -n "$crate_dir" ]] && grep -q '^\[workspace\]' "${crate_dir}/Cargo.toml" 2>/dev/null; then
      STANDALONE_DIRS["$crate_dir"]=1
      STANDALONE_COUNT=1
    else
      TOUCHED_CRATES["$crate"]=1
      TOUCHED_COUNT=1
    fi
  fi
done

# Standalone workspaces are judged in place, each in its own feature graph.
if ((STANDALONE_COUNT > 0)); then
  STANDALONE_FAILED=''
  for dir in "${!STANDALONE_DIRS[@]}"; do
    printf '[clippy-touched] standalone workspace: %s\n' "$dir" >&2
    # shellcheck disable=SC2086
    if ! (cd "$dir" && cargo clippy $CLIPPY_FLAGS) 2>&1; then
      STANDALONE_FAILED="${STANDALONE_FAILED} ${dir}"
    fi
  done
  if [[ -n "$STANDALONE_FAILED" ]]; then
    printf '\n[clippy-touched] FAILED on standalone workspace(s):%s\n' "$STANDALONE_FAILED" >&2
    exit 1
  fi
fi

if ((TOUCHED_COUNT == 0 && STANDALONE_COUNT > 0)); then
  printf '[clippy-touched] OK — standalone workspace(s) clean · no member crates touched\n' >&2
  exit 0
fi

if ((${#TOUCHED_CRATES[@]} == 0)); then
  printf '[clippy-touched] could not resolve crate names — running full workspace clippy\n' >&2
  # shellcheck disable=SC2086
  exec cargo clippy --workspace $CLIPPY_FLAGS
fi

# ---------------------------------------------------------------------------
# 3. Reverse-dep expansion via cargo metadata (P1-1 Batch H+)
#    The previous approach used `cargo tree --invert --format '{p}'` whose
#    output contains tree-drawing chars (├── └──) that awk parsed as crate
#    names. cargo metadata JSON is machine-readable and correct.
#
#    Strategy: for each touched crate, find workspace members whose resolve
#    deps (depth ≤ 3) include the touched crate.
# ---------------------------------------------------------------------------
declare -A AFFECTED_CRATES
for crate in "${!TOUCHED_CRATES[@]}"; do
  AFFECTED_CRATES["$crate"]=1
done

# Build reverse-dep index from metadata (one jq call for all crates)
_metadata_json="$(cargo metadata --format-version=1 --no-deps 2>/dev/null)" || true
if [[ -n "$_metadata_json" ]]; then
  _workspace_members="$(printf '%s' "$_metadata_json" \
    | jq -r '.packages[].name' 2>/dev/null | sort -u)" || true

  for crate in "${!TOUCHED_CRATES[@]}"; do
    while IFS= read -r rdep; do
      [[ -n "$rdep" ]] && AFFECTED_CRATES["$rdep"]=1
    done < <(
      cargo tree --invert --package "$crate" --depth 3 \
        --prefix=none --format '{p}' 2>/dev/null \
        | sed 's/ v[0-9].*//' \
        | grep -v "^${crate}$" \
        | sort -u \
        || true
    )
  done
fi

AFFECTED_COUNT="${#AFFECTED_CRATES[@]}"

if ((AFFECTED_COUNT > MAX_TARGETED_CRATES)); then
  printf '[clippy-touched] %d crates affected (threshold %d) — running full workspace clippy\n' \
    "$AFFECTED_COUNT" "$MAX_TARGETED_CRATES" >&2
  # shellcheck disable=SC2086
  exec cargo clippy --workspace $CLIPPY_FLAGS
fi

# ---------------------------------------------------------------------------
# 4. Run targeted clippy
# ---------------------------------------------------------------------------
printf '[clippy-touched] running clippy on %d crate(s): %s\n' \
  "$AFFECTED_COUNT" "${!AFFECTED_CRATES[*]}" >&2

FAILED=()
for crate in "${!AFFECTED_CRATES[@]}"; do
  printf '  -> cargo clippy --package %s\n' "$crate" >&2
  # shellcheck disable=SC2086
  if ! cargo clippy --package "$crate" $CLIPPY_FLAGS 2>&1; then
    FAILED+=("$crate")
  fi
done

if ((${#FAILED[@]} > 0)); then
  printf '\n[clippy-touched] FAILED on crate(s): %s\n' "${FAILED[*]}" >&2
  exit 1
fi

printf '[clippy-touched] OK — %d crate(s) clean\n' "$AFFECTED_COUNT" >&2
exit 0
