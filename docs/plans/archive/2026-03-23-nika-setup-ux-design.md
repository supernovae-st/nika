# `nika setup` — Complete UX Design

**Date:** 2026-03-23
**Status:** Design
**Scope:** Machine-level setup wizard (global, run once)

---

## Overview

`nika setup` configures a user's MACHINE for Nika development. It is the complement to `nika init` (project-level). You run `nika setup` once per machine; you run `nika init` once per project.

```
nika setup   →  machine-level  →  ~/.nika/, editors, AI tools, shell, git
nika init    →  project-level  →  .nika/, workflows, context, schemas
```

---

## Command Surface

```
nika setup                    # Interactive wizard (cliclack)
nika setup editors             # Non-interactive: editors + extensions + LSP
nika setup ai                  # Non-interactive: AI coding tool rules
nika setup completions         # Non-interactive: shell completions
nika setup git                 # Non-interactive: git hooks + config
nika setup --all               # Non-interactive: everything
nika setup --check             # Dry-run: show what WOULD be done
```

---

## 1. `nika setup` — Interactive Wizard

### Full Terminal Flow

```
┌  nika v0.39.1 // machine setup
│
🦋  Setting up your machine for Nika development.
│   This configures editors, AI tools, shell, and git.
│   Run once per machine. Safe to re-run (idempotent).
│
◆  Scanning environment...
│
├  System
│  ✓ macOS 15.4 (arm64)
│  ✓ nika v0.39.1 (/opt/homebrew/bin/nika)
│  ✓ nika-lsp compiled in
│  ✓ rustc 1.87.0
│  ✓ node v22.14.0 / npx 10.9.2
│
├  Editors detected
│  ✓ VS Code 1.96.2 (/usr/local/bin/code)
│  ✓ Cursor 0.45.3 (/usr/local/bin/cursor)
│  ✗ Windsurf (not found)
│  ✗ Zed (not found)
│  ✓ Neovim 0.11.0 (/opt/homebrew/bin/nvim)
│  ✗ Sublime Text (not found)
│  ✗ JetBrains (not found)
│
◆  Which editors should Nika configure?
│  ◻ VS Code — install nika-lang extension + LSP config
│  ◻ Cursor — install nika-lang extension + LSP config
│  ◻ Neovim — configure LSP (lspconfig)
│  (only detected editors shown)
│
├  Installing nika-lang extension...
│  ✓ VS Code: supernovae-st.nika-lang installed
│  ✓ Cursor: supernovae-st.nika-lang installed
│
├  Configuring LSP...
│  ✓ VS Code: .vscode/settings.json updated (nika-lsp path)
│  ✓ Cursor: LSP settings configured
│  ✓ Neovim: ~/.config/nvim/lua/plugins/nika.lua created
│
├  AI coding tools detected
│  ✓ Claude Code (~/.claude/)
│  ✗ GitHub Copilot (not found)
│  ✗ Roo Code (not found)
│
◆  Configure AI tool integration?
│  ◻ Claude Code — install agent skills + MCP server
│  (only detected tools shown)
│
├  Configuring Claude Code...
│  ✓ MCP server added to ~/.claude/settings.json
│  ✓ Agent skill installed: ~/.claude/skills/nika-workflow/
│
◆  Configure shell completions?
│  ● zsh (detected shell)
│  ○ bash
│  ○ fish
│  ○ Skip
│
├  Shell completions
│  ✓ Wrote ~/.zfunc/_nika
│  ✓ Added fpath to ~/.zshrc (autoload -Uz compinit)
│
◆  Configure git integration?
│  ◻ prepare-commit-msg hook (co-author line)
│  ◻ .gitattributes (*.nika.yaml linguist-language=YAML)
│
├  Git integration
│  ✓ Global gitattributes updated
│
└  Setup complete! 9 actions performed.

   Next steps:
     nika init              Initialize a project
     nika doctor            Verify everything works
     nika setup --check     See current status anytime
```

### Wizard Steps (Detailed)

#### Step 1: Welcome + System Scan

```rust
cliclack::set_theme(NikaTheme);
cliclack::intro("nika v0.39.1 // machine setup");
cliclack::note("Setting up your machine...", "...");
```

System scan detects:
- OS + architecture (`uname -s`, `uname -m`)
- nika binary path (`which nika`)
- nika version (`env!("CARGO_PKG_VERSION")`)
- LSP feature flag (`cfg!(feature = "lsp")`)
- rustc version (`rustc --version`)
- node + npx (`node --version`, `npx --version`)

#### Step 2: Editor Detection

| Editor | Binary names | macOS .app fallback | Config path |
|--------|-------------|---------------------|-------------|
| VS Code | `code` | `/Applications/Visual Studio Code.app/.../bin/code` | `~/.vscode/` |
| Cursor | `cursor` | `/Applications/Cursor.app/.../bin/cursor` | `~/.cursor/` |
| Windsurf | `windsurf` | `/Applications/Windsurf.app/.../bin/windsurf` | `~/.windsurf/` |
| Zed | `zed` | `/Applications/Zed.app/Contents/MacOS/zed` | `~/.config/zed/` |
| Neovim | `nvim` | n/a | `~/.config/nvim/` |
| Sublime Text | `subl` | `/Applications/Sublime Text.app/.../bin/subl` | `~/Library/Application Support/Sublime Text/` |
| JetBrains | `idea`, `webstorm`, `clion`, `pycharm`, `rustrover` | n/a | Plugin marketplace |

Detection method:
```rust
fn detect_editor(bins: &[&str], app_path: Option<&str>) -> Option<EditorInfo> {
    // 1. Try each binary with `--version`
    // 2. If none in PATH, try macOS .app bundle path
    // 3. Return EditorInfo { name, version, bin_path, config_dir }
}
```

#### Step 3: Editor Configuration

For **VS Code / Cursor / Windsurf** (VS Code forks):

1. **Extension install:**
   ```bash
   code --install-extension supernovae-st.nika-lang
   ```

2. **LSP configuration** (only if extension not yet configured):
   Writes to `~/.vscode/settings.json` (global user settings):
   ```json
   {
     "nika.lsp.path": "/opt/homebrew/bin/nika",
     "nika.lsp.args": ["lsp"]
   }
   ```

3. **Check if already installed:**
   ```bash
   code --list-extensions | grep -i nika-lang
   ```
   If already installed, show checkmark and skip.

For **Zed:**

Writes `~/.config/zed/settings.json` LSP entry:
```json
{
  "lsp": {
    "nika-lsp": {
      "binary": { "path": "/opt/homebrew/bin/nika", "arguments": ["lsp"] },
      "languages": ["YAML"]
    }
  }
}
```

For **Neovim:**

Creates `~/.config/nvim/lua/plugins/nika.lua`:
```lua
-- Nika LSP configuration (generated by `nika setup`)
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.nika_lsp then
  configs.nika_lsp = {
    default_config = {
      cmd = { 'nika', 'lsp' },
      filetypes = { 'yaml' },
      root_dir = lspconfig.util.root_pattern('.nika', '.git'),
      settings = {},
    },
  }
end

lspconfig.nika_lsp.setup({
  on_attach = function(client, bufnr)
    -- Only activate for .nika.yaml files
    local filename = vim.api.nvim_buf_get_name(bufnr)
    if not filename:match('%.nika%.yaml$') then
      client.stop()
    end
  end,
})
```

For **Sublime Text:**

Creates `~/Library/Application Support/Sublime Text/Packages/User/LSP-nika.sublime-settings`:
```json
{
  "clients": {
    "nika-lsp": {
      "enabled": true,
      "command": ["nika", "lsp"],
      "selector": "source.yaml",
      "settings": {}
    }
  }
}
```

For **JetBrains:**

Shows a manual instruction (no CLI API):
```
🦋  JetBrains IDEs require manual LSP setup:
    1. Install "YAML" plugin (if not already)
    2. Settings → Languages → Language Servers → Add
    3. Command: nika lsp
    4. File pattern: *.nika.yaml
```

#### Step 4: AI Tool Detection

| Tool | Detection method | Config location |
|------|-----------------|-----------------|
| Claude Code | `~/.claude/` directory exists | `~/.claude/settings.json` |
| Cursor AI | `~/.cursor/` directory exists (already covered by editor) | `.cursor/rules/` |
| Copilot | `gh extension list \| grep copilot` | `.github/copilot-instructions.md` |
| Roo Code | `~/.roo/` directory exists | `~/.roo/rules/` |
| Windsurf AI | `~/.windsurf/` directory exists (already covered by editor) | `.windsurf/rules/` |
| Cline | `~/.cline/` directory exists | `~/.cline/rules/` |

Detection method:
```rust
fn detect_ai_tool(name: &str, home_dir: &Path) -> Option<AiToolInfo> {
    // Check for config directory existence
    // For Claude Code: also check for ~/.claude/settings.json
    // For Copilot: check `gh extension list`
}
```

#### Step 5: AI Tool Configuration

For **Claude Code:**

1. **MCP server** — add to `~/.claude/settings.json`:
   ```json
   {
     "mcpServers": {
       "nika": {
         "command": "nika",
         "args": ["mcp", "serve"]
       }
     }
   }
   ```

2. **Agent skill** — create `~/.claude/skills/nika-workflow/skill.md`:
   ```markdown
   ---
   name: nika-workflow
   description: Create, validate, and run Nika YAML workflows
   ---

   # Nika Workflow Skill

   You can help users write and debug Nika workflows (.nika.yaml files).

   ## Schema
   All workflows start with `schema: "nika/workflow@0.12"`

   ## 5 Verbs
   - `infer:` — LLM generation
   - `exec:` — Shell commands
   - `fetch:` — HTTP requests
   - `invoke:` — MCP tool calls
   - `agent:` — Multi-turn loops

   ## Commands
   - `nika check <file>` — Validate syntax
   - `nika run <file>` — Execute workflow
   - `nika new --wizard` — Create from template

   ## Key Rules
   - Extension must be `.nika.yaml`
   - Tasks need `id:` and exactly one verb
   - Use `depends_on: [task_id]` for ordering
   - Use `with: { alias: $task_id }` for data flow
   - `$` prefix required in with bindings
   ```

For **Cursor AI** (global rules):

Creates `~/.cursor/rules/nika.mdc`:
```markdown
---
description: Rules for working with Nika workflow files (.nika.yaml)
globs: ["*.nika.yaml"]
---

# Nika Workflow Rules

Schema: `nika/workflow@0.12`

## 5 Verbs
- `infer:` — LLM generation (prompt, model, temperature, max_tokens)
- `exec:` — Shell commands (command, working_dir, env)
- `fetch:` — HTTP requests (url, method, headers, body)
- `invoke:` — MCP tool calls (tool, params, server)
- `agent:` — Multi-turn loops (goal, tools, max_turns)

## Key Patterns
- `depends_on: [task_id]` for task ordering
- `with: { alias: $task_id }` for data flow ($ prefix required)
- `{{with.alias}}` for template interpolation
- `{{inputs.key}}` for workflow inputs

## Validation
Run `nika check <file>` to validate before committing.
```

For **Copilot:**

Creates `~/.github/copilot-instructions.md` (global):
```markdown
# Nika Workflow Instructions

When working with .nika.yaml files, follow the Nika workflow schema (nika/workflow@0.12).

## Structure
- `schema:` — Always "nika/workflow@0.12"
- `workflow:` — Workflow name
- `tasks:` — Array of task objects
  - `id:` — Unique task identifier
  - One verb per task: `infer:`, `exec:`, `fetch:`, `invoke:`, `agent:`
  - `depends_on:` — Array of task IDs for ordering
  - `with:` — Data bindings using `$task_id` prefix
```

#### Step 6: Shell Completions

Detection:
```rust
fn detect_shell() -> Shell {
    // 1. Check $SHELL env var
    // 2. Parse basename (zsh, bash, fish)
    // 3. Default to current shell from process
}
```

Installation paths:

| Shell | Completion file | Extra setup needed |
|-------|----------------|--------------------|
| zsh | `~/.zfunc/_nika` | Add `fpath=(~/.zfunc $fpath)` + `autoload -Uz compinit && compinit` to `~/.zshrc` |
| bash | `~/.local/share/bash-completion/completions/nika` | None (auto-loaded by bash-completion) |
| fish | `~/.config/fish/completions/nika.fish` | None (auto-loaded by fish) |

Generation:
```rust
clap_complete::generate(shell, &mut Cli::command(), "nika", &mut file);
```

Idempotency:
- Check if completion file already exists and has same content → skip with checkmark
- For zsh: check if `~/.zshrc` already contains `fpath` line → skip

#### Step 7: Git Integration

```
◆  Configure git integration?
│  ◻ prepare-commit-msg hook (auto-add co-author line)
│  ◻ .gitattributes (*.nika.yaml linguist-language=YAML)
│  ◻ .gitignore patterns (.nika/traces/, .nika/cache/, .nika/media/store/)
```

**prepare-commit-msg hook** — writes to `~/.config/git/hooks/prepare-commit-msg` (global):
```bash
#!/bin/sh
# Nika: auto-add co-author line (installed by `nika setup`)
# Only activates in repos with a .nika/ directory

if [ -d ".nika" ] && ! grep -q "Co-Authored-By: Nika" "$1"; then
  echo "" >> "$1"
  echo "Co-Authored-By: Nika 🦋 <nika@supernovae.studio>" >> "$1"
fi
```

**Global .gitattributes** — appends to `~/.config/git/attributes`:
```
*.nika.yaml linguist-language=YAML
```

**Global .gitignore** — appends to `~/.config/git/ignore`:
```
# Nika (added by nika setup)
.nika/traces/
.nika/cache/
.nika/media/store/
```

#### Step 8: Summary + Outro

```
└  Setup complete! 9 actions performed.

   Next steps:
     nika init              Initialize a project
     nika doctor            Verify everything works
     nika setup --check     See current status anytime
```

---

## 2. `nika setup editors` — Non-Interactive

Runs only the editor detection + configuration steps. No prompts — configures ALL detected editors.

```
$ nika setup editors

┌─ Nika Setup: Editors ─────────────────────────────┐
│ v0.39.1 | Configuring editor integrations...       │
└────────────────────────────────────────────────────┘

  ✓ Editor VS Code 1.96.2 detected
  ✓ Extension supernovae-st.nika-lang already installed
  ✓ LSP path configured in VS Code settings

  ✓ Editor Cursor 0.45.3 detected
  ✓ Extension supernovae-st.nika-lang installed
  ✓ LSP path configured in Cursor settings

  ✓ Editor Neovim 0.11.0 detected
  ✓ LSP config written to ~/.config/nvim/lua/plugins/nika.lua

  ⚠ Editor Zed not found (skipped)
  ⚠ Editor Windsurf not found (skipped)
  ⚠ Editor Sublime Text not found (skipped)
  ⚠ Editor JetBrains not found (skipped)

──────────────────────────────────────────────────
✓ 3 editors configured, 4 not found
```

---

## 3. `nika setup ai` — Non-Interactive

Runs only the AI tool detection + rules installation.

```
$ nika setup ai

┌─ Nika Setup: AI Tools ────────────────────────────┐
│ v0.39.1 | Configuring AI coding tool integration...│
└────────────────────────────────────────────────────┘

  ✓ Claude Code detected (~/.claude/)
  ✓ MCP server "nika" added to ~/.claude/settings.json
  ✓ Agent skill installed: ~/.claude/skills/nika-workflow/

  ⚠ GitHub Copilot not found (skipped)
  ⚠ Roo Code not found (skipped)

──────────────────────────────────────────────────
✓ 1 AI tool configured, 2 not found
```

---

## 4. `nika setup completions` — Non-Interactive

Detects shell and installs completions.

```
$ nika setup completions

┌─ Nika Setup: Shell Completions ───────────────────┐
│ v0.39.1 | Installing shell completions...          │
└────────────────────────────────────────────────────┘

  ✓ Detected shell: zsh
  ✓ Wrote completion file: ~/.zfunc/_nika
  ✓ fpath already configured in ~/.zshrc
  🦋 Restart your shell or run: source ~/.zshrc

──────────────────────────────────────────────────
✓ Shell completions installed for zsh
```

---

## 5. `nika setup git` — Non-Interactive

Configures git hooks and attributes.

```
$ nika setup git

┌─ Nika Setup: Git Integration ─────────────────────┐
│ v0.39.1 | Configuring git for Nika...              │
└────────────────────────────────────────────────────┘

  ✓ Global gitattributes: *.nika.yaml linguist-language=YAML
  ✓ Global gitignore: .nika/traces/, .nika/cache/, .nika/media/store/
  ⚠ prepare-commit-msg hook: skipped (already exists)

──────────────────────────────────────────────────
✓ Git integration configured
```

---

## 6. `nika setup --all` — Non-Interactive

Runs all four subcommands in sequence. Same output as `nika setup editors` + `nika setup ai` + `nika setup completions` + `nika setup git`, concatenated.

```
$ nika setup --all

┌─ Nika Setup: Full ────────────────────────────────┐
│ v0.39.1 | Configuring everything...                │
└────────────────────────────────────────────────────┘

  ── Editors ──────────────────────────────────────
  ✓ VS Code: extension + LSP configured
  ✓ Cursor: extension + LSP configured
  ✓ Neovim: LSP configured

  ── AI Tools ─────────────────────────────────────
  ✓ Claude Code: MCP server + agent skill

  ── Shell ────────────────────────────────────────
  ✓ zsh completions installed

  ── Git ──────────────────────────────────────────
  ✓ gitattributes configured
  ✓ gitignore configured

──────────────────────────────────────────────────
✓ Setup complete! 9 actions performed.
```

---

## 7. `nika setup --check` — Dry Run

Shows current state without changing anything. Uses the doctor-style icons.

```
$ nika setup --check

┌─ Nika Setup: Status ──────────────────────────────┐
│ v0.39.1 | Checking machine configuration...        │
└────────────────────────────────────────────────────┘

  ── Editors ──────────────────────────────────────
  ✓ VS Code 1.96.2 — extension installed, LSP configured
  ✗ Cursor 0.45.3 — extension not installed
    → Run: nika setup editors
  ✓ Neovim 0.11.0 — LSP configured

  ── AI Tools ─────────────────────────────────────
  ✓ Claude Code — MCP server configured, skill installed
  ✗ Copilot — instructions not found
    → Run: nika setup ai

  ── Shell ────────────────────────────────────────
  ✓ zsh completions installed (~/.zfunc/_nika)

  ── Git ──────────────────────────────────────────
  ✓ gitattributes configured
  ✗ gitignore not configured
    → Run: nika setup git

──────────────────────────────────────────────────
⚠ 2 items need attention — run: nika setup --all
```

---

## 8. `nika init` Additions (Project-Level AI Integration)

When `nika init` runs (project mode), it should ALSO create these project-level AI configuration files. These are added to the existing `init_project()` function.

### New files created by `nika init`:

#### `.vscode/extensions.json`
```json
{
  "recommendations": [
    "supernovae-st.nika-lang"
  ]
}
```

#### `.claude/rules/nika.md`
```markdown
# Nika Workflow Rules

This project uses Nika workflows (`.nika.yaml` files).

## Schema
All workflows use `schema: "nika/workflow@0.12"`

## 5 Verbs
| Verb | Purpose |
|------|---------|
| `infer:` | LLM generation |
| `exec:` | Shell commands |
| `fetch:` | HTTP requests |
| `invoke:` | MCP tool calls |
| `agent:` | Multi-turn loops |

## Commands
- `nika check <file>` — Validate workflow
- `nika run <file>` — Execute workflow
- `nika ui` — Interactive TUI

## Conventions
- Extension: `.nika.yaml` (not `.yaml`)
- Tasks need unique `id:` + exactly one verb
- `depends_on: [task_id]` for ordering
- `with: { alias: $task_id }` for data flow (`$` prefix required)
- `{{with.alias}}` for template interpolation
```

#### `.cursor/rules/nika.mdc`
```markdown
---
description: Rules for working with Nika workflow files
globs: ["*.nika.yaml"]
---

# Nika Workflow Rules

(same content as above, with Cursor MDC frontmatter)
```

#### `.github/copilot-instructions.md`
(Same content, Copilot format — only if `.github/` exists already)

#### `.windsurf/rules/nika.md`
(Same content — only if `.windsurf/` exists already or windsurf detected)

#### `.roo/rules/nika.md`
(Same content — only if `.roo/` exists already or roo detected)

#### `AGENTS.md`
```markdown
# Agents

This project uses [Nika](https://github.com/supernovae-st/nika) for AI workflows.

## Workflow Files
All `.nika.yaml` files define DAG workflows with 5 verbs:
`infer:`, `exec:`, `fetch:`, `invoke:`, `agent:`

## Validation
Run `nika check <file>` before committing any workflow changes.

## Execution
Run `nika run <file>` to execute a workflow headlessly.
Run `nika ui` for the interactive terminal UI.
```

#### `.git/hooks/prepare-commit-msg` (project-level)
```bash
#!/bin/sh
# Nika: auto-add co-author line (installed by `nika init`)
if ! grep -q "Co-Authored-By: Nika" "$1"; then
  echo "" >> "$1"
  echo "Co-Authored-By: Nika 🦋 <nika@supernovae.studio>" >> "$1"
fi
```

### Updated `nika init` wizard output

The wizard adds a new step after mode selection:

```
◆  Configure AI integration? (recommended)
│  ◻ .claude/rules/nika.md (Claude Code)
│  ◻ .cursor/rules/nika.mdc (Cursor)
│  ◻ AGENTS.md (general)
│  ◻ .vscode/extensions.json (extension recommendation)
│  ◻ .git/hooks/prepare-commit-msg (co-author)
│  (pre-checked based on detected tools)
```

In `--yes` mode, all are created by default.

### Updated init summary output

```
    📁  AI Integration
    ├── .claude/rules/nika.md
    ├── .cursor/rules/nika.mdc
    ├── .vscode/extensions.json
    ├── AGENTS.md
    └── .git/hooks/prepare-commit-msg
```

---

## 9. `nika doctor` Additions

New checks to add to the existing `doctor.rs`:

### New Check Functions

```rust
// ── Editor extension status ──────────────────────────────
fn check_editor_extension_status() -> Vec<DiagnosticCheck> {
    // For each detected editor:
    // - Is the nika-lang extension installed?
    // - Is the LSP path configured correctly?
    // - Does the configured LSP binary exist?
}

// ── AI tool rules status ─────────────────────────────────
fn check_ai_tool_rules() -> Vec<DiagnosticCheck> {
    // For each detected AI tool:
    // - Claude Code: ~/.claude/skills/nika-workflow/ exists?
    // - Claude Code: MCP server "nika" in settings.json?
    // - Cursor: ~/.cursor/rules/nika.mdc exists?
}

// ── LSP health ───────────────────────────────────────────
fn check_lsp_health() -> DiagnosticCheck {
    // - Is nika compiled with lsp feature?
    // - Can `nika lsp --version` run? (quick startup check)
    // - Is LSP configured in at least one editor?
}

// ── Git hooks status ─────────────────────────────────────
fn check_git_hooks() -> Vec<DiagnosticCheck> {
    // - Global prepare-commit-msg exists?
    // - Project-level prepare-commit-msg exists?
    // - Global gitattributes has *.nika.yaml entry?
    // - Global gitignore has .nika/ entries?
}

// ── MCP connectivity (enhanced) ──────────────────────────
async fn check_mcp_connectivity_full() -> Vec<DiagnosticCheck> {
    // - For each configured MCP server in global + project config:
    //   - Can we connect?
    //   - Can we list tools?
    //   - Response time
}

// ── Shell completions status ─────────────────────────────
fn check_shell_completions() -> DiagnosticCheck {
    // - Detect shell (zsh/bash/fish)
    // - Check if completion file exists at expected path
    // - For zsh: check if fpath includes ~/.zfunc
}
```

### Updated Doctor Output

```
$ nika doctor --full

┌─ Nika Doctor ──────────────────────────────────┐
│ v0.39.1 | Checking system health...            │
└────────────────────────────────────────────────┘

  ✓ Project .nika directory found at /Users/dev/my-project/.nika
  ✓ Config config.toml is valid TOML
  ✓ API Key Claude configured (ANTHROPIC_API_KEY, 108 chars)
  ✓ API Key OpenAI configured (OPENAI_API_KEY, 56 chars)
  ✓ Traces 42 trace files
  ✓ Version nika 0.39.1
  ✓ Rust rustc 1.87.0
  ✓ Workflows 12 workflow files found
  ✓ npx npx 10.9.2 available
  ✓ MCP neo4j connected (tools: 14, 230ms)
  ✓ LSP Language server compiled in (nika lsp)
  ✓ Editor VS Code 1.96.2 detected
  ✓ Extension nika-lang extension installed
  ✓ Editor LSP LSP configured and binary exists                    # NEW
  ✓ AI Rules Claude Code skills installed                          # NEW
  ✓ AI Rules Claude Code MCP server configured                    # NEW
  ⚠ AI Rules Cursor rules not found (.cursor/rules/nika.mdc)      # NEW
    → Run: nika setup ai
  ✓ Git Hooks prepare-commit-msg configured                        # NEW
  ✓ Git Attrs *.nika.yaml linguist entry present                   # NEW
  ✓ Shell zsh completions installed                                # NEW

──────────────────────────────────────────────────
⚠ Mostly healthy — 18 passed, 1 warnings, 0 failed
```

---

## 10. Idempotency Rules

Every action in `nika setup` MUST be safe to run multiple times:

| Action | Idempotency strategy |
|--------|---------------------|
| Extension install | `--list-extensions` check first; `--install-extension` is already idempotent |
| LSP config in editor settings | Read JSON, check if key exists, merge only if absent |
| Neovim lua file | Check if file exists with same content → skip |
| AI tool skills | Check if directory exists with `skill.md` → skip or overwrite with version check |
| MCP server in settings | Parse JSON, check if `"nika"` key exists → skip |
| Shell completions | Compare file content → skip if identical |
| Git hooks | Check if file exists → skip (never overwrite user hooks) |
| Gitattributes | Check if line already present → skip |
| Gitignore | Check if line already present → skip |

Output when skipping:
```
  ✓ VS Code: nika-lang extension already installed (skipped)
```

---

## 11. Error Handling

| Error scenario | Behavior |
|---------------|----------|
| Editor binary found but `--install-extension` fails | Show warning, continue to next editor |
| Permission denied writing config file | Show error with `sudo` suggestion, continue |
| `~/.zshrc` not found (zsh completions) | Create `~/.zshrc` with just the fpath line |
| JSON parse error in editor settings | Show warning, skip that editor, continue |
| No editors detected | Show info message, skip editor step entirely |
| No AI tools detected | Show info message, skip AI step entirely |
| User cancels wizard (Ctrl+C) | `cliclack` handles this → clean exit with "Setup cancelled." |
| `--check` finds issues | Exit code 1 (like `nika doctor`) |
| Non-interactive subcommand finds nothing | Exit code 0, show "nothing to do" |

---

## 12. File Manifest

### Files `nika setup` may CREATE (machine-level, `~/.*/`)

```
~/.zfunc/_nika                                              # zsh completions
~/.local/share/bash-completion/completions/nika              # bash completions
~/.config/fish/completions/nika.fish                         # fish completions
~/.config/nvim/lua/plugins/nika.lua                          # Neovim LSP config
~/.config/zed/settings.json                                  # Zed LSP config (merge)
~/.config/git/attributes                                     # Global gitattributes (append)
~/.config/git/ignore                                         # Global gitignore (append)
~/.config/git/hooks/prepare-commit-msg                       # Global git hook
~/Library/Application Support/Sublime Text/.../LSP-nika...   # Sublime LSP
~/.claude/settings.json                                      # Claude Code MCP (merge)
~/.claude/skills/nika-workflow/skill.md                      # Claude Code skill
~/.cursor/rules/nika.mdc                                     # Cursor global rules
```

### Files `nika init` may CREATE (project-level, `./*`)

```
.vscode/extensions.json                                      # Extension recommendations
.claude/rules/nika.md                                        # Claude Code project rules
.cursor/rules/nika.mdc                                       # Cursor project rules
.github/copilot-instructions.md                              # Copilot instructions
.windsurf/rules/nika.md                                      # Windsurf rules
.roo/rules/nika.md                                           # Roo Code rules
AGENTS.md                                                    # General agent instructions
.git/hooks/prepare-commit-msg                                # Git co-author hook
```

---

## 13. CLI Structure (clap)

```rust
/// Configure your machine for Nika development
#[command(visible_alias = "s")]
Setup {
    #[command(subcommand)]
    action: Option<SetupAction>,

    /// Configure everything without prompts
    #[arg(long, conflicts_with = "check")]
    all: bool,

    /// Show current setup status without changing anything
    #[arg(long, conflicts_with = "all")]
    check: bool,
},

#[derive(Subcommand)]
enum SetupAction {
    /// Configure editor extensions and LSP
    Editors,
    /// Configure AI coding tool integration
    Ai,
    /// Install shell completions
    Completions,
    /// Configure git hooks and attributes
    Git,
}
```

Dispatch logic:
```rust
Some(Commands::Setup { action, all, check }) => {
    if check {
        cli::setup::handle_setup_check().await
    } else if all {
        cli::setup::handle_setup_all().await
    } else if let Some(action) = action {
        match action {
            SetupAction::Editors => cli::setup::handle_setup_editors().await,
            SetupAction::Ai => cli::setup::handle_setup_ai().await,
            SetupAction::Completions => cli::setup::handle_setup_completions(),
            SetupAction::Git => cli::setup::handle_setup_git(),
        }
    } else {
        // Interactive wizard
        cli::setup::handle_setup_wizard().await
    }
}
```

---

## 14. Implementation Plan

### Phase 1: Core Detection (1 session)
- `setup.rs` in `nika-cli` crate
- System scan (OS, nika, rust, node)
- Editor detection (all 7 editors)
- AI tool detection (all 6 tools)
- Shell detection
- Git config detection
- `nika setup --check` working

### Phase 2: Editor Configuration (1 session)
- Extension install for VS Code / Cursor / Windsurf
- LSP config for all editors
- `nika setup editors` working

### Phase 3: AI + Shell + Git (1 session)
- AI tool rules/skills/MCP for all tools
- Shell completions with auto-setup
- Git hooks + attributes + ignore
- `nika setup ai`, `nika setup completions`, `nika setup git` working

### Phase 4: Interactive Wizard (1 session)
- Full cliclack wizard flow
- `nika setup` (interactive) working
- `nika setup --all` working

### Phase 5: `nika init` additions (1 session)
- Add AI integration files to `init_project()`
- Add wizard step for AI file selection
- Update `nika doctor` with new checks

### Estimated total: 5 sessions

---

## 15. Dependency on Existing Crates

- **cliclack 0.5** — already in `nika-cli/Cargo.toml`
- **console 0.16** — already in `nika-cli/Cargo.toml`
- **clap + clap_complete** — already in workspace
- **serde_json** — already in workspace (for JSON config merging)
- **dirs** — may need to add for `home_dir()`, `config_dir()` (check if already available via nika-engine)
- No new external dependencies expected

---

## 16. Naming Rationale

| Name | Reason |
|------|--------|
| `setup` not `install` | "install" implies downloading Nika itself |
| `setup` not `configure` | Too long, and `config` subcommand already exists |
| `setup editors` not `setup editor` | Plural — configures ALL detected editors |
| `setup ai` not `setup ai-tools` | Short, memorable, matches the category |
| `setup git` not `setup hooks` | Covers more than hooks (attributes, ignore) |
| `setup completions` not `setup shell` | More specific about what it does |
| `--check` not `--dry-run` | Matches `nika doctor` pattern; it's a status check, not a dry run of writes |
| `--all` not `--auto` | Clearer — means "do everything" |
