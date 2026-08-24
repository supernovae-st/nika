#!/usr/bin/env bash
# issue-proof.sh — refuse (reopen) a closed issue whose proven_by job
# does not exist. GitHub closes issues on PR merge; that is the lying ✅
# at the issue layer. This is the gate.
#
# Usage:
#   issue-proof.sh --issue N
# Env:
#   ISSUE_BODY   the issue body (passed via env, never interpolated)
#   GH_REPO      supernovae-st/nika
#   GH_TOKEN     for gh issue reopen
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -euo pipefail

ISSUE=""
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
GH_REPO="${GH_REPO:-supernovae-st/nika}"

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

This gate refuses a close whose proof does not exist. GitHub closing the issue on PR merge is the lying checkmark at the issue layer. Name a live CI job in \`wiring.yaml\` (\`proven_by:\` + \`issue: $ISSUE\`) and close again." >/dev/null 2>&1 || true
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
