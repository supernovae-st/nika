#!/usr/bin/env bash
# Gate auxiliary — ADR coverage for admitted workspace crates.
#
# Checks that every admitted workspace member is mentioned in at least one ADR
# under `docs/adr/adr-*.md`. This ensures the rationale for admitting a crate
# is publicly documented.
#
# Status: warn-only (exits 0 even on miss). Promote to fail when coverage
# matures. Can be called standalone or from the crate-admit skill.
#
# Usage:
#   bash scripts/ci/check-adr-coverage.sh           # warn-only
#   FAIL_ON_MISS=1 bash scripts/ci/check-adr-coverage.sh   # strict mode

set -uo pipefail
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/../.." && pwd)

ADR_DIR="$REPO_ROOT/docs/adr"
CARGO_TOML="$REPO_ROOT/Cargo.toml"

# Color helpers (portable macOS / Linux)
readonly C_RED=$'\033[0;31m'
readonly C_YELLOW=$'\033[0;33m'
readonly C_GREEN=$'\033[0;32m'
readonly C_BLUE=$'\033[0;34m'
readonly C_RESET=$'\033[0m'

if [ ! -d "$ADR_DIR" ]; then
  printf '%s[adr-coverage]%s no docs/adr/ directory — nothing to check\n' "$C_YELLOW" "$C_RESET"
  exit 0
fi

if [ ! -f "$CARGO_TOML" ]; then
  printf '%s[adr-coverage]%s no Cargo.toml at repo root\n' "$C_RED" "$C_RESET" >&2
  exit 2
fi

# Extract admitted workspace members (first-level only, skip exclude list)
# Handles two styles: `members = ["tools/...", ...]` on a single line, or multi-line array.
members=$(awk '
  /^members[[:space:]]*=/ { in_members=1 }
  in_members {
    # Remember close-bracket presence BEFORE stripping brackets
    has_close = ($0 ~ /\]/)
    # strip comments, brackets, commas, quotes
    gsub(/#.*$/, "")
    gsub(/[\[\]"]/, "")
    gsub(/,/, " ")
    for (i=1; i<=NF; i++) {
      if ($i ~ /^tools\//) print $i
    }
    if (has_close) in_members=0
  }
' "$CARGO_TOML" | sort -u)

if [ -z "$members" ]; then
  printf '%s[adr-coverage]%s no workspace members detected (regex failed?)\n' "$C_YELLOW" "$C_RESET" >&2
  exit 0
fi

missing=()
found=()
# Pre-seed to tolerate empty expansion under set -u
missing+=()
found+=()

while IFS= read -r member; do
  [ -z "$member" ] && continue
  crate_name=$(basename "$member")
  # Grep ADR files for any occurrence of the crate name
  if grep -lE "\\b${crate_name}\\b" "$ADR_DIR"/adr-*.md >/dev/null 2>&1; then
    found+=("$crate_name")
  else
    missing+=("$crate_name")
  fi
done <<<"$members"

# Report
echo
printf '%s═══ ADR coverage ═══%s\n' "$C_BLUE" "$C_RESET"
if [ "${#found[@]}" -gt 0 ]; then
  printf '%sCovered%s  (%d): %s\n' "$C_GREEN" "$C_RESET" "${#found[@]}" "${found[*]}"
else
  printf '%sCovered%s  (0): none yet\n' "$C_YELLOW" "$C_RESET"
fi
if [ "${#missing[@]}" -gt 0 ]; then
  printf '%sMissing%s  (%d):\n' "$C_YELLOW" "$C_RESET" "${#missing[@]}"
  for m in "${missing[@]}"; do
    printf '   %s↳%s %s  (add reference in docs/adr/adr-NNN-*.md or write a new ADR)\n' "$C_YELLOW" "$C_RESET" "$m"
  done
  # P1-5 Batch H+ (Q-RATCHET scoped fail): if the staged diff adds a NEW
  # Cargo.toml under tools/ or crates/ without a matching ADR in the same
  # diff, FAIL hard. Historical crates without ADR coverage remain warn-only.
  if [ "${FAIL_ON_MISS:-0}" = "1" ]; then
    exit 1
  fi

  new_crate_manifests="$(git diff --cached --name-only --diff-filter=A 2>/dev/null \
    | grep -E '^(tools|crates)/[^/]+/Cargo\.toml$' || true)"
  if [ -n "$new_crate_manifests" ]; then
    new_adrs="$(git diff --cached --name-only --diff-filter=A 2>/dev/null \
      | grep -E '^docs/adr/adr-[0-9]+-.*\.md$' || true)"
    for manifest in $new_crate_manifests; do
      new_crate="$(basename "$(dirname "$manifest")")"
      has_adr=0
      for adr in $new_adrs; do
        if grep -lqE "\\b${new_crate}\\b" "$REPO_ROOT/$adr" 2>/dev/null; then
          has_adr=1
          break
        fi
      done
      if [ "$has_adr" -eq 0 ]; then
        printf '%s[adr-coverage] FAIL%s: new crate %s has no ADR in this commit\n' \
          "$C_RED" "$C_RESET" "$new_crate" >&2
        printf '  Stage a docs/adr/adr-NNN-*.md mentioning "%s" in the same commit.\n' \
          "$new_crate" >&2
        exit 1
      fi
    done
  fi
else
  printf '%sAll%s admitted crates have ADR coverage\n' "$C_GREEN" "$C_RESET"
fi
echo
