#!/usr/bin/env bash
# A broken judge must not turn permission-looking stdout into an allow.
# Exercise the installed shim, not a copy of its dispatch, in both dialects.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHIM="$HERE/../guard-run.sh"
judge_root="$(mktemp -d)"
cleanup() {
  rm -f "$judge_root/nika"
  rmdir "$judge_root"
}
trap cleanup EXIT
printf '%s\n' '#!/bin/sh' '/bin/cat >/dev/null' \
  "printf '%s' \"\$GUARD_TEST_OUTPUT\"" "exit \"\$GUARD_TEST_EXIT\"" \
  >"$judge_root/nika"
chmod +x "$judge_root/nika"
fails=0
checks=0

judge() {
  local dialect="$1" code="$2" payload="$3" supplied="$4" expected="$5" out
  checks=$((checks + 1))
  out="$(printf '%s' "$payload" | env -i PATH="$judge_root:/usr/bin:/bin" \
    GUARD_TEST_OUTPUT="$supplied" GUARD_TEST_EXIT="$code" /bin/bash "$SHIM")"
  if [ "$expected" = unavailable ]; then
    if printf '%s' "$out" | jq -e --arg dialect "$dialect" '
      (if $dialect == "cursor" then .permission == "deny"
       else .hookSpecificOutput.permissionDecision == "deny" end) and
      ((.agent_message // .hookSpecificOutput.permissionDecisionReason // "")
        | contains("guard_unavailable"))' >/dev/null; then
      printf 'ok    %s exit %s refuses\n' "$dialect" "$code"
      return
    fi
  elif [ "$out" = "$expected" ]; then
    printf 'ok    %s exit %s preserves verdict\n' "$dialect" "$code"
    return
  fi
  printf 'FAIL  %s exit %s: expected %s, got %s\n' \
    "$dialect" "$code" "$expected" "$out" >&2
  fails=$((fails + 1))
}

for dialect in cursor claude; do
  if [ "$dialect" = cursor ]; then
    payload='{"command":"nika run fixture.nika.yaml","cwd":"/tmp"}'
    unrelated='{"command":"ls","cwd":"/tmp"}'
    allow='{"permission":"allow"}'
    deny='{"permission":"deny","agent_message":"fixture judge refused"}'
  else
    payload='{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"nika run fixture.nika.yaml"},"cwd":"/tmp"}'
    unrelated='{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"ls"},"cwd":"/tmp"}'
    allow='{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"}}'
    deny='{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"fixture judge refused"}}'
  fi
  judge "$dialect" 0 "$payload" "$allow" "$allow"
  for code in 2 3; do
    judge "$dialect" "$code" "$payload" "$deny" "$deny"
  done
  for code in 1 4 127 255; do
    judge "$dialect" "$code" "$payload" "$allow" unavailable
  done
  for code in 0 2 3; do
    judge "$dialect" "$code" "$payload" '' unavailable
  done
  judge "$dialect" 1 "$unrelated" "$allow" '{}'
done

if [ "$fails" -ne 0 ]; then
  printf 'FAIL  %s of %s judge-exit controls\n' "$fails" "$checks" >&2
  exit 1
fi
printf 'OK  %s judge-exit controls; no workflow or effect was executed\n' "$checks"
