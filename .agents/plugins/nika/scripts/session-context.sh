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
# gets silence ({}), never noise — and a chat without a reliable
# payload cwd stays chat_only: the hook's own process cwd is NEVER
# evidence of a project (P0-14).
#
# Two health probes ride the same seed (the kit teaches, it never
# installs):
#   · binary probe — every surface of this kit (MCP oracle · commands
#     · hooks · subagents) invokes the nika binary; without it the
#     whole kit is dead, so the seed teaches the install line in EVERY
#     workspace, nika-enabled or not. A broken install is the one
#     state where silence costs more than noise.
#   · version handshake — the kit is cut against a binary release
#     train; when the two diverge at major.minor (patch drift is
#     normal between trains) the seed names both versions and the
#     exact align command, direction-aware.
#
# The context string is STATIC — fixed content, hand-escaped once.
# The ONLY interpolated tokens are version strings sanitized to
# [0-9.] (tr -cd) and the workspace root, JSON-escaped at emission —
# so nothing user-controlled can break the JSON.
set -euo pipefail

input="$(cat)"

# Dialect sniff: `hook_event_name` is Claude Code's (SessionStart);
# Cursor's sessionStart payload has no such field. Same map, two
# envelope shapes (Cursor: additional_context · Claude Code:
# hookSpecificOutput.additionalContext).
cc=""
case "$input" in *hook_event_name*) cc=1 ;; esac

emit() {
  if [ -n "$cc" ]; then
    printf '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"%s"}}\n' "$1"
  else
    printf '{"additional_context":"%s"}\n' "$1"
  fi
}

# Plugin root — resolved BEFORE any cd (a relative $0 dies after cd).
# The host sets CLAUDE_PLUGIN_ROOT; standalone invocation falls back
# to the script's own location.
plugin_root="${CLAUDE_PLUGIN_ROOT:-}"
if [ -z "$plugin_root" ]; then
  plugin_root="$(cd "$(dirname "$0")/.." 2>/dev/null && pwd || true)"
fi

# Binary probe — PATH-based, workspace-independent.
bin_ok=""
command -v nika >/dev/null 2>&1 && bin_ok=1

install_teach='Nika kit note: the nika binary is not on PATH. Every surface of this kit (MCP oracle · /nika: commands · hooks · subagents) invokes it and stays dead until it lands. Install: brew install supernovae-st/tap/nika (other paths: nika.sh) · then restart the session.'

# The payload's cwd, when the envelope carries one. The value is then
# stripped of control characters (POSIX allows a newline in a path):
# a control byte would break the JSON envelope we later emit, and a
# stripped path that resolves nowhere simply stays chat_only (P0-14).
if command -v python3 >/dev/null 2>&1; then
  cwd="$(printf '%s' "$input" | python3 -c 'import json,sys
try:
    print(json.load(sys.stdin).get("cwd", ""))
except Exception:
    print("")' 2>/dev/null || true)"
else
  # No JSON parser: the FIRST "cwd" key wins — a later field echoing the
  # text "cwd" (a note, a nested doc) must not hijack the workspace root
  # (the greedy last-match sed used to hand it exactly that).
  cwd="$(printf '%s' "$input" | grep -o '"cwd"[[:space:]]*:[[:space:]]*"[^"]*"' | head -n 1 | sed 's/.*"\([^"]*\)"$/\1/')"
fi
cwd="$(printf '%s' "$cwd" | tr -d '\000-\037')"

# Workspace root: the payload cwd, and ONLY the payload cwd. A chat
# without a reliable folder stays chat_only — the hook's own process
# cwd is NEVER evidence of a project (P0-14 · audit UX 2026-07-30:
# payload {} or a dead cwd silently inherited the host's cwd and
# "found" a workspace no one named). Absent · invalid · unreachable
# → no cd, no detection.
evidence=""
root=""
if [ -n "$cwd" ] && [ -d "$cwd" ] && cd "$cwd" 2>/dev/null; then
  evidence="payload_cwd"
  # A session opened in a SUBDIR of the workspace must still get the
  # map (proven lost 2026-07-12): resolve the git toplevel when there
  # is one — the workspace markers (.nika/ · .cursor/rules/nika.mdc ·
  # *.nika.yaml) live at the root. Not a git repo → stay on the
  # payload cwd (old behavior).
  # The ambient git environment is NOT evidence either. `GIT_DIR` and
  # friends override `-C` and the cwd entirely, so an inherited one
  # would make this resolve somebody else's repository as the session's
  # root — the same class as trusting the process cwd, which the law
  # above already refuses. Scrubbed per-command, never for the caller.
  top="$(env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE -u GIT_COMMON_DIR \
    git rev-parse --show-toplevel 2>/dev/null || true)"
  [ -n "$top" ] && [ -d "$top" ] && cd "$top" 2>/dev/null || true
  root="$(pwd)"
fi

# Nika-enabled = a .nika/ store, an equipped repo (`nika init` wrote
# .cursor/rules/nika.mdc — the session right after init is exactly
# when the map matters, before any workflow exists), or any workflow
# file near the root. Bounded probe (depth 3 · prune the heavy dirs)
# — this runs at every session open and must stay in the milliseconds.
# chat_only (no payload cwd) → no probe at all: the process cwd is
# not a workspace.
enabled=""
if [ -n "$evidence" ]; then
  if [ -d .nika ] || [ -f .cursor/rules/nika.mdc ]; then
    enabled=1
  else
    hit="$(find . -maxdepth 3 \( -name node_modules -o -name .git -o -name target -o -name dist \) -prune -o \( -name '*.nika.yaml' -o -name '*.nika.yml' \) -print -quit 2>/dev/null || true)"
    [ -n "$hit" ] && enabled=1
  fi
fi

if [ -z "$enabled" ]; then
  # No workspace map — but a dead kit still teaches its own repair.
  if [ -z "$bin_ok" ]; then
    emit "$install_teach"
  else
    printf '{}\n'
  fi
  exit 0
fi

# The map is STATIC — fixed content, hand-escaped once, zero
# interpolation. Both envelopes carry the SAME text.
map='This workspace uses Nika (nika.sh): repeatable AI work lives in .nika.yaml workflow files, audited BEFORE they run (nika check), cost-bounded while they run, hash-chain traced after (.nika/traces/). Laws: (1) nika check <file> must pass before any run. When the human asks you to run it, run it yourself with --max-cost-usd (announce the ceiling first); nika guard judges every run at the hook. NEVER answer a human gate for them: when a run pauses (exit 4), surface the gate'"'"'s question in the conversation verbatim, wait for their answer, then resume with --resume <trace> --answer <task>=<their answer>. (2) Cost honesty: report the ceiling; a local model is unpriced, never free. (3) The boundary: an absent permits: block is ZERO authority, not a floor · any effect with no grant refuses NIKA-AUTH-006 at check. (4) Values ride three authorities · inputs (caller-supplied, and deployment-supplied via required: false + a default:) · const (baked in the file) · secrets (store references) · vars: and env: are dead envelope fields, and config: is not a field at all (NIKA-PARSE-005). Installed surfaces: read-only MCP oracle (nika_check, nika_inspect, nika_explain, nika_schema, nika_examples, nika_template, nika_canon, nika_catalog, nika_tools) · subagents nika-author (write a workflow), nika-debugger (root-cause a run from its trace), nika-migrator (port a script) · skills nika-authoring, nika-debugging, nika-operating, nika-migration · commands check, explain, new, trace, permits, doctor (slash-prefixed per your client). CLI: nika check|run|try|test|trace|explain|inspect|new|catalog|doctor|welcome|wire|model|init|spec|sign|key|mcp|lsp|dap|completions. When the user describes repeatable or multi-step AI work, propose a Nika workflow.'

# A claim names its proof: the resolved root and the evidence source
# (P0-14 — an unnamed "this workspace" taught the agent to trust a
# path it could not see). The root is the one interpolated token
# besides the sanitized versions: control characters are stripped (a
# POSIX-legal newline would otherwise break the envelope), then \ and "
# are JSON-escaped so a hostile or accented path cannot break it either.
root_json="$(printf '%s' "$root" | tr -d '\000-\037' | sed 's/\\/\\\\/g; s/"/\\"/g')"
origin="Workspace root: $root_json (evidence: $evidence). "

if [ -z "$bin_ok" ]; then
  emit "$origin$map $install_teach"
  exit 0
fi

# Version handshake — kit manifest vs `nika --version`, major.minor
# only (the kit follows release trains; patch drift is not a finding).
# Both tokens sanitized to [0-9.]; unreadable on either side → silence
# (never guess a version).
drift=""
kit_raw=""
for m in .claude-plugin/plugin.json .codex-plugin/plugin.json .cursor-plugin/plugin.json; do
  if [ -n "$plugin_root" ] && [ -f "$plugin_root/$m" ]; then
    kit_raw="$(grep -m1 '"version"' "$plugin_root/$m" 2>/dev/null || true)"
    [ -n "$kit_raw" ] && break
  fi
done
kitv="$(printf '%s' "$kit_raw" | tr -cd '0-9.')"
binv="$(nika --version 2>/dev/null | tr -cd '0-9.' || true)"
if [ -n "$kitv" ] && [ -n "$binv" ]; then
  kit_maj="$(printf '%s' "$kitv" | cut -d. -f1)"
  kit_min="$(printf '%s' "$kitv" | cut -d. -f2)"
  bin_maj="$(printf '%s' "$binv" | cut -d. -f1)"
  bin_min="$(printf '%s' "$binv" | cut -d. -f2)"
  if [ -n "$kit_maj" ] && [ -n "$kit_min" ] && [ -n "$bin_maj" ] && [ -n "$bin_min" ]; then
    if [ "$bin_maj" -lt "$kit_maj" ] || { [ "$bin_maj" -eq "$kit_maj" ] && [ "$bin_min" -lt "$kit_min" ]; }; then
      drift=' Version drift: plugin kit '"$kitv"' rides ahead of nika binary '"$binv"'. Align the binary: brew upgrade nika.'
    elif [ "$bin_maj" -gt "$kit_maj" ] || { [ "$bin_maj" -eq "$kit_maj" ] && [ "$bin_min" -gt "$kit_min" ]; }; then
      drift=' Version drift: nika binary '"$binv"' rides ahead of plugin kit '"$kitv"'. Refresh the kit from your marketplace (Codex: codex plugin marketplace upgrade nika · Claude Code: claude plugin marketplace update nika, then claude plugin update nika@nika).'
    fi
  fi
fi

emit "$origin$map$drift"
exit 0
