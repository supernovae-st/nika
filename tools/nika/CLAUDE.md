# Nika CLI — Developer Reference

Source code for `nika` binary. See `nika/CLAUDE.md` for user-facing docs.

## Source Tree

```
src/
├── main.rs              # CLI entry (clap)
├── lib.rs               # Public API
├── error.rs             # NikaError (NIKA-XXX codes)
├── core/                # Zero-dep definitions (providers, models, mcp_aliases)
├── ast/                 # Three-phase: Raw → Analyzed → Lower
├── dag/                 # DAG validation + cycle detection
├── runtime/             # Execution engine (executor, runner, agent loop, builtins)
├── mcp/                 # MCP client (rmcp)
├── provider/            # LLM providers (rig-core cloud + mistral.rs native)
├── binding/             # Data flow ({{with.alias}})
├── event/               # NDJSON trace events
├── secrets/             # OS Keychain + daemon IPC
├── tui/                 # Terminal UI (ratatui)
└── lsp/                 # Language Server Protocol (feature-gated)
```

## Error Codes

| Range | Category |
|-------|----------|
| 000-009 | Workflow |
| 010-019 | Schema/validation |
| 020-029 | DAG |
| 030-039 | Provider |
| 040-049 | Template/binding |
| 050-059 | Path/task/security |
| 060-069 | Output (JSON/schema validation) |
| 070-089 | With block + DAG validation |
| 090-099 | JSONPath/IO |
| 100-109 | MCP |
| 110-119 | Agent |
| 120-129 | Resilience |
| 130-139 | TUI/Config |
| 140-151 | AST analysis (Phase 2) |
| 160-164 | Parse errors (Phase 1 parser) |
| 200-219 | File tools + Builtin tools |
| 280-289 | Artifacts |
| 250-259 | Context errors |
| 260-269 | Package URI errors |
| 270-279 | Skill errors |
| 280-289 | Artifacts |
| 300-309 | Structured output |

## Testing

```bash
cargo test                    # All tests
cargo test --features lsp     # Include LSP tests
cargo clippy -- -D warnings   # Zero warnings policy
```

## Conventions

- **Errors:** `NikaError` with NIKA-XXX codes, not `anyhow`
- **AST:** Always Raw -> Analyzed -> Lower. Never skip phases.
- **Providers:** `RigProvider::auto()` for auto-detect. `native` for local GGUF.
- **Extensions:** `.nika.yaml` for workflows
- **Logging:** `tracing` macros
- **Tests:** TDD preferred. `insta` for snapshots.

## Common Mistakes

| Mistake | Correct |
|---------|---------|
| Using `anyhow` for errors | Use `NikaError` with NIKA-XXX code |
| Direct Neo4j/Cypher access | Use MCP `invoke:` verb |
| Skipping AST analyzer phase | Always Raw -> Analyzed -> Lower |
| Hardcoding provider | Use `RigProvider::auto()` |
| `.yaml` extension | Use `.nika.yaml` for workflows |
