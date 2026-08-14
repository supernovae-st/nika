#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-cancel-safety.sh scripts/hygiene/check-kernel-no-spawn.sh
# Mutation proof for the two L0.5 kernel vectors — check-kernel-no-spawn
# and check-cancel-safety.
#
# Both used to name `crates/nika-kernel/src` outright. The 2026-06-10 split
# emptied that path: no-spawn was left guarding 263 of 13302 lines of L0.5
# source, and cancel-safety was left guarding a directory with zero async
# fn in it. Neither said so; both reported OK.
#
# So the cases below plant their violations in a SIBLING crate — the half
# of the layer the old scope could not see — and every crate name here is
# invented, which is the point: the vectors read the layer metadata, so a
# crate that did not exist when they were written is covered anyway.
#
# The empty-harvest cases matter as much as the violation ones. An empty
# scope is the exact shape both vectors sat in for months, and it looked
# identical to a clean one.
#
# Every builder below reaches its call site through `expect`, which takes
# the function name as an argument — shellcheck cannot see that (SC2329).
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

# seed <dir> <layers-block>
seed() {
  local dir="$1" layers="$2"
  mkdir -p "$dir/scripts/hygiene"
  cp "$ROOT/scripts/hygiene/check-kernel-no-spawn.sh" "$dir/scripts/hygiene/"
  cp "$ROOT/scripts/hygiene/check-cancel-safety.sh" "$dir/scripts/hygiene/"
  {
    printf '[workspace]\nmembers = []\n\n[workspace.metadata.diamond]\n'
    printf '%s\n' "$layers"
  } >"$dir/Cargo.toml"
}

# rs <dir> <crate> <body>
rs() {
  mkdir -p "$1/crates/$2/src"
  printf '%s\n' "$3" >"$1/crates/$2/src/lib.rs"
}

# expect <RED|GREEN> <vector> <label> <builder>
expect() {
  local want="$1" vector="$2" label="$3" builder="$4"
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  "$builder" "$dir"
  bash "$dir/scripts/hygiene/$vector" >/dev/null 2>&1
  local rc=$?
  local got="RED"
  [ "$rc" -eq 0 ] && got="GREEN"
  if [ "$got" = "$want" ]; then
    printf '  ok   [%s] %s (%s, rc=%d)\n' "${vector#check-}" "$label" "$got" "$rc"
  else
    fails=$((fails + 1))
    printf '  FAIL [%s] %s — wanted %s, got %s (rc=%d)\n' \
      "${vector#check-}" "$label" "$want" "$got" "$rc" >&2
  fi
}

L05_SET='layers.demo-kernel = "L0.5"
layers.demo-core = "L0.5"
layers.demo-mock = "L0.5"
layers.demo-app = "L2"'

# ---- builders -------------------------------------------------------------
# The historical shape: demo-kernel is the empty husk the old scope named,
# demo-core is the sibling that actually holds the code.
clean_tree() {
  seed "$1" "$L05_SET"
  rs "$1" demo-kernel 'pub const K: u8 = 1;'
  rs "$1" demo-core '/// CANCEL SAFETY: not applicable (pure compute).
pub async fn work() -> u8 { 1 }'
  rs "$1" demo-app 'pub const A: u8 = 2;'
}

spawn_in_sibling() {
  clean_tree "$1"
  rs "$1" demo-core '/// CANCEL SAFETY: not applicable (pure compute).
pub async fn work() -> u8 { tokio::spawn(async {}); 1 }'
}

spawn_out_of_layer() {
  clean_tree "$1"
  rs "$1" demo-app 'pub fn go() { tokio::spawn(async {}); }'
}

spawn_only_in_comment() {
  clean_tree "$1"
  rs "$1" demo-core '/// CANCEL SAFETY: not applicable (pure compute).
// tokio::spawn is banned here
pub async fn work() -> u8 { 1 }'
}

async_unmarked_in_sibling() {
  clean_tree "$1"
  rs "$1" demo-core 'pub async fn work() -> u8 { 1 }'
}

async_marked_on_trait() {
  clean_tree "$1"
  rs "$1" demo-core '/// A seam.
///
/// CANCEL SAFETY: dropping the future ends the run.
pub trait Seam {
    /// Do the thing.
    async fn work(&self) -> u8;
}'
}

async_unmarked_in_mock() {
  clean_tree "$1"
  rs "$1" demo-mock 'pub async fn stub() -> u8 { 0 }'
}

no_l05_at_all() {
  seed "$1" 'layers.demo-app = "L2"'
  rs "$1" demo-app 'pub const A: u8 = 2;'
}

echo "L0.5 kernel vectors · mutation proof"

# --- check-kernel-no-spawn -------------------------------------------------
expect GREEN check-kernel-no-spawn.sh "pristine layer" clean_tree
expect RED check-kernel-no-spawn.sh "spawn in a SIBLING L0.5 crate" spawn_in_sibling
expect GREEN check-kernel-no-spawn.sh "spawn above the layer (out of scope)" spawn_out_of_layer
expect GREEN check-kernel-no-spawn.sh "spawn named only in a comment" spawn_only_in_comment
expect RED check-kernel-no-spawn.sh "no L0.5 crate at all — must fail CLOSED" no_l05_at_all

# --- check-cancel-safety ---------------------------------------------------
expect GREEN check-cancel-safety.sh "pristine layer" clean_tree
expect RED check-cancel-safety.sh "async fn unmarked in a SIBLING L0.5 crate" async_unmarked_in_sibling
expect GREEN check-cancel-safety.sh "marker on the enclosing trait" async_marked_on_trait
expect GREEN check-cancel-safety.sh "unmarked async fn in a *-mock crate" async_unmarked_in_mock
expect RED check-cancel-safety.sh "no L0.5 crate at all — must fail CLOSED" no_l05_at_all

# --- idempotence -----------------------------------------------------------
# check-cancel-safety appended findings to a fixed /tmp path that only its
# GREEN branch removed, so a red tree reported 1, then 2, then 3 findings
# across identical runs. Two runs must agree.
cases=$((cases + 1))
IDEM="$WORK/idem"
async_unmarked_in_sibling "$IDEM"
a="$(bash "$IDEM/scripts/hygiene/check-cancel-safety.sh" 2>&1 | head -1)"
b="$(bash "$IDEM/scripts/hygiene/check-cancel-safety.sh" 2>&1 | head -1)"
if [ "$a" = "$b" ]; then
  printf '  ok   [cancel-safety] two runs agree on an unchanged tree (%s)\n' "$a"
else
  fails=$((fails + 1))
  printf '  FAIL [cancel-safety] run 1 said "%s", run 2 said "%s"\n' "$a" "$b" >&2
fi

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — a vector does not catch what it claims.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
