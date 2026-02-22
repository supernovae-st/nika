# Nika Workspace

DAG workflow runner for AI tasks with MCP integration.

## Crate Architecture

```
nika/
├── crates/
│   ├── nika-core/      # Core types, AST, DAG, events (516 tests)
│   ├── nika-mcp/       # MCP client integration (132 tests)
│   ├── nika-provider/  # LLM providers via rig-core (30 tests)
│   ├── nika-runtime/   # Execution engine (177 tests)
│   ├── nika-tui/       # Terminal UI (ratatui) (806 tests)
│   └── nika-cli/       # CLI binary + integration tests (662 tests)
├── examples/           # Workflow examples
├── Cargo.toml          # Workspace manifest
└── README.md           # User documentation
```

## Dependency Graph

```
nika-core ← nika-mcp ← nika-provider ← nika-runtime ← nika-tui ← nika-cli
    │           │           │               │
    │           │           │               └── Runner, Executor, AgentLoop
    │           │           └── RigProvider, NikaMcpTool
    │           └── McpClient, Protocol, Validation
    └── AST, DAG, Store, Event, Config, Error
```

## 5 Semantic Verbs

| Verb | Crate | Handler |
|------|-------|---------|
| `infer:` | nika-runtime | `execute_infer()` |
| `exec:` | nika-runtime | `execute_exec()` |
| `fetch:` | nika-runtime | `execute_fetch()` |
| `invoke:` | nika-runtime | `execute_invoke()` via nika-mcp |
| `agent:` | nika-runtime | `RigAgentLoop` via nika-provider |

## Commands

```bash
# Development
cargo build                    # Build all crates
cargo test                     # Run all tests (2,323 total)
cargo run                      # Run CLI (TUI mode)

# CLI usage
cargo run -- --help            # Show help
cargo run -- chat              # Chat mode
cargo run -- studio            # Studio mode
cargo run -- workflow.yaml     # Run workflow
cargo run -- check flow.yaml   # Validate
```

## Key Files

| File | Purpose |
|------|---------|
| `crates/nika-core/src/ast/` | Workflow, Task, TaskAction types |
| `crates/nika-core/src/error.rs` | NikaError (40+ variants) |
| `crates/nika-runtime/src/executor.rs` | Task execution dispatch |
| `crates/nika-runtime/src/rig_agent_loop.rs` | Agent execution |
| `crates/nika-provider/src/rig.rs` | LLM provider wrapper |
| `crates/nika-mcp/src/client.rs` | MCP client |

## Providers

6 LLM providers via rig-core v0.31:
- Claude (Anthropic)
- OpenAI
- Mistral
- Groq
- DeepSeek
- Ollama (local)

## Version

v0.7.1 - Full streaming, spawn_agent, decompose:, lazy: bindings
