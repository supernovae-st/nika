<p align="center">
  <img src="./assets/nika-logo.png" alt="Nika" width="96">
</p>

# nika · author AI workflows as checkable files

Teach your agent to hand repeatable work to
[Nika](https://github.com/supernovae-st/nika): a plain-text
`.nika.yaml` workflow it can **check before a token is spent** and
**verify after**. One `Add` installs the full bundle — your agent
learns to author, debug, operate and migrate workflows on its own.

```sh
brew install supernovae-st/tap/nika   # the binary first; the plugin invokes it
```

## What one install gives your agent

| Component | What it does |
|---|---|
| `nika-authoring` skill | the author → check → repair loop, taught step by step |
| `nika-debugging` skill | run forensics: trace ls → show → outputs → verify · resume lines · surgical reruns |
| `nika-operating` skill | day-2 hardening: spend caps · permits · secrets · model swaps · CI goldens · OTLP export |
| `nika-migration` skill | convert scripts, CI jobs and prompt chains to workflows — mapping table + parity protocol |
| `nika-author` subagent | routes an intent to a template, fills the `# SLOT:` markers, loops `nika check` until rc=0 — read-only, never runs the workflow |
| `nika-debugger` subagent | root-causes a failed or paused run from its hash-chained trace, hands back the exact resume line |
| `nika-migrator` subagent | ports existing automation: inventory → native-first mapping → check loop → golden pin |
| language rule | the 4-verb surface (`infer` · `exec` · `invoke` · `agent`), auto-loaded on `*.nika.yaml` |
| delegation rule | teaches the agent WHEN to propose a workflow (repeatable · multi-step · spend-bound AI work) and which bundled surface to reach for |
| `/nika:check` · `/nika:explain` · `/nika:new` | audit a file · explain a finding code · scaffold from a template |
| `/nika:trace` · `/nika:permits` | read a run's flight recorder (verdict · root cause · resume line) · infer and paste the tightest permits boundary |
| session-context hook | a workspace with workflows greets the agent with the full nika map at session start (surfaces · laws · where traces live) — Cursor **and** Claude Code dialects |
| check-on-edit hook | every agent edit to a `*.nika.yaml` is audited immediately (findings in the hook log; never blocks the edit) |
| guard-run hook | `nika run` on a file that fails `nika check` is denied with the findings — the audit-before-run law, structurally unskippable (Cursor `beforeShellExecution` · Claude Code `PreToolUse`) |
| MCP oracle (9 tools) | `nika_check` · `nika_explain` · `nika_schema` · `nika_examples` · `nika_template` · `nika_canon` · `nika_catalog` · `nika_tools` · `nika_inspect` — read-only, by design |

<p align="center">
  <img src="https://raw.githubusercontent.com/supernovae-st/nika-vscode/main/media/check-as-you-type.gif" alt="nika check findings appearing as you type" width="640">
</p>

## The loop it teaches

1. route the intent to a template (`nika new --from <name>` or the
   `nika_template` tool)
2. fill every `# SLOT:` marker — touch nothing else
3. `nika check` — findings carry `NIKA-XXXX` codes with fix hints
4. repair, re-check, until `rc=0`
5. the human runs it: `nika run <file>` — and the trace proves it
   (`nika trace verify`)

No plugin store to audit on the workflow side either: everything
callable is a tool under `invoke:`, and the engine ships its own
[builtin library](https://nika.sh/tools).

## Good to know

- **macOS GUI PATH**: Cursor may not inherit your shell PATH — if the
  MCP oracle does not start, launch Cursor from a terminal once
  (`open -a Cursor`) or ensure `nika` is reachable from GUI apps.
- **Windows**: the hooks are bash scripts; without a bash on PATH they
  fail open (edits and runs always proceed — hooks never brick a
  machine).
- Every hook fails open by design: a missing binary, an unreadable
  file or a broken oracle never blocks you. The ONLY deny is a
  `nika run` on a file with live check findings — and the denial
  carries the findings, so the agent repairs and reruns by itself.
- Everything the oracle answers is read-only by design: the plugin can
  audit and teach, only YOU run workflows.

## Links

Docs: <https://docs.nika.sh> · Spec: <https://github.com/supernovae-st/nika-spec>
· Site: <https://nika.sh> · Engine (AGPL-3.0-or-later):
<https://github.com/supernovae-st/nika>
