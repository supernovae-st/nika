# AI Coding Tool Integration Research

> Date: 2026-03-23
> Purpose: Complete specification of rules/config formats for all major AI coding tools
> Goal: Enable Nika to ship integration files for every tool

---

## Table of Contents

1. [Windsurf (Codeium)](#1-windsurf-codeium)
2. [GitHub Copilot](#2-github-copilot)
3. [Cline / Roo Code](#3-cline--roo-code)
4. [Amazon Q Developer](#4-amazon-q-developer)
5. [Continue.dev](#5-continuedev)
6. [Zed AI](#6-zed-ai)
7. [Aider](#7-aider)
8. [Cross-Tool: AGENTS.md Standard](#8-cross-tool-agentsmd-standard)
9. [Comparison Matrix](#9-comparison-matrix)
10. [Nika Integration Strategy](#10-nika-integration-strategy)

---

## 1. Windsurf (Codeium)

### 1.1 Rules Directory

**Path**: `.windsurf/rules/` (project root)
**Legacy**: `.windsurfrules` (single file, pre-Wave 8)
**Format**: Individual `.md` files with YAML frontmatter

**Limits**: Each file max 6,000 chars. Total global + workspace rules capped at 12,000 chars (global rules prioritized first).

### 1.2 Frontmatter Fields

```yaml
---
trigger: rule-name        # Manual @mention activation (e.g., "@rule-name" in Cascade)
globs: "*.py"             # Glob pattern(s) for file-based activation
description: Brief text   # Used by Cascade to decide relevance (model decision mode)
alwaysApply: true         # Boolean: apply to every Cascade action regardless of context
---
```

**Activation modes** (exactly one should be primary):

| Mode | Field | Behavior |
|------|-------|----------|
| Always | `alwaysApply: true` | Included in every Cascade interaction |
| Glob | `globs: "pattern"` | Activates when working with matching files |
| Manual | `trigger: name` | User types `@name` in Cascade panel |
| Model Decision | `description: ...` | Cascade reads description and decides if relevant |

### 1.3 Rule File Examples

```markdown
<!-- .windsurf/rules/nika-workflows.md -->
---
globs: "*.nika.yaml"
description: Rules for Nika workflow files
alwaysApply: false
---

# Nika Workflow Rules

- All workflow files use `.nika.yaml` extension
- Schema is `nika/workflow@0.12`
- 5 verbs: infer, exec, fetch, invoke, agent
- Use `with:` for bindings, `{{with.alias}}` for templates
- `depends_on: [task_id]` for ordering
- `timeout:` values are in seconds
```

```markdown
<!-- .windsurf/rules/rust-style.md -->
---
alwaysApply: true
---

# Rust Conventions

- Use `NikaError` with NIKA-XXX codes, never `anyhow`
- Always Raw -> Analyzed -> Lower for AST phases
- Use `cargo test --lib` (never bare `cargo test`)
```

### 1.4 MCP Configuration

**Path**: `~/.codeium/windsurf/mcp_config.json`
**Alternative**: Settings UI > Cascade > MCP Servers

**Platform-specific settings paths** (alternative location):
- macOS: `~/Library/Application Support/Windsurf/User/settings.json`
- Linux: `~/.config/Windsurf/User/settings.json`
- Windows: `%APPDATA%\Windsurf\User\settings.json`

```json
{
  "mcpServers": {
    "server-name": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-package", "optional-args"],
      "env": { "KEY": "value" },
      "disabled": false
    }
  }
}
```

**Remote HTTP server**:
```json
{
  "mcpServers": {
    "remote-server": {
      "serverUrl": "https://example.com/mcp",
      "headers": { "API_KEY": "Bearer ${env:AUTH_TOKEN}" }
    }
  }
}
```

**Fields**:
- `command` -- executable to launch (stdio transport)
- `args` -- array of arguments
- `env` -- environment variables (supports `${env:VAR}` interpolation)
- `disabled` -- boolean to disable without removing
- `serverUrl` -- for remote HTTP MCP servers
- `headers` -- HTTP headers for remote servers
- `alwaysAllow` -- optional permissions array

---

## 2. GitHub Copilot

### 2.1 Directory Structure

```
.github/
  copilot-instructions.md              # Repo-wide instructions (all IDEs)
  instructions/
    frontend.instructions.md           # Path-specific instructions
    testing.instructions.md            # Path-specific instructions
    nika-workflows.instructions.md     # Path-specific instructions
```

**Scope**: Works in VS Code, JetBrains, Xcode, Visual Studio, Eclipse.
**Agent files also recognized**: `AGENTS.md` (anywhere in repo, nearest ancestor wins), `CLAUDE.md`, `GEMINI.md` (repo root).

### 2.2 copilot-instructions.md

**Path**: `.github/copilot-instructions.md`
**Format**: Plain Markdown, no frontmatter required
**Behavior**: Auto-applied to all Copilot Chat interactions in the repo

```markdown
<!-- .github/copilot-instructions.md -->
# Project: Nika

Nika is a semantic YAML workflow engine for AI tasks.

## Tech Stack
- Language: Rust
- Schema: nika/workflow@0.12
- Testing: cargo test --lib (never bare cargo test)

## Conventions
- Errors use NikaError with NIKA-XXX codes, never anyhow
- AST phases: Raw -> Analyzed -> Lower (never skip)
- Workflow files use .nika.yaml extension
- 5 verbs: infer, exec, fetch, invoke, agent

## Do NOT
- Access Neo4j/Cypher directly (use MCP invoke: verb)
- Use image::load_from_memory() (use decode_image_safe())
- Skip SVG sanitization before parsing
```

**Best practices**:
- Keep concise and focused on key rules
- Natural language Markdown
- Focus on style, libraries, patterns to avoid, error handling
- No special formatting required -- paragraphs, lists, or blocks all work

### 2.3 Path-Specific Instructions (.instructions.md)

**Path**: `.github/instructions/*.instructions.md`
**Format**: Markdown with YAML frontmatter

```yaml
---
name: Optional display name        # Defaults to filename if omitted
description: Short hover text       # Optional
applyTo: "**/*.nika.yaml"          # Glob pattern(s), comma-separated for multiple
excludeAgent: "code-review"         # Optional: exclude from "code-review" or "code-agent"
---
```

**Example**:
```markdown
<!-- .github/instructions/nika-workflows.instructions.md -->
---
applyTo: "**/*.nika.yaml"
---

# Nika Workflow Guidelines

- Schema: nika/workflow@0.12
- Use `with:` for data bindings with `$` prefix for task references
- `depends_on:` for task ordering
- `timeout:` values are in seconds
- Available verbs: infer, exec, fetch, invoke, agent
```

### 2.4 Copilot Extensions

**Two types**:

| Type | Scope | Access |
|------|-------|--------|
| Client-side | VS Code only | Local workspace, terminal, uncommitted code |
| Server-side | All IDEs + GitHub.com + Mobile | Backend-hosted, GitHub API, no local access |

**Architecture**: Extensions integrate via `@mention` in Copilot Chat (e.g., `@docker`, `@perplexity`).

**Building an extension**:
1. Choose client-side (VS Code extension API) or server-side (backend service)
2. Implement via MCP servers (resources, prompts, tools, sampling)
3. Auth via OIDC (replaced X-GitHub-Token)
4. Publish to GitHub Marketplace

**Existing extensions**: Perplexity, Docker, Stack Overflow, Mermaid Chart, ReadMe, DataStax.

### 2.5 Built-in Chat Participants

| Participant | Purpose |
|-------------|---------|
| `@workspace` | Provides context from current IDE workspace/codebase |
| `@terminal` | Accesses terminal output/state |
| `@vscode` | VS Code editor features (navigation, completion) |

---

## 3. Cline / Roo Code

### 3.1 File Format Evolution

| Era | Format | Status |
|-----|--------|--------|
| Legacy | `.clinerules` (single file, project root) | Deprecated |
| Legacy | `.clinerules-{mode}.md` (e.g., `.clinerules-architect`) | Deprecated |
| Current | `.roo/rules/` directory | Primary (post v3.11.8) |
| Current | `.roo/rules-{mode}/` subdirectories | Mode-specific rules |

### 3.2 .roo/ Directory Structure

```
.roo/
  rules/                          # Global workspace rules (all modes)
    01-general.md                 # Numeric prefix for ordering (optional)
    coding-standards.md
    security.md
  rules-code/                     # Code mode specific rules
    rust-conventions.md
  rules-architect/                # Architect mode specific rules
    design-patterns.md
  rules-test/                     # Test mode specific rules
    testing-rules.md
  rules-{custom-slug}/            # Custom mode specific rules
    custom-rules.md
```

### 3.3 Rule File Frontmatter

```yaml
---
paths: ["**/*.nika.yaml", "src/**/*.rs"]  # Glob patterns for conditional application
---
```

**Confirmed fields**:
- `paths` -- array of glob patterns (supports `*`, `**`, `?`, `[abc]`)

**Rule body**: Standard Markdown with headings and lists.

**File handling**:
- Files sorted by basename (alphabetical)
- Numeric prefixes for explicit ordering (e.g., `01-general.md`, `02-security.md`)
- Symlinks supported (depth limit: 5)
- Workspace rules override global (`~/.roo/rules/`)

### 3.4 Custom Modes (.roomodes)

**Path**: `.roomodes` (project root)
**Format**: JSON

```json
{
  "customModes": [
    {
      "slug": "nika",
      "name": "Nika Workflow Expert",
      "roleDefinition": "You are an expert in Nika workflow engine. You understand the 5 verbs (infer, exec, fetch, invoke, agent), YAML schema nika/workflow@0.12, DAG execution, and MCP integration.",
      "groups": [
        ["read", {}],
        ["edit", { "fileRegex": "\\.nika\\.yaml$" }],
        ["command", { "allowedCommands": ["nika check", "nika run"] }]
      ],
      "customInstructions": "Always validate workflows with 'nika check' before suggesting they are complete. Use NikaError codes (NIKA-XXX) when discussing errors."
    }
  ]
}
```

**Mode fields**:
- `slug` -- unique lowercase identifier with hyphens
- `name` -- display name
- `roleDefinition` -- persona/expertise description
- `groups` -- tool permission groups (read, edit, command, etc.)
- `customInstructions` -- behavioral rules text

**Built-in modes**: Code, Architect, Ask, Debug, Orchestrator

**Mode-specific rules**: Place in `.roo/rules-{slug}/` directory (e.g., `.roo/rules-nika/`)

### 3.5 MCP Configuration

Roo Code uses VS Code's MCP settings. Configuration is typically in VS Code settings or the Roo Code extension settings, not a standalone `.roo/mcp.json` file. MCP servers are configured similarly to other VS Code AI extensions.

---

## 4. Amazon Q Developer

### 4.1 Rules Directory

**Path**: `.amazonq/rules/` (project root)
**Format**: Markdown (.md) files, no required frontmatter
**Behavior**: Auto-loaded for all project chats

### 4.2 Rule File Format

Amazon Q uses structured Markdown with recommended sections:

```markdown
# Rule Name

## Purpose
A clear, concise statement explaining why this rule exists.

## Instructions
- Specific directives for Amazon Q (e.g., ID: EVALUATE_REUSABILITY)
- Conditions under which instructions apply

## Priority
[Critical/High/Medium/Low]

## Error Handling
- Fallback strategies or behaviors for exceptions
```

**Example**:
```markdown
<!-- .amazonq/rules/nika.md -->
# Nika Workflow Engine

## Purpose
Defines conventions for working with Nika YAML workflow files.

## Instructions
- All workflow files use `.nika.yaml` extension (ID: WORKFLOW_EXTENSION)
- Schema is `nika/workflow@0.12` (ID: SCHEMA_VERSION)
- 5 verbs only: infer, exec, fetch, invoke, agent (ID: VERB_RESTRICTION)
- Use NikaError with NIKA-XXX codes, never anyhow (ID: ERROR_CODES)
- AST pipeline: Raw -> Analyzed -> Lower, never skip (ID: AST_PHASES)
- Zero Cypher in Nika -- use MCP invoke: verb (ID: ZERO_CYPHER)

## Priority
Critical

## Error Handling
- If unsure about error code range, refer to NIKA-XXX table in CLAUDE.md
```

**Priority resolution**: Critical > High > Medium > Low (resolves conflicts between rules).

**Instruction IDs**: Unique identifiers (e.g., `ID: EVALUATE_REUSABILITY`) for transparency and debugging.

### 4.3 MCP Configuration

**Path**: `.amazonq/mcp.json` (workspace) or `~/.aws/amazonq/mcp.json` (global)
**Format**: JSON

```json
{
  "mcpServers": {
    "server-name": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-package"],
      "env": { "KEY": "value" }
    }
  }
}
```

**Transport types**:
- **stdio** (local): `command` + `args` + `env`
- **HTTP** (remote, added Sept 2025): `url` + OAuth authentication + optional `headers`

**Admin control**: Administrators can enable/disable MCP organization-wide via AWS console (checked at session start + every 24h).

### 4.4 Context Features

- `@workspace` -- indexes code/configs (excludes .gitignore'd files)
- `@files`, `@folders`, `@code` -- manual context additions in chat
- CLI: `/save` and `/load` for session persistence

---

## 5. Continue.dev

### 5.1 Directory Structure

```
.continue/
  config.yaml          # Main configuration (required)
  rules/               # Local rule files (auto-loaded)
  prompts/             # Custom prompt templates / slash commands
  models/              # Optional local model configs
```

**Global equivalent**: `~/.continue/` (same structure)

### 5.2 config.yaml Format

```yaml
name: Nika Development
version: 1.0.0
schema: v1

models:
  - name: Claude Sonnet
    provider: anthropic
    model: claude-sonnet-4-20250514
    contextLength: 200000

context:
  - provider: file
  - provider: code
    params:
      onlyMyCode: true
  - provider: diff
  - provider: docs

rules:
  - "Always use NikaError with NIKA-XXX codes."
  - uses: local/nika-conventions     # Reference to local rule file
  - name: Nika Workflows
    globs: ["**/*.nika.yaml"]
    alwaysApply: false
    description: "Rules for Nika workflow files"

prompts:
  - uses: local/nika-check           # Slash command

mcpServers:
  - name: nika-tools
    command: "nika"
    args: ["mcp-serve"]

docs:
  - "https://docs.nika.dev"
```

### 5.3 Rule Files (.continue/rules/)

**Format**: Markdown with YAML frontmatter
**Loading**: Auto-loaded, sorted lexicographically (use numeric prefixes)

**Frontmatter fields**:

```yaml
---
name: Rule Name              # Required: display name
description: Brief text      # Used by agent to decide inclusion when alwaysApply=false
globs: "**/*.nika.yaml"      # String or array of glob patterns
alwaysApply: true             # true=always | false=globs/agent-decision | undefined=default
regex: "^import .* from"     # String or array of regex patterns (match file content)
---
```

**Loading order**: Hub assistant rules > Referenced Hub rules (`uses:`) > Local `.continue/rules/` > Global `~/.continue/rules/`

**Example**:
```markdown
<!-- .continue/rules/nika-workflows.md -->
---
name: Nika Workflow Rules
globs: ["**/*.nika.yaml"]
alwaysApply: false
description: Conventions for Nika semantic YAML workflows
---

# Nika Workflow Conventions

- Schema: `nika/workflow@0.12`
- Extension: `.nika.yaml` (not `.yaml`)
- 5 verbs: infer, exec, fetch, invoke, agent
- Bindings: `with: { alias: $task_id }` ($ prefix required)
- Templates: `{{with.alias}}` with pipe transforms
- Ordering: `depends_on: [task_id]`
- Timeout: values in seconds
```

### 5.4 Custom Slash Commands

**Path**: `.continue/prompts/` or referenced in `config.yaml`
**Format**: Markdown with YAML frontmatter

```markdown
<!-- .continue/prompts/nika-check.md -->
---
name: Nika Check
description: Validate a Nika workflow file
invokable: true
---

Run `nika check` on the current workflow file. Report any errors using
NIKA-XXX error code format. Suggest fixes for each error found.
```

**Usage**: Type `/Nika Check` in Continue chat.

### 5.5 Context Providers

Built-in providers: `file`, `code`, `diff`, `terminal`, `problems`, `folder`, `codebase`, `docs`, `web`, `http`.

Custom context providers can be built as TypeScript/Node.js plugins implementing the provider interface. Configuration:

```yaml
context:
  - provider: my-custom-provider
    params:
      customParam: value
```

### 5.6 Continue Hub

Continue Hub (`continue.dev/hub`) hosts shareable rules and configurations. Reference via `uses:` in config.yaml. Local rules auto-load alongside Hub configs.

---

## 6. Zed AI

### 6.1 Settings Paths

- **Global**: `~/.config/zed/settings.json`
- **Project**: `.zed/settings.json` (project root, overrides global)

### 6.2 AI Configuration in settings.json

```json
{
  "agent": {
    "default_model": {
      "provider": "anthropic",
      "model": "claude-sonnet-4-20250514"
    },
    "inline_assistant_model": {
      "provider": "openai",
      "model": "gpt-4o"
    },
    "model_parameters": [
      {
        "provider": "anthropic",
        "temperature": 0.3
      }
    ],
    "tool_permissions": {
      "default": "confirm",
      "tools": {
        "terminal": {
          "default": "confirm",
          "always_allow": [
            { "pattern": "^cargo\\s+(build|test|check|clippy)" },
            { "pattern": "^nika\\s+(check|run|ui)" }
          ],
          "always_deny": [
            { "pattern": "rm\\s+-rf\\s+(/|~)" }
          ]
        },
        "edit_file": {
          "always_deny": [
            { "pattern": "\\.env" },
            { "pattern": "\\.(pem|key)$" }
          ]
        }
      }
    }
  },
  "language_models": {
    "ollama": {
      "api_url": "http://localhost:11434"
    }
  }
}
```

### 6.3 MCP / Context Servers

**Key**: `context_servers` in settings.json

```json
{
  "context_servers": {
    "nika-tools": {
      "source": "custom",
      "command": "/usr/local/bin/nika",
      "args": ["mcp-serve"],
      "env": { "NIKA_LOG": "info" }
    },
    "github": {
      "command": {
        "command": "npx",
        "args": ["-y", "@modelcontextprotocol/server-github"],
        "env": { "GITHUB_PERSONAL_ACCESS_TOKEN": "token" }
      },
      "settings": {}
    }
  }
}
```

**Extension-provided servers**: Install via Zed's extension marketplace, then add settings:

```json
{
  "context_servers": {
    "mcp-server-github": {
      "settings": {
        "api_key": "ghp_xxx"
      }
    }
  }
}
```

### 6.4 Project-Specific AI Instructions

Zed does **not** have a native equivalent to CLAUDE.md or copilot-instructions.md. Project-specific AI behavior is configured through:
- `.zed/settings.json` -- model selection, tool permissions
- Context servers -- MCP-based tool/data providers
- Agent Panel -- interactive instructions (not file-based)

Zed does **not** currently support `AGENTS.md` natively. No prompt library directory exists.

### 6.5 Zed Extension API

Extensions use Rust/WASM via `zed_extension_api`. They can:
- Provide language servers (LSP)
- Define context server commands (`context_server_command`)
- Add themes, languages, snippets
- Currently limited: no custom AI agent logic, no project-level rules injection

---

## 7. Aider

### 7.1 CONVENTIONS.md

**Path**: `./CONVENTIONS.md` (project root)
**Format**: Plain Markdown
**Behavior**: Auto-read at startup, included in every LLM prompt

```markdown
<!-- CONVENTIONS.md -->
# Nika Development Conventions

## Language
- Rust, edition 2021
- AGPL-3.0-or-later license

## Error Handling
- NikaError with NIKA-XXX codes, never anyhow
- Error code ranges documented in tools/nika/CLAUDE.md

## AST Pipeline
- Always Raw -> Analyzed -> Lower
- Never skip phases

## Testing
- `cargo test --lib` (never bare `cargo test` -- triggers Keychain)
- TDD preferred, insta for snapshots

## Naming
- PascalCase for types
- snake_case for functions
- UPPER_SNAKE_CASE for constants

## Workflow Files
- Extension: .nika.yaml
- Schema: nika/workflow@0.12
- 5 verbs: infer, exec, fetch, invoke, agent
```

**Best practices**: Keep under ~2000 tokens. Concise, scannable rules. Update frequently.

### 7.2 .aider.conf.yml

**Paths** (later overrides earlier):
1. `~/.aider.conf.yml` (global)
2. `{git-root}/.aider.conf.yml` (repo)
3. `./.aider.conf.yml` (current directory)

**Complete field reference**:

```yaml
# Model Selection
model: claude-sonnet-4-20250514
architect: true                    # Enable architect mode
auto-accept-architect: false       # Auto-approve architect suggestions
edit-format: diff                  # "diff" or "whole"

# File Handling
read:                              # Read-only context files (not editable)
  - CONVENTIONS.md
  - docs/ARCHITECTURE.md
watch-files: true                  # Auto-add modified files
encoding: utf-8                    # File encoding

# Git
auto-commits: true                 # Auto-commit AI changes
attribute-author: true             # Add Aider as git author
attribute-co-authored-by: true     # Add co-authored-by trailer
commit-prompt: "Describe changes"  # Custom commit message prompt
git-commit-verify: true            # Enforce git commit hooks
dry-run: false                     # Simulate without applying

# Testing & Linting
lint-cmd: "cargo clippy --workspace -- -D warnings"
test-cmd: "cargo test --workspace --lib"
auto-test: true                    # Run tests after edits

# API & Model Settings
api-key: sk-xxx                    # LLM API key (prefer .env)
api-base: https://api.example.com  # Custom API endpoint
verify-ssl: true                   # SSL verification
timeout: 60                        # Request timeout (seconds)
reasoning-effort: high             # For reasoning models
thinking-tokens: 4000              # Thinking budget

# Display
dark-mode: true
show-diffs: true
stream: true                       # Live output streaming
notify: true                       # Desktop notifications

# Advanced
map-tokens: 1024                   # Token budget for repo map
model-aliases:
  fast: openai/gpt-4o-mini
  smart: anthropic/claude-sonnet-4-20250514
```

### 7.3 .aiderignore

**Path**: `./aiderignore` (project root)
**Format**: Same as `.gitignore` syntax

```gitignore
# .aiderignore
node_modules/
target/
*.log
.env
credentials.json
# Include specific important file
!src/important.log
```

### 7.4 DSL Support

Aider has no native DSL extension mechanism. Custom DSL understanding is achieved through:
1. **CONVENTIONS.md** -- describe DSL syntax and rules
2. **Read-only files** -- `/add schema.md --read-only` for reference docs
3. **repo map** -- Aider scans function signatures; works best with popular languages

```yaml
# .aider.conf.yml for Nika DSL support
read:
  - docs/reference/schema.md
  - docs/reference/verbs.md
model: claude-sonnet-4-20250514
```

---

## 8. Cross-Tool: AGENTS.md Standard

### 8.1 Overview

AGENTS.md is an emerging open standard (proposed 2025, 60k+ repos by mid-2025) for providing AI coding agents with project instructions. It is tool-agnostic and uses plain Markdown.

### 8.2 Format

**Path**: `AGENTS.md` (repo root, or nested in subdirectories for monorepos)
**Format**: Plain Markdown, no frontmatter or special syntax

**Guidelines**:
- First ~100 lines contain key information
- Stay under 500 lines to avoid truncation
- No secrets or tool-specific details
- Nearest ancestor file takes precedence (monorepo support)

### 8.3 Recommended Sections

```markdown
# Project Name
Brief description and goals.

## Development Environment
- Build/test commands
- Dependencies

## Code Style Guidelines
- Formatting/naming rules
- Architecture patterns

## Project Context
- Key files/gotchas
- Performance areas

## Testing Instructions
- Runners/coverage
- CI/CD
```

### 8.4 Tool Support

| Tool | AGENTS.md Support |
|------|-------------------|
| OpenAI Codex | Yes |
| Google Gemini CLI / Jules | Yes |
| Cursor | Yes |
| Aider | Yes |
| Zed | Yes |
| Roo Code | Yes |
| GitHub Copilot | Yes (coding agent) |
| Claude Code | Prefers CLAUDE.md |
| Factory AI | Yes |

---

## 9. Comparison Matrix

### 9.1 Rules File Format

| Tool | Path | Format | Frontmatter |
|------|------|--------|-------------|
| Claude Code | `CLAUDE.md` | Markdown | None |
| Cursor | `.cursor/rules/` | Markdown | `description`, `globs`, `alwaysApply` |
| Windsurf | `.windsurf/rules/` | Markdown | `trigger`, `globs`, `description`, `alwaysApply` |
| Copilot | `.github/instructions/` | Markdown | `applyTo`, `name`, `description`, `excludeAgent` |
| Roo Code | `.roo/rules/` | Markdown | `paths` |
| Amazon Q | `.amazonq/rules/` | Markdown | None (structured body) |
| Continue | `.continue/rules/` | Markdown | `name`, `description`, `globs`, `alwaysApply`, `regex` |
| Zed | `.zed/settings.json` | JSON | N/A (no rules files) |
| Aider | `CONVENTIONS.md` | Markdown | None |
| Universal | `AGENTS.md` | Markdown | None |

### 9.2 MCP Configuration

| Tool | MCP Config Path | Format |
|------|-----------------|--------|
| Claude Code | `~/.claude/` (settings) | JSON |
| Cursor | `.cursor/mcp.json` | JSON |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | JSON |
| Copilot | VS Code settings | JSON |
| Roo Code | VS Code settings | JSON |
| Amazon Q | `.amazonq/mcp.json` or `~/.aws/amazonq/mcp.json` | JSON |
| Continue | `config.yaml` (`mcpServers:`) | YAML |
| Zed | `settings.json` (`context_servers:`) | JSON |
| Aider | N/A | N/A |

### 9.3 Glob Pattern Support

| Tool | Glob Field | Example |
|------|------------|---------|
| Cursor | `globs` | `"*.nika.yaml"` |
| Windsurf | `globs` | `"*.nika.yaml"` |
| Copilot | `applyTo` | `"**/*.nika.yaml"` |
| Roo Code | `paths` | `["**/*.nika.yaml"]` |
| Continue | `globs` | `["**/*.nika.yaml"]` |
| Amazon Q | N/A | No glob support |
| Zed | N/A | No rules files |
| Aider | N/A | No glob support |

### 9.4 Custom Modes / Agents

| Tool | Custom Mode Support | Mechanism |
|------|---------------------|-----------|
| Roo Code | Yes | `.roomodes` JSON + `rules-{slug}/` dirs |
| Continue | Partial | Hub assistants |
| Copilot | No | Built-in @participants only |
| Windsurf | No | @mention triggers only |
| Cursor | No | N/A |
| Amazon Q | No | N/A |
| Zed | No | N/A |
| Aider | Partial | `--architect` mode toggle |

---

## 10. Nika Integration Strategy

### 10.1 Files to Generate

A `nika init` or `nika dx` command could generate all integration files:

```
project/
  AGENTS.md                                # Universal standard
  CLAUDE.md                                # Claude Code
  CONVENTIONS.md                           # Aider
  .cursor/rules/nika.md                    # Cursor
  .windsurf/rules/nika.md                  # Windsurf
  .github/
    copilot-instructions.md                # GitHub Copilot (repo-wide)
    instructions/
      nika-workflows.instructions.md       # Copilot (path-specific)
  .roo/
    rules/nika.md                          # Roo Code
    rules-code/nika.md                     # Roo Code (code mode)
  .roomodes                                # Roo Code custom mode
  .amazonq/rules/nika.md                   # Amazon Q
  .continue/
    rules/nika.md                          # Continue.dev
    prompts/nika-check.md                  # Continue.dev slash command
  .zed/settings.json                       # Zed (model + tool perms only)
  .aider.conf.yml                          # Aider config
```

### 10.2 Content Reuse Strategy

All tools use Markdown for rules. The core content can be generated once and adapted per tool's frontmatter:

**Core content** (shared across all tools):
```markdown
# Nika Workflow Engine

- Schema: nika/workflow@0.12
- Extension: .nika.yaml
- 5 verbs: infer, exec, fetch, invoke, agent
- Errors: NikaError with NIKA-XXX codes
- AST: Raw -> Analyzed -> Lower (never skip)
- Testing: cargo test --lib (never bare cargo test)
- MCP: invoke: verb for NovaNet (Zero Cypher rule)
```

**Per-tool frontmatter wrappers**:

| Tool | Wrapper |
|------|---------|
| Cursor | `globs: "**/*.nika.yaml"` |
| Windsurf | `globs: "*.nika.yaml"\ndescription: Nika workflow rules` |
| Copilot | `applyTo: "**/*.nika.yaml"` |
| Roo Code | `paths: ["**/*.nika.yaml"]` |
| Continue | `name: Nika\nglobs: ["**/*.nika.yaml"]` |
| Amazon Q | No frontmatter, use `## Priority\nCritical` |
| Aider | No frontmatter (CONVENTIONS.md) |
| Claude Code | No frontmatter (CLAUDE.md) |

### 10.3 MCP Server Registration

For tools that support MCP, Nika could register as an MCP server:

```json
{
  "mcpServers": {
    "nika": {
      "command": "nika",
      "args": ["mcp-serve"],
      "env": {}
    }
  }
}
```

This would work identically across Windsurf, Cursor, Amazon Q, and Zed (with `context_servers` key).

---

## Sources

1. Windsurf docs -- codeium.com/windsurf (Wave 8+ rules format)
2. GitHub Copilot docs -- docs.github.com/copilot (instructions format)
3. Roo Code docs -- docs.roocode.com (rules, modes, MCP)
4. Amazon Q docs -- docs.aws.amazon.com/amazonq (project rules, MCP)
5. Continue.dev docs -- docs.continue.dev (config.yaml, rules, prompts)
6. Zed docs -- zed.dev/docs (settings.json, context_servers)
7. Aider docs -- aider.chat (CONVENTIONS.md, .aider.conf.yml)
8. AGENTS.md -- Community standard specification

## Methodology

- Tools used: Perplexity sonar-pro (12 queries)
- Sources cross-referenced: Official docs, GitHub repos, community examples
- Date: 2026-03-23
- Coverage: 7 tools + 1 universal standard

## Confidence Levels

| Tool | Confidence | Notes |
|------|------------|-------|
| Windsurf | High | Well-documented frontmatter fields |
| GitHub Copilot | High | Official GitHub docs comprehensive |
| Roo Code | Medium | Rapidly evolving; .roomodes schema partially inferred |
| Amazon Q | High | AWS docs clear; MCP config confirmed |
| Continue.dev | High | Complete frontmatter spec documented |
| Zed | Medium | AI features evolving fast; no rules file support |
| Aider | High | Stable, well-documented CLI-to-YAML mapping |
| AGENTS.md | Medium | Emerging standard, no formal spec document |
