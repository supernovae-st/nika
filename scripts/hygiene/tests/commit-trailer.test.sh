#!/usr/bin/env bash
# Mutation proof for the commit-msg gate's co-author trailer.
#
# The gate had two patterns and passed if EITHER matched. The second was
# `^Co-Authored-By: Nika( |$)`, where `( |$)` is an alternation INSIDE the
# pattern, not a terminator for it: match `Nika` followed by a space and
# everything to the right is unconstrained. So an impostor walked, and so
# did `Nika Claude <claude@anthropic.com>` — the one attribution the
# alignment doctrine exists to keep out, reachable by prefixing a word.
#
# A bare `Claude` trailer was rejected correctly the whole time. That is
# what made the hole invisible, and it is why the control cases below
# matter: three of them pass in both directions, so a green here means the
# probe discriminated rather than agreeing with everything.
#
# shellcheck disable=SC2329
set -uo pipefail

unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_NAMESPACE \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
HOOK="$ROOT/scripts/hooks/validate-conventional-commit.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

SUBJECT='fix(hygiene): a subject that says what changed and why it changed'
BODY='A body long enough to satisfy the body-discipline rule, describing the
change in ordinary prose across a couple of lines.'

# expect <ACCEPT|REJECT> <label> <trailer-block>
expect() {
  local want="$1" label="$2" trailer="$3"
  cases=$((cases + 1))
  local f="$WORK/msg-$cases"
  printf '%s\n\n%s\n\n%s\n' "$SUBJECT" "$BODY" "$trailer" >"$f"
  local out rc
  out="$(bash "$HOOK" "$f" 2>&1)"
  rc=$?
  local got="ACCEPT"
  [ "$rc" -eq 0 ] || got="REJECT"
  if [ "$got" = "$want" ]; then
    printf '  ok   %s (%s, rc=%d)\n' "$label" "$got" "$rc"
  else
    fails=$((fails + 1))
    printf '  FAIL %s — wanted %s, got %s (rc=%d)\n' "$label" "$want" "$got" "$rc" >&2
  fi
}

# expect_quiet <label> <needle-that-must-NOT-appear> <trailer-block>
expect_quiet() {
  local label="$1" needle="$2" trailer="$3"
  cases=$((cases + 1))
  local f="$WORK/msg-$cases"
  printf '%s\n\n%s\n\n%s\n' "$SUBJECT" "$BODY" "$trailer" >"$f"
  local out
  out="$(bash "$HOOK" "$f" 2>&1)"
  if printf '%s' "$out" | grep -qF -- "$needle"; then
    fails=$((fails + 1))
    printf '  FAIL %s — the gate still says: %s\n' "$label" "$needle" >&2
  else
    printf '  ok   %s (no "%s" warning)\n' "$label" "$needle"
  fi
}

CANON='Co-Authored-By: Nika 🦋 <nika@supernovae.studio>'
DCO='Signed-off-by: Thibaut Melen <20891897+ThibautMelen@users.noreply.github.com>'

echo "commit-msg co-author gate · mutation proof"

# --- must REJECT -----------------------------------------------------------
expect REJECT "an impostor wearing the Nika name" \
  'Co-Authored-By: Nika Impostor <evil@example.com>'
expect REJECT "vendor attribution behind a Nika prefix" \
  'Co-Authored-By: Nika Claude <claude@anthropic.com>'
expect REJECT "a bare vendor trailer (control)" \
  'Co-Authored-By: Claude <noreply@anthropic.com>'
expect REJECT "no trailer at all (control)" \
  'nothing here'
expect REJECT "the trailer only MENTIONED in prose" \
  "we always write $CANON on commits"

# --- must ACCEPT -----------------------------------------------------------
expect ACCEPT "the canonical trailer (control)" "$CANON"
expect ACCEPT "the bot form that really occurs" \
  'Co-Authored-By: nika-bot 🦋 <nika@supernovae.studio>'
expect ACCEPT "the release-bot form that really occurs" \
  'Co-Authored-By: nika-release[bot] <nika@supernovae.studio>'

# --- must not NAG ----------------------------------------------------------
# The DCO trailer this repo requires is 77 chars. It was not exempt from the
# body-line-length check, so every correctly signed commit drew a warning
# it could do nothing about.
expect_quiet "the required DCO trailer draws no length warning" \
  'Body line' "$CANON
$DCO"

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — the gate admits what it must refuse,\nor nags about what it requires.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct.\n' "$cases" "$cases"
exit 0
