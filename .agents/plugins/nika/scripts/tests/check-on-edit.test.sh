#!/usr/bin/env bash
# check-on-edit.test.sh — the project file is audited at the edit.
#
# Plays Claude Code PostToolUse payloads, and one Cursor afterFileEdit
# payload, against the REAL hook bytes. An edit to nika.yaml gets
# `nika check` (keys, required values) and `nika arm` (every beat:
# cadence, zone, suspension, workflow); a refused or UNREADY beat reaches
# the agent (exit 2, findings on stderr; Cursor: stderr, never a veto),
# while a READY beat, a told suspension and a project with no beat stay
# silent. A workflow edit keeps its own audit. The judge is NIKA_BIN,
# else this tree's own debug build, never the PATH's `nika`: a PATH
# launcher can depend on the HOME this test replaces. With neither, the
# test says SKIP visibly: the hook stays silent without a binary by
# design, so there is nothing to assert.
#
# Usage: check-on-edit.test.sh  (exit 0 = all green or SKIP)
set -uo pipefail
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_OBJECT_DIRECTORY

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOK="$HERE/../check-on-edit.sh"
REPO="$(cd "$HERE/../../../../.." && pwd)"
# The binary under test, most specific first, as check-taught-commands.sh
# resolves it: an explicit NIKA_BIN, the session target dir, the repo's own
# debug build.
JUDGE=""
for candidate in \
  "${NIKA_BIN:-}" \
  "${CARGO_TARGET_DIR:-}/debug/nika" \
  "$REPO/target/debug/nika"; do
  if [ -n "$candidate" ] && [ -x "$candidate" ]; then
    JUDGE="$candidate"
    break
  fi
done
if [ -z "$JUDGE" ]; then
  printf 'SKIP check-on-edit.test: no nika to judge with (set NIKA_BIN, or build target/debug/nika); nothing was asserted\n'
  exit 0
fi
ROOT="$(mktemp -d)"
trap 'rm -rf "$ROOT"' EXIT
fails=0

BEAT='nika: brief
permits: {}
tasks:
  noop:
    invoke: { tool: "nika:log", args: { message: "beat" } }
'

# project <dir> <arm entry lines after the cadence>: writes nika.yaml and the beat's workflow
project() {
  local dir="$ROOT/$1" cadence="$2" rest="$3"
  mkdir -p "$dir/workflows"
  printf '%s' "$BEAT" >"$dir/workflows/brief.nika"
  printf 'nika: my-project\narm:\n  - workflow: workflows/brief.nika\n    cadence: "%s"\n%s' "$cadence" "$rest" \
    >"$dir/nika.yaml"
}

# edit <name> <file> <want rc> <needle or ''> [cursor]: Claude Code's PostToolUse payload, or with a fifth
# argument Cursor's afterFileEdit payload (file_path at the root, findings on stderr, never a veto)
edit() {
  local name="$1" file="$2" want="$3" needle="$4" dialect="${5:-claude}" payload out rc=0
  if [ "$dialect" = cursor ]; then
    payload="$(printf '{"file_path":"%s"}' "$file")"
  else
    payload="$(printf '{"hook_event_name":"PostToolUse","tool_name":"Edit","tool_input":{"file_path":"%s"}}' "$file")"
  fi
  out="$(printf '%s' "$payload" | NIKA_BIN="$JUDGE" HOME="$ROOT" bash "$HOOK" 2>&1)" || rc=$?
  if [ "$rc" -ne "$want" ] || { [ -n "$needle" ] && [[ "$out" != *"$needle"* ]]; }; then
    printf 'FAIL  %s: want exit %s%s, got %s\n      %s\n' "$name" "$want" "${needle:+ with $needle}" "$rc" \
      "$(printf '%s' "$out" | head -c 400)" >&2
    fails=$((fails + 1))
  else
    printf 'ok    %s\n' "$name"
  fi
}

project ready 'TZ=Europe/Paris 30 7 * * 1-5' $'    plafond: 0.20\n    manqué: sauter\n'
edit 'a READY beat stays silent' "$ROOT/ready/nika.yaml" 0 ''

project zoneless '30 7 * * 1-5' $'    plafond: 0.20\n    manqué: sauter\n'
edit 'a cadence without its zone reaches the agent' "$ROOT/zoneless/nika.yaml" 2 'cadence.tz-missing'
edit 'the re-check names both judges' "$ROOT/zoneless/nika.yaml" 2 'arm --json'
edit "Cursor's dialect gets the finding without a veto" "$ROOT/zoneless/nika.yaml" 0 'cadence.tz-missing' cursor

project noplafond 'TZ=Europe/Paris 30 7 * * 1-5' $'    manqué: sauter\n'
edit 'a beat without plafond reaches the agent' "$ROOT/noplafond/nika.yaml" 2 'project.bad-value'

project unready 'TZ=Europe/Paris 30 7 * * 1-5' $'    plafond: 0.20\n    manqué: sauter\n'
rm "$ROOT/unready/workflows/brief.nika"
edit 'an UNREADY beat reaches the agent' "$ROOT/unready/nika.yaml" 2 '[UNREADY]'

project suspended 'TZ=Europe/Paris 30 7 * * 1-5' $'    plafond: 0.20\n    manqué: sauter\n    actif: false\n    raison: "maintenance"\n    jusqu_au: 2099-12-31\n'
edit 'a told suspension stays silent' "$ROOT/suspended/nika.yaml" 0 ''

project untold 'TZ=Europe/Paris 30 7 * * 1-5' $'    plafond: 0.20\n    manqué: sauter\n    actif: false\n    jusqu_au: 2099-12-31\n'
edit 'a suspension without its reason reaches the agent' "$ROOT/untold/nika.yaml" 2 'cadence.suspension-sans-raison'

if command -v python3 >/dev/null 2>&1; then
  mkdir -p "$ROOT/nested/workflows"
  printf 'nika: brief\npermits:\n  tools: ["nika:log"]\ntasks:\n  noop:\n    invoke: { tool: "nika:log", args: { message: "child" } }\n' \
    >"$ROOT/nested/workflows/brief.nika"
  printf 'nika: parent\npermits:\n  tools: ["nika:log"]\ntasks:\n  call:\n    invoke: { workflow: "./brief.nika" }\n' \
    >"$ROOT/nested/workflows/parent.nika"
  printf 'nika: my-project\narm:\n  - workflow: workflows/parent.nika\n    cadence: "TZ=Europe/Paris 30 7 * * 1-5"\n    plafond: 0.20\n    manqué: sauter\n' \
    >"$ROOT/nested/nika.yaml"
  edit 'a beat calling a child (an unknown, no blocker) stays silent' "$ROOT/nested/nika.yaml" 0 ''
fi

mkdir -p "$ROOT/starter" "$ROOT/nobeat"
cat >"$ROOT/starter/nika.yaml" <<'YAML'
# nika.yaml — the project file (OPTIONAL · an absent key is its built-in default).
nika: my-project

# ceiling: 0.50     # default max-cost-usd for runs of this project · --max-cost-usd always wins
YAML
edit 'the starter project file stays silent' "$ROOT/starter/nika.yaml" 0 ''
printf 'nika: my-project\nceiling: 0.50\ntraces:\n  keep: 30d\n' >"$ROOT/nobeat/nika.yaml"
edit 'a project without beats stays silent' "$ROOT/nobeat/nika.yaml" 0 ''

mkdir -p "$ROOT/plain"
printf 'a: 1\n' >"$ROOT/plain/settings.yaml"
edit 'another yaml file is not audited' "$ROOT/plain/settings.yaml" 0 ''

printf 'nika: broken\nschedule: "0 7 * * 1-5"\ntasks:\n  noop:\n    invoke: { tool: "nika:log", args: { message: "x" } }\n' \
  >"$ROOT/plain/broken.nika"
edit 'a workflow keeps its own audit' "$ROOT/plain/broken.nika" 2 'NIKA-PARSE-005'

if [ "$fails" -ne 0 ]; then
  printf '%s failure(s) · judge %s\n' "$fails" "$JUDGE" >&2
  exit 1
fi
printf 'check-on-edit: all green · judge %s\n' "$JUDGE"
