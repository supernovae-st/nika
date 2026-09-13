# Nika in this project

Workflows are `*.nika.yaml` files. A plain `nika init` founds the project
around the hello lesson, `workflows/01-hello.nika.yaml` (the file
`nika try 01-hello` rehearses). Audit it, run it offline, then create and
check your own file:

```sh
nika check workflows/01-hello.nika.yaml
nika run workflows/01-hello.nika.yaml --model mock/echo
nika new 01-hello my-first.nika.yaml
nika check my-first.nika.yaml
```

The hello lesson names a small local model; `--model mock/echo` rehearses it
with no key and no network. For a read-and-infer skeleton, use
`nika new chain chain.nika.yaml`, fill its `<SLOT: …>` prompt and provide
the `README.md` it reads before checking and running it.

A model override only mocks inference;
tools, subprocesses, file writes and network calls still execute. Read the
workflow's permits and model pins before running a different template.
`nika test <file>` compares a simulated run with an existing golden;
`--update` records a new expectation and is not a regression check.

Plain `nika init --yes` lays the project files, `nika.yaml` and the hello
lesson. `nika init --recipe agentic --yes` founds around a workflow
curriculum instead, and `--recipe minimal` lays the project files only.
`starter` uses a conversation on a terminal; headless callers can use
`nika new`.

## Team settings

`nika init` lays `nika.yaml`, the project file. Its budget ceiling and
trace-retention lines ride commented, so it governs nothing until your team
edits it; review those options together.
Project settings do not grant a workflow extra authority or authorize a run.
Existing files are preserved; `--force` explicitly replaces generated files.
To upgrade an existing setup, generate into a separate directory and merge
the changes you want.

## What the generated files do

- `AGENTS.md` holds shared workflow instructions; `CLAUDE.md` points to it.
  `.agents/skills/nika-authoring/` contains the skill and its linked guides.
- `.vscode/settings.json` connects YAML validation. Cursor rules and agents,
  plus Copilot instructions, adapt the same workflow guidance to those clients.
- `.mcp.json`, `.cursor/mcp.json` and `.agents/mcp_config.json` configure
  the Nika MCP oracle. `nika wire --help` lists the installed client registry;
  `nika wire detected --dry-run` previews machine-level client wiring.
- `.cursor/hooks.json` and `.claude/settings.json` register the scripts in
  each client's `hooks-nika/` directory. They provide session context, check
  workflow edits and judge shell execution. The client must load these settings
  and find `nika` on its PATH. Review Claude's `/hooks` view after configuration
  changes. A skipped settings file may still need the new hooks merged into it.
- `workflows/README.md` indexes the scaffolded workflows with their check
  and run commands.
- `.gitignore` keeps `.nika/traces/` out of Git. Traces can contain prompts,
  outputs and sensitive context; review them before sharing.

## What to commit

Review and commit workflows, expected goldens, `nika.yaml`, this guide,
agent instructions, local skills, hook scripts and shared editor/MCP settings.
Remove machine-specific paths and secrets before sharing configuration.
Review generated artifacts individually before committing them.

Keep credentials, `.env` secret values, signing private keys and raw traces
out of Git. The generated trace ignore is not a complete secret policy;
use your team's existing secret store and ignore rules.

`nika doctor` diagnoses the local installation. For authoring details, open
[AGENTS.md](AGENTS.md) and the linked project-local skill.
