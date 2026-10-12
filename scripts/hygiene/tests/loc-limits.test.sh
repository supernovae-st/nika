#!/usr/bin/env bash
# COVERS: scripts/ci/check-loc-limits.sh scripts/ci/file-size-policy.sh
#
# Mutation proof for the CI file ratchet (ADR-154): a tracked .rs file may hold
# 2,500 lines and no more. A file above the 1,500-line warning passes here: the
# warning lives in the hygiene dashboard (vector 12) and never blocks. A file
# above 2,500 lines fails with its count and the limit.
set -uo pipefail

unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_NAMESPACE \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

# expect <want-rc> <label> <n> [<needle>] tracks one n-line .rs file in a
# scratch repository and runs the ratchet there; the ratchet sources _lib.sh,
# whose filters prove themselves, so the whole scripts/ci directory comes along.
expect() {
  local want="$1" label="$2" n="$3" needle="${4:-}"
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  mkdir -p "$dir/scripts" "$dir/crates/demo/src"
  cp -R "$ROOT/scripts/ci" "$dir/scripts/ci"
  awk -v n="$n" 'BEGIN { for (i = 0; i < n; i++) print "// line" }' \
    >"$dir/crates/demo/src/lib.rs"
  git -C "$dir" init -q
  git -C "$dir" add crates
  local out rc
  out=$(cd "$dir" && bash scripts/ci/check-loc-limits.sh 2>&1)
  rc=$?
  if [ "$rc" != "$want" ]; then
    fails=$((fails + 1))
    printf '  FAIL %s — wanted rc=%s, got rc=%d\n%s\n' "$label" "$want" "$rc" "$out" >&2
    return
  fi
  if [ -n "$needle" ] && [[ "$out" != *"$needle"* ]]; then
    fails=$((fails + 1))
    printf '  FAIL %s — output lacks %s\n%s\n' "$label" "$needle" "$out" >&2
    return
  fi
  printf '  ok   %s (rc=%d)\n' "$label" "$rc"
}

echo "CI file ratchet · limit mutation proof"

expect 0 'a file above the 1,500-line warning passes the CI ratchet' 1501
expect 0 'a file of exactly 2,500 lines passes' 2500
expect 1 'a file above 2,500 lines fails with its count and the limit' 2501 \
  'crates/demo/src/lib.rs  2501 LOC (over the 2500-line limit without a LOC-EXEMPT marker)'

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — the CI file limit moved.\n' "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
