<p align="center">
  <img src="./assets/nika-logo.png" alt="Nika" width="96">
</p>

# nika · author AI workflows as checkable files

Teach your agent to hand repeatable work to
[Nika](https://github.com/supernovae-st/nika): a plain-text
`.nika.yaml` workflow it can **check before a token is spent** and
**verify after**. One `Add` installs the full bundle.

```sh
brew install supernovae-st/tap/nika   # the binary first; the plugin invokes it
```

## What one install gives your agent

| Component | What it does |
|---|---|
| `nika-authoring` skill | the author → check → repair loop, taught step by step |
| `nika-author` subagent | routes an intent to a template, fills the `# SLOT:` markers, loops `nika check` until rc=0 — read-only, never runs the workflow |
| language rule | the 4-verb surface (`infer` · `exec` · `invoke` · `agent`), auto-loaded on `*.nika.yaml` |
| `/nika:check` · `/nika:explain` · `/nika:new` | audit a file · explain a finding code · scaffold from a template |
| check-on-edit hook | every agent edit to a `*.nika.yaml` is audited immediately (findings in the hook log; never blocks the edit) |
| MCP oracle (8 tools) | `nika_check` · `nika_explain` · `nika_schema` · `nika_examples` · `nika_template` · `nika_canon` · `nika_catalog` · `nika_tools` — read-only, by design |

<p align="center">
  <img src="https://raw.githubusercontent.com/supernovae-st/nika-vscode/main/media/check-as-you-type.gif" alt="nika check findings appearing as you type" width="640">
</p>

## The loop it teaches

1. route the intent to a template (`nika new --from <name>` or the
   `nika_template` tool)
2. fill every `# SLOT:` marker — touch nothing else
3. `nika check` — findings carry `NIKA-XXXX` codes with fix hints
4. repair, re-check, until `rc=0`
5. the human runs it: `nika run <file>`

No plugin store to audit on the workflow side either: everything
callable is a tool under `invoke:`, and the engine ships its own
[builtin library](https://nika.sh/tools).

## Good to know

- **macOS GUI PATH**: Cursor may not inherit your shell PATH — if the
  MCP oracle does not start, launch Cursor from a terminal once
  (`open -a Cursor`) or ensure `nika` is reachable from GUI apps.
- **Windows**: the check-on-edit hook is a bash script; without a bash
  on PATH it fails open (the edit always proceeds — hooks never block).
- Everything the oracle answers is read-only by design: the plugin can
  audit and teach, only YOU run workflows.

## Links

Docs: <https://docs.nika.sh> · Spec: <https://github.com/supernovae-st/nika-spec>
· Site: <https://nika.sh> · Engine (AGPL-3.0-or-later):
<https://github.com/supernovae-st/nika>
