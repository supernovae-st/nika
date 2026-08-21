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
