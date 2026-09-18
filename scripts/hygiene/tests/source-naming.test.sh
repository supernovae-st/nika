#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-source-naming.sh
#
# Mutation proof: a live `.nika.yaml` path is red; the allowlisted
# lexical owner stays green.

set -uo pipefail

unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_NAMESPACE \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
VECTOR="$ROOT/scripts/hygiene/check-source-naming.sh"
ALLOW="$ROOT/scripts/hygiene/source-naming-allowlist.tsv"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0

seed() {
  mkdir -p "$1/scripts/hygiene"
  git -C "$1" init -q
  git -C "$1" config user.email "test@example.test"
  git -C "$1" config user.name "test"
  cp "$VECTOR" "$ALLOW" "$1/scripts/hygiene/"
  chmod +x "$1/scripts/hygiene/check-source-naming.sh"
}

if (
  seed "$WORK/green"
  mkdir -p "$WORK/green/crates/nika-source/src"
  printf 'pub const PROGRAM_SUFFIX: &str = ".nika";\n' >"$WORK/green/crates/nika-source/src/lib.rs"
  git -C "$WORK/green" add crates/nika-source/src/lib.rs scripts/hygiene
  git -C "$WORK/green" commit -qm seed
  bash "$WORK/green/scripts/hygiene/check-source-naming.sh" >/dev/null
); then
  echo "  ok   clean tree is green"
else
  echo "  FAIL clean tree should be green" >&2
  fails=$((fails + 1))
fi

if (
  seed "$WORK/red"
  mkdir -p "$WORK/red/examples"
  printf 'nika: live\n' >"$WORK/red/examples/hello.nika.yaml"
  git -C "$WORK/red" add examples/hello.nika.yaml scripts/hygiene
  git -C "$WORK/red" commit -qm seed
  bash "$WORK/red/scripts/hygiene/check-source-naming.sh" >/dev/null
); then
  echo "  FAIL planted live .nika.yaml should be red" >&2
  fails=$((fails + 1))
else
  echo "  ok   planted live .nika.yaml is red"
fi

if [ "$fails" -ne 0 ]; then
  exit 2
fi
exit 0
