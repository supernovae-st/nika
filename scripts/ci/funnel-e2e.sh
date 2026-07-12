#!/usr/bin/env bash
# funnel-e2e · the stranger's first path, played against a BUILT binary.
#
# Release-gate version of the V-arc deep e2e (2026-07-09): structural
# asserts only — sections exist · promised commands exist in the clap
# tree · JSON envelope versions · stable semantic needles (FLOOR ·
# unpriced) · exit codes. Never whole transcripts (those live in docs,
# re-captured by hand per the RELEASED law).
#
# Runs OFFLINE in a throwaway HOME (no keys · no configs · TERM=dumb).
# Usage: funnel-e2e.sh <nika-binary>
set -euo pipefail
BIN="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
ROOT=$(mktemp -d)
trap 'rm -rf "$ROOT"' EXIT
HOME_DIR="$ROOT/home"
WS="$ROOT/ws"
mkdir -p "$HOME_DIR" "$WS"
cd "$WS"
FAILS=0
say() { printf '%s\n' "$*"; }
fail() {
  say "FAIL $*"
  FAILS=$((FAILS + 1))
}
run() { # run <name> <expected-exit> -- cmd...
  local name="$1" want="$2"
  shift 2
  [ "$1" = "--" ] && shift
  set +e
  OUT=$(env -i HOME="$HOME_DIR" PATH=/usr/bin:/bin TERM=dumb "$@" 2>&1)
  local got=$?
  set -e
  if [ "$got" -ne "$want" ]; then
    fail "[$name] exit=$got want=$want"
    printf '%s\n' "$OUT" | head -6 | sed 's/^/    /'
    return 1
  fi
}
need() { printf '%s' "$OUT" | grep -qF "$2" || fail "[$1] missing: $2"; }

HELP=$(env -i HOME="$HOME_DIR" PATH=/usr/bin:/bin TERM=dumb "$BIN" --help 2>&1)
has_cmd() { printf '%s' "$HELP" | grep -Eq "^[[:space:]]+$1([[:space:]]|$)"; }

say "── funnel e2e · $("$BIN" --version)"

# 1 · the mirror: sections + every arrow-promised command exists
run welcome 0 -- "$BIN" welcome
need welcome "this machine"
need welcome "start here"
for c in $(printf '%s' "$OUT" | grep -oE '→ nika [a-z-]+' | awk '{print $3}' | sort -u); do
  has_cmd "$c" || fail "[welcome] promises 'nika $c' — clap tree lacks it"
done
FIRST=$(printf '%s' "$OUT" | grep -oE 'nika examples run [a-z0-9-]+ --model mock/echo' | head -1)
[ -n "$FIRST" ] || fail "[welcome] no offline first command promised"
# shellcheck disable=SC2086 # the promise is played verbatim, word-split intended
[ -n "$FIRST" ] && run first-promise 0 -- "$BIN" ${FIRST#nika }
run welcome-json 0 -- "$BIN" welcome --json
need welcome-json '"welcome_version"'
# sovereignty canary: a key VALUE must never surface
OUT=$(env -i HOME="$HOME_DIR" PATH=/usr/bin:/bin TERM=dumb OPENAI_API_KEY=sk-CANARY-9911 "$BIN" welcome --json 2>&1)
printf '%s' "$OUT" | grep -q "sk-CANARY-9911" && fail "[welcome] key VALUE leaked"

# 2 · scaffold → audit (the inputs trap is TAUGHT) → provision → run → story → verify
run new-from 0 -- "$BIN" new --from chain first.nika.yaml
[ -f first.nika.yaml ] || fail "[new] no file created"
run check 0 -- "$BIN" check first.nika.yaml
need check "audited"
need check "[inputs]" # the scaffold's input trap is taught BEFORE the run
# Provision the input the scaffold DECLARES (./README.md since the
# pack-SSOT era · ./input.txt before) — the funnel plays the file,
# it never assumes the era.
SRC=$(sed -n 's/^  source: "\(\.\/[A-Za-z0-9._-]*\)".*/\1/p' first.nika.yaml | head -1)
echo demo >"${SRC:-./input.txt}"
run run-mock 0 -- "$BIN" run first.nika.yaml --model mock/echo
TRACE=$(find .nika/traces -name '*.ndjson' 2>/dev/null | sort | tail -1)
[ -n "$TRACE" ] || fail "[run] no trace recorded"
if has_cmd explain; then
  run explain-file 0 -- "$BIN" explain first.nika.yaml
  need explain-file "FLOOR"
  need explain-file "unpriced"
  need explain-file "flight recorder"
  need explain-file "$(basename "$TRACE")"
fi
run verify 0 -- "$BIN" trace verify "$TRACE"

# 3 · the workspace aggregate — `welcome --deep --json` since the 0.100
# Rams fold (`context` folded into it · same context_version contract),
# the retired verb as the older-binary door.
if has_cmd context; then
  CTX_CMD="context --json"
else
  CTX_CMD="welcome --deep --json"
fi
if true; then
  # shellcheck disable=SC2086 # two-word door, split intended
  run context-json 0 -- "$BIN" $CTX_CMD
  need context-json '"context_version"'
  need context-json '"rollups"'
  # a journal truncated after its opening line folds to verdict null
  head -1 "$TRACE" >.nika/traces/zz-truncated.ndjson
  # shellcheck disable=SC2086
  run context-trunc 0 -- "$BIN" $CTX_CMD
  printf '%s' "$OUT" | python3 -c '
import json, sys
d = json.load(sys.stdin)
runs = [r for r in d["workspace"]["runs"] if "zz-truncated" in r["trace"]]
assert runs, "truncated journal missing from runs"
assert runs[0]["verdict"] is None, f"opening event read as verdict: {runs[0]}"
' || fail "[context-trunc] crashed-run fold not honest"
fi

# 4 · agent briefing: every taught verb exists
run init 0 -- "$BIN" init
[ -f AGENTS.md ] || fail "[init] no AGENTS.md"
run init-again 0 -- "$BIN" init
for c in $(grep -oE '`nika [a-z-]+' AGENTS.md | awk '{print $2}' | tr -d '`' | sort -u); do
  has_cmd "$c" || fail "[agents-md] teaches 'nika $c' — clap tree lacks it"
done

# 5 · doctor diagnoses offline · broken files fail WITH a code
run doctor 0 -- "$BIN" doctor
# shellcheck disable=SC2016 # the ${{ }} island must reach the file UNEXPANDED
printf 'nika: v1\nworkflow: broken\nmodel: mock/echo\ntasks:\n  - id: a\n    exec: { command: "echo ${{ tasks.ghost.output }}" }\n' >broken.nika.yaml
set +e
OUT=$(env -i HOME="$HOME_DIR" PATH=/usr/bin:/bin TERM=dumb "$BIN" check broken.nika.yaml 2>&1)
GOT=$?
set -e
[ "$GOT" -eq 0 ] && fail "[broken] invalid workflow checked clean"
printf '%s' "$OUT" | grep -qE "NIKA-[A-Z0-9-]+[0-9]" || fail "[broken] no error code in voice"

# 6 · the managed-host wire: the http transport answers the same truth
# as stdio (one server · initialize + a real tools/call · the origin
# gate holds). curl ships on every runner this plays on; loopback only.
if command -v curl >/dev/null 2>&1; then
  MCP_PORT=$((20000 + RANDOM % 20000))
  env -i HOME="$HOME_DIR" PATH=/usr/bin:/bin TERM=dumb \
    "$BIN" mcp --transport http --port "$MCP_PORT" >/dev/null 2>&1 &
  MCP_PID=$!
  # the bind is asynchronous — poll briefly instead of a blind sleep
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    curl -s -o /dev/null -m 1 "http://127.0.0.1:$MCP_PORT/mcp" 2>/dev/null && break
    sleep 0.3
  done
  OUT=$(curl -s -m 8 -X POST "http://127.0.0.1:$MCP_PORT/mcp" \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"funnel","version":"0"}}}')
  printf '%s' "$OUT" | grep -qF '"protocolVersion"' || fail "[mcp-http] initialize: $OUT"
  OUT=$(curl -s -m 8 -X POST "http://127.0.0.1:$MCP_PORT/mcp" \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":2,"method":"tools/list"}')
  printf '%s' "$OUT" | grep -qF 'nika_check' || fail "[mcp-http] tools/list: $OUT"
  CODE=$(curl -s -o /dev/null -w '%{http_code}' -m 8 -X POST \
    -H 'Origin: https://evil.example.com' -H 'Content-Type: application/json' \
    -d '{}' "http://127.0.0.1:$MCP_PORT/mcp")
  [ "$CODE" = "403" ] || fail "[mcp-http] foreign origin answered $CODE, want 403"
  kill "$MCP_PID" 2>/dev/null || true
else
  say "── mcp-http leg skipped (no curl on this runner)"
fi

say "── funnel e2e: FAILS=$FAILS"
exit $((FAILS > 0))
