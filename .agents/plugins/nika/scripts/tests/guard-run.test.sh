#!/usr/bin/env bash
# guard-run.test.sh — the shim's scope, both directions.
#
# The shim is the only part of the guard that runs when the judge does
# NOT. Its whole job in that moment is to degrade visibly for a run —
# and to stay out of the way for everything else. It got the second half
# wrong: with the binary off PATH it denied every shell command in the
# session, and told the user « nika run blocked » about `ls`. The
# README's own macOS bullet says a GUI-launched Cursor missing the shell
# PATH is ordinary, so that was not a corner (2026-08-02).
#
# Both directions are pinned here. Run with the binary genuinely absent
# — a PATH with a shell and nothing else.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHIM="$HERE/../guard-run.sh"
BARE_PATH="/usr/bin:/bin"
fails=0

# ask <name> <payload> <want: silent|deny>
ask() {
  local name="$1" payload="$2" want="$3" out
  out="$(printf '%s' "$payload" | env PATH="$BARE_PATH" /bin/bash "$SHIM" 2>/dev/null)"
  local got="deny"
  [ "$(printf '%s' "$out" | tr -d '[:space:]')" = "{}" ] && got="silent"
  case "$out" in *'"permission":"deny"'* | *'"permissionDecision":"deny"'*) got="deny" ;; esac
  if [ "$got" != "$want" ]; then
    printf 'FAIL  %s — want %s, got %s\n      %s\n' "$name" "$want" "$got" "$out" >&2
    fails=$((fails + 1))
  else
    printf 'ok    %s (%s)\n' "$name" "$got"
  fi
}

# --- not ours: the shim was never in the picture --------------------------
ask 'a plain command' '{"command":"ls -la","cwd":"/tmp"}' silent
ask 'a destructive command' '{"command":"rm -rf /","cwd":"/tmp"}' silent
ask 'a git push' '{"command":"git push --force","cwd":"/tmp"}' silent
ask 'the claude dialect too' '{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"ls"},"cwd":"/tmp"}' silent

# --- ours, and unjudgeable: deny, visibly ---------------------------------
ask 'a bare run' '{"command":"nika run x.nika.yaml","cwd":"/tmp"}' deny
ask 'an absolute-path run' '{"command":"/opt/nika/bin/nika run x","cwd":"/tmp"}' deny
ask 'a wrapped run' '{"command":"sh -c \"nika run x\"","cwd":"/tmp"}' deny
ask 'the cargo target name' '{"command":"nika-cli run x","cwd":"/tmp"}' deny
ask 'a chained run' '{"command":"echo hi && nika run x","cwd":"/tmp"}' deny
ask 'the claude dialect run' '{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"nika run x"},"cwd":"/tmp"}' deny

if [ "$fails" -gt 0 ]; then
  printf '\nFAIL  %d guard-run scope case(s)\n' "$fails" >&2
  exit 1
fi

echo "OK  the shim degrades for runs and stays out of the way otherwise"
