# Nika AI Integration Suite — Master Plan

> **Codename**: Project Butterfly Wings
> **Status**: Design Complete — Ready for Implementation
> **Author**: Thibaut + Claude (30+ research agents, 4 waves)
> **Date**: 2026-03-23
> **Scope**: `nika setup` + `nika init` AI files + Agent Skills + Claude Code Plugin + per-tool rules + git hooks + doctor upgrade + MCP server + llms.txt + AGENTS.md

---

## Executive Summary

When someone installs Nika, every AI coding tool on their machine should instantly understand how to write `.nika.yaml` workflows. This plan delivers that vision through 4 tiers of integration across 10+ AI tools, 8+ editors, and 3 operating systems.

### The 4-Tier Strategy

| Tier | Scope | Reaches | Effort |
|------|-------|---------|--------|
| **1. AGENTS.md + Universal Agent Skills** | 43+ AI agents via `.agents/skills/` | Claude, Cursor, Copilot, Codex, Gemini, Windsurf, Roo, 36 more | Medium |
| **2. Claude Code Plugin** | Full power: skills + agents + hooks + MCP + LSP | Claude Code users | Medium |
| **3. Native Rules per Tool** | Optimal format per tool (.mdc, .instructions.md, etc.) | Each tool specifically | Low |
| **4. MCP Server + llms.txt** | Runtime integration + web discovery | All MCP-capable tools + web | High |

### Key Decisions (from Brainstorm)

| # | Decision | Choice |
|---|----------|--------|
| Q1 | Trigger | `nika setup` standalone + proposed in `nika init` wizard |
| Q2 | Scope | `nika setup` = machine-level, `nika init` = project-level |
| Q3 | AI files | Native format per tool, short (~30 lines), only for detected tools |
| Q4 | Git co-author | prepare-commit-msg hook + AI rules |
| Q5 | Command name | `nika setup` with subcommands (editors, ai, git, completions) |

---

## Phase 0 — Foundation (Pre-requisites)

### 0.1 AGENTS.md Symlink Migration

**Why**: AGENTS.md is the universal standard (60k+ repos, 20+ tools, Linux Foundation). Next.js already does this.

**What**:
```bash
# At nika/ repo root
mv CLAUDE.md AGENTS.md
ln -s AGENTS.md CLAUDE.md

# At tools/nika/
mv CLAUDE.md AGENTS.md
ln -s AGENTS.md CLAUDE.md

# At supernovae-agi/ root
mv CLAUDE.md AGENTS.md
ln -s AGENTS.md CLAUDE.md
```

**Files**: 3 renames + 3 symlinks
**Risk**: Zero — Claude Code reads CLAUDE.md (symlink), all other tools read AGENTS.md
**Test**: `cargo test --workspace --lib` still passes, Claude Code still loads instructions

### 0.2 VS Code Extension Marketplace Publishing

**Why**: `code --install-extension supernovae.nika-lang` requires marketplace listing
**What**: Publish the existing `editors/vscode/` extension to:
- VS Code Marketplace (primary)
- Open VSX Registry (for Cursor, Windsurf, VSCodium)

**Pre-requisites**:
- Azure DevOps PAT for `supernovae` publisher
- `npm run package` produces valid VSIX
- Version bump to match `nika --version`

**Test**: `code --install-extension supernovae.nika-lang` succeeds

### 0.3 Cargo Workspace Crate: `nika-setup`

**Why**: Isolate setup logic from existing crates, keep `nika-cli` focused
**What**: New crate `tools/nika-setup/` with:
- Editor detection (`detect.rs`)
- Extension installation (`editors.rs`)
- AI rules generation (`ai_rules.rs`)
- Git hooks management (`git.rs`)
- Shell completions (`completions.rs`)
- Doctor checks (`doctor_checks.rs`)

**Structure**:
```
tools/nika-setup/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── detect.rs         # Editor & AI tool detection
    ├── editors.rs        # Extension install per editor
    ├── ai_rules.rs       # Generate rules per tool
    ├── git.rs            # Git hooks & co-author
    ├── completions.rs    # Shell completion install
    ├── doctor_checks.rs  # New doctor checks
    ├── content/          # Embedded rule content
    │   ├── agents_md.rs
    │   ├── cursor_rule.rs
    │   ├── copilot_instructions.rs
    │   ├── windsurf_rule.rs
    │   ├── roo_rule.rs
    │   ├── claude_rule.rs
    │   └── universal_context.rs
    └── templates/        # Template files
        ├── prepare_commit_msg.sh
        ├── roomodes.json
        └── vscode_extensions.json
```

---

## Phase 1 — `nika setup` Command (Machine-Level)

### 1.1 Editor Detection Module

**File**: `nika-setup/src/detect.rs`

Detect installed editors across platforms:

| Editor | macOS Detection | Linux | Windows |
|--------|----------------|-------|---------|
| VS Code | `mdfind` bundle ID `com.microsoft.VSCode` + `/Applications/` + `which code` | `which code` + `.desktop` | `where code` + registry |
| Cursor | bundle `com.todesktop.230313mzl4w4u92` + `which cursor` | `which cursor` | `where cursor` |
| Windsurf | bundle `com.codeium.windsurf` + `which windsurf` | `which windsurf` | `where windsurf` |
| Zed | bundle `dev.zed.Zed` + `which zed` | `which zed` | N/A |
| Neovim | `which nvim` | `which nvim` | `where nvim` |
| Sublime | bundle `com.sublimetext.4` + `which subl` | `which subl` | `where subl` |
| JetBrains | `mdfind` per IDE + Toolbox scripts | Toolbox `~/.local/share/JetBrains/` | Toolbox |

**Output**: `Vec<DetectedEditor>` with binary path, version, extension status

Detect installed AI coding tools:

| Tool | Detection Method |
|------|-----------------|
| Claude Code | `which claude` + `~/.claude/` exists |
| Cursor AI | Cursor detected + `.cursor/` convention |
| Copilot | VS Code + `code --list-extensions \| grep github.copilot` |
| Roo Code | VS Code + `code --list-extensions \| grep rooveterinaryinc` |
| Windsurf AI | Windsurf detected |
| Cline | VS Code + `code --list-extensions \| grep saoudrizwan.claude-dev` |
| Continue | `~/.continue/` exists |
| Aider | `which aider` |
| Amazon Q | `~/.aws/amazonq/` exists |

### 1.2 Editor Extension Install

**File**: `nika-setup/src/editors.rs`

For each detected editor:

| Editor | Install Method | Config |
|--------|---------------|--------|
| VS Code | `code --install-extension supernovae.nika-lang` | — |
| Cursor | `cursor --install-extension supernovae.nika-lang` | — |
| Windsurf | `windsurf --install-extension supernovae.nika-lang` | — |
| Zed | Merge `auto_install_extensions.nika` into `~/.config/zed/settings.json` | JSON merge |
| Neovim | Print config snippet for `lsp/nika.lua` | Manual |
| Sublime | Merge into `Package Control.sublime-settings` + LSP settings | JSON merge |
| JetBrains | Print instructions (no reliable CLI install) | Manual |

### 1.3 AI Tool Global Configuration

**File**: `nika-setup/src/ai_rules.rs`

For each detected AI tool, install **global** Agent Skills:

```bash
# Universal (works for 43 agents)
~/.agents/skills/nika-syntax/SKILL.md
~/.agents/skills/nika-create/SKILL.md
~/.agents/skills/nika-validate/SKILL.md
~/.agents/skills/nika-run/SKILL.md
```

For Claude Code specifically, also offer:
```bash
# Claude Code plugin install
claude plugin install nika@supernovae-st
# Or: claude --plugin-dir ~/.claude/plugins/nika
```

MCP server registration per tool:
```bash
# Claude Code
claude mcp add nika -- nika mcp serve

# Cursor: merge into ~/.cursor/mcp.json
# Windsurf: merge into ~/.codeium/windsurf/mcp_config.json
# Roo Code: merge into global MCP settings
```

### 1.4 Shell Completions

**File**: `nika-setup/src/completions.rs`

Auto-detect shell and install:

| Shell | Target Path | RC Update |
|-------|------------|-----------|
| zsh | `$(brew --prefix)/share/zsh/site-functions/_nika` or `~/.zfunc/_nika` | Add `fpath` if needed |
| bash | `~/.local/share/bash-completion/completions/nika` | — |
| fish | `~/.config/fish/completions/nika.fish` | — |

### 1.5 Interactive Wizard UX

Using `cliclack` (same as `nika init`):

```
┌  Nika Setup v0.40.0
│
◇  System scan complete
│  macOS 24.6.0 | nika 0.40.0 | rustc 1.86.0 | node 22.0.0
│
◆  Detected editors:
│  ✓ VS Code (1.106.3) — nika-lang extension installed
│  ✓ Cursor (0.49.0) — nika-lang extension missing
│  ○ Zed (0.185.0) — no Nika extension
│
◆  Install Nika extension in Cursor?
│  ● Yes  ○ No
│
◇  cursor --install-extension supernovae.nika-lang ✓
│
◆  Detected AI tools:
│  ✓ Claude Code — plugin not installed
│  ✓ Copilot — no instructions file
│  ✓ Roo Code — no rules
│
◆  Install Agent Skills globally? (~/.agents/skills/nika-*)
│  ● Yes  ○ No
│
◇  4 skills installed to ~/.agents/skills/
│
◆  Install shell completions for zsh?
│  ● Yes  ○ No
│
◇  Completions installed to /opt/homebrew/share/zsh/site-functions/_nika
│
└  Setup complete! Run `nika doctor` to verify.
```

### 1.6 CLI Subcommands

```rust
#[derive(Subcommand)]
pub enum SetupAction {
    /// Interactive setup wizard (default)
    Run,
    /// Install editor extensions only
    Editors,
    /// Install AI rules and skills only
    Ai,
    /// Install git hooks only
    Git,
    /// Install shell completions only
    Completions,
}
```

Plus flags: `--all` (non-interactive, everything), `--check` (dry-run), `--verbose`

### Tests Phase 1

| Test | Method | Pass Criteria |
|------|--------|---------------|
| Editor detection macOS | Unit test with mock `which` | Detects VS Code, Cursor |
| Editor detection Linux | Unit test | Detects `code`, `nvim` |
| Extension install | Integration test (CI) | `code --list-extensions` shows nika-lang |
| Skills install | Unit test | Files exist at `~/.agents/skills/nika-*/SKILL.md` |
| Completions install | Unit test | File written, parseable |
| Wizard flow | Snapshot test (insta) | Expected output matches |
| Idempotency | Run twice | No errors, no duplicates |

---

## Phase 2 — `nika init` AI Files (Project-Level)

### 2.1 New Wizard Step

Add to the existing `nika init` wizard (after provider selection):

```
◆  Generate AI coding assistant rules?
│  ● Yes (recommended)  ○ No
│
◆  Select tools to configure:
│  ◻ Claude Code (.claude/rules/nika.md)
│  ◻ Cursor (.cursor/rules/nika/RULE.md)
│  ◻ Copilot (.github/copilot/nika.instructions.md)
│  ◻ Windsurf (.windsurf/rules/nika.md)
│  ◻ Roo Code (.roo/rules/nika.md + .roomodes)
│  ◻ AGENTS.md (universal)
│  ◻ Git co-author hook
│  ◻ VS Code extension recommendations
│
◇  8 files generated ✓
```

### 2.2 Generated Files

**Always generated** (universal):

| File | Purpose | Lines |
|------|---------|-------|
| `AGENTS.md` | Universal agent instructions | ~150 |
| `CLAUDE.md` → symlink to `AGENTS.md` | Claude Code compat | — |
| `.vscode/extensions.json` | Extension recommendations | 5 |

**Generated per detected tool**:

| File | Tool | Format | Lines |
|------|------|--------|-------|
| `.claude/rules/nika.md` | Claude Code | MD + `paths:` frontmatter | ~200 |
| `.cursor/rules/nika/RULE.md` | Cursor | MD + `description/globs/alwaysApply` | ~150 |
| `.github/copilot/nika.instructions.md` | Copilot | MD + `applyTo` | ~80 |
| `.github/workflows/copilot-setup-steps.yml` | Copilot Coding Agent | YAML | ~25 |
| `.windsurf/rules/nika.md` | Windsurf | MD + `trigger/globs` | ~150 |
| `.roo/rules/nika.md` | Roo Code | MD + `description/globs` | ~150 |
| `.roomodes` | Roo Code | JSON custom mode | ~50 |
| `.roo/rules-nika/01-workflow-syntax.md` | Roo nika mode | MD | ~100 |
| `.amazonq/rules/nika.md` | Amazon Q | MD | ~100 |
| `CONVENTIONS.md` section | Aider | MD | ~80 |

**Git integration** (if .git exists):

| File | Purpose |
|------|---------|
| `.git/hooks/prepare-commit-msg` | Auto-add co-author lines |
| `.gitattributes` | `.nika.yaml` diff driver |

### 2.3 Content Architecture

All rule files share ONE source of truth — a `NikaAiContext` struct in Rust that generates tool-specific output:

```rust
pub struct NikaAiContext {
    pub verbs: Vec<VerbDoc>,        // 5 verbs with all fields
    pub transforms: Vec<Transform>, // 25 pipe transforms
    pub providers: Vec<Provider>,   // 19 providers
    pub error_codes: Vec<ErrorRange>,
    pub examples: Vec<WorkflowExample>,
    pub common_mistakes: Vec<Mistake>,
}

impl NikaAiContext {
    pub fn to_claude_rule(&self) -> String { /* detailed, path-scoped */ }
    pub fn to_cursor_rule(&self) -> String { /* MDC format, glob-scoped */ }
    pub fn to_copilot_instructions(&self) -> String { /* applyTo, concise */ }
    pub fn to_windsurf_rule(&self) -> String { /* trigger: glob */ }
    pub fn to_roo_rule(&self) -> String { /* globs + mode */ }
    pub fn to_agents_md(&self) -> String { /* universal, no frontmatter */ }
    pub fn to_conventions_md(&self) -> String { /* Aider section */ }
    pub fn to_universal_context(&self) -> String { /* standalone reference */ }
}
```

**This ensures all tools get the same correct information, automatically updated with each Nika release.**

### Tests Phase 2

| Test | Method | Pass Criteria |
|------|--------|---------------|
| File generation | Unit test per tool | Output matches snapshot |
| Content accuracy | Cross-check with AST structs | All verb fields present |
| Frontmatter validity | Parse test per format | Valid YAML/MDC |
| Init integration | E2E `nika init --minimal` | All files created |
| Idempotency | Run init twice | No duplicates, clean merge |
| .roomodes validity | JSON parse | Valid Roo mode config |
| Git hook | E2E commit test | Co-author lines added |

---

## Phase 3 — Agent Skills Package (Universal, 43 Agents)

### 3.1 Repository: `github.com/supernovae-st/nika-skills`

```
nika-skills/
├── .claude-plugin/
│   ├── plugin.json           # Claude Code plugin manifest
│   └── marketplace.json      # Plugin marketplace
├── skills/
│   ├── nika-syntax/          # Tier 1: Background knowledge
│   │   ├── SKILL.md
│   │   └── references/
│   │       ├── verbs.md
│   │       ├── transforms.md
│   │       └── providers.md
│   ├── nika-create/          # Tier 1: Create workflows
│   │   ├── SKILL.md
│   │   ├── templates/
│   │   │   ├── basic.nika.yaml
│   │   │   ├── pipeline.nika.yaml
│   │   │   └── agent.nika.yaml
│   │   └── scripts/
│   │       └── validate.sh
│   ├── nika-validate/        # Tier 1: Validate & fix
│   │   └── SKILL.md
│   ├── nika-run/             # Tier 1: Execute & explain
│   │   └── SKILL.md
│   ├── nika-fetch/           # Tier 2: Fetch specialist
│   │   └── SKILL.md
│   ├── nika-agent/           # Tier 2: Agent verb specialist
│   │   └── SKILL.md
│   ├── nika-invoke/          # Tier 2: MCP/invoke specialist
│   │   └── SKILL.md
│   ├── nika-infer/           # Tier 2: Infer specialist
│   │   └── SKILL.md
│   ├── nika-structured/      # Tier 3: Structured output
│   │   └── SKILL.md
│   ├── nika-vision/          # Tier 3: Multimodal
│   │   └── SKILL.md
│   ├── nika-media/           # Tier 3: Media pipeline
│   │   └── SKILL.md
│   ├── nika-dag/             # Tier 3: DAG optimization
│   │   └── SKILL.md
│   ├── nika-convert/         # Tier 3: Format conversion
│   │   └── SKILL.md
│   ├── nika-debug/           # Tier 3: Debug specialist
│   │   └── SKILL.md
│   └── nika-course/          # Tier 3: Course tutor
│       └── SKILL.md
├── agents/                   # Claude Code only
│   ├── workflow-architect.md
│   ├── workflow-debugger.md
│   └── nika-assistant.md
├── hooks/
│   └── hooks.json
├── scripts/
│   ├── validate-nika.sh
│   └── detect-workspace.sh
├── .mcp.json
├── .lsp.json
├── AGENTS.md
├── README.md
├── LICENSE                   # AGPL-3.0-or-later
└── CHANGELOG.md
```

### 3.2 Installation Methods

```bash
# Method 1: npx skills (Vercel CLI — 43 agents)
npx skills add supernovae-st/nika-skills

# Method 2: Claude Code plugin
claude plugin install nika@supernovae-nika-skills

# Method 3: Manual (any agent)
git clone https://github.com/supernovae-st/nika-skills
cp -r nika-skills/skills/nika-* .agents/skills/

# Method 4: nika setup (auto-detected)
nika setup ai
```

### 3.3 Skill Tiers

| Tier | Skills | Auto-load | Purpose |
|------|--------|-----------|---------|
| **1 — Core** | syntax, create, validate, run | Always (via description matching) | Every Nika user needs these |
| **2 — Verb Specialists** | fetch, agent, invoke, infer | When using specific verbs | Deep expertise per verb |
| **3 — Advanced** | structured, vision, media, dag, convert, debug, course | On demand | Specialized tasks |

### Tests Phase 3

| Test | Method | Pass Criteria |
|------|--------|---------------|
| SKILL.md frontmatter | `npx skills check` | All skills pass validation |
| Skill body length | Automated check | Each < 500 lines, < 5000 tokens |
| Description trigger | Manual test with each AI tool | Skills activate on ".nika.yaml" keywords |
| Cross-agent install | `npx skills add --all` | Installs to .claude/, .cursor/, .agents/ |
| Plugin validation | `claude plugin validate` | No errors |
| Hook execution | Mock PreToolUse with .nika.yaml Write | Blocks invalid YAML |
| Template workflows | `nika check` on each template | All valid |

---

## Phase 4 — MCP Server (`nika mcp serve`)

### 4.1 MCP Tool Surface

Using `rmcp` (already in dependency tree via `nika-mcp`):

| MCP Tool | Description | Input | Output |
|----------|-------------|-------|--------|
| `nika:check` | Validate a .nika.yaml workflow | `{ path: string }` | `{ valid: bool, errors: Error[] }` |
| `nika:run` | Execute a workflow | `{ path: string, dry_run?: bool }` | `{ status, output, trace_id }` |
| `nika:list_workflows` | List all .nika.yaml files | `{ dir?: string }` | `{ workflows: Workflow[] }` |
| `nika:scaffold` | Generate workflow from description | `{ description: string, verbs?: string[] }` | `{ yaml: string }` |
| `nika:explain` | Explain what a workflow does | `{ path: string }` | `{ explanation: string, dag: string }` |
| `nika:schema` | Return the workflow JSON schema | `{ version?: string }` | `{ schema: object }` |
| `nika:providers` | List configured providers | `{}` | `{ providers: Provider[] }` |
| `nika:error_lookup` | Explain a NIKA-XXX error | `{ code: string }` | `{ description, fix }` |

### 4.2 MCP Resources

| Resource URI | Description |
|--------------|-------------|
| `nika://schema/workflow` | Full workflow JSON schema |
| `nika://reference/verbs` | All 5 verbs documentation |
| `nika://reference/transforms` | 25 pipe transforms |
| `nika://reference/providers` | Provider catalog |
| `nika://reference/errors` | Error code ranges |

### 4.3 Implementation

**File**: `tools/nika-mcp/src/server.rs` (new, alongside existing client code)

```rust
#[tool_router]
impl NikaMcpServer {
    #[tool(description = "Validate a Nika .nika.yaml workflow file")]
    async fn check(&self, params: CheckParams) -> Result<CallToolResult, McpError> {
        let result = nika_engine::check_workflow(&params.path)?;
        Ok(CallToolResult::text(serde_json::to_string(&result)?))
    }

    #[tool(description = "Execute a Nika workflow")]
    async fn run(&self, params: RunParams) -> Result<CallToolResult, McpError> {
        let result = nika_engine::run_workflow(&params.path, params.dry_run).await?;
        Ok(CallToolResult::text(serde_json::to_string(&result)?))
    }
    // ... 6 more tools
}
```

**CLI Entry**: `nika mcp serve [--stdio|--http <port>]`

### 4.4 Auto-Configuration by `nika setup`

For each detected AI tool, register the MCP server:

| Tool | Config File | Format |
|------|------------|--------|
| Claude Code | `~/.claude.json` → `mcpServers.nika` | `{ command: "nika", args: ["mcp", "serve"] }` |
| VS Code/Copilot | `.vscode/mcp.json` | `{ servers: { nika: { command: "nika", args: ["mcp", "serve"] } } }` |
| Cursor | `.cursor/mcp.json` | `{ mcpServers: { nika: { ... } } }` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | `{ mcpServers: { nika: { ... } } }` |
| Roo Code | `.roo/mcp.json` | `{ mcpServers: { nika: { ... } } }` |

### Tests Phase 4

| Test | Method | Pass Criteria |
|------|--------|---------------|
| MCP server starts | `nika mcp serve --stdio` + JSON-RPC init | Responds with capabilities |
| check tool | Send check request with valid workflow | `{ valid: true }` |
| check tool (invalid) | Send check with bad YAML | `{ valid: false, errors: [...] }` |
| run tool (dry_run) | Execute with dry_run: true | Returns plan without execution |
| scaffold tool | Send description | Valid .nika.yaml output |
| schema resource | Read nika://schema/workflow | Valid JSON schema |
| E2E: Claude Code | `claude mcp add nika -- nika mcp serve` | Tools appear in Claude |
| E2E: Cursor | Configure .cursor/mcp.json | Tools appear in Cursor Agent |
| E2E: Copilot | Configure .vscode/mcp.json | Tools appear in Copilot Agent |
| Load test | 10 concurrent requests | No crashes, < 100ms per check |
| Open provider E2E | Run scaffold with `provider: groq` | Generates valid workflow |

---

## Phase 5 — `nika doctor` Upgrade

### 5.1 New Diagnostic Checks

Add to existing `doctor.rs` (currently 11 checks):

| Check | Category | Status |
|-------|----------|--------|
| `check_editor_extensions()` | Editors | Pass: extension installed. Warn: editor found, no extension |
| `check_ai_rules()` | AI Integration | Pass: rules present. Warn: tool detected, no rules |
| `check_agent_skills()` | AI Integration | Pass: skills in ~/.agents/ or .agents/. Warn: missing |
| `check_lsp_health()` | Editor | Pass: LSP responds to init. Warn: binary found, no response |
| `check_git_hooks()` | Git | Pass: hook installed. Warn: .git exists, no hook |
| `check_mcp_connectivity()` | MCP | UPGRADE placeholder → real connection test |
| `check_completions()` | Shell | Pass: completion file exists for current shell |
| `check_agents_md()` | Universal | Pass: AGENTS.md exists. Warn: only CLAUDE.md |
| `check_copilot_setup_steps()` | CI | Pass: copilot-setup-steps.yml exists |
| `check_mcp_server()` | MCP | Pass: `nika mcp serve` starts. Warn: MCP tools configured but server unavailable |

### 5.2 Doctor Output Upgrade

```
┌─ Nika Doctor ──────────────────────────────────────────┐
│ v0.40.0 | Checking system health...                    │
└────────────────────────────────────────────────────────┘

  Core
  ✓ Project .nika directory found
  ✓ Config file valid
  ✓ 12 workflow files found
  ✓ Nika v0.40.0 | Rust 1.86.0

  Providers
  ✓ Claude (ANTHROPIC_API_KEY set)
  ✓ OpenAI (OPENAI_API_KEY set)
  ⚠ Groq (GROQ_API_KEY not set)
    → Set GROQ_API_KEY or run: nika keys set groq

  Editors
  ✓ VS Code — nika-lang 0.40.0 installed
  ✓ Cursor — nika-lang 0.40.0 installed
  ⚠ Zed — Nika extension not configured
    → Run: nika setup editors

  AI Integration
  ✓ Claude Code — .claude/rules/nika.md present
  ✓ Cursor — .cursor/rules/nika/RULE.md present
  ⚠ Copilot — .github/copilot/nika.instructions.md missing
    → Run: nika init (select Copilot)
  ✓ Agent Skills — 4 skills in .agents/skills/nika-*

  MCP
  ✓ MCP server responds (nika mcp serve)
  ✓ 8 tools available

  Git
  ✓ Co-author hook installed
  ✓ AGENTS.md present (CLAUDE.md symlinked)
  ⚠ copilot-setup-steps.yml missing
    → Run: nika init (select Copilot)

  Shell
  ✓ Zsh completions installed

──────────────────────────────────────────────────────────
✓ 16 passed, 3 warnings, 0 failed
```

### Tests Phase 5

| Test | Method | Pass Criteria |
|------|--------|---------------|
| New checks unit tests | Mock filesystem | Each check returns correct status |
| Doctor output snapshot | `insta` snapshot | Matches expected format |
| Doctor JSON output | `--format json` | Valid JSON with all new checks |
| Doctor with no setup | Clean environment | All new checks are Warn (not Fail) |
| Doctor after setup | Post `nika setup --all` | All checks Pass |

---

## Phase 6 — llms.txt + Web Integration

### 6.1 llms.txt for Nika Docs Site

```markdown
# Nika

> Semantic YAML workflow engine for AI tasks. Schema nika/workflow@0.12.

## Quick Reference

- [Full Documentation](llms-full.txt): Complete Nika reference
- [Workflow Syntax](llms-syntax.txt): 5 verbs, fields, bindings, transforms
- [Examples](llms-examples.txt): 30+ workflow patterns
- [Error Codes](llms-errors.txt): NIKA-XXX error reference
- [API Reference](llms-api.txt): CLI commands and MCP tools

## Optional

- [Media Tools](llms-media.txt): 26 builtin nika:* tools
- [Course Guide](llms-course.txt): 12-level learning course
```

### 6.2 Generate from Source

Build script that extracts content from Rust source and generates llms-*.txt files:

```bash
nika docs generate-llms-txt --output docs/llms/
```

### Tests Phase 6

| Test | Method | Pass Criteria |
|------|--------|---------------|
| llms.txt format | Parse as Markdown | Valid structure per spec |
| Content freshness | Compare with AST structs | All verbs/fields present |
| Nika fetch test | `fetch: { url: "https://nika.dev/llms.txt", extract: llm_txt }` | Returns valid content |

---

## Phase 7 — E2E Testing & Validation

### 7.1 Cross-Tool E2E Test Matrix

| Scenario | Tool | Test |
|----------|------|------|
| Create workflow from description | Claude Code | `/nika:nika-create "fetch HN and summarize"` → valid .nika.yaml |
| Create workflow from description | Cursor | Edit .nika.yaml, Cursor suggests valid syntax |
| Create workflow from description | Copilot | Copilot Agent generates valid .nika.yaml |
| Validate on save | Claude Code | Write invalid .nika.yaml → hook blocks, feedback shown |
| Debug failing workflow | Claude Code | `/nika:nika-debug workflow.nika.yaml` → identifies error |
| MCP tool call | Claude Code | Ask "check my workflow" → uses nika:check MCP tool |
| MCP tool call | Cursor | Agent uses nika:check in conversation |
| MCP tool call | Copilot | Agent mode calls nika:check |
| Course tutoring | Claude Code | `/nika:nika-course next` → explains exercise without spoiling |
| Roo Code nika mode | Roo Code | Switch to nika mode → only edits .nika.yaml files |

### 7.2 Open Provider E2E Tests

Test with FREE/open providers to validate skills work without API keys:

| Provider | Model | Test |
|----------|-------|------|
| Groq | `llama-3.1-8b-instant` | `nika run` with infer: task |
| Mistral | `mistral-small-latest` | `nika run` with infer: + structured |
| Gemini | `gemini-2.0-flash` | `nika run` with vision content |
| Native | Local GGUF | `nika run` with native provider |

### 7.3 MCP E2E Tests

```bash
# Test 1: MCP server lifecycle
nika mcp serve --stdio &
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | nc localhost 3000
# Expect: capabilities with 8 tools

# Test 2: Validate via MCP
echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"nika:check","arguments":{"path":"examples/minimal.nika.yaml"}}}' | nc localhost 3000
# Expect: { valid: true }

# Test 3: Cross-tool MCP
claude mcp add nika-test -- nika mcp serve
# In Claude Code: "check my workflow" → uses nika:check
```

### 7.4 Code Review Checkpoints

| After Phase | Review Focus | Reviewer |
|-------------|-------------|----------|
| Phase 1 | Editor detection accuracy, platform compat | `spn-powers:code-reviewer` |
| Phase 2 | Content accuracy vs AST source of truth | `spn-powers:code-reviewer` |
| Phase 3 | Skill quality, description trigger rate | Manual + AI eval |
| Phase 4 | MCP server safety, error handling | `spn-rust:rust-security` |
| Phase 5 | Doctor UX, check completeness | `spn-powers:code-reviewer` |
| Phase 7 | Full E2E pass rate | Manual cross-tool testing |

---

## Implementation Timeline

### Session 1 — Foundation (Phase 0 + Phase 1 core)
- AGENTS.md symlink migration
- `nika-setup` crate scaffolding
- Editor detection module
- Extension install module
- Unit tests for detection

### Session 2 — Setup Command + Init AI Files (Phase 1 + Phase 2)
- `nika setup` CLI wiring (clap)
- Interactive wizard (cliclack)
- Shell completions install
- Git hooks management
- `nika init` wizard step
- Content generation (`NikaAiContext`)
- Per-tool rule file generation
- Snapshot tests

### Session 3 — Agent Skills Package (Phase 3)
- Create `nika-skills` repository
- Write all 15 SKILL.md files
- Write 3 agent definitions
- Write hooks.json + scripts
- Plugin manifest
- `npx skills check` validation
- Cross-agent install testing

### Session 4 — MCP Server (Phase 4)
- `nika mcp serve` subcommand
- 8 MCP tools implementation
- 5 MCP resources
- stdio + HTTP transport
- Auto-configuration in `nika setup`
- MCP E2E tests

### Session 5 — Doctor + llms.txt + Polish (Phase 5 + 6)
- 10 new doctor checks
- Doctor output upgrade
- llms.txt generation
- Documentation
- Final E2E testing

### Session 6 — Cross-Tool E2E + Code Review (Phase 7)
- Full E2E test matrix
- Open provider testing
- Code review with `spn-powers:code-reviewer`
- Security review with `spn-rust:rust-security`
- Performance testing
- Bug fixes from review

### Session 7 — Release + Distribution
- VS Code Marketplace publish
- `nika-skills` repo publish
- npm package for `npx skills add`
- GitHub MCP Registry submission
- llms.txt deploy to docs site
- Release notes + changelog
- Blog post / announcement

---

## File Manifest

### New Crate
```
tools/nika-setup/              # ~2000 lines estimated
```

### New Files in Repo
```
AGENTS.md                      # Renamed from CLAUDE.md
CLAUDE.md -> AGENTS.md         # Symlink
tools/nika/AGENTS.md           # Renamed
tools/nika/CLAUDE.md -> AGENTS.md
.github/workflows/copilot-setup-steps.yml
```

### New Repository
```
github.com/supernovae-st/nika-skills/
├── 15 skills, 3 agents, hooks, scripts, MCP, LSP config
└── ~3000 lines total
```

### Generated by `nika init` (project-level)
```
AGENTS.md                      # ~150 lines
CLAUDE.md -> AGENTS.md         # Symlink
.claude/rules/nika.md          # ~200 lines
.cursor/rules/nika/RULE.md     # ~150 lines
.github/copilot/nika.instructions.md  # ~80 lines
.github/workflows/copilot-setup-steps.yml  # ~25 lines
.windsurf/rules/nika.md        # ~150 lines
.roo/rules/nika.md             # ~150 lines
.roomodes                      # ~50 lines
.roo/rules-nika/*.md           # ~300 lines total
.amazonq/rules/nika.md         # ~100 lines
.vscode/extensions.json        # ~5 lines
.git/hooks/prepare-commit-msg  # ~20 lines
```

### Generated by `nika setup` (machine-level)
```
~/.agents/skills/nika-*/SKILL.md  # 4 core skills
Shell completion file             # 1 file
MCP server config per tool        # 1-3 files
```

---

## Success Metrics

| Metric | Target | How to Measure |
|--------|--------|----------------|
| AI tools that "just work" with Nika | 10+ | Manual testing matrix |
| Skill trigger rate | >80% on Nika keywords | Description optimization testing |
| `nika check` after AI-generated workflow | >90% pass rate | E2E testing |
| `nika doctor` all-green after `nika setup` | 100% | Automated test |
| Time from `cargo install nika` to first AI-generated workflow | < 5 min | Manual timing |
| Cross-platform support | macOS + Linux + Windows | CI matrix |

---

## Risk Register

| Risk | Impact | Mitigation |
|------|--------|------------|
| Claude Code adds AGENTS.md support (makes symlink unnecessary) | Low (positive) | Symlink still works, remove when native |
| Cursor changes .mdc to RULE.md format | Medium | Already using new RULE.md format |
| Agent Skills spec breaking change | Medium | Pin version, follow spec repo |
| VS Code Marketplace rejection | High | Follow all guidelines, test thoroughly |
| MCP server security (arbitrary command execution) | Critical | Sandbox, validate inputs, allowlist |
| Rule content gets stale vs Nika releases | Medium | Generate from Rust source, CI check |

---

## Research Artifacts

This plan was informed by **34 research agents** across 4 waves:

| Wave | Agents | Focus |
|------|--------|-------|
| 1 | 10 | Foundation: VS Code ext, LSP, init, CLI, AI rules formats, DX patterns |
| 2 | 10 | Deep dive: Claude plugin, skills, Cursor, schema, examples, OS integration |
| 3 | 6 | Excellence: skill best practices, hooks, Anthropic skills, auto-trigger |
| 4 | 8 | Perfection: AGENTS.md, npx skills, Roo modes, Copilot, MCP bridge, doctor, UX |

Research documents saved at:
- `docs/research/cursor-integration-research.md`
- `docs/research/2026-03-23-ai-coding-tool-integration.md`
- `docs/plans/2026-03-23-nika-setup-ux-design.md`
