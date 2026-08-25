#!/usr/bin/env bash
# test-strip-test-items.sh — the shared filter self-tests before it judges.
#
# `strip_test_items` decides what FOUR ratchets are allowed to see:
# .unwrap(), .expect(, dead-code, and error-one-voice. A bug here does not
# make a gate fail loudly — it makes a gate pass QUIETLY, which is the
# worse direction. On 2026-08-02 exactly that was proven: an unbalanced
# brace inside a string literal in a #[cfg(test)] item left the filter's
# skip state stuck on, blanking every remaining line of the file, so a
# real production `.unwrap()` counted 1 in the file and 0 through the
# filter. 223 of 664 production files carried the trigger shape.
#
# So the filter is pinned by cases, both directions: production code must
# survive, test code must be hidden. Run before the ratchets in CI.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=./_lib.sh
. "$HERE/_lib.sh"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
fails=0

# case <name> <expected-count> <needle> <<< source
case_is() {
  local name="$1" want="$2" needle="$3" body="$4"
  printf '%s' "$body" >"$TMP/case.rs"
  local got
  got=$(strip_test_items "$TMP/case.rs" | grep -c -- "$needle" || true)
  if [ "$got" != "$want" ]; then
    # shellcheck disable=SC2016
    printf 'FAIL  %s — expected %s occurrence(s) of `%s` through the filter, saw %s\n' \
      "$name" "$want" "$needle" "$got" >&2
    fails=$((fails + 1))
  else
    printf 'ok    %s\n' "$name"
  fi
}

# --- the filter must HIDE genuine test code -------------------------------
case_is 'a #[cfg(test)] mod is hidden' 0 'unwrap()' \
  '#[cfg(test)]
mod t {
    #[test]
    fn x() { let v: Option<u8> = None; let _ = v.unwrap(); }
}
'

case_is 'a bare #[test] fn is hidden' 0 'unwrap()' \
  '#[test]
fn x() {
    let v: Option<u8> = None;
    let _ = v.unwrap();
}
'

# --- a #[cfg(test)] DECLARATION must not swallow the next block (#1207) ---
#
# The bug this pins never lived here — it lived in three private copies of
# this rule, one of which CI ran as the `expect` ratchet. Each read
# `#[cfg(test)]` as an attribute waiting for a brace, so `mod tests;` left
# the flag pending and the NEXT block became the "test" region. Measured on
# two fixtures differing only in the module's shape: the shipped reader saw
# 0 where this filter saw 1.
#
# The copies are gone and both ratchets read through here now, which makes
# this the only place the behaviour can regress. It was correct and untested
# for as long as it has existed; correct-and-untested is one edit from wrong.
case_is 'a #[cfg(test)] mod DECLARATION does not hide the code after it' 1 'unwrap()' \
  '#[cfg(test)]
mod tests;

pub fn production() -> String {
    std::env::var("HOME").unwrap()
}
'

case_is 'a #[cfg(test)] mod DECLARATION does not hide a later .expect(' 1 'expect(' \
  '#[cfg(test)]
mod tests;

pub fn production() -> String {
    std::env::var("HOME").expect("HOME must be set")
}
'

# The same shape with a pub modifier and a doc comment between the attribute
# and the declaration — the pending flag must still end at the `;`.
case_is 'a documented pub mod DECLARATION does not hide the code after it' 1 'unwrap()' \
  '#[cfg(test)]
pub mod tests;

/// Production, and it stays visible.
pub fn production() -> String {
    std::env::var("HOME").unwrap()
}
'

# --- the cfg composites, and the one that must NOT hide (#1207) -----------
#
# The python copies this filter replaced accepted `all`/`any` around `test`
# and rejected `not`. Teaching that here was not optional: nika-harness
# guards its hermetic adapter fixtures with `#[cfg(all(test, unix))]`, so a
# filter that only knew the bare attribute reported ten lines of real test
# code as production the moment the ratchets started reading through it.
# NO `#[test]` inside these two. A bare `#[test]` hides its own fn body, so a
# fixture carrying one passes whether or not the composite attribute is
# understood — it proves the predicate without proving it is wired. The
# helper below is reachable ONLY through the `cfg` line above it.
case_is 'a #[cfg(all(test, unix))] mod is hidden' 0 'expect(' \
  '#[cfg(all(test, unix))]
mod t {
    fn helper() { let _ = std::env::var("HOME").expect("HOME"); }
}
'

case_is 'a #[cfg(any(test, feature = "x"))] mod is hidden' 0 'unwrap()' \
  '#[cfg(any(test, feature = "x"))]
mod t {
    fn helper() { let v: Option<u8> = None; let _ = v.unwrap(); }
}
'

# `all(test)` with no second predicate. Valid Rust, and the first draft of
# this fix dropped it: the trailing class admitted space and comma but not
# `)`, so test code read as production. It went unnoticed because the same
# class was ALSO what kept `not(test)` out — two guards for one job, and the
# weak one hides behind the strong one until a fixture separates them.
case_is 'a #[cfg(all(test))] mod is hidden' 0 'unwrap()' \
  '#[cfg(all(test))]
mod t {
    fn helper() { let v: Option<u8> = None; let _ = v.unwrap(); }
}
'

# The dangerous direction. `not(test)` guards code that ships; hiding it
# would silence the ratchet exactly where it is most load-bearing. POSIX awk
# has no negative lookahead, so the composite arm admits `all` and `any` and
# nothing else — this case is what proves that construction holds.
case_is 'a #[cfg(not(test))] mod is PRODUCTION and stays visible' 1 'unwrap()' \
  '#[cfg(not(test))]
mod shipped {
    pub fn f() { let v: Option<u8> = None; let _ = v.unwrap(); }
}
'

# --- the filter must SHOW production code ---------------------------------
case_is 'plain production code survives' 1 'unwrap()' \
  'fn prod() { let x: Option<u8> = None; let _ = x.unwrap(); }
'

# THE REGRESSION (2026-08-02): one brace in a string used to swallow the
# rest of the file, silencing every ratchet downstream of it.
case_is 'an unbalanced brace in a test string does not swallow the file' 1 'unwrap()' \
  '#[cfg(test)]
fn helper() {
    let weird = "oops {";
}

fn prod() { let x: Option<u8> = None; let _ = x.unwrap(); }
'

case_is 'a closing brace in a test string does not end the skip early' 0 'unwrap()' \
  '#[cfg(test)]
mod t {
    fn helper() { let weird = "}"; }
    fn x() { let v: Option<u8> = None; let _ = v.unwrap(); }
}
'

case_is 'a lifetime is not a char literal' 1 'unwrap()' \
  "#[cfg(test)]
fn h<'a>(s: &'a str) -> &'a str { s }

fn prod() { let x: Option<u8> = None; let _ = x.unwrap(); }
"

case_is 'a brace char literal does not desync' 1 'unwrap()' \
  "#[cfg(test)]
fn h() { let _ = '{'; }

fn prod() { let x: Option<u8> = None; let _ = x.unwrap(); }
"

case_is 'a raw string keeps its braces to itself' 1 'unwrap()' \
  '#[cfg(test)]
fn h() { let _ = r#"a raw { brace"#; }

fn prod() { let x: Option<u8> = None; let _ = x.unwrap(); }
'

case_is 'a block comment brace does not desync' 1 'unwrap()' \
  '#[cfg(test)]
fn h() { /* a { comment */ }

fn prod() { let x: Option<u8> = None; let _ = x.unwrap(); }
'

if [ "$fails" -gt 0 ]; then
  printf '\nFAIL  %d strip_test_items case(s) — the four ratchets downstream cannot be trusted\n' "$fails" >&2
  exit 1
fi

echo "OK  strip_test_items honest in both directions"
