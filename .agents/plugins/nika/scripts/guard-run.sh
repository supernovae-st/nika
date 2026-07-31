#!/usr/bin/env bash
# guard-run — the execution seatbelt, THIN: the judgement now lives IN
# the binary (`nika guard`). This shim only carries the host's payload
# to it and its verdict back — the regex/split era is over (P0-15 ·
# audit UX 2026-07-30: 21 documented bypasses, from the absolute-path
# miss to the unquoted split that failed open on an empty file token).
#
# ONE script, TWO dialects, sniffed from stdin (`hook_event_name` is
# Claude Code's, absent from Cursor's; Codex emits the Claude Code
# dialect verbatim — live-proven 2026-07-12):
#   Cursor  (docs/agent/hooks · beforeShellExecution): in {command, cwd}
#   Claude Code (hooks.md · PreToolUse · matcher Bash): in
#     {hook_event_name, tool_name, tool_input:{command}, cwd}
#
# Loi 12 — the hooks are a seatbelt, never the airbag: a missing or
# broken judge must be VISIBLE. The fallback below emits a
# guard_unavailable DENIAL naming the degradation, never the silent
# fail-open that made the old hook claim the check had passed.
set -uo pipefail

input="$(cat)"

unavailable() {
  # $1 = a one-line reason — fixed strings and an exit code only, never
  # raw payload bytes (hand-interpolated JSON stays injection-free by
  # construction, the same law the old fixed-message fallback obeyed).
  case "$input" in
    *hook_event_name*)
      printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"guard_unavailable: %s — the guard could not judge, and an unjudged run never gets its allow. Judge by hand: nika check <file> — or run outside the agent, where the run belongs to you."}}\n' "$1"
      ;;
    *)
      printf '{"permission":"deny","agent_message":"guard_unavailable: %s — the guard could not judge, and an unjudged run never gets its allow. Judge by hand: nika check <file>.","user_message":"nika run blocked: the guard is unavailable (%s) — run nika check yourself."}\n' "$1" "$1"
      ;;
  esac
  exit 0
}

command -v nika >/dev/null 2>&1 || unavailable "the nika binary is not on PATH"

# 0 allow · 2 deny · 3 guard_unavailable all carry the verdict JSON on
# stdout; anything else (a crash, silence) means the judge itself broke.
out="$(printf '%s' "$input" | nika guard --stdin 2>/dev/null)" && rc=0 || rc=$?
if [ -z "$out" ] || [ "$rc" -gt 3 ]; then
  unavailable "nika guard failed (exit $rc)"
fi

printf '%s\n' "$out"
exit 0
