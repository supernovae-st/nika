#!/usr/bin/env bash
# next-tag-project asks the tag gate's estate question BEFORE the tag.
#
# v0.117.1 was tagged on a tree whose estate.yaml described the commit before
# it; every build leg refused the tag at "The estate manifest is true of the
# tagged tree". This proves --check goes red on a stale manifest and green
# again once it is regenerated, on a throwaway copy of the real tree (the
# real rules, so coverage is never the reason the check speaks).
set -euo pipefail

unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_PREFIX GIT_COMMON_DIR \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE \
  GIT_QUARANTINE_PATH

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
COPY="$(mktemp -d)"
trap 'rm -rf "$COPY"' EXIT

fail() {
  printf 'next-tag-estate.test: %s\n' "$1" >&2
  exit 1
}

# The tracked tree only, at HEAD: the copy is a repository of its own, so the
# manifest it carries describes ITS index, never the caller's.
git -C "$ROOT" archive --format=tar HEAD | tar -x -C "$COPY"
git -C "$COPY" init -q
# estate.py derives the repo slug from the origin remote; the copy names the real one.
git -C "$COPY" remote add origin https://github.com/supernovae-st/nika.git
git -C "$COPY" -c user.name=t -c user.email=t@t add -A
git -C "$COPY" -c user.name=t -c user.email=t@t -c commit.gpgsign=false \
  commit -q -m fixture
(cd "$COPY" && python3 scripts/estate.py --write >/dev/null)
git -C "$COPY" add estate.yaml

project() {
  bash "$ROOT/scripts/ci/next-tag-project.sh" --repo "$COPY" --check
}

out="$(project 2>&1)" || fail "a fresh manifest reads UNPROVEN: $out"
printf '%s\n' "$out" | grep -q 'estate manifest describes the tree' \
  || fail 'the green line does not say the manifest describes the tree'

# Move one tracked file after the manifest was written: the v0.117.1 shape.
printf '\n# moved after the manifest\n' >>"$COPY/Cargo.toml"
git -C "$COPY" add Cargo.toml
if out="$(project 2>&1)"; then
  fail 'a stale manifest passed --check'
fi
printf '%s\n' "$out" | grep -q 'estate.yaml does not describe the tree' \
  || fail "the stale row is not named: $out"
printf '%s\n' "$out" | grep -q 'LAST commit before the tag' \
  || fail 'the stale row does not say where the regeneration belongs'

json="$(bash "$ROOT/scripts/ci/next-tag-project.sh" --repo "$COPY" --json)"
printf '%s\n' "$json" | grep -q '"estate_stale":1' \
  || fail "the JSON face does not carry estate_stale: $json"

(cd "$COPY" && python3 scripts/estate.py --write >/dev/null)
git -C "$COPY" add estate.yaml
project >/dev/null 2>&1 || fail 'a regenerated manifest still reads UNPROVEN'

echo 'next-tag-estate.test: PASS'
