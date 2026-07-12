# Changelog · the nika plugin

The bundle every marketplace installs (Claude Code · Codex · Cursor).
Versions move together across all manifests (the mirror gate pins it).

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
