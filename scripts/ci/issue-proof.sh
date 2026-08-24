#!/usr/bin/env bash
# issue-proof.sh — refuse (reopen) a closed issue whose proven_by job
# does not exist. GitHub closes issues on PR merge; that is the lying ✅
# at the issue layer. This is the gate.
#
# The workflow judges EVERY close — it carries no `if:` (#1200). Proof used
# to be opt-in by the FILER (a `proven_by:` string in the body, or a
# `capability` label), so the closes most likely to lie — issues filed before
# the gate existed, auto-closed by a merge — were exactly the ones it skipped.
# The opt-out now belongs to the CLOSER and is judged HERE, where fixtures can
# pin it: see `waived_reason` below and test-issue-proof.sh.
#
# Usage:
#   issue-proof.sh --issue N
# Env:
#   ISSUE_BODY          the issue body (passed via env, never interpolated)
#   ISSUE_STATE_REASON  GitHub close reason — completed | not_planned | duplicate
#   ISSUE_LABELS        comma-joined label names
#   GH_REPO             supernovae-st/nika
#   GH_TOKEN            for gh issue reopen
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -euo pipefail

ISSUE=""
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
GH_REPO="${GH_REPO:-supernovae-st/nika}"

# This gate proves itself before it judges anything — the _lib.sh discipline,
# fail CLOSED. A guard that cannot demonstrate its own verdicts must refuse to
# render one, not emit a green. Skipped only from inside the self-test, which
# sources this file to reach `waived_reason`.
if [ -z "${NIKA_SKIP_ISSUE_PROOF_SELFTEST:-}" ]; then
  _selftest="$(cd "$(dirname "$0")" && pwd)/test-issue-proof.sh"
  if [ ! -f "$_selftest" ]; then
    printf 'issue-proof: no self-test at %s — this gate cannot be trusted\n' "$_selftest" >&2
    exit 2
  fi
  if ! NIKA_SKIP_ISSUE_PROOF_SELFTEST=1 bash "$_selftest" >/dev/null 2>&1; then
    printf 'issue-proof: the self-test does not pass — this gate cannot be trusted:\n' >&2
    NIKA_SKIP_ISSUE_PROOF_SELFTEST=1 bash "$_selftest" >&2 || true
    exit 2
  fi
fi

# WHY this close needs no proof, or empty when it does.
#
# Opt-out is the CLOSER's explicit act, never the filer's. Two forms:
#
#   1. GitHub's own `state_reason` — `not_planned` and `duplicate` both mean
#      "nothing was delivered", so there is no capability to prove. A merge
#      auto-close (the event this gate exists for) is always `completed` and
#      can never reach this branch.
#   2. An explicit label the closer adds. `question` and `invalid` are
#      GitHub's defaults; `no-capability` is ours, for the issue that IS a
#      defect report but claims no new capability on close.
#
# Deliberately NOT here: `wontfix`. It is routinely added at FILING time as a
# triage marker, which would put the opt-out back in the filer's hands — the
# whole defect. A close that genuinely will not be fixed is `not_planned`.
waived_reason() {
  local reason="${ISSUE_STATE_REASON:-}" labels="${ISSUE_LABELS:-}" label
  case "$reason" in
    not_planned)
      printf 'closed not_planned · nothing was delivered to prove'
      return 0
      ;;
    duplicate)
      printf 'closed as duplicate · the proof rides the original'
      return 0
      ;;
    *) ;;
  esac
  # Exact whole-label match: a `no-capability-yet` label must not waive, and
  # substring matching is how a blocklist quietly widens.
  for label in ${labels//,/ }; do
    case "$label" in
      no-capability | question | invalid)
        # No backticks in this format string: they are literal inside single
        # quotes but shellcheck reads them as a substitution the author forgot
        # to quote (SC2016), and double-quoting to silence it would make them
        # a REAL command substitution. The sentence carries itself.
        printf 'the closer labelled it %s · that claims no capability' "$label"
        return 0
        ;;
      *) ;;
    esac
  done
  return 1
}

while [ $# -gt 0 ]; do
  case "$1" in
    --issue)
      ISSUE="${2:-}"
      shift 2
      ;;
    -h | --help)
      sed -n '2,16p' "$0"
      exit 0
      ;;
    *)
      printf 'issue-proof: unknown argument %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

[ -n "$ISSUE" ] || {
  printf 'issue-proof: --issue N is required\n' >&2
  exit 2
}

# The closer's opt-out is read BEFORE the ledger: a `not_planned` close has
# no capability to look up, so it must not depend on wiring.yaml existing.
if WAIVER="$(waived_reason)"; then
  printf 'issue-proof · WAIVED · #%s · %s\n' "$ISSUE" "$WAIVER"
  exit 0
fi

LEDGER="$ROOT/wiring.yaml"
[ -f "$LEDGER" ] || {
  printf 'issue-proof: no wiring.yaml at %s · cannot judge\n' "$LEDGER" >&2
  exit 2
}

JOBS="$(
  awk '
    $0 ~ /^jobs:[[:space:]]*$/ { in_jobs=1; next }
    in_jobs && $0 ~ /^[^[:space:]#]/ { in_jobs=0 }
    in_jobs && $0 ~ /^  [A-Za-z0-9_-]+:/ { sub(/:.*/, "", $1); print $1 }
  ' "$ROOT"/.github/workflows/*.yml 2>/dev/null | sort -u || true
)"

# 1. wiring.yaml issue: N → proven_by
LEDGER_JOB="$(
  awk -v want="$ISSUE" '
    /^capabilities:/{grab=1; next}
    grab && /^[a-z_]+:/ && $0 !~ /^[[:space:]]/{grab=0}
    grab && $0 ~ /proven_by:[[:space:]]*/ {
      sub(/.*proven_by:[[:space:]]*/, ""); gsub(/["\047]/, ""); job=$0
    }
    grab && $0 ~ /issue:[[:space:]]*/ {
      sub(/.*issue:[[:space:]]*/, ""); gsub(/["\047]/, "")
      if ($0 == want) { print job; exit }
    }
  ' "$LEDGER"
)"

# 2. body proven_by: <job> (env, never interpolated into the script source)
BODY_JOB=""
if [ -n "${ISSUE_BODY:-}" ]; then
  BODY_JOB="$(
    printf '%s\n' "$ISSUE_BODY" \
      | awk '/proven_by:[[:space:]]*/ {
          sub(/.*proven_by:[[:space:]]*/, "")
          gsub(/["\047`]/, "")
          gsub(/[[:space:]].*/, "")
          print
          exit
        }'
  )"
fi

JOB="${LEDGER_JOB:-$BODY_JOB}"

hold() {
  printf 'issue-proof · HOLD · #%s proven_by=%s exists\n' "$ISSUE" "$1"
  exit 0
}

refuse() {
  local why="$1"
  printf 'issue-proof · FAIL · #%s · %s\n' "$ISSUE" "$why"
  if command -v gh >/dev/null 2>&1; then
    gh issue reopen "$ISSUE" --repo "$GH_REPO" >/dev/null 2>&1 || true
    gh issue comment "$ISSUE" --repo "$GH_REPO" --body "$why

This gate refuses a close whose proof does not exist. GitHub closing the issue on PR merge is the lying checkmark at the issue layer.

Two honest ways to close:

- **It delivered a capability** — name a live CI job in \`wiring.yaml\` (\`proven_by:\` + \`issue: $ISSUE\`), or put \`proven_by: <job>\` in the body, and close again.
- **It delivered nothing** — close it as **not planned** (or as a duplicate), or add a \`no-capability\` / \`question\` / \`invalid\` label. That is an explicit act by whoever closes, and it is recorded.

What this gate no longer accepts is silence. Until 2026-08-25 it only judged closes whose author had happened to write \`proven_by:\` in the body, which meant the closes most likely to lie were the ones it skipped." >/dev/null 2>&1 || true
  fi
  exit 1
}

if [ -z "$JOB" ] || [ "$JOB" = "null" ]; then
  refuse "no proven_by (not in wiring.yaml, not in the issue body) · a capability close needs a named CI job"
fi

if printf '%s\n' "$JOBS" | grep -qxF "$JOB"; then
  hold "$JOB"
fi
refuse "proven_by job \`$JOB\` is in no workflow · the judge does not exist"
