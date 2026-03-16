# Nika CLI — Developer Reference

Source code for the `nika` binary. See `nika/CLAUDE.md` for user-facing docs (verbs, providers, workflow syntax).

## Source Tree

```
src/
├── main.rs              # CLI entry (clap)
├── lib.rs               # Public API
├── error.rs             # NikaError (40+ variants, NIKA-XXX codes)
├── core/                # Zero-dep definitions
│   ├── providers.rs     # KNOWN_PROVIDERS (18: 6 LLM + 11 MCP + 1 Local)
│   ├── models.rs        # KNOWN_MODELS (15 curated for native inference)
│   └── mcp_aliases.rs   # MCP_ALIASES (48 server shortcuts)
├── ast/                 # Three-phase YAML → Runtime
│   ├── raw/             # Phase 1: YAML → Raw AST (spans preserved)
│   ├── analyzed/        # Phase 2: Validated, TaskId interning
│   └── lower.rs         # Phase 3: Analyzed → Runtime Workflow
├── dag/                 # DAG validation + cycle detection
├── runtime/             # Execution engine
│   ├── executor.rs      # Task dispatch (5 verbs + for_each)
│   ├── runner.rs        # Workflow orchestration
│   ├── rig_agent_loop.rs # rig-core AgentBuilder
│   └── builtin/         # 12 builtin tools (7 core + 5 file)
├── mcp/                 # MCP client (rmcp v0.16)
├── provider/            # LLM providers
│   ├── rig.rs           # RigProvider (6 cloud via rig-core v0.32)
│   └── native/          # NativeRuntime (mistral.rs for GGUF)
├── binding/             # Data flow ({{with.alias}})
├── event/               # NDJSON trace (34 event variants)
├── secrets/             # OS Keychain + daemon IPC
├── tui/                 # 4-view Terminal UI (ratatui)
└── lsp/                 # Language Server Protocol (feature-gated)
```

## Error Codes

| Range | Category |
|-------|----------|
| NIKA-000-009 | Workflow |
| NIKA-010-019 | Schema/validation |
| NIKA-020-029 | DAG |
| NIKA-030-039 | Provider |
| NIKA-040-049 | Template/binding |
| NIKA-050-059 | Path/task/security |
| NIKA-070-089 | With block + DAG validation |
| NIKA-090-099 | JSONPath/IO |
| NIKA-100-109 | MCP |
| NIKA-110-119 | Agent |
| NIKA-140-149 | AST analysis (Phase 2) |
| NIKA-200-219 | File tools + Builtin tools |
| NIKA-280-289 | Artifacts |
| NIKA-300-309 | Structured output |
| NIKA-400-429 | Daemon/IO/Sync |

## Key Files

| Path | Purpose |
|------|---------|
| `src/core/providers.rs` | KNOWN_PROVIDERS (18) |
| `src/core/models.rs` | KNOWN_MODELS (15) |
| `src/core/mcp_aliases.rs` | MCP_ALIASES (48) |
| `src/ast/lower.rs` | Three-phase lowering |
| `src/runtime/executor.rs` | Task dispatch |
| `src/runtime/rig_agent_loop.rs` | Agent execution |
| `src/tui/views/mod.rs` | TuiView enum (4 views) |
| `schemas/nika-workflow.schema.json` | JSON Schema |

## Testing

```bash
cargo test                    # 6,264 tests
cargo test --features lsp     # Include LSP tests
cargo clippy -- -D warnings   # Zero warnings policy
cargo fmt                     # Format
```

## Conventions

- **Errors:** `NikaError` with codes, not `anyhow`
- **Logging:** `tracing` macros (debug!, info!, warn!, error!)
- **Imports:** Group by std, external, internal
- **Tests:** TDD preferred, write failing test first
