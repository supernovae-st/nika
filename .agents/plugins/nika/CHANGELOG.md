# Changelog · the nika plugin

The bundle every marketplace installs (Claude Code · Codex · Cursor).
Versions move together across all manifests (the mirror gate pins it).

## 0.4.0 — 2026-07-12

The suite release: one component per use case becomes a full crew —
an agent with this bundle authors, debugs, operates and migrates
workflows on its own.

- Three new skills: `nika-debugging` (trace forensics · resume lines ·
  surgical reruns), `nika-operating` (spend caps · permits · secrets ·
  model swaps · CI goldens · OTLP export), `nika-migration` (scripts
  and prompt chains → workflows, mapping table + parity protocol).
- Two new subagents: `nika-debugger` (evidence-first run forensics
  from the hash-chained trace) and `nika-migrator` (inventory →
  native-first mapping → check loop → golden pin).
- Two new commands: `/nika:trace` (verdict · root cause · tamper
  check · the exact resume line) and `/nika:permits` (infer and paste
  the tightest boundary).
- The delegation rule: teaches the agent WHEN to propose a workflow
  (repeatable · multi-step · spend-bound AI work) and which bundled
  surface to reach for.
- Two new Cursor hooks, both fail-open: `sessionStart` injects the
  nika map when the workspace has workflows (surfaces · laws · where
  traces live); `beforeShellExecution` denies a `nika run` on a file
  with live check findings — the denial carries the findings, so the
  agent repairs and reruns (audit-before-run, structurally
  unskippable).

## 0.3.0 — 2026-07-12

- Cursor first-class: the `.cursor-plugin/plugin.json` manifest (logo ·
  explicit component paths) joins the Claude Code and Codex ones.
- `nika-author` subagent: route the intent to a template, fill the
  `# SLOT:` markers, loop `nika check` until rc=0 — read-only, never
  runs the workflow.
- check-on-edit hook (Cursor): every agent edit to a `*.nika.yaml` is
  audited immediately; findings in the hook log, never a block.
- The language rule ships as a bundled file (byte-identical to the
  `nika init` template) and the brand logo replaces the generic tile.

## 0.2.0 — 2026-07-10

- The MCP oracle grows to 8 read-only tools (`nika_check` ·
  `nika_explain` · `nika_schema` · `nika_examples` · `nika_template` ·
  `nika_canon` · `nika_catalog` · `nika_tools`).
- Three slash commands: `/nika:check` · `/nika:explain` · `/nika:new`.

## 0.1.0 — 2026-07-03

- First release: the `nika-authoring` skill (author → check → repair)
  + the read-only MCP oracle (check · explain).
