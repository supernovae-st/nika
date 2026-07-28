#!/usr/bin/env bash
# check-on-edit — the plugin's seatbelt: after the agent edits a
# *.nika.yaml, run the audit so findings reach the agent immediately
# (the file is the contract; check is the oracle).
#
# ONE script, TWO dialects, THREE surfaces (Codex emits the Claude Code
# dialect verbatim — live-proven 2026-07-12). Sniffed from stdin — `hook_event_name` is
# Claude Code's, absent from Cursor's):
#   Cursor (afterFileEdit): file_path at the payload root; findings go
#     to STDERR (the hook log) and stdout stays `{}` — never a veto.
#   Claude Code (PostToolUse · matcher Edit|Write|MultiEdit): file_path
#     at tool_input.file_path; findings go to STDERR + exit 2 — the
#     documented feedback channel (the tool already ran, so exit 2 is
#     non-blocking here: Claude SEES the findings and repairs).
#
# Capability-honest: no nika binary means no verdict, never a failure.
set -euo pipefail

input="$(cat)"

cc=""
case "$input" in *hook_event_name*) cc=1 ;; esac

done_quiet() {
  printf '{}\n'
  exit 0
}

# file_path from the stdin JSON — python3 when present, sed fallback
# (both dialects carry exactly one "file_path" key, so the flat
# fallback matches either nesting).
if command -v python3 >/dev/null 2>&1; then
  file="$(printf '%s' "$input" | python3 -c 'import json,sys
try:
    d = json.load(sys.stdin)
    print(d.get("file_path") or d.get("tool_input", {}).get("file_path", ""))
except Exception:
    print("")' 2>/dev/null || true)"
else
  file="$(printf '%s' "$input" | sed -n 's/.*"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
fi

case "$file" in
  *.nika.yaml | *.nika.yml) ;;
  *) done_quiet ;;
esac

# NIKA_BIN wins when set — the seatbelt judges with the binary the WORK
# targets, not only whatever the PATH serves. Proven need 2026-07-27: the
# E-split window, where every ratified-grammar file on this machine drew a
# cry from the PATH's 0.105 while engine main already spoke the new forms.
# Pointing NIKA_BIN at a main-built nika lets the hook judge 0.106 work
# BEFORE brew serves it. Same capability-honesty: an unset or broken
# NIKA_BIN falls back to silence, never to a failure.
NIKA="${NIKA_BIN:-nika}"

if [ ! -f "$file" ] || ! command -v "$NIKA" >/dev/null 2>&1; then
  # Missing file or binary: nothing to audit (the skill teaches the
  # install line) — stay silent and let the edit flow.
  done_quiet
fi

# --native-strict, not a bare check. The bare verdict passes a workflow
# whose real work happens inside `exec python3 helper.py` — the shape an
# agent reaches for the moment a builtin refuses it, and the one that
# leaves nothing for the permits boundary to bound. Under the flag that
# hint becomes rc=2 and lands here, at the edit, where the reflex forms.
# Measured before wiring: a script wrapper fails, `exec git` passes with
# or without a ledger entry. The flag costs legitimate execs nothing.
set +e
findings="$("$NIKA" check "$file" --native-strict --color never 2>&1)"
rc=$?
set -e

if [ "$rc" -ne 2 ]; then
  # Clean (0) or broken oracle (3): nothing to teach — silence.
  done_quiet
fi

printf '%s\n' "$findings" | head -c 2000 >&2
# Name the FLAG, not just the verb. A reader told to "re-run nika check"
# runs the bare form, reads a green that this hook does not accept, and
# loops against a gate it cannot see.
printf '\nre-check with the same oracle this hook used:\n  nika check --native-strict %s\n' "$file" >&2
if [ -n "$cc" ]; then
  # PostToolUse exit 2 = stderr fed to Claude, edit already applied.
  exit 2
fi
printf '{}\n'
exit 0
