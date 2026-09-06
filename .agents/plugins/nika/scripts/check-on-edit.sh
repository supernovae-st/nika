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

# The oracle order (F14 · 2026-07-30). NIKA_BIN wins when set — the
# seatbelt judges with the binary the WORK targets, not only whatever the
# PATH serves. Proven need 2026-07-27: the E-split window, where every
# ratified-grammar file on this machine drew a cry from the PATH's 0.105
# while engine main already spoke the new forms. When NIKA_BIN is unset
# AND the edited file lives in a tree that builds its own nika, the
# tree's build IS the binary the work targets — the hook judges with it
# by default, because the PATH binary can lag the tree (the wrong-judge
# class: a green from the release the tree is in the middle of fixing).
# Everywhere else falls back to the PATH's nika. Same capability-honesty:
# an unset or broken NIKA_BIN and a missing binary fall back to silence,
# never to a failure.
NIKA="${NIKA_BIN:-}"
if [ -z "$NIKA" ]; then
  root="$(git -C "$(dirname "$file")" rev-parse --show-toplevel 2>/dev/null || true)"
  if [ -n "$root" ] && [ -x "$root/target/debug/nika" ]; then
    NIKA="$root/target/debug/nika"
  else
    NIKA="nika"
  fi
fi

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

# WHICH oracle answered. A verdict that does not name the binary behind it
# claims more than it covers — the same law this hook exists to enforce on
# workflows, applied to the hook.
#
# Not hypothetical. Measured 2026-07-28, mid-session: `NIKA_BIN` unset
# resolves to the PATH's brew build, which was one release behind the tree
# and still deferred `${{ const.x }}` paths to run time. On the same file:
#
#   brew 0.106.1  ✔ PERMITS  body fits the declared boundary
#   engine main   ✖ NIKA-SEC-004 ./secret/keys.txt is outside permits.fs.read
#
# The hook fires on its own after every edit, so an agent working ON the
# engine was handed a green from the release it was in the middle of
# fixing — and had no way to see which binary said it. Printing the
# resolved path and version costs one line and makes the divergence
# self-evident the moment it exists.
oracle="$(command -v "$NIKA" 2>/dev/null || printf '%s' "$NIKA")"
# The PATH is the identity. The version string is a TAG, and a tag does not
# order builds by what they contain: measured 2026-07-29, this tree's
# target/debug/nika reports 0.106.0 and carries the fs-permit FIX, while
# the PATH's brew build reports 0.106.1 and carries the FAIL-OPEN. A debug
# build's version tracks the last tag, not the tree, so the higher patch
# number was the vulnerable one. Printing the version first inverted the
# safety ordering for a reader — that was this hook's own repair, one
# iteration ago, and an agent sent to audit something else caught it.
tag="$("$NIKA" --version 2>/dev/null | head -1)"

if [ "$rc" -eq 0 ] && [ -n "${NIKA_CHECK_ANNOUNCE_CLEAN:-}" ]; then
  printf 'nika check --native-strict · clean · %s (tag %s)\n' "$oracle" "${tag:-unknown}" >&2
fi

if [ "$rc" -ne 2 ]; then
  # Clean (0) or broken oracle (3): nothing to teach — silence.
  done_quiet
fi

printf '%s\n' "$findings" | head -c 2000 >&2
# Name the FLAG, not just the verb. A reader told to "re-run nika check"
# runs the bare form, reads a green that this hook does not accept, and
# loops against a gate it cannot see.
printf '\nre-check with the same oracle this hook used:\n  %s check --native-strict %s\n' "$oracle" "$file" >&2
printf 'oracle: %s (tag %s · a tag does not say what the build contains)\n' \
  "$oracle" "${tag:-unknown}" >&2
printf 'inside the engine tree the hook prefers the tree build by default (F14) — override:\n  export NIKA_BIN=<path-to-nika>\n' >&2
if [ -n "$cc" ]; then
  # PostToolUse exit 2 = stderr fed to Claude, edit already applied.
  exit 2
fi
printf '{}\n'
exit 0
