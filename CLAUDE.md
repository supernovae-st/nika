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

**Current Version**: v0.17.0 — Registry Optimizations + pkg: Includes + Security Fixes
**Tests**: 3,358+ passing | **Roadmap**: `ROADMAP.md` | **Changelog**: `CHANGELOG.md`
**Target Application**: QR Code AI (https://qrcode-ai.com)

**v0.17.0 Changes:**
- **pkg: Support for Workflow Includes** — `@workflows/name` in include blocks
- **Registry v0.17 Optimizations** — Arc-based DashMap caching, spn.lock support
- **Runtime Package Support** — @agents, @prompts, @skills packages
- **3 Critical Security Fixes** — Memory leak, file corruption, TOCTOU race condition

**v0.15.0 Features (inherited):**
- Security hardening: `exec:` defaults to `shell: false` (shlex parsing)
- Infer LLM control: `temperature`, `system`, `max_tokens` support
- Gemini provider (7th): `RigProvider::gemini()` with full streaming
- File tools (5 new): `nika:read`, `nika:write`, `nika:edit`, `nika:glob`, `nika:grep`
- 11 builtin tools total (6 core + 5 file)
- Zero clippy warnings, comprehensive test coverage

```
CRITICAL: 5 Semantic Verbs Only

infer:   → LLM generation (rig-core, 7 providers)
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
│   │   ├── dag/         # DAG validation
│   │   ├── runtime/     # Execution engine
│   │   ├── mcp/         # MCP client (rmcp v0.16)
│   │   ├── event/       # NDJSON trace writer
│   │   ├── tui/         # Terminal UI (ratatui)
│   │   ├── binding/     # Data flow + lazy bindings
│   │   └── provider/    # rig-core v0.31 wrapper
│   ├── CLAUDE.md        # Tool-level detailed context
│   └── Cargo.toml       # v0.17.0
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

# Development
cd tools/nika
cargo test                    # Run 4,369 tests
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
| `tools/nika/src/runtime/executor.rs` | Task dispatch |
| `tools/nika/src/runtime/rig_agent_loop.rs` | Agent execution (rig-core) |
| `tools/nika/src/core/security.rs` | Path traversal security (v0.14.3) |
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
