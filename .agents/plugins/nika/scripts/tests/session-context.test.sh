#!/usr/bin/env bash
# session-context.test · artifact test for the session-context hook:
# plays JSON payloads against the REAL shipped bytes, never a copy.
#
# The law under test (P0-14 · audit UX 2026-07-30): a chat without a
# reliable folder stays chat_only — the hook's own process cwd is NEVER
# evidence of a project. And when a workspace IS found, the emitted
# context names the resolved root and the evidence source, so the
# agent can tell proof from guesswork.
#
# Usage: session-context.test.sh  (exit 0 = all green)
set -euo pipefail

# This test builds throwaway repositories. An inherited git environment
# points every one of them at the caller's repository instead — which is
# exactly what happens under a git hook, where GIT_DIR is always set.
# The test died with « this operation must be run in a work tree » the
# first time anything ran it, from the pre-push gate (2026-08-02); it
# had sat unexecuted long enough for nobody to know.
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_OBJECT_DIRECTORY

HOOK="$(cd "$(dirname "$0")/.." && pwd)/session-context.sh"
ROOT="$(mktemp -d)"
trap 'rm -rf "$ROOT"' EXIT
FAILS=0
say() { printf '%s\n' "$*"; }
fail() {
  say "FAIL $*"
  FAILS=$((FAILS + 1))
}
pass() { say "ok   $*"; }

# play <dir-to-run-from> <payload> → OUT holds the hook's stdout.
play() {
  local from="$1" payload="$2"
  OUT="$(cd "$from" && printf '%s' "$payload" | bash "$HOOK")"
}

need() { # need <case> <needle>
  if printf '%s' "$OUT" | grep -qF "$2"; then
    pass "[$1] has: $2"
  else
    fail "[$1] missing: $2"
  fi
}
deny() { # deny <case> <anti-needle>
  if printf '%s' "$OUT" | grep -qF "$2"; then
    fail "[$1] forbidden: $2"
  else
    pass "[$1] clean of: $2"
  fi
}

say "── session-context · artifact test ($HOOK)"

# A poisoned inherited cwd: it LOOKS like a Nika workspace, but no
# payload ever named it — the hook must not call it one.
POISON="$ROOT/poison"
mkdir -p "$POISON/.nika"

# 1 · empty payload {} — zero folder evidence → chat_only, never the
# process cwd's markers (the reproduced P0-14).
play "$POISON" '{}'
deny empty-payload 'This workspace uses Nika'

# 2 · payload cwd pointing nowhere — invalid evidence is no evidence;
# falling back to the process cwd is the same bug wearing a hat.
play "$POISON" '{"cwd":"/nika-p014-no-such-dir"}'
deny invalid-payload-cwd 'This workspace uses Nika'

# 3 · valid payload cwd holding .nika — the normal lane still works,
# and the context NAMES the resolved root + the evidence source.
WS="$ROOT/ws"
mkdir -p "$WS/.nika"
ws_resolved="$(cd "$WS" && pwd)"
play "$ROOT" "{\"cwd\":\"$WS\"}"
need valid-payload-cwd 'This workspace uses Nika'
need valid-payload-cwd "$ws_resolved"
need valid-payload-cwd 'payload_cwd'

# 4 · payload cwd = a SUBDIR of a git workspace — the map must name the
# resolved git root (the subdir law, proven lost 2026-07-12), with the
# evidence still owned by the payload.
REPO="$ROOT/repo"
mkdir -p "$REPO/.nika" "$REPO/deep/nest"
git -C "$REPO" init -q
repo_resolved="$(git -C "$REPO" rev-parse --show-toplevel)"
play "$ROOT" "{\"cwd\":\"$REPO/deep/nest\"}"
need subdir-payload-cwd 'This workspace uses Nika'
need subdir-payload-cwd "$repo_resolved"
need subdir-payload-cwd 'payload_cwd'

# 4 · a workspace path carrying a POSIX-legal control character (a
# newline) — the envelope must stay parseable JSON, with no raw control
# byte leaking through the interpolated root.
EVIL="$ROOT/evil
path"
mkdir -p "$EVIL/.nika"
play "$ROOT" "$(printf '{"cwd":"%s"}' "$EVIL")"
if command -v python3 >/dev/null 2>&1; then
  if printf '%s' "$OUT" | python3 -c 'import json,sys; json.loads(sys.stdin.read())' 2>/dev/null; then
    pass "[control-char-path] envelope stays parseable JSON"
  else
    fail "[control-char-path] envelope broke on a control byte"
  fi
else
  deny control-char-path "$(printf '\n')"
fi

# 5 · the no-python3 fallback: a later field echoing the text "cwd"
# must not hijack the workspace root — the FIRST key wins.
REALWS="$ROOT/realws"
FAKEWS="$ROOT/fakews"
mkdir -p "$REALWS/.nika" "$FAKEWS/.nika"
NO_PY="$ROOT/nopy-bin"
mkdir -p "$NO_PY"
for tool in bash sh git sed grep tr find dirname head cat; do
  src="$(command -v "$tool" 2>/dev/null || true)"
  [ -n "$src" ] && ln -sf "$src" "$NO_PY/$tool"
done
OUT="$(cd "$ROOT" && printf '%s' "{\"cwd\":\"$REALWS\",\"note\":\"see \\\"cwd\\\": \\\"$FAKEWS\\\"\"}" | env "PATH=$NO_PY" bash "$HOOK")"
need fallback-first-cwd "$(cd "$REALWS" && pwd)"
deny fallback-first-cwd 'fakews'

if [ "$FAILS" -gt 0 ]; then
  say "── $FAILS failure(s)"
  exit 1
fi
say "── all green"
