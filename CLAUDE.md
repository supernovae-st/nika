# Nika

Cargo workspace for Nika — semantic YAML workflow engine for AI tasks.

## Auto-Imported Context

@README.md @CHANGELOG.md

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

**Current Version**: v0.7.1 — Full Streaming (6 providers) + RLM Enhancements (spawn_agent, decompose:, lazy:)
**Tests**: 1,842 passing | **Roadmap**: `ROADMAP.md` | **Changelog**: `CHANGELOG.md`
**Target Application**: QR Code AI (https://qrcode-ai.com)

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
│   │   ├── dag/         # DAG validation
│   │   ├── runtime/     # Execution engine
│   │   ├── mcp/         # MCP client (rmcp v0.16)
│   │   ├── event/       # NDJSON trace writer
│   │   ├── tui/         # Terminal UI (ratatui)
│   │   ├── binding/     # Data flow + lazy bindings
│   │   └── provider/    # rig-core v0.31 wrapper
│   ├── CLAUDE.md        # Tool-level detailed context
│   └── Cargo.toml       # v0.7.1
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
cargo test                    # Run 1,842 tests
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
| `tools/nika/src/runtime/executor.rs` | Task dispatch |
| `tools/nika/src/runtime/rig_agent_loop.rs` | Agent execution (rig-core) |
| `tools/nika/schemas/nika-workflow.schema.json` | JSON Schema for YAML validation |
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

**MCP tools available**: `novanet_query`, `novanet_generate`, `novanet_traverse`,
`novanet_describe`, `novanet_search`, `novanet_assemble`, `novanet_atoms`, `novanet_introspect`
