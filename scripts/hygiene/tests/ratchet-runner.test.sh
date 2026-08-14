#!/usr/bin/env bash
# COVERS: scripts/hooks/run-ci-ratchets.sh scripts/ci/check-tests.sh
# Mutation proof for the two vectors that used to narrow their own scope
# and stay green doing it — run-ci-ratchets.sh and check-tests.sh.
#
# run-ci-ratchets counted the ratchets it ATTEMPTED, not the ones it
# DECLARES. A ratchet that was missing, or merely not executable, printed a
# WARNING, hit `continue`, and dropped out of the count — so `chmod 644` on
# one file turned "all 9 ratchets passed" into "all 8 ratchets passed",
# exit 0, push accepted. The number stayed arithmetically true, which is
# precisely why it would never look wrong.
#
# check-tests judged the environment and the tool with one `&&`, so a
# missing cargo-nextest read the same as "not CI" and fell through to
# `--lib` — reinstating, in CI, the 2026-07-06 regression its own header
# says was closed.
#
# Nothing here compiles: cargo is a shim that prints its argv, so the cases
# assert on WHICH command would have run.
#
# Every builder reaches its call site through `expect`, which takes the
# function name as an argument; shellcheck cannot see that (SC2329).
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

RATCHETS='loc-limits crate-size fn-length unwrap expect dead-code
no-default-features adr-coverage credential-headers'

# A tree with all nine declared ratchets present, executable, and green.
seed_runner() {
  local dir="$1"
  mkdir -p "$dir/scripts/hooks" "$dir/scripts/ci"
  cp "$ROOT/scripts/hooks/run-ci-ratchets.sh" "$dir/scripts/hooks/"
  printf '#!/usr/bin/env bash\nexit 0\n' >"$dir/scripts/ci/test-strip-test-items.sh"
  chmod +x "$dir/scripts/ci/test-strip-test-items.sh"
  for r in $RATCHETS; do
    printf '#!/usr/bin/env bash\necho "OK %s"\nexit 0\n' "$r" >"$dir/scripts/ci/check-$r.sh"
    chmod +x "$dir/scripts/ci/check-$r.sh"
  done
}

runner_all_green() { seed_runner "$1"; }
runner_one_not_executable() {
  seed_runner "$1"
  chmod -x "$1/scripts/ci/check-expect.sh"
}
runner_one_deleted() {
  seed_runner "$1"
  rm -f "$1/scripts/ci/check-expect.sh"
}
runner_one_fails() {
  seed_runner "$1"
  printf '#!/usr/bin/env bash\necho "boom"\nexit 1\n' >"$1/scripts/ci/check-expect.sh"
  chmod +x "$1/scripts/ci/check-expect.sh"
}

# expect_runner <RED|GREEN> <label> <builder>
expect_runner() {
  local want="$1" label="$2" builder="$3"
  cases=$((cases + 1))
  local dir="$WORK/r-$cases"
  "$builder" "$dir"
  local out rc
  out="$(bash "$dir/scripts/hooks/run-ci-ratchets.sh" 2>&1)"
  rc=$?
  local got="RED"
  [ "$rc" -eq 0 ] && got="GREEN"
  if [ "$got" = "$want" ]; then
    printf '  ok   [ratchets] %s (%s, rc=%d)\n' "$label" "$got" "$rc"
  else
    fails=$((fails + 1))
    printf '  FAIL [ratchets] %s — wanted %s, got %s (rc=%d)\n%s\n' \
      "$label" "$want" "$got" "$rc" "$out" >&2
  fi
}

# ---- check-tests ----------------------------------------------------------
# cargo is a shim: it prints the command line instead of running it.
seed_tests() { # $1 dir  $2 with-nextest(yes|no)
  local dir="$1"
  mkdir -p "$dir/scripts/ci" "$dir/bin"
  cp "$ROOT/scripts/ci/check-tests.sh" "$dir/scripts/ci/"
  printf '[workspace]\nmembers = ["crates/demo"]\n' >"$dir/Cargo.toml"
  printf '#!/usr/bin/env bash\nprintf "CARGO %%s\\n" "$*"\nexit 0\n' >"$dir/bin/cargo"
  chmod +x "$dir/bin/cargo"
  if [ "$2" = yes ]; then
    printf '#!/usr/bin/env bash\nexit 0\n' >"$dir/bin/cargo-nextest"
    chmod +x "$dir/bin/cargo-nextest"
  fi
}

# expect_tests <label> <ci(1|"")> <nextest(yes|no)> <want-rc-zero(y|n)> <want-substring>
expect_tests() {
  local label="$1" ci="$2" nx="$3" wantok="$4" needle="$5"
  cases=$((cases + 1))
  local dir="$WORK/t-$cases"
  seed_tests "$dir" "$nx"
  local out rc
  out="$(cd "$dir" && PATH="$dir/bin:/usr/bin:/bin" CI="$ci" bash scripts/ci/check-tests.sh 2>&1)"
  rc=$?
  local ok=y
  [ "$rc" -eq 0 ] || ok=n
  # `--` matters: a needle like `--workspace --lib` is otherwise read as
  # grep's own options.
  if [ "$ok" = "$wantok" ] && printf '%s' "$out" | grep -qF -- "$needle"; then
    printf '  ok   [check-tests] %s (rc=%d)\n' "$label" "$rc"
  else
    fails=$((fails + 1))
    printf '  FAIL [check-tests] %s — wanted rc-zero=%s and %s; got rc=%d, out: %s\n' \
      "$label" "$wantok" "$needle" "$rc" "$(printf '%s' "$out" | tr '\n' ' ')" >&2
  fi
}

echo "scope-narrowing vectors · mutation proof"

expect_runner GREEN "all nine declared ratchets run" runner_all_green
expect_runner RED "one ratchet not executable" runner_one_not_executable
expect_runner RED "one ratchet missing" runner_one_deleted
expect_runner RED "one ratchet genuinely fails (control)" runner_one_fails

expect_tests "CI + nextest -> the FULL battery" 1 yes y "nextest run --workspace"
expect_tests "CI, nextest ABSENT -> refuses to narrow" 1 no n "Refusing to fall back"
expect_tests "local + nextest -> --lib, as the Keychain law wants" "" yes y "--workspace --lib"
expect_tests "local, no nextest -> --lib but SAYS so" "" no y "tests/ integration suites are NOT executed"

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — a vector narrows its scope without saying so.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
