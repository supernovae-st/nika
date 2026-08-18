#!/usr/bin/env bash
# test-rs-prod-files.sh — the FILE-level filter self-tests before it judges.
#
# Sibling of test-strip-test-items.sh, which pins the IN-FILE filter. This one
# pins `rs_test_only_files` / `rs_prod_files`: which whole FILES four ratchets
# (crate-size, unwrap, expect, dead-code) are allowed to see.
#
# `rs_prod_files` documented itself as mirroring clippy's `#[cfg(test)]`
# exclusion and delivered only the `tests.rs` BASENAME half. Measured
# 2026-08-18: six files declared `#[cfg(test)] mod <name>;` — among them
# `nika-runtime/src/adversarial/mod.rs`, whose own doc-comment reads
# "Test-only: no public surface, no production code" — were charged to the
# production budget of five crates. The direction of that bug is the bad one:
# it does not fail a gate loudly, it inflates a budget quietly.
#
# Both directions are pinned here, and the LAST case is the one that matters:
# a module declared under `#[cfg(test)]` in one place AND normally in another
# must stay PRODUCTION. Widening an exclusion without tightening its exemption
# is how a correction becomes a hole.
# shellcheck disable=SC2329  # rs_src_files is invoked indirectly, through rs_test_only_files
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export NIKA_SKIP_FILTER_SELFTEST=1 # we source the lib; it must not recurse
# shellcheck source-path=SCRIPTDIR
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fails=0
cases=0

# The fixtures live under $TMP; rs_src_files is replaced below so the filter is
# tested on a KNOWN file set instead of the repo's — no git fixture needed, and
# no dependency on what the workspace happens to contain today.
#
# expect <hidden|shown> <relative-path> <label>
expect() {
  local want="$1" path="$2" label="$3"
  cases=$((cases + 1))
  local hidden
  hidden=$(cd "$TMP" && rs_test_only_files | grep -cxF "$path" || true)
  local got="shown"
  [ "$hidden" != "0" ] && got="hidden"
  if [ "$got" = "$want" ]; then
    printf '  ok   %s (%s)\n' "$label" "$got"
  else
    fails=$((fails + 1))
    printf '  FAIL %s — wanted %s, got %s\n' "$label" "$want" "$got" >&2
  fi
}

mkdir -p "$TMP/src/dirmod" "$TMP/src/both"

# The declaring roots.
cat >"$TMP/src/lib.rs" <<'RS'
#[cfg(test)]
mod solo;

#[cfg(test)]
#[path = "elsewhere.rs"]
mod attributed;

#[cfg(test)]
mod dirmod;

mod production;

#[cfg(test)]
use std::fmt::Debug;

#[cfg(test)]
mod both;
RS

cat >"$TMP/src/both/mod.rs" <<'RS'
// Declared #[cfg(test)] in lib.rs and plainly in main.rs — BOTH resolve to the
// SAME root (`src/both`), which is the only shape where the exemption can fire.
RS

# The production half of the both-ways pair. `main.rs` is a module ROOT like
# lib.rs/mod.rs, so its declarations resolve into `src/` — the same directory
# lib.rs declares into. An earlier version of this fixture put the second
# declaration in `src/other.rs`, whose declarations resolve into `src/other/`:
# two DIFFERENT roots, no overlap, and the case passed with the exemption
# deleted. It was a decorative assertion; the mutation below is what caught it.
cat >"$TMP/src/main.rs" <<'RS'
mod both;
RS

: >"$TMP/src/solo.rs"
: >"$TMP/src/attributed.rs"
: >"$TMP/src/production.rs"
: >"$TMP/src/dirmod/mod.rs"
: >"$TMP/src/dirmod/child.rs"
: >"$TMP/src/tests.rs"

# rs_src_files normally shells out to `git ls-files`; replace it with the
# fixture list so the unit under test is the DECLARATION logic, not git.
rs_src_files() {
  printf '%s\n' \
    src/lib.rs src/main.rs src/solo.rs src/attributed.rs src/production.rs \
    src/dirmod/mod.rs src/dirmod/child.rs src/both/mod.rs src/tests.rs
}

echo "rs_prod_files · file-level filter self-test"

# --- the filter must HIDE test-only modules --------------------------------
expect hidden src/solo.rs 'a #[cfg(test)] mod hides its file'
expect hidden src/attributed.rs 'an attribute between cfg(test) and mod does not break the pairing'
expect hidden src/dirmod/mod.rs 'a #[cfg(test)] dir-module hides its root'
expect hidden src/dirmod/child.rs 'a #[cfg(test)] dir-module hides its subtree'

# --- the filter must SHOW production code ----------------------------------
expect shown src/production.rs 'a plain mod stays production'
expect shown src/lib.rs 'the declaring file itself stays production'

# --- THE case: the exemption is tightened, not merely widened --------------
expect shown src/both/mod.rs 'a module declared BOTH ways stays production'

# --- the older half must not regress ---------------------------------------
# `tests.rs` is excluded by BASENAME in rs_prod_files, not by declaration; it
# is pinned here so a future refactor cannot drop one half while fixing the
# other.
if rs_prod_files | grep -qxF src/tests.rs; then
  fails=$((fails + 1))
  cases=$((cases + 1))
  printf '  FAIL the tests.rs basename half regressed — it reached rs_prod_files\n' >&2
else
  cases=$((cases + 1))
  printf '  ok   the tests.rs basename half still holds\n'
fi

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — a budget inflated quietly is worse than a gate that fails loudly.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
