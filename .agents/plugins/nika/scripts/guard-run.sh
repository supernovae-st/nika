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
  #
  # SCOPE FIRST. A degradation is only ours to report about a command
  # that could have been ours. Without this, a missing binary denied
  # EVERY shell command in the session — `ls` came back « nika run
  # blocked » — and the README's own macOS bullet says a GUI-launched
  # Cursor not inheriting PATH is a normal Tuesday. Fail-visible must
  # not mean fail-on-everything (2026-08-02).
  #
  # The filter is EXACT, not a heuristic, and it is not the regex era
  # returning: the judge itself only ever claims a command whose word is
  # `nika` or `nika-cli` (guard.rs, the dispatch), and both contain this
  # substring. A payload without it would come back NotOurs from the
  # binary too — so staying silent here loses nothing the guard ever had.
  case "$input" in
    *[Nn][Ii][Kk][Aa]*) ;;
    *)
      printf '{}\n'
      exit 0
      ;;
  esac
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
case "$rc" in
  0 | 2 | 3) ;;
  *) unavailable "nika guard failed (exit $rc)" ;;
esac
if [ -z "$out" ]; then
  unavailable "nika guard failed (exit $rc)"
fi

printf '%s\n' "$out"
exit 0
