#!/usr/bin/env bash
# COVERS: scripts/ci/check-layering.sh scripts/hygiene/check-layer-deps.sh
# Mutation proof for the two layer vectors — check-layering (upward deps)
# and check-layer-deps (per-layer banned third-party deps).
#
# Both build an associative array with `declare -A X` and no `=()`, which
# leaves the name UNSET. Under `set -u` and without `set -e` that is not a
# crash: the expansion fails, the `if` guarding it fails with it, the
# protective `exit 2` is skipped, and the script walks on to `exit 0`.
# check-layering announced "OK: all 0 admitted crates respect layer
# discipline" with a real upward dep in the tree; check-layer-deps had no
# harvest check at all and enforced zero bans in green.
#
# So the empty-harvest cases below are the point, not an afterthought. Each
# is a way the metadata can stop being found — renamed section, no bans —
# and each must be RED, because nothing to check is not nothing to fix.
#
# Every builder reaches its call site through `expect`, which takes the
# function name as an argument; shellcheck cannot see that (SC2329).
# shellcheck disable=SC2329
set -uo pipefail

# Vector 49 runs this from the pre-push gate, where git exports GIT_DIR and
# GIT_INDEX_FILE. Neither vector shells out to git, but a test that inherits
# them is one edit away from writing into the real repository.
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_NAMESPACE \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

# seed <dir> <diamond-section-name> <layers+bans block>
seed() {
  local dir="$1" section="$2" block="$3"
  mkdir -p "$dir/scripts/ci" "$dir/scripts/hygiene"
  cp "$ROOT/scripts/ci/check-layering.sh" "$dir/scripts/ci/"
  cp "$ROOT/scripts/hygiene/check-layer-deps.sh" "$dir/scripts/hygiene/"
  {
    printf '[workspace]\nmembers = []\n\n[%s]\n' "$section"
    printf '%s\n' "$block"
  } >"$dir/Cargo.toml"
}

# manifest <dir> <crate> <deps-block>
manifest() {
  mkdir -p "$1/crates/$2"
  {
    printf '[package]\nname = "%s"\n\n[dependencies]\n' "$2"
    printf '%s\n' "$3"
  } >"$1/crates/$2/Cargo.toml"
}

STD_META='layers.nika-demolow = "L0"
layers.nika-demomid = "L1"
layers.nika-demohigh = "L4"
layer-bans.L0 = ["tokio", "futures"]'

# expect <RED|GREEN> <vector-relpath> <label> <builder>
expect() {
  local want="$1" vector="$2" label="$3" builder="$4"
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  "$builder" "$dir"
  bash "$dir/$vector" >/dev/null 2>&1
  local rc=$?
  local got="RED"
  [ "$rc" -eq 0 ] && got="GREEN"
  if [ "$got" = "$want" ]; then
    printf '  ok   [%s] %s (%s, rc=%d)\n' "$(basename "$vector" .sh)" "$label" "$got" "$rc"
  else
    fails=$((fails + 1))
    printf '  FAIL [%s] %s — wanted %s, got %s (rc=%d)\n' \
      "$(basename "$vector" .sh)" "$label" "$want" "$got" "$rc" >&2
  fi
}

# ---- builders -------------------------------------------------------------
clean_tree() {
  seed "$1" 'workspace.metadata.diamond' "$STD_META"
  manifest "$1" nika-demolow ''
  manifest "$1" nika-demomid 'nika-demolow = { path = "../nika-demolow" }'
  manifest "$1" nika-demohigh 'nika-demomid = { path = "../nika-demomid" }'
}

upward_dep() {
  clean_tree "$1"
  # L0 reaching UP to L4 — the one thing check-layering exists to stop.
  manifest "$1" nika-demolow 'nika-demohigh = { path = "../nika-demohigh" }'
}

same_layer_dep() {
  seed "$1" 'workspace.metadata.diamond' 'layers.nika-demoa = "L1"
layers.nika-demob = "L1"
layer-bans.L0 = ["tokio"]'
  manifest "$1" nika-demoa 'nika-demob = { path = "../nika-demob" }'
  manifest "$1" nika-demob ''
}

# The section renamed: the awk stops matching, the map stays empty, and
# nothing downstream can fire. The upward dep is left in place so a vector
# that still worked would have something to find.
section_renamed() {
  upward_dep "$1"
  seed "$1" 'workspace.metadata.diamondx' "$STD_META"
}

unknown_layer() {
  seed "$1" 'workspace.metadata.diamond' 'layers.nika-demolow = "L9"
layers.nika-demohigh = "L4"
layer-bans.L0 = ["tokio"]'
  manifest "$1" nika-demolow ''
  manifest "$1" nika-demohigh 'nika-demolow = { path = "../nika-demolow" }'
}

banned_dep() {
  clean_tree "$1"
  manifest "$1" nika-demolow 'tokio = "1"'
}

banned_dep_exempt() {
  clean_tree "$1"
  manifest "$1" nika-demolow '# LAYER-BAN-EXEMPT: fixture
tokio = "1"'
}

no_bans_declared() {
  seed "$1" 'workspace.metadata.diamond' 'layers.nika-demolow = "L0"'
  manifest "$1" nika-demolow 'tokio = "1"'
}

echo "layer vectors · mutation proof"

# --- check-layering --------------------------------------------------------
expect GREEN scripts/ci/check-layering.sh "pristine, downward deps only" clean_tree
expect RED scripts/ci/check-layering.sh "an upward dep L0 -> L4" upward_dep
expect GREEN scripts/ci/check-layering.sh "same-layer dep is allowed" same_layer_dep
expect RED scripts/ci/check-layering.sh "metadata section renamed — empty harvest" section_renamed
expect RED scripts/ci/check-layering.sh "a layer the rank table cannot rank" unknown_layer

# --- check-layer-deps ------------------------------------------------------
expect GREEN scripts/hygiene/check-layer-deps.sh "pristine, no banned dep" clean_tree
expect RED scripts/hygiene/check-layer-deps.sh "a banned dep at its layer" banned_dep
expect GREEN scripts/hygiene/check-layer-deps.sh "banned dep with an EXEMPT marker" banned_dep_exempt
expect RED scripts/hygiene/check-layer-deps.sh "metadata section renamed — empty harvest" section_renamed
expect RED scripts/hygiene/check-layer-deps.sh "no layer-bans declared — enforces nothing" no_bans_declared

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — a vector does not catch what it claims.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
