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
# The one branch that BLOCKS — a new crate staged without an ADR — reads
# `git diff --cached`. This script's only caller was run-ci-ratchets, which
# runs at PRE-PUSH, where everything is already committed and the index is
# empty. So the hard branch was structurally unreachable in the one chain
# that invoked it: a crate could be committed and pushed with no ADR and
# nothing would say a word. `--staged-only` gives the pre-commit hook a
# quiet form of exactly that check, where the staged diff actually exists.
#
# Usage:
#   bash scripts/ci/check-adr-coverage.sh                  # coverage report, warn-only
#   bash scripts/ci/check-adr-coverage.sh --staged-only    # pre-commit: silent unless a
#                                                          # NEW crate is staged ADR-less
#   FAIL_ON_MISS=1 bash scripts/ci/check-adr-coverage.sh   # strict mode

set -uo pipefail

STAGED_ONLY=0
for arg in "$@"; do
  case "$arg" in
    --staged-only) STAGED_ONLY=1 ;;
  esac
done
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

# A NEW crate staged without an ADR in the same commit. Hoisted out of the
# coverage report it used to be nested inside: it depends only on the staged
# diff, so it belongs where the diff is, and running it first means it can
# be the whole job in --staged-only mode.
check_staged_new_crates() {
  local new_crate_manifests new_adrs manifest new_crate adr has_adr
  new_crate_manifests="$(git diff --cached --name-only --diff-filter=A 2>/dev/null \
    | grep -E '^(tools|crates)/[^/]+/Cargo\.toml$' || true)"
  [ -n "$new_crate_manifests" ] || return 0
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
      return 1
    fi
  done
  return 0
}

check_staged_new_crates || exit 1

# --staged-only says its piece by staying silent: the coverage report below
# is a standing inventory, and printing eleven historical misses on every
# commit is how a gate trains people to stop reading it.
if [ "$STAGED_ONLY" -eq 1 ]; then
  exit 0
fi

# Extract admitted workspace members (first-level only, skip exclude list)
# Handles two styles: `members = ["crates/...", ...]` on a single line, or multi-line array.
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
      if ($i ~ /^crates\//) print $i
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
  # Historical crates without ADR coverage remain warn-only. The
  # staged-new-crate check that used to live here ran BEFORE this report
  # (see check_staged_new_crates above) — it was nested inside this branch,
  # so it could only fire when some OTHER crate also happened to be
  # uncovered, which is not a condition it should ever have depended on.
  if [ "${FAIL_ON_MISS:-0}" = "1" ]; then
    exit 1
  fi
else
  printf '%sAll%s admitted crates have ADR coverage\n' "$C_GREEN" "$C_RESET"
fi
echo
