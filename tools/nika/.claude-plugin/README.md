# Nika Claude Code Plugin

Claude Code plugin for [Nika](https://supernovae.studio) — semantic YAML workflow engine for AI tasks.

**Publisher**: SuperNovae Studio
**License**: AGPL-3.0-or-later
**Requires**: Nika CLI (`nika` binary in PATH)

## Installation

### 1. Install Nika

```bash
# Homebrew (macOS)
brew tap supernovae-st/tap && brew install nika

# Cargo (cross-platform)
cargo install nika

# Verify
nika --version
```

### 2. Enable Plugin

The plugin activates automatically when Claude Code detects `.nika.yaml` files or a `.nika/` directory in your workspace.

## Features

### Skills (5)

| Skill | Command | Description |
|-------|---------|-------------|
| Workflow Wizard | `/nika-wizard` | Interactive workflow creation — asks questions, designs DAG, generates YAML, validates |
| Doctor | `/nika-doctor` | Run diagnostics, check installation, suggest fixes |
| Setup | `/nika-setup` | Guided machine setup — binary, editors, API keys, project init |
| MCP Connect | `/nika-mcp-connect` | Configure MCP servers — add, test, list, troubleshoot |
| Course Tutor | `/nika-course-tutor` | Intelligent course tutoring with progressive hints |

### Agents (3)

| Agent | Description |
|-------|-------------|
| Workflow Architect | Design complex multi-task DAG workflows with optimal patterns |
| Workflow Debugger | Trace and debug workflow failures systematically |
| Nika Assistant | Answer questions about syntax, patterns, error codes |

### Hooks (3)

| Hook | Trigger | Action |
|------|---------|--------|
| PostToolUse | Write/Edit `.nika.yaml` | Auto-validate with `nika check` |
| SessionStart | Session opens | Detect workspace, report status |
| Stop | Session ends | Validate modified `.nika.yaml` files |

### MCP Integration

Configures Nika as an MCP server for Claude Code via `nika mcp serve`.

### LSP Integration

Configures the Nika Language Server for `.nika.yaml` files:
- Diagnostics (syntax errors, validation)
- Completions (verbs, fields, task references)
- Hover documentation
- Go to definition
- Code actions

## File Structure

```
.claude-plugin/
  plugin.json          Manifest
  hooks.json           Hook definitions
  .mcp.json            MCP server config
  .lsp.json            LSP server config
  README.md            This file
  skills/
    nika-wizard.md         Workflow creation wizard
    nika-doctor.md         Diagnostics and fixes
    nika-setup.md          Machine setup guide
    nika-mcp-connect.md    MCP server configuration
    nika-course-tutor.md   Course tutoring
  agents/
    workflow-architect.md  DAG design expert
    workflow-debugger.md   Failure tracing specialist
    nika-assistant.md      General help
  scripts/
    validate-nika.sh       PostToolUse hook script
    detect-workspace.sh    SessionStart hook script
```

## Configuration

In `plugin.json`:

| Setting | Default | Description |
|---------|---------|-------------|
| `nika.binary` | `"nika"` | Path to nika binary |
| `nika.autoValidate` | `true` | Auto-validate after editing .nika.yaml |
| `nika.strictValidation` | `false` | Use --strict mode (connects to MCP servers) |
| `nika.courseRoot` | `""` | Override course root directory |

## Requirements

- Nika CLI v0.39.0+ installed and in PATH
- At least one LLM API key set (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, etc.)
- LSP feature requires `--features lsp` compilation flag

## Links

- [Nika Repository](https://github.com/supernovae-st/nika)
- [SuperNovae Studio](https://supernovae.studio)
- [QR Code AI](https://qrcode-ai.com)
