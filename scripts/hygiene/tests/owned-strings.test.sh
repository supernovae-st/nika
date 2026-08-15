#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-owned-strings.sh
#
# Mutation proof for vector 31 (nika-catalog public API uses `&'static str`
# or owned String — ADR-008).
#
# Found by sweeping the unproven vectors for the three causes the
# 2026-08-13 audit named. It carried cause 3: `TARGET=crates/<one>/src`
# followed by `[ ! -d "$TARGET" ] && echo "not found — skipping" && exit 0`.
#
# That is not hypothetical. It is the SAME shape, line for line, that made
# check-cancel-safety guard 0 of 94 async fn and check-kernel-no-spawn guard
# 2% of its layer when nika-kernel was split in June — both reporting OK
# throughout. A rename of nika-catalog would have done it here.
#
# So the case that matters below is the missing subject: a vector that
# cannot find what it watches has not cleared it.
#
# shellcheck disable=SC2329
set -uo pipefail

unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_NAMESPACE \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
VECTOR="$ROOT/scripts/hygiene/check-owned-strings.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

seed() { # <dir>
  mkdir -p "$1/scripts/hygiene" "$1/crates/nika-catalog/src"
  cp "$VECTOR" "$1/scripts/hygiene/"
  chmod +x "$1/scripts/hygiene/check-owned-strings.sh"
}

# expect <want-rc> <label> <builder>
expect() {
  local want="$1" label="$2" builder="$3"
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  seed "$dir"
  "$builder" "$dir"
  local rc
  bash "$dir/scripts/hygiene/check-owned-strings.sh" >/dev/null 2>&1
  rc=$?
  if [ "$rc" = "$want" ]; then
    printf '  ok   %s (rc=%d)\n' "$label" "$rc"
  else
    fails=$((fails + 1))
    printf '  FAIL %s — wanted rc=%s, got rc=%d\n' "$label" "$want" "$rc" >&2
  fi
}

# The detector anchors on `^pub <ident>:` after stripping indentation, so a
# field must sit on its own line — ordinary rustfmt output. A struct written
# on ONE line matches nothing, which would make the GREEN case pass for the
# wrong reason: not "no violation found" but "nothing was ever looked at".
clean_api() {
  cat >"$1/crates/nika-catalog/src/lib.rs" <<'RS'
pub struct A {
    pub name: &'static str,
    pub owned: String,
}
RS
}
borrowed_field() {
  cat >"$1/crates/nika-catalog/src/lib.rs" <<'RS'
pub struct A<'a> {
    pub name: &'a str,
}
RS
}
exempt_borrow() {
  cat >"$1/crates/nika-catalog/src/lib.rs" <<'RS'
pub struct A<'a> {
    // OWNED-STRINGS-EXEMPT: fixture
    pub name: &'a str,
}
RS
}
subject_gone() {
  clean_api "$1"
  rm -rf "$1/crates/nika-catalog"
}

echo "vector 31 · mutation proof"

expect 0 "static-str and String only" clean_api
expect 2 "a non-static borrowed pub field" borrowed_field
expect 0 "the same borrow, with an EXEMPT marker" exempt_borrow
expect 2 "the crate is GONE — must fail CLOSED, not skip" subject_gone

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — the vector does not catch what it claims.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
