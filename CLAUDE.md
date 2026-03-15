# Nika

Cargo workspace for Nika — semantic YAML workflow engine for AI tasks.

## Auto-Imported Context

@README.md @CHANGELOG.md

---

## 🦋 🐔 🐤 Mascots & Hierarchy

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              SUPERNOVAE MASCOTS                               ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   🦋 NIKA = LE RUNTIME (Papillon)                                             ║
║   ─────────────────────────────────────────────────────────────────────────   ║
║   • Orchestre les 5 verbes sémantiques                                        ║
║   • Exécute les workflows YAML en DAG                                         ║
║   • Chat UI ($ nika chat) où Nika parle à l'utilisateur                       ║
║   • Lance les agents quand le verbe agent: est invoqué                        ║
║                                                                               ║
║   🐔 AGENT = UN VERBE (Space Chicken)                                         ║
║   ─────────────────────────────────────────────────────────────────────────   ║
║   • UN des 5 verbes (infer, exec, fetch, invoke, agent)                       ║
║   • Multi-turn agentic loop avec MCP tools                                    ║
║   • Peut spawner des subagents via spawn_agent                                ║
║   • Protégé par depth_limit contre la récursion infinie                       ║
║                                                                               ║
║   🐤 SUBAGENT = Spawné par agent (Poussin)                                    ║
║   ─────────────────────────────────────────────────────────────────────────   ║
║   • Créé par l'agent via spawn_agent tool                                     ║
║   • Exécute une sous-tâche spécifique                                         ║
║   • Retourne son résultat à l'agent parent                                    ║
║   • Hérite du depth_limit (décrémenté)                                        ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝

  $ nika chat
  ┌────────────────────────────────────────────────────────────────┐
  │ 🦋 Bonjour! Je suis Nika. Comment puis-je vous aider?          │
  │                                                                │
  │ User: /agent "Research AI papers and summarize"                │
  │                                                                │
  │ 🦋 Je lance un agent pour cette tâche...                       │
  │   │                                                            │
  │   ├─🐔 Agent: Searching for AI papers...                       │
  │   │   ├─🐤 Subagent: Fetching arxiv.org...                     │
  │   │   └─🐤 Subagent: Parsing results...                        │
  │   └─🐔 Agent: Done! Found 15 papers.                           │
  │                                                                │
  │ 🦋 L'agent a terminé. Voici les résultats...                   │
  └────────────────────────────────────────────────────────────────┘
```

---

## Why Nika Exists

**Problem**: Orchestrating multi-step AI workflows is fragile, opaque, and hard to debug.
- LLM calls buried in code are untraceable and non-reproducible
- Chaining tools, agents, and MCP calls requires custom glue code each time
- No standard format for AI workflow definitions

**Solution**: Nika executes YAML-defined DAG workflows with 5 semantic verbs.
- Workflows are version-controlled, human-readable YAML files
- Full observability via NDJSON trace files per run
- Native MCP client connects to NovaNet (and any MCP server)

**Result**: AI workflows as first-class artifacts — readable, testable, reproducible.

---

## Overview

Nika is the "body" of the SuperNovae AGI architecture, executing workflows that leverage NovaNet's "brain".

**Current Version**: v0.27.0 — spn→nika Feature Fusion
**Tests**: 5,054 passing | **Roadmap**: `ROADMAP.md` | **Changelog**: `CHANGELOG.md`
**Target Application**: QR Code AI (https://qrcode-ai.com)

**v0.27.0 Changes (spn→nika Feature Fusion):**
- **Unified CLI** — spn features merged into nika for a single tool experience
  - `nika provider` — API key management (list, set, get, test, migrate)
  - `nika model` — Local model management (list, pull, info, search)
  - `nika mcp` — MCP server management (add, remove, list, test, tools)
  - `nika sync` — Editor synchronization (enable, disable, status)
  - `nika setup` — Interactive onboarding wizard (nika, novanet, claude-code)
  - `nika daemon` — Background service management (start, stop, status)
  - `nika jobs` — Background job execution (submit, cancel, output, list)
  - `nika backup` — SuperNovae data backup (create, restore, list, prune)
- **Core Module** — New `src/core/` with zero-dependency provider/model/MCP definitions
  - `KNOWN_PROVIDERS` — 6 LLM + 11 MCP + 1 Local = 18 providers (Ollama removed v0.27)
  - `KNOWN_MODELS` — 16+ curated models for native inference
  - `MCP_ALIASES` — 48 MCP server aliases for auto-configuration
- **spn Deprecation** — Running `spn <cmd>` now shows deprecation warning directing to `nika`

**v0.26.0 Changes (ADR-008: Inference Architecture Refactor):**
- **Native Inference** — mistral.rs runtime for local GGUF models
  - `provider: native` in workflows for local LLM inference
  - `NativeRuntime` replaces deprecated `NativeClient`
  - Streaming support via `infer_stream()` with async channels
  - Metal (macOS) and CUDA (Linux) acceleration
- **Inference Moved from spn** — All inference now in Nika (spn is storage-only)
- **InferenceBackend Trait** — Unified interface for local models
- **Download Models** — `spn model pull llama3.2:1b` then use in workflows

```yaml
# Native inference example (no API key required)
tasks:
  - id: local_llm
    infer:
      provider: native
      model: ~/.cache/huggingface/models/llama3.2-1b-q4.gguf
      prompt: "Explain quantum computing"
      temperature: 0.7
```

**v0.24.0 Changes:**
- **StructuredOutput Layers 3 & 4** — Now actually call LLM for retry/repair
- **Control Flow: fail_fast** — Properly cancels in-flight tasks with `tokio::select!`
- **Deadlock Detection** — Distinguishes true deadlock from dependency chain failure
- **MCP Operation Timeouts** — 5 minute deadline for all MCP tasks
- **Sleep Tool Limits** — 5 minute maximum to prevent unbounded sleep
- **MCP Error Code Preservation** — JSON-RPC error codes preserved from servers
- **New Error Codes** — NIKA-025, NIKA-026, NIKA-027 for task failures
- **New TaskStatus Variants** — `DependencyFailed`, `Skipped` with reasons

**v0.22.0 Changes:**
- **exec.env** — Environment variable injection: `env: { KEY: value }`
- **fetch.json** — Auto-serialize JSON body: `json: { ... }`
- **inputs.xxx references** — Access workflow inputs in use: blocks
- **$inputs binding** — for_each accepts `$inputs.items` expressions
- **TaskStatus::Queued/Skipped** — New task status variants
- **TUI panels/ module** — TaskListPanel, TaskBoxFlow, BrowserPanel

**v0.20.x Changes:**
- **4-View TUI Architecture** — Streamlined from 8 views to 4 focused views
  - `Studio`: 3-panel unified workspace (Browser | Editor | DAG Preview)
  - `Runner`: Real-time execution monitoring
  - `Chat`: Conversational agent interface
  - `Settings`: Provider config and preferences
  - Keyboard shortcuts: `1-4` or `s/r/c/,`
- **Tree Widget Integration** — tui-tree-widget v0.24 for VS Code-like file browser
- **spn Daemon Secret Management** — Unified keychain access
- **Two-Phase IR Architecture** — Raw AST → Analyzed AST pipeline

**v0.19.x Changes:**
- **Structured Output Enforcement** — JSON Schema validation with retry loops
- **Output Policy** — Task-level schema injection for infer/agent prompts
- **{{inputs.*}} Templates** — Access workflow inputs in templates
- **`nika new` Command** — Interactive workflow creation wizard
- **103 Test Suite Workflows** — Comprehensive coverage for all features

**v0.18.0 Changes (Artifacts):**
- **Artifact System** — Complete file persistence for task outputs
  - `io::atomic` — Atomic writes with temp+fsync+rename pattern
  - `io::security` — Path validation, traversal prevention
  - `io::template` — Variable interpolation (`{{task_id}}`, `{{date}}`, etc.)
  - `io::writer` — `ArtifactWriter` combining all modules
- **Security Hardening** — Template injection prevention, TOCTOU mitigation, JSON validation
- **Artifact Events** — `ArtifactWritten`, `ArtifactFailed` for observability
- **Error Codes NIKA-280-289** — New artifact error variants

**v0.15.0 Features (inherited):**
- Security hardening: `exec:` defaults to `shell: false` (shlex parsing)
- Infer LLM control: `temperature`, `system`, `max_tokens` support
- Gemini provider (7th): `RigProvider::gemini()` with full streaming
- File tools (5 new): `nika:read`, `nika:write`, `nika:edit`, `nika:glob`, `nika:grep`
- 11 builtin tools total (6 core + 5 file)
- Zero clippy warnings, comprehensive test coverage

---

## Release Strategy

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  📦 RELEASE GRANULARITY RULES                                                  ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  RULE 1: 1 Module = 1 Release                                                 ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  Each logical module gets its own PATCH release.                              ║
║  Example: io::atomic, io::security, io::template → 3 separate releases        ║
║                                                                               ║
║  RULE 2: Milestone Tags                                                       ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  When a feature has multiple milestones, create milestone tags:               ║
║  v0.X.0-m1-name, v0.X.0-m2-name, v0.X.0-m3-name, v0.X.0 (release)            ║
║                                                                               ║
║  RULE 3: Beautiful CHANGELOGs                                                 ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  Major releases use ASCII art boxes for visual clarity.                       ║
║  Each milestone documented with its own section.                              ║
║                                                                               ║
║  RULE 4: Version Lock                                                         ║
║  ─────────────────────────────────────────────────────────────────────────    ║
║  Nika will NEVER be v1.0.0. Forever 0.x.x.                                    ║
║  PATCH = bug fixes, MINOR = new features, no MAJOR.                           ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Tag Naming Convention:**

| Type | Format | Example |
|------|--------|---------|
| Release | `vX.Y.Z` | `v0.18.0` |
| Milestone | `vX.Y.Z-mN-name` | `v0.18.0-m1-atomic` |
| Pre-release | `vX.Y.Z-rc.N` | `v0.18.0-rc.1` |

```
CRITICAL: 5 Semantic Verbs Only

infer:   → LLM generation (rig-core, 6 providers)
exec:    → Shell command execution
fetch:   → HTTP request
invoke:  → MCP tool call
agent:   → Multi-turn agentic loop
```

---

## Architecture

```
nika-dev/
├── tools/nika/          # Rust binary (main source)
│   ├── src/
│   │   ├── ast/         # YAML → Rust structs
│   │   ├── core/        # Zero-dep provider/model/MCP definitions (v0.27)
│   │   ├── dag/         # DAG validation
│   │   ├── runtime/     # Execution engine
│   │   ├── mcp/         # MCP client (rmcp v0.16)
│   │   ├── event/       # NDJSON trace writer
│   │   ├── tui/         # Terminal UI (ratatui)
│   │   ├── binding/     # Data flow + lazy bindings
│   │   ├── secrets/     # Keychain + daemon IPC (v0.27)
│   │   └── provider/    # rig-core v0.32 + native inference (mistral.rs)
│   ├── CLAUDE.md        # Tool-level detailed context
│   └── Cargo.toml       # v0.27.0
└── docs/                # Plans + research
```

---

## Commands

```bash
# Run workflows
nika workflow.nika.yaml      # Run (positional arg)
nika check workflow.nika.yaml # Validate

# TUI
nika                          # TUI Home view
nika chat                     # Chat view (conversational agent)
nika studio [file]            # Studio view (YAML editor)

# Traces
nika trace list               # List traces
nika trace show <id>          # Display events
nika trace export <id>        # Export JSON/YAML

# Provider Management (v0.27 spn fusion)
nika provider list            # Show all providers with status
nika provider set anthropic   # Store API key in OS keychain
nika provider test claude     # Validate key with provider
nika provider migrate         # Migrate env vars to keychain

# Model Management (v0.27 spn fusion)
nika model list               # List available local models
nika model pull llama3.2:1b   # Download model from HuggingFace
nika model info qwen3:8b      # Show model details

# MCP Server Management (v0.27 spn fusion)
nika mcp add neo4j            # Add MCP server (48 aliases)
nika mcp list                 # List configured servers
nika mcp test neo4j           # Test server connection
nika mcp tools neo4j          # List available tools

# Editor Sync (v0.27 spn fusion)
nika sync                     # Sync to enabled editors
nika sync --status            # Show sync status
nika sync --enable claude-code

# Background Jobs (v0.27 spn fusion)
nika jobs submit workflow.yaml    # Run workflow in background
nika jobs list                    # List background jobs
nika jobs output <id> --follow    # Stream job output
nika jobs cancel <id>             # Cancel running job

# Backup & Restore (v0.27 spn fusion)
nika backup create            # Create unified backup
nika backup list              # List available backups
nika backup restore           # Restore from latest backup

# Setup (v0.27 spn fusion)
nika setup                    # Interactive onboarding wizard
nika setup nika               # Install Nika + LSP + Daemon
nika setup novanet            # Configure NovaNet + Neo4j

# Daemon (v0.27 spn fusion)
nika daemon start             # Start background daemon
nika daemon status            # Show daemon status
nika daemon stop              # Stop daemon

# Development
cd tools/nika
cargo test                    # Run 5,054 tests
cargo clippy -- -D warnings   # Lint
cargo fmt                     # Format
cargo install --path . --locked # Install binary

# ZSH shortcuts
nk                  → nika TUI
nk workflow.nika.yaml → nika run
nk v <file>         → nika validate
nk tl               → nika trace list
```

---

## Key Files

| Path | Purpose |
|------|---------|
| `tools/nika/CLAUDE.md` | Detailed tool context (architecture, verbs, ADRs) |
| `tools/nika/src/ast/action.rs` | 5 verbs definition |
| `tools/nika/src/ast/context.rs` | context: file loading (v0.14.3) |
| `tools/nika/src/ast/include.rs` | include: DAG fusion (v0.14.3) |
| `tools/nika/src/core/mod.rs` | Core types: providers, models, MCP aliases (v0.27) |
| `tools/nika/src/core/providers.rs` | KNOWN_PROVIDERS (6 LLM + 11 MCP + 1 Local = 18) (v0.27) |
| `tools/nika/src/core/models.rs` | KNOWN_MODELS (16+ curated models) (v0.27) |
| `tools/nika/src/core/mcp_aliases.rs` | MCP_ALIASES (48 aliases) (v0.27) |
| `tools/nika/src/core/mcp_config.rs` | MCP server configuration (v0.27) |
| `tools/nika/src/secrets/mod.rs` | Unified secrets management (v0.27) |
| `tools/nika/src/runtime/executor.rs` | Task dispatch |
| `tools/nika/src/runtime/rig_agent_loop.rs` | Agent execution (rig-core) |
| `tools/nika/schemas/nika-workflow.schema.json` | JSON Schema @0.9 for YAML validation |
| `docs/plans/` | MVP plans |
| `docs/research/` | Research documents |

---

## Integration with NovaNet

```yaml
# Nika workflow calling NovaNet MCP
workflow: generate-content
mcp:
  servers:
    novanet:
      command: node
      args: ["/path/to/novanet-mcp/dist/index.js"]
tasks:
  - id: get_entity
    invoke: novanet_generate
    params:
      entity: "qr-code"
      locale: "fr-FR"
```

**v0.8.0** - Studio DX improvements with Edit History, Session Persistence, Solarized Theme, Config System

### v0.8.0 Studio DX Features
1. **Edit History (Undo/Redo)** - Ctrl+Z/Ctrl+Y with intelligent 500ms coalescing
2. **Session Persistence** - Auto-save editor state to `.nika/sessions/` with auto-cleanup (max 50 sessions)
3. **Solarized Theme** - Light/Dark unified color palette based on Ethan Schoonover
4. **Config System** - `.nika/config.toml` for editor preferences, session settings, provider defaults
5. **File browser** - Navigate workflows in project
6. **YAML syntax highlighting** - Real-time editor feedback
7. **Schema validation** - Inline error diagnostics via miette
8. **Tab management** - Alt+← /→ navigation, Ctrl+W close
9. **Fuzzy file search** - Ctrl+P / `/` quick open
10. **Background task tracking** - MCP server status indicators
11. **Real-time trace streaming** - Live event visualization
12. **Command palette** - Quick commands and workflows

**v0.7.2** - Full streaming, VS Code-like TUI, spawn_agent, decompose:, lazy: bindings
