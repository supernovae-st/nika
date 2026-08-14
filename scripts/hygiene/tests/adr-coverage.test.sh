#!/usr/bin/env bash
# Mutation proof for the ADR-coverage gate's one BLOCKING branch: a new
# crate staged without the ADR that justifies admitting it.
#
# That branch reads `git diff --cached`. Its only caller was
# run-ci-ratchets, which runs at PRE-PUSH — where everything is already
# committed and the index is empty. Measured on this repo mid-push:
# `git diff --cached --name-only --diff-filter=A | wc -l` → 0. So the
# branch was structurally unreachable in the one chain that invoked it.
#
# It was also nested INSIDE the historical-coverage report, under
# `if [ "${#missing[@]}" -gt 0 ]`. That makes it depend on some OTHER
# crate happening to be uncovered — and gives the case below its teeth: a
# crate whose Cargo.toml is staged but which is not yet in `members` never
# enters `missing`, so the old code took the "All admitted crates have ADR
# coverage" branch and returned 0 with an ADR-less crate in the commit.
#
# shellcheck disable=SC2329
set -uo pipefail

unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_NAMESPACE \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
VECTOR="$ROOT/scripts/ci/check-adr-coverage.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

# seed <dir> <members-list>
seed() {
  local dir="$1" members="$2"
  mkdir -p "$dir/scripts/ci" "$dir/docs/adr" "$dir/crates/nika-old/src"
  cp "$VECTOR" "$dir/scripts/ci/check-adr-coverage.sh"
  printf '[workspace]\nmembers = [%s]\n' "$members" >"$dir/Cargo.toml"
  printf '[package]\nname = "nika-old"\n' >"$dir/crates/nika-old/Cargo.toml"
  # The pre-existing crate is covered, so `missing` is empty unless a new
  # crate joins members.
  printf '# ADR-001\n\nAdmits nika-old.\n' >"$dir/docs/adr/adr-001-old.md"
  git -C "$dir" init -q
  git -C "$dir" add -A
  git -C "$dir" -c user.email=t@e -c user.name=t commit -qm base
}

# stage_new_crate <dir> <crate> <with-adr(y|n)>
stage_new_crate() {
  local dir="$1" crate="$2" adr="$3"
  mkdir -p "$dir/crates/$crate/src"
  printf '[package]\nname = "%s"\n' "$crate" >"$dir/crates/$crate/Cargo.toml"
  [ "$adr" = y ] && printf '# ADR-002\n\nAdmits %s.\n' "$crate" >"$dir/docs/adr/adr-002-new.md"
  git -C "$dir" add -A
}

# expect <want-rc> <label> <members> <stage(none|no-adr|with-adr)> [extra-args]
expect() {
  local wantrc="$1" label="$2" members="$3" stage="$4" extra="${5:-}"
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  seed "$dir" "$members"
  case "$stage" in
    no-adr) stage_new_crate "$dir" nika-newthing n ;;
    with-adr) stage_new_crate "$dir" nika-newthing y ;;
  esac
  local rc
  # shellcheck disable=SC2086
  (cd "$dir" && bash scripts/ci/check-adr-coverage.sh $extra >/dev/null 2>&1)
  rc=$?
  if [ "$rc" = "$wantrc" ]; then
    printf '  ok   %s (rc=%d)\n' "$label" "$rc"
  else
    fails=$((fails + 1))
    printf '  FAIL %s — wanted rc=%s, got rc=%d\n' "$label" "$wantrc" "$rc" >&2
  fi
}

MEMBERS_OLD='"crates/nika-old"'
MEMBERS_BOTH='"crates/nika-old", "crates/nika-newthing"'

echo "adr-coverage · mutation proof"

# THE case: the crate is staged but not yet a member, so it never enters
# `missing` and the old nested check could not run at all.
expect 1 "new crate staged, not yet a member, NO adr" "$MEMBERS_OLD" no-adr
expect 1 "same, --staged-only (the pre-commit form)" "$MEMBERS_OLD" no-adr --staged-only

# Controls, both directions.
expect 1 "new crate IS a member, NO adr" "$MEMBERS_BOTH" no-adr
expect 0 "new crate staged WITH an adr naming it" "$MEMBERS_OLD" with-adr
expect 0 "same, --staged-only" "$MEMBERS_OLD" with-adr --staged-only
expect 0 "nothing staged — the pre-push shape" "$MEMBERS_OLD" none --staged-only

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — the blocking branch does not reach.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
