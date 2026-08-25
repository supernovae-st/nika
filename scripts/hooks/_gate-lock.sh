#!/usr/bin/env bash
# _gate-lock.sh — ONE pre-push gate at a time, per machine (#1064).
#
# Sourced by pre-push-gate.sh. Not executable on its own.
#
# ## Why
#
# Measured 2026-08-24: three `lefthook run pre-push` processes against one 30G
# `target/` turned THREE UNRELATED ratchets red, none of which reproduces idle.
#
#   adr-schema-valid     timeout after 300s      passes idle
#   doc-private-items    timeout after 300s      passes idle
#   credential-headers   "x-goog-api-key absent  6/6 green idle
#                         from SENSITIVE_HEADERS"
#
# The third is the expensive one: it named a credential leak THAT DOES NOT
# EXIST (the header is listed, at nika-kernel-core/src/io/http.rs:210, and the
# gate reads that file directly). A security vector that cries wolf trains the
# next reader to wave it through. And measured the same evening: one gate alone
# takes 210-620s; three in parallel took 1958s and ALL THREE failed.
#
# So the contention is not merely slow, it is DISHONEST — it manufactures
# verdicts. Serialising is the fix that removes the trigger rather than
# widening a timeout around it. Waiting also makes the real cost visible
# instead of hiding it behind three progress bars that are starving each other.
#
# ## Why mkdir and not flock
#
# `flock(1)` does not exist on macOS (dev) and `shlock` does not ship on the
# ubuntu runners (CI). `mkdir` is atomic on every POSIX filesystem and is the
# one primitive both hosts agree on.
#
# ## Liveness
#
# A lock that cannot tell whether its owner is alive is a lock that eventually
# wedges the repo for everyone: one killed gate (^C, a closed terminal, an OOM)
# and every later push waits on a corpse. So the holder records `pid host
# epoch`, and a waiter whose host matches probes `kill -0`. A dead owner's lock
# is STOLEN — via `mv`, so that when two waiters notice the same corpse exactly
# one of them wins the rename and there is still only ever one holder.

# Where the lease lives. Inside .git/ so it is never committed, never walked by
# a ratchet, and dies with the clone. Per WORKTREE would defeat the purpose —
# the contention is over one target/ and one machine's CPU, which several
# worktrees share, so `git rev-parse --git-common-dir` (the MAIN .git, shared
# by every worktree) is the correct scope.
gate_lock_path() {
  local common
  common="$(git rev-parse --git-common-dir 2>/dev/null)" || common=".git"
  [ -n "$common" ] || common=".git"
  printf '%s/nika-pre-push.lock' "$common"
}

# Acquire, or wait for, the machine's single pre-push lease.
#
#   gate_lock_acquire [max_wait_seconds]
#
# Returns 0 holding the lease · 1 if it could not be had within the budget.
# The caller must REFUSE on 1 — a gate that runs anyway is the race this
# exists to end.
gate_lock_acquire() {
  local max_wait="${1:-2700}" # 45 min · three full gates at their measured worst
  local lock waited=0 owner opid ohost announced=false
  lock="$(gate_lock_path)"

  while :; do
    if mkdir "$lock" 2>/dev/null; then
      # Won it. Record the owner AFTER the atomic act, never before — the
      # mkdir is the lock; this file is only how a waiter judges liveness.
      printf '%s %s %s\n' "$$" "$(hostname)" "$(date +%s)" >"$lock/owner" 2>/dev/null || true
      GATE_LOCK_HELD="$lock"
      return 0
    fi

    owner="$(cat "$lock/owner" 2>/dev/null || true)"
    opid="${owner%% *}"
    ohost="$(printf '%s' "$owner" | awk '{print $2}')"

    # An owner file that is absent or unparseable means the holder won the
    # mkdir microseconds ago and has not written it yet. That is a LIVE owner,
    # not a stale one — treat it as alive rather than stealing a lease that is
    # being taken. (Stealing here is how a race becomes two holders.)
    if [ -n "$opid" ] && [ "$ohost" = "$(hostname)" ] && ! kill -0 "$opid" 2>/dev/null; then
      printf '[gate-lock] owner pid %s is gone — reclaiming the stale lease\n' "$opid" >&2
      # `mv` so exactly ONE of several waiters wins the reclaim.
      if mv "$lock" "$lock.stale.$$" 2>/dev/null; then
        rm -rf "$lock.stale.$$"
      fi
      continue
    fi

    if [ "$waited" -ge "$max_wait" ]; then
      printf '[gate-lock] still held by pid %s after %ss — refusing rather than racing.\n' \
        "${opid:-?}" "$waited" >&2
      printf '[gate-lock] Three concurrent gates took 1958s and all three failed (#1064).\n' >&2
      printf '[gate-lock] Wait for the other push, or set NIKA_GATE_NO_LOCK=1 to opt out\n' >&2
      printf '[gate-lock] (opting out re-enables the race — the ratchets may lie).\n' >&2
      return 1
    fi

    if [ "$announced" = false ]; then
      printf '[gate-lock] another pre-push gate is running (pid %s) — waiting.\n' "${opid:-?}" >&2
      printf '[gate-lock] One gate takes 210-620s; three in parallel took 1958s and ALL failed.\n' >&2
      announced=true
    fi
    sleep 5
    waited=$((waited + 5))
  done
}

# Release the lease if this shell holds it. Idempotent, safe from a trap.
gate_lock_release() {
  [ -n "${GATE_LOCK_HELD:-}" ] || return 0
  rm -rf "$GATE_LOCK_HELD"
  GATE_LOCK_HELD=""
}
