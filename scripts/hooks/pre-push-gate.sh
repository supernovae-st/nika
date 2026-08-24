#!/usr/bin/env bash
# pre-push gate — the four heavy legs behind ONE stdin-aware door.
#
# git hands pre-push a line per ref on stdin:
#   <local ref> <local sha> <remote ref> <remote sha>
# A DELETION carries the zero sha as <local sha>: nothing new reaches the
# remote, so there is nothing to test — the gate skips instantly (the
# >90s hang on `git push origin --delete` was the full workspace suite
# running for a ref removal).
#
# FAIL-SAFE: if stdin carries no refs (lefthook not forwarding · manual
# invocation), the gate RUNS — a silent skip must be impossible.
set -uo pipefail

ZERO=0000000000000000000000000000000000000000
saw_ref=false
deletion_only=true
# The read is BOUNDED so this loop cannot outlive its input. git closes the
# pipe after the refspec, but a harness that forwards stdin without closing
# it would leave an unbounded `read` waiting on an EOF that never arrives.
# Five seconds is far more than a refspec needs, and a silent stdin still
# lands on the fail-safe below (the gate RUNS), so the bound can never turn
# into a skipped gate. Defensive only: no such harness has been observed.
while read -r -t 5 _local_ref local_sha _remote_ref _remote_sha; do
  [ -z "${local_sha:-}" ] && continue
  saw_ref=true
  if [ "$local_sha" != "$ZERO" ]; then
    deletion_only=false
  fi
done

if [ "$saw_ref" = true ] && [ "$deletion_only" = true ]; then
  echo "pre-push gate: deletion-only push — nothing new to test, skipping."
  exit 0
fi

if [ "${NIKA_GATE_DRYRUN:-}" = "1" ]; then
  echo "pre-push gate: WOULD RUN (saw_ref=$saw_ref deletion_only=$deletion_only)"
  exit 0
fi

# ONE gate at a time on this machine (#1064). Three concurrent gates over one
# 30G target/ turned three unrelated ratchets red — two timeouts and, worse, a
# credential-leak finding that did not exist. The legs below are not safe to
# race, so they do not: a second push WAITS, and the wall-clock cost becomes
# visible instead of being paid in false verdicts.
#
# Taken AFTER the deletion-only skip and the dry-run door, so neither ever
# waits on a lease it does not need.
if [ "${NIKA_GATE_NO_LOCK:-}" != "1" ]; then
  # shellcheck source-path=SCRIPTDIR
  # shellcheck source=./_gate-lock.sh
  . "$(cd "$(dirname "$0")" && pwd)/_gate-lock.sh"
  trap gate_lock_release EXIT INT TERM
  gate_lock_acquire "${NIKA_GATE_LOCK_WAIT:-2700}" || exit 1
fi

set -e
# stdin closed on purpose. This script is a pre-push hook, so its stdin is
# git's refspec pipe, and cargo hands whatever it inherits down to every
# test binary it spawns. A test suite has no business reading stdin, and a
# half-drained refspec pipe is the last thing it should read if it tries.
#
# If this gate ever looks frozen, do NOT judge it by cargo's own CPU time.
# During the test phase cargo sits in wait4 and its CPU is SUPPOSED to stay
# near zero while its children do the work; `ps time` does not count them.
# The honest judges are `sample <cargo-pid>` (it names the blocking frame)
# and `pgrep -P <cargo-pid>` (a changing child name means it is advancing).
# Measured 2026-07-27: a whole run at 0.4s of cargo CPU was healthy, and
# every test binary was stalled pre-main in _dyld_start waiting on
# Gatekeeper to scan it. That is a machine setting, not a bug in here.
cargo test --workspace --lib --quiet </dev/null
# `--lib` is a house absolute (a `--test` run pops the macOS Keychain), so
# this leg CANNOT see the integration tier. It is a real platform limit,
# not a bug — but a gate that stays silent about what it did not look at
# reads as full coverage, and on 2026-08-02 a green push landed a red CI:
# `init_smoke::the_first_hour_walks_end_to_end` lives in `tests/`, and
# nothing local runs it. Say the limit out loud; CI's nextest leg is what
# closes it.
printf '[gate] lib tests green · %s integration file(s) NOT run here (--lib · Keychain) — CI nextest covers them\n' \
  "$(find crates -path '*/tests/*.rs' -type f 2>/dev/null | wc -l | tr -d ' ')"
cargo clippy --workspace --all-targets -- -D warnings </dev/null
# hygiene: YELLOW (rc=1) passes with its stdout · only RED (rc=2) blocks —
# the exact contract the old inline leg carried.
rc=0
bash scripts/hygiene/check-all.sh --quiet || rc=$?
if [ "$rc" -eq 2 ]; then
  echo "engine hygiene RED — push blocked." >&2
  exit 1
fi
bash scripts/hooks/run-ci-ratchets.sh
