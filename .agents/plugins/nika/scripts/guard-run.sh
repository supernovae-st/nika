#!/usr/bin/env bash
# guard-run — the execution seatbelt: before the agent shells out a
# `nika run`, audit the workflow. Check is the gate the language is
# built on; an agent must not be able to skip it by accident.
#
# ONE script, TWO dialects, THREE surfaces (Codex emits the Claude Code
# dialect verbatim — live-proven 2026-07-12). Sniffed from stdin — `hook_event_name` is
# Claude Code's, absent from Cursor's):
#   Cursor  (docs/agent/hooks · beforeShellExecution): in {command, cwd}
#     → out {"permission":"allow"|"deny", agent_message/user_message}.
#   Claude Code (hooks.md · PreToolUse · matcher Bash): in
#     {hook_event_name, tool_name, tool_input:{command}, cwd} → deny is
#     {"hookSpecificOutput":{"hookEventName":"PreToolUse",
#     "permissionDecision":"deny","permissionDecisionReason":…}}; the
#     no-opinion pass is `{}` — NEVER "allow", which would skip the
#     user's own permission prompt (the hook teaches, it never widens).
#
# Deny is TEACHING, not punishment: the reason carries the findings so
# the agent repairs and reruns — the check loop becomes structurally
# unskippable. Everything else fails open: a missing binary, a file we
# cannot find, a broken oracle (exit 3) must never block a terminal.
set -euo pipefail

input="$(cat)"

cc=""
case "$input" in *hook_event_name*) cc=1 ;; esac

allow() {
  if [ -n "$cc" ]; then
    printf '{}\n' # no opinion — the user's permission flow proceeds
  else
    printf '{"permission":"allow"}\n'
  fi
  exit 0
}

# command + cwd from the stdin JSON — python3 when present, sed fallback
# (in a Bash PreToolUse payload the only "command" key is
# tool_input.command, so the flat fallback matches both dialects).
if command -v python3 >/dev/null 2>&1; then
  cmd="$(printf '%s' "$input" | python3 -c 'import json,sys
try:
    d = json.load(sys.stdin)
    print(d.get("command") or d.get("tool_input", {}).get("command", ""))
except Exception:
    print("")' 2>/dev/null || true)"
  cwd="$(printf '%s' "$input" | python3 -c 'import json,sys
try:
    print(json.load(sys.stdin).get("cwd", ""))
except Exception:
    print("")' 2>/dev/null || true)"
else
  cmd="$(printf '%s' "$input" | sed -n 's/.*"command"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
  cwd="$(printf '%s' "$input" | sed -n 's/.*"cwd"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
fi

# Only `nika run …` is gated — every other command flows.
printf '%s' "$cmd" | grep -qE '(^|[;&|[:space:]])nika[[:space:]]+run([[:space:]]|$)' || allow

# A resumed run replays an already-audited plan from its trace; the
# gate is for fresh runs of the file as it stands now.
case "$cmd" in *--resume*) allow ;; esac

# The workflow file: first *.nika.yaml / *.nika.yml token in the command.
file=""
# set -f: the unquoted expansion must split into words WITHOUT
# globbing (a literal `nika run *.nika.yaml` would otherwise expand
# against the hook's own cwd).
set -f
for word in $cmd; do
  case "$word" in
    *.nika.yaml | *.nika.yml)
      file="$word"
      break
      ;;
  esac
done
set +f
[ -n "$file" ] || allow

# Resolve relative to the payload cwd; unfindable file fails open
# (the run itself will name the real error better than we can).
case "$file" in
  /*) ;;
  *) [ -n "$cwd" ] && file="$cwd/$file" ;;
esac
[ -f "$file" ] || allow
command -v nika >/dev/null 2>&1 || allow

set +e
findings="$(nika check "$file" --color never 2>&1)"
rc=$?
set -e

# rc 0 = clean → run flows. rc 2 = findings → deny and teach.
# Anything else (3 = broken oracle) fails open.
if [ "$rc" -ne 2 ]; then
  allow
fi

findings="$(printf '%s' "$findings" | head -c 2000)"

if command -v python3 >/dev/null 2>&1; then
  # env on the CONSUMER: `VAR=x cmd1 | cmd2` binds VAR to cmd1 only.
  printf '%s' "$findings" | CC="$cc" python3 -c '
import json, os, sys
findings = sys.stdin.read()
msg = ("nika check failed on this workflow - repair the findings, "
       "re-run nika check until it passes, then run again.\n\n" + findings)
if os.environ.get("CC"):
    print(json.dumps({"hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "permissionDecision": "deny",
        "permissionDecisionReason": msg,
    }}))
else:
    print(json.dumps({
        "permission": "deny",
        "agent_message": msg,
        "user_message": "nika run blocked: the workflow does not pass nika check yet.",
    }))'
else
  # No python3: a fixed, pre-escaped message (never interpolate raw
  # findings into JSON by hand).
  if [ -n "$cc" ]; then
    printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"nika check failed on this workflow - run nika check on the file, repair the findings it names, then run again."}}\n'
  else
    printf '{"permission":"deny","agent_message":"nika check failed on this workflow - run nika check on the file, repair the findings it names, then run again.","user_message":"nika run blocked: the workflow does not pass nika check yet."}\n'
  fi
fi
exit 0
