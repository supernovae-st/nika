#!/usr/bin/env bash
# test-gate-lock.sh — the pre-push lease proves itself (#1064).
#
# Four properties, and the last two are the ones that matter. A lock is easy
# to write and easy to write WRONG in exactly two ways: it lets two holders in
# (and the race it was built to end continues, now invisibly), or it never
# lets go of a dead owner's lease (and one ^C wedges every later push on this
# machine forever).
#
#   1. an uncontended acquire succeeds and releases
#   2. a SECOND acquire while held does not get in — it waits, then refuses
#   3. a lease whose owner PID is gone is RECLAIMED, not waited on
#   4. a lease held by a LIVE owner is never stolen
#
# Run directly: bash scripts/hooks/test-gate-lock.sh
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=./_gate-lock.sh
. "$HERE/_gate-lock.sh"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fails=0
cases=0

ok() {
  cases=$((cases + 1))
  printf '  ok   %s\n' "$1"
}
bad() {
  cases=$((cases + 1))
  fails=$((fails + 1))
  printf '  FAIL %s\n' "$1" >&2
}

# Point the lease at the fixture instead of a real .git.
gate_lock_path() { printf '%s/nika-pre-push.lock' "$TMP"; }
LOCK="$(gate_lock_path)"

printf 'gate-lock · one gate at a time\n'

# --- 1 · uncontended ---------------------------------------------------------
if gate_lock_acquire 5 2>/dev/null && [ -d "$LOCK" ]; then
  ok "an uncontended acquire takes the lease"
else
  bad "an uncontended acquire takes the lease"
fi
gate_lock_release
if [ ! -d "$LOCK" ]; then
  ok "release removes the lease"
else
  bad "release removes the lease"
fi

# --- 2 · MUTUAL EXCLUSION ----------------------------------------------------
# The lease is held by a process that is genuinely ALIVE (this shell), so a
# second acquire must not get in. Budget 0s so it refuses immediately instead
# of making the test sleep.
gate_lock_acquire 5 2>/dev/null
held_by_us="$GATE_LOCK_HELD"
(
  GATE_LOCK_HELD=""
  gate_lock_acquire 0 2>/dev/null
) && second_got_in=true || second_got_in=false
if [ "$second_got_in" = false ]; then
  ok "a second acquire is REFUSED while a live owner holds it"
else
  bad "a second acquire got in — the race this lock exists to end is still open"
fi
if [ -d "$held_by_us" ]; then
  ok "the first holder still owns the lease"
else
  bad "the first holder lost the lease"
fi
gate_lock_release

# --- 3 · STALE RECLAIM -------------------------------------------------------
# A dead owner must not wedge the machine. Forge a lease owned by a PID that
# cannot be alive: start a process, wait for it, reuse its number.
mkdir -p "$LOCK"
dead_pid="$(
  bash -c 'echo $$' &
  wait $!
)"
dead_pid="$(bash -c 'echo $$')" # the shell has already exited by now
while kill -0 "$dead_pid" 2>/dev/null; do sleep 0.1; done
printf '%s %s %s\n' "$dead_pid" "$(hostname)" "$(date +%s)" >"$LOCK/owner"
if gate_lock_acquire 5 2>/dev/null; then
  ok "a lease whose owner is GONE is reclaimed (pid $dead_pid), not waited on"
else
  bad "a dead owner's lease wedged the acquire — one ^C would block every later push"
fi
gate_lock_release

# --- 4 · a LIVE owner is never stolen ---------------------------------------
# The inverse of case 3, and the one a careless stale-check breaks: widening a
# reclaim without tightening its liveness probe hands the lease to two holders.
sleep 300 &
live_pid=$!
mkdir -p "$LOCK"
printf '%s %s %s\n' "$live_pid" "$(hostname)" "$(date +%s)" >"$LOCK/owner"
if gate_lock_acquire 0 2>/dev/null; then
  bad "a LIVE owner's lease was stolen — two gates would run at once"
  gate_lock_release
else
  ok "a LIVE owner's lease is never stolen (pid $live_pid)"
fi
kill "$live_pid" 2>/dev/null
wait "$live_pid" 2>/dev/null
rm -rf "$LOCK"

# --- 5 · an owner file not yet written reads as ALIVE ------------------------
# The holder wins mkdir, then writes `owner`. In that window the file is
# absent. Treating "no owner file" as stale would steal a lease that is being
# taken — a race turned into two holders by the very code meant to prevent it.
mkdir -p "$LOCK"
if gate_lock_acquire 0 2>/dev/null; then
  bad "an owner-less lease was stolen — that is the acquire window, not a corpse"
  gate_lock_release
else
  ok "a lease with no owner file yet is treated as LIVE, never stolen"
fi
rm -rf "$LOCK"

printf '\n%d case(s) · %d failure(s)\n' "$cases" "$fails"
[ "$fails" -eq 0 ] || exit 1
