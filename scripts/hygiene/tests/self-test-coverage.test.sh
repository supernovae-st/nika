#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-hygiene-self-tests.sh
#
# Mutation proof for vector 49 — the vector that demands proofs, which had
# none of its own.
#
# It used to end at `OK: N hygiene self-test(s) passed`. That counts tests
# that EXIST; it reads identically whether 9 guards are proven or 45 are. The
# board carried 47 registered vectors and 9 proofs and said nothing about the
# other 38 — the same offscreen-denominator shape as the guards it watches.
#
# So the cases below pin the three verdicts that number can produce, and the
# two ways a coverage figure can quietly inflate: a proof that declares
# nothing, and a proof that declares a guard which is no longer there. The
# second is the exact class this whole board exists to catch — a declaration
# that outlived its subject.
#
# shellcheck disable=SC2329
set -uo pipefail

unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_NAMESPACE \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
VECTOR="$ROOT/scripts/hygiene/check-hygiene-self-tests.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

# seed <dir> — a miniature board: two registered vectors, both real files.
seed() {
  local dir="$1"
  mkdir -p "$dir/scripts/hygiene/tests"
  cp "$VECTOR" "$dir/scripts/hygiene/"
  printf '#!/usr/bin/env bash\nexit 0\n' >"$dir/scripts/hygiene/check-alpha.sh"
  printf '#!/usr/bin/env bash\nexit 0\n' >"$dir/scripts/hygiene/check-beta.sh"
  chmod +x "$dir/scripts/hygiene/"*.sh
  {
    printf '#!/usr/bin/env bash\n'
    printf 'run_check "1 alpha" "check-alpha.sh"\n'
    printf 'run_check "2 beta " "check-beta.sh"\n'
  } >"$dir/scripts/hygiene/check-all.sh"
}

# add_test <dir> <name> <covers-line-or-EMPTY> <exit>
add_test() {
  local dir="$1" name="$2" covers="$3" rc="$4"
  {
    printf '#!/usr/bin/env bash\n'
    [ "$covers" = EMPTY ] || printf '# COVERS: %s\n' "$covers"
    printf 'exit %s\n' "$rc"
  } >"$dir/scripts/hygiene/tests/$name"
}

# expect <want-rc> <label> <builder>
expect() {
  local want="$1" label="$2" builder="$3"
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  seed "$dir"
  "$builder" "$dir"
  local rc
  bash "$dir/scripts/hygiene/check-hygiene-self-tests.sh" >/dev/null 2>&1
  rc=$?
  if [ "$rc" = "$want" ]; then
    printf '  ok   %s (rc=%d)\n' "$label" "$rc"
  else
    fails=$((fails + 1))
    printf '  FAIL %s — wanted rc=%s, got rc=%d\n' "$label" "$want" "$rc" >&2
  fi
}

# ---- builders -------------------------------------------------------------
all_proven() {
  add_test "$1" a.test.sh scripts/hygiene/check-alpha.sh 0
  add_test "$1" b.test.sh scripts/hygiene/check-beta.sh 0
}
one_unproven() {
  add_test "$1" a.test.sh scripts/hygiene/check-alpha.sh 0
}
no_declaration() {
  add_test "$1" a.test.sh scripts/hygiene/check-alpha.sh 0
  add_test "$1" b.test.sh EMPTY 0
}
stale_declaration() {
  add_test "$1" a.test.sh scripts/hygiene/check-alpha.sh 0
  add_test "$1" b.test.sh scripts/hygiene/check-gone.sh 0
}
a_test_fails() {
  add_test "$1" a.test.sh scripts/hygiene/check-alpha.sh 0
  add_test "$1" b.test.sh scripts/hygiene/check-beta.sh 1
}
no_tests_at_all() { :; }

echo "vector 49 · mutation proof"

expect 0 "every board vector proven" all_proven
expect 1 "a vector with no proof — the ratchet, listed" one_unproven
expect 2 "a proof that declares nothing" no_declaration
expect 2 "a proof declaring a guard that is gone" stale_declaration
expect 2 "a self-test that fails (control)" a_test_fails
expect 1 "no self-tests at all (control)" no_tests_at_all

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — the coverage figure can still inflate.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
