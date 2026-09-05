#!/usr/bin/env bash
# guard-run is a transport adapter: the engine alone decodes the host
# payload, judges shell commands and renders the two hook dialects.
# Cursor uses {command, cwd}; Claude Code/Codex use
# {hook_event_name, tool_input:{command}, cwd}.
#
# Comfort hooks may degrade quietly. This execution guard cannot infer
# NotOurs from raw substrings when its sole judge is unavailable.
set -uo pipefail

unavailable() {
  # Fixed diagnostics only, never interpolated command or payload bytes.
  # JSON escapes and shell quoting defeat a raw "nika" substring filter;
  # command text can also contain a misleading "hook_event_name".
  # Both hosts document exit 2 as blocking, so no fallback scope parser
  # or guessed JSON envelope is needed.
  # https://cursor.com/docs/hooks
  # https://code.claude.com/docs/en/hooks
  printf 'guard_unavailable: %s — the hook could not judge this action. Restore the nika binary on the editor PATH or repair the reported judge failure, then retry.\n' "$1" >&2
  exit 2
}

command -v nika >/dev/null 2>&1 || unavailable "the nika binary is not on PATH"

# Stdin goes directly to the engine's bounded reader, never into an
# unbounded shell variable. 0/2 carry a judged verdict; 3 is unavailable.
out="$(nika guard --stdin 2>/dev/null)" && rc=0 || rc=$?
case "$rc" in
  0 | 2 | 3) ;;
  *) unavailable "nika guard failed (exit $rc)" ;;
esac
if [ -z "$out" ]; then
  unavailable "nika guard failed (exit $rc)"
fi
if [ "$rc" -eq 3 ]; then
  # An unreadable/truncated payload cannot establish the host dialect.
  # Preserve the engine's explanation, but block via the shared protocol.
  printf '%s\n' "$out" >&2
  unavailable "nika guard could not judge (exit 3)"
fi

printf '%s\n' "$out"
exit 0
