#!/usr/bin/env bash
# guard-run.test.sh — missing-judge refusals in both host dialects.
#
# The shim is the only part of the guard that runs when the judge does
# NOT. It cannot prove that an encoded or quoted command is unrelated.
# Exit 2 is the hosts' shared blocking protocol, independent of payload
# substrings or a guessed JSON dialect. No command below is executed.
#
# Run with the binary genuinely absent
# — a PATH with a shell and nothing else.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHIM="$HERE/../guard-run.sh"
BARE_PATH="/usr/bin:/bin"
fails=0

# ask <name> <payload>
ask() {
  local name="$1" payload="$2" out rc=0
  out="$(printf '%s' "$payload" | env -i PATH="$BARE_PATH" /bin/bash "$SHIM" 2>&1)" || rc=$?
  if [ "$rc" -ne 2 ] || [[ "$out" != *guard_unavailable* ]]; then
    printf 'FAIL  %s — want exit 2 with guard_unavailable, got %s\n      %s\n' "$name" "$rc" "$out" >&2
    fails=$((fails + 1))
  else
    printf 'ok    %s (blocked)\n' "$name"
  fi
}

# No healthy judge exists to prove NotOurs, even for ordinary commands.
ask 'a plain command' '{"command":"ls -la","cwd":"/tmp"}'
ask 'a git inspection' '{"command":"git status","cwd":"/tmp"}'
ask 'the claude dialect too' '{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"ls"},"cwd":"/tmp"}'

# --- ours, and unjudgeable: deny, visibly ---------------------------------
ask 'a bare run' '{"command":"nika run x.nika.yaml","cwd":"/tmp"}'
ask 'an absolute-path run' '{"command":"/opt/nika/bin/nika run x","cwd":"/tmp"}'
ask 'a wrapped run' '{"command":"sh -c \"nika run x\"","cwd":"/tmp"}'
ask 'the cargo target name' '{"command":"nika-cli run x","cwd":"/tmp"}'
ask 'a chained run' '{"command":"echo hi && nika run x","cwd":"/tmp"}'
ask 'the claude dialect run' '{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"nika run x"},"cwd":"/tmp"}'
ask 'JSON-escaped run' '{"command":"n\u0069ka run x.yaml","cwd":"/tmp"}'
ask 'shell-escaped run' '{"command":"n\\ika run x.yaml","cwd":"/tmp"}'
ask 'a dialect marker in command text' '{"command":"echo hook_event_name; n\\ika run x.yaml","cwd":"/tmp"}'
ask 'an escaped Claude key' '{"hook_event_\u006eame":"PreToolUse","tool_input":{"command":"n\\ika run x.yaml"},"cwd":"/tmp"}'

if [ "$fails" -gt 0 ]; then
  printf '\nFAIL  %d guard-run scope case(s)\n' "$fails" >&2
  exit 1
fi

echo "OK  missing-judge actions block without a second scope or dialect parser"
