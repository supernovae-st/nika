#!/usr/bin/env bash
# COVERS: scripts/ci/_lib.sh
# Mutation proof for scripts/ci/_lib.sh — the file thirteen ratchets source.
#
# Two properties, both of which it failed until 2026-08-14, and neither of
# which had a permanent test: the fix shipped with a side-by-side proof and
# nothing to keep it honest afterwards.
#
#   1. FAIL CLOSED. The filter self-test is what four ratchets (.unwrap,
#      .expect, dead-code, error-one-voice) stand on. `_lib.sh` used to skip
#      it and report success when it was absent OR merely not executable, so
#      one mode bit disarmed all four and they went on printing OK.
#
#   2. SET NO OPTIONS. It is SOURCED, so `set -euo pipefail` in it
#      reconfigured the CALLER. Six hygiene vectors declare `set -uo
#      pipefail` with errexit off deliberately; every one had it forced back
#      on, which killed them at the first command that reported a finding
#      and left their `exit 2` unreachable.
#
# shellcheck disable=SC2329
set -uo pipefail

unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_NAMESPACE \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

# A scratch scripts/ci/ holding _lib.sh, its self-test, and one consumer.
# selftest: ok | noexec | gone | gone-files    filter: honest | broken
#
# `gone-files` removes the SECOND self-test. `_lib.sh` ships two filters —
# `strip_test_items` (hides test items inside a file) and `rs_prod_files`
# (hides whole files) — and since 2026-08-18 it proves BOTH before any ratchet
# reads a verdict. Seeding only the first one is what this fixture used to do,
# and it is exactly how a new proof requirement gets silently un-enforced.
seed() {
  local dir="$1" selftest="$2" filter="$3"
  mkdir -p "$dir/scripts/ci" "$dir/crates/demo/src"
  cp "$ROOT/scripts/ci/_lib.sh" "$dir/scripts/ci/"
  cp "$ROOT/scripts/ci/test-strip-test-items.sh" "$dir/scripts/ci/"
  cp "$ROOT/scripts/ci/test-rs-prod-files.sh" "$dir/scripts/ci/"
  cp "$ROOT/scripts/ci/check-unwrap.sh" "$dir/scripts/ci/"
  chmod +x "$dir/scripts/ci/"*.sh
  if [ "$filter" = broken ]; then
    # Re-define the filter to blank every line — the 2026-08-02 shape, where
    # a stuck skip state hid a real production .unwrap() from the ratchet.
    cat >>"$dir/scripts/ci/_lib.sh" <<'BROKEN'

strip_test_items() { awk '{ print "" }' "${1:-/dev/stdin}"; }
BROKEN
  fi
  case "$selftest" in
    noexec) chmod -x "$dir/scripts/ci/test-strip-test-items.sh" ;;
    gone) rm -f "$dir/scripts/ci/test-strip-test-items.sh" ;;
    gone-files) rm -f "$dir/scripts/ci/test-rs-prod-files.sh" ;;
  esac
  printf 'pub fn boom() { let x: Option<u8> = None; let _ = x.unwrap(); }\n' \
    >"$dir/crates/demo/src/lib.rs"
  git -C "$dir" init -q
  git -C "$dir" add -A
}

# expect_rc <want> <label> <selftest> <filter>
expect_rc() {
  local want="$1" label="$2" st="$3" filter="$4"
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  seed "$dir" "$st" "$filter"
  local rc
  (cd "$dir" && bash scripts/ci/check-unwrap.sh >/dev/null 2>&1)
  rc=$?
  if [ "$rc" = "$want" ]; then
    printf '  ok   %s (rc=%d)\n' "$label" "$rc"
  else
    fails=$((fails + 1))
    printf '  FAIL %s — wanted rc=%s, got rc=%d\n' "$label" "$want" "$rc" >&2
  fi
}

echo "ci/_lib.sh · mutation proof"

# --- 1. the honesty proof must fail CLOSED --------------------------------
# A real production .unwrap() is planted in every case, so rc=1 means the
# ratchet SAW it and rc=2 means the ratchet refused to judge. rc=0 — "OK
# zero .unwrap()" — is the false green this file exists to prevent.
expect_rc 2 "broken filter, self-test present" ok broken
expect_rc 2 "broken filter, self-test NOT EXECUTABLE" noexec broken
expect_rc 2 "broken filter, self-test DELETED" gone broken
expect_rc 2 "honest filter, self-test DELETED (cannot prove itself)" gone honest
# The SECOND filter's proof is load-bearing too. `_lib.sh` gained rs_prod_files'
# self-test on 2026-08-18; without this case, deleting that file would leave
# every ratchet judging on an unproven file-level filter and still printing OK.
expect_rc 2 "honest filter, the FILE-level self-test DELETED" gone-files honest
# Negative control: nothing wrong, and the real .unwrap() is still found.
expect_rc 1 "honest filter, self-test present — finds the unwrap" ok honest

# --- 2. sourcing must not reconfigure the caller ---------------------------
# The caller below declares errexit OFF, exactly as the six hygiene vectors
# do, then runs a command that fails. Under a leaked `set -e` the script
# dies there and never reaches its verdict.
cases=$((cases + 1))
CALLER="$WORK/caller.sh"
cat >"$CALLER" <<CALLER_EOF
set -uo pipefail
. "$ROOT/scripts/ci/_lib.sh"
false
grep -q 'no-such-needle' /dev/null
echo REACHED_THE_VERDICT
exit 2
CALLER_EOF
out="$(NIKA_SKIP_FILTER_SELFTEST=1 bash "$CALLER" 2>&1)"
rc=$?
if [ "$rc" = 2 ] && printf '%s' "$out" | grep -q REACHED_THE_VERDICT; then
  printf '  ok   a caller with errexit OFF keeps it off (rc=%d)\n' "$rc"
else
  fails=$((fails + 1))
  printf '  FAIL sourcing _lib.sh turned errexit back on — wanted rc=2 and the\n       verdict line; got rc=%d, out: %s\n' \
    "$rc" "$(printf '%s' "$out" | tr '\n' ' ')" >&2
fi

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — the shared ratchet library is not trustworthy.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
