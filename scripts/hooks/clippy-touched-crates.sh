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
    || true
)

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

declare -A TOUCHED_CRATES
for rs_file in "${STAGED_RS[@]}"; do
  if crate=$(find_crate_name "$rs_file" 2>/dev/null); then
    TOUCHED_CRATES["$crate"]=1
  fi
done

if ((${#TOUCHED_CRATES[@]} == 0)); then
  printf '[clippy-touched] could not resolve crate names — running full workspace clippy\n' >&2
  # shellcheck disable=SC2086
  exec cargo clippy --workspace $CLIPPY_FLAGS
fi

# ---------------------------------------------------------------------------
# 3. Reverse-dep expansion
#    For each directly touched crate, add crates that depend on it.
#    Limit: if total expands beyond MAX_TARGETED_CRATES, fall back to workspace.
# ---------------------------------------------------------------------------
declare -A AFFECTED_CRATES
for crate in "${!TOUCHED_CRATES[@]}"; do
  AFFECTED_CRATES["$crate"]=1
  # cargo tree --invert shows what depends on this crate
  while IFS= read -r dep_crate; do
    [[ -n "$dep_crate" ]] && AFFECTED_CRATES["$dep_crate"]=1
  done < <(
    cargo tree --invert --package "$crate" --depth 3 \
      --format '{p}' 2>/dev/null \
      | awk '{print $1}' \
      | grep -v "^${crate}$" \
      | sort -u \
      || true
  )
done

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
