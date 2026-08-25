#!/usr/bin/env bash
# test-issue-proof.sh — the issue-proof gate self-tests before it judges.
#
# Sibling of test-rs-prod-files.sh / test-strip-test-items.sh, same contract:
# issue-proof.sh runs this and REFUSES to render a verdict if it fails. A gate
# nobody can demonstrate is a gate nobody should believe.
#
# What is pinned here is the thing that was broken (#1200): WHO may decline
# the proof. Until 2026-08-25 the workflow's `if:` made it the FILER — an
# issue whose body lacked the string `proven_by:` was never judged at all, so
# the closes most likely to lie (issues filed before the gate existed,
# auto-closed by a merge) were exactly the ones it skipped. Measured one
# minute apart on one issue: `skipped`, then `success` once the string was
# added to the body.
#
# The guard is gone, so every close is judged and the opt-out moved HERE,
# where it is fixture-pinned in BOTH directions:
#
#   - the honest non-capability close WAIVES (and says so), and
#   - the `completed` close with no proof does NOT — including when the issue
#     carries a filer-side triage label like `wontfix`, which is precisely the
#     hole a label-based opt-out would re-open.
#
# The last cases are the ones that matter: widening an exclusion without
# tightening its exemption is how a correction becomes a hole.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# We source the gate to reach `waived_reason`; it must not re-run this test.
export NIKA_SKIP_ISSUE_PROOF_SELFTEST=1

fails=0
cases=0

# expect <waived|judged> <state_reason> <labels> <label-text>
expect() {
  local want="$1" reason="$2" labels="$3" label="$4"
  cases=$((cases + 1))
  local got why
  if why="$(ISSUE_STATE_REASON="$reason" ISSUE_LABELS="$labels" waived_reason)"; then
    got="waived"
  else
    got="judged"
    why="(no waiver)"
  fi
  if [ "$got" = "$want" ]; then
    printf '  ok   %s (%s · %s)\n' "$label" "$got" "$why"
  else
    fails=$((fails + 1))
    printf '  FAIL %s — wanted %s, got %s (%s)\n' "$label" "$want" "$got" "$why" >&2
  fi
}

# Source ONLY the function under test. issue-proof.sh exits on a missing
# --issue, so it is read into a subshell-safe form: everything above the
# argument parser is what we need, and `waived_reason` lives there.
eval "$(sed -n '/^waived_reason() {$/,/^}$/p' "$HERE/issue-proof.sh")"
if ! declare -F waived_reason >/dev/null; then
  printf 'FAIL  waived_reason() not found in issue-proof.sh — the self-test is testing nothing\n' >&2
  exit 2
fi

printf 'issue-proof · who may decline the proof\n'

# --- the CLOSER's act waives -------------------------------------------------
expect waived not_planned '' 'not_planned waives — nothing was delivered'
expect waived duplicate '' 'duplicate waives — the proof rides the original'
expect waived '' 'no-capability' 'the no-capability label waives'
expect waived '' 'question' 'the question label waives'
expect waived '' 'invalid' 'the invalid label waives'
expect waived '' 'bug,no-capability,area/cli' 'a waiving label among others still waives'

# --- everything else is JUDGED -----------------------------------------------
# The merge auto-close: `completed`, no label, empty body. THE case the gate
# exists for, and the one the old `if:` skipped.
expect judged completed '' 'a completed close with no label is judged'
expect judged '' '' 'a close with no state_reason at all is judged'
expect judged completed 'bug,area/cli' 'ordinary labels do not waive'

# `wontfix` is added at FILING time as a triage marker. Honouring it would put
# the opt-out back in the filer's hands — the exact defect #1200 names.
expect judged completed 'wontfix' 'wontfix does NOT waive — it is a filer-side marker'

# Substring matching is how a blocklist quietly widens. These three must be
# read as whole labels, never as prefixes of one.
expect judged completed 'no-capability-yet' 'no-capability-yet does NOT waive'
expect judged completed 'questionable' 'questionable does NOT waive'
expect judged completed 'invalidated' 'invalidated does NOT waive'

# A state_reason that merely CONTAINS a waiving word is not that reason.
expect judged 'not_planned_yet' '' 'not_planned_yet does NOT waive'

printf '\n%d case(s) · %d failure(s)\n' "$cases" "$fails"
[ "$fails" -eq 0 ] || exit 1
