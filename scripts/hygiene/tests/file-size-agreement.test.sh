#!/usr/bin/env bash
# COVERS: scripts/ci/file-size-policy.sh scripts/ci/check-loc-limits.sh scripts/hygiene/check-file-loc.sh
#
# The CI file ratchet and hygiene vector 12 must judge a file the same way, as ADR-154's table
# says: a whitelisted LOC-EXEMPT marker (codegen, lookup-table, enum-mega, on one of the first 5
# lines) carries a file above 2,500 lines up to 5,000, never above; an unmarked file and an
# unlisted reason are refused above 2,500. Each fixture runs through BOTH gates: they must reach
# the expected verdict, and a refused file must get the same sentence from each, since one
# policy (scripts/ci/file-size-policy.sh) decides and explains for both.
set -uo pipefail

unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_NAMESPACE \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

# run_gates <n> [<marker>] tracks one n-line source file (the marker first) in a scratch
# repository holding the gates, runs both, and leaves CI_RC, CI_OUT, HY_RC and HY_OUT.
run_gates() {
  local n="$1" marker="${2:-}"
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  mkdir -p "$dir/scripts/hygiene" "$dir/crates/demo/src"
  cp -R "$ROOT/scripts/ci" "$dir/scripts/ci"
  cp "$ROOT/scripts/hygiene/check-file-loc.sh" "$dir/scripts/hygiene/"
  {
    if [ -n "$marker" ]; then
      printf '%s\n' "$marker"
      n=$((n - 1))
    fi
    awk -v n="$n" 'BEGIN { for (i = 0; i < n; i++) print "// line" }'
  } >"$dir/crates/demo/src/lib.rs"
  git -C "$dir" init -q
  git -C "$dir" add crates
  CI_OUT=$(cd "$dir" && bash scripts/ci/check-loc-limits.sh 2>&1)
  CI_RC=$?
  HY_OUT=$(bash "$dir/scripts/hygiene/check-file-loc.sh" 2>&1)
  HY_RC=$?
}

# sentence <output> prints the parenthesised sentence of the demo file's line, if any.
sentence() {
  printf '%s\n' "$1" | grep -F 'crates/demo/src/lib.rs' | head -1 \
    | sed -E 's/.* LOC \((.*)\)$/\1/'
}

# expect <pass|block> <label> <n> [<marker>]
expect() {
  local want="$1" label="$2" ci hy why=''
  run_gates "$3" "${4:-}"
  case "$CI_RC" in 0) ci=pass ;; 1) ci=block ;; *) ci="error($CI_RC)" ;; esac
  case "$HY_RC" in 0 | 1) hy=pass ;; 2) hy=block ;; *) hy="error($HY_RC)" ;; esac
  [ "$ci" = "$want" ] || why="CI says $ci"
  [ "$hy" = "$want" ] || why="${why:+$why; }vector 12 says $hy"
  if [ -z "$why" ] && [ "$want" = block ]; then
    local ci_said hy_said
    ci_said=$(sentence "$CI_OUT")
    hy_said=$(sentence "$HY_OUT")
    if [ -z "$ci_said" ] || [ "$ci_said" != "$hy_said" ]; then
      why="the gates state different verdicts: CI «${ci_said}», vector 12 «${hy_said}»"
    fi
  fi
  if [ -z "$why" ]; then
    printf '  ok   %s (%s in both)\n' "$label" "$want"
  else
    fails=$((fails + 1))
    printf '  FAIL %s — wanted %s in both: %s\n' "$label" "$want" "$why" >&2
  fi
}

echo "file-size policy · the CI ratchet and vector 12 agree"

expect pass 'a codegen marker carries 2,501 lines' 2501 '// LOC-EXEMPT: codegen'
expect pass 'a lookup-table marker carries exactly 5,000 lines' 5000 '// LOC-EXEMPT: lookup-table'
expect pass 'an enum-mega marker carries 3,000 lines' 3000 '// LOC-EXEMPT: enum-mega [nika]'
expect block 'no marker carries 5,001 lines' 5001 '// LOC-EXEMPT: codegen'
expect block 'an unmarked 2,501-line file is refused' 2501
expect block 'an unlisted reason is refused' 3000 '// LOC-EXEMPT: convenience'

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — the two file gates no longer read one table.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
