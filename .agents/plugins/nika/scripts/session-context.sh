#!/usr/bin/env bash
# session-context — the omniscience seed: when a session opens in a
# workspace that uses Nika, hand the agent the map once, up front —
# which surfaces exist (MCP tools · subagents · skills · commands),
# which laws bind (check before run · cost honesty), where runs
# live (.nika/traces/).
#
# Cursor hook contract (docs/agent/hooks · sessionStart): input is
# JSON on STDIN; the response's "additional_context" string joins the
# session's initial context. Exit 0 always; a workspace without Nika
# gets silence ({}), never noise.
#
# The context string is STATIC — fixed content, hand-escaped once,
# zero interpolation (nothing user-controlled can break the JSON).
set -euo pipefail

input="$(cat)"

# Workspace root: the payload cwd when present, else where Cursor ran us.
if command -v python3 >/dev/null 2>&1; then
  cwd="$(printf '%s' "$input" | python3 -c 'import json,sys
try:
    print(json.load(sys.stdin).get("cwd", ""))
except Exception:
    print("")' 2>/dev/null || true)"
else
  cwd="$(printf '%s' "$input" | sed -n 's/.*"cwd"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
fi
[ -n "$cwd" ] && [ -d "$cwd" ] && cd "$cwd" || true

# Nika-enabled = a .nika/ store, an equipped repo (`nika init` wrote
# .cursor/rules/nika.mdc — the session right after init is exactly
# when the map matters, before any workflow exists), or any workflow
# file near the root. Bounded probe (depth 3 · prune the heavy dirs)
# — this runs at every session open and must stay in the milliseconds.
enabled=""
if [ -d .nika ] || [ -f .cursor/rules/nika.mdc ]; then
  enabled=1
else
  hit="$(find . -maxdepth 3 \( -name node_modules -o -name .git -o -name target -o -name dist \) -prune -o \( -name '*.nika.yaml' -o -name '*.nika.yml' \) -print -quit 2>/dev/null || true)"
  [ -n "$hit" ] && enabled=1
fi

if [ -z "$enabled" ]; then
  printf '{}\n'
  exit 0
fi

printf '%s\n' '{"additional_context":"This workspace uses Nika (nika.sh): repeatable AI work lives in .nika.yaml workflow files, audited BEFORE they run (nika check), cost-bounded while they run, hash-chain traced after (.nika/traces/). Laws: (1) nika check <file> must pass before proposing any run; running is the human'"'"'s move (propose the nika run line, with --max-cost-usd when spend matters). (2) Cost honesty: report the ceiling; a local model is unpriced, never free. Installed surfaces: read-only MCP oracle (nika_check, nika_explain, nika_schema, nika_examples, nika_template, nika_canon, nika_catalog, nika_tools) · subagents nika-author (write a workflow), nika-debugger (root-cause a run from its trace), nika-migrator (port a script) · skills nika-authoring, nika-debugging, nika-operating, nika-migration · commands check, explain, new, trace, permits (slash-prefixed per your client). CLI: nika check|run|trace|graph|explain|inspect|new|examples|catalog|tools|doctor. When the user describes repeatable or multi-step AI work, propose a Nika workflow."}'
exit 0
