#!/usr/bin/env bash
# check-gate-lock.sh — the pre-push lease's self-test, run by CI.
#
# `scripts/hooks/_gate-lock.sh` serialises the pre-push gate so three
# concurrent runs cannot manufacture verdicts (#1064 · three gates over one
# target/ turned three unrelated ratchets red, one of them a credential-leak
# finding that did not exist). It ships with a self-test that proves mutual
# exclusion, stale reclaim, and that a LIVE owner is never robbed.
#
# Nothing ran it.
#
# That is the same shape twice written into the matrix beside this entry —
# `credential-headers` and `changelog-fragments` are both there because they
# "ran pre-push ONLY: a contributor without the hook installed, and every PR
# from a fork, met no check at all". The lease was worse: its test ran
# NOWHERE, not even pre-push, because a hook helper has no runner. Its own
# issue was reopened by the issue-proof gate for exactly that reason — a
# close whose proof does not exist — which is the first thing that gate
# caught after shipping, and it caught its own author.
#
# WHAT THIS PROVES, precisely: the lease LOGIC is sound. It does not prove
# the hook is installed on any particular machine — `lefthook install` is a
# local gesture and CI has no pre-push to run. The honest claim is the one
# the self-test makes: given this code, two holders cannot coexist, a dead
# owner cannot wedge the repo, and a live owner cannot be robbed.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
SELFTEST="$ROOT/hooks/test-gate-lock.sh"

# Fail CLOSED: an absent self-test is not a pass. The file being gone is
# precisely the state this entry exists to make impossible to reach quietly.
if [ ! -f "$SELFTEST" ]; then
  printf 'FAIL  no lease self-test at %s — the pre-push lease cannot be trusted\n' "$SELFTEST" >&2
  exit 1
fi

if ! bash "$SELFTEST"; then
  printf '\nFAIL  the pre-push lease self-test is red — one gate at a time is not guaranteed.\n' >&2
  exit 1
fi

echo "OK  the pre-push lease passes its exclusion and ownership checks"

# A correct reader is useless if the dispatcher skips it before it sees refs.
# This uses real installed Lefthook hooks and disposable local Git remotes;
# Cargo is a deliberately failing fixture, not a substitute Rust verdict.
python3 "$ROOT/hooks/test-pre-push-dispatch.py" || exit 1
echo "OK  tag, empty-input, deletion, mixed and protected-ref dispatch hold"
