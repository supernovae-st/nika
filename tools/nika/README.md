# Nika — Developer Reference

[![ARMADA](https://github.com/SuperNovae-studio/nika/actions/workflows/armada-checkpoints.yml/badge.svg)](https://github.com/SuperNovae-studio/nika/actions/workflows/armada-checkpoints.yml)
[![Version](https://img.shields.io/badge/version-0.36.0-blue?logo=rust&logoColor=white)](Cargo.toml)
[![Version Lock](https://img.shields.io/badge/0.x.x-forever-orange?logo=semver&logoColor=white)](CHANGELOG.md)

Source code for the `nika` binary. For user-facing docs, see [root README](../../README.md).

## Build

```bash
cargo build --release           # Release build
cargo build                     # Debug build
cargo build --no-default-features  # Minimal (no TUI, no native, no media)
```

## Test

```bash
cargo test --lib                # 7,400+ unit tests (safe — no keychain)
cargo test --lib --features lsp # + 283 LSP tests
cargo clippy -- -D warnings     # Zero warnings policy
cargo fmt --check               # Format check
```

**WARNING:** `cargo test` (without `--lib`) runs contract tests that may trigger macOS Keychain popups. Always use `--lib` for safe testing.

## Source Tree

```
src/
├── main.rs              # CLI entry (clap)
├── lib.rs               # Public API
├── error.rs             # NikaError (NIKA-XXX codes)
├── config.rs            # Configuration types
├── core/                # Zero-dep definitions (providers, models, mcp_aliases)
├── ast/                 # Three-phase: Raw → Analyzed → Lower
│   ├── raw/             #   Phase 1: YAML → Raw AST (spans)
│   ├── analyzed/        #   Phase 2: Validated, resolved
│   ├── analyzer/        #   Validation + transformation
│   └── lower.rs         #   Phase 3: Analyzed → Runtime types
├── dag/                 # DAG validation + cycle detection
├── runtime/             # Execution engine
│   ├── runner.rs        #   Main workflow runner
│   ├── executor/        #   Task executor (5 verb dispatch)
│   ├── rig_agent_loop/  #   Agent loop (per-provider)
│   ├── builtin/         #   12 core + 26 media tools
│   │   └── media/       #   Media: import, thumbnail, chart, etc.
│   └── security.rs      #   Command blocklist + env validation
├── mcp/                 # MCP client (rmcp 0.16, pool, retry)
├── provider/            # 8 LLM providers (rig-core + mistral.rs)
├── binding/             # Data flow: templates, transforms, JSONPath
├── tools/               # File tools: read, write, edit, glob, grep
├── event/               # 41 event types + NDJSON tracing
├── media/               # CAS store (blake3 + zstd)
├── cli/                 # CLI subcommands
├── display/             # CLI rendering (summary, dag_render, colors)
├── init/                # nika init templates (6 tiers, 30 workflows)
├── tui/                 # Terminal UI (3 views: Studio, Command, Control)
├── lsp/                 # Embedded LSP (feature-gated)
├── secrets/             # Keyring + daemon IPC
├── registry/            # Package registry client
├── store/               # RunContext + TaskResult
├── io/                  # Atomic file I/O
├── source/              # Source spans + registry
└── util/                # Constants, fs helpers
```

## Error Codes

| Range | Category |
|:------|:---------|
| `000-009` | Workflow parsing |
| `010-019` | Schema validation |
| `020-029` | DAG (cycles, missing deps) |
| `030-039` | Provider errors |
| `040-049` | Template/binding |
| `050-059` | Security (path traversal, blocked commands) |
| `060-069` | Output validation |
| `100-109` | MCP (connection, tool errors) |
| `110-119` | Agent + Guardrails |
| `200-219` | Builtin tools |
| `251-259` | Media pipeline |
| `290-297` | Media tools |
| `300-309` | Structured output |

## Security Model

- `exec:` defaults to `shell: false` (no shell injection)
- Command blocklist (30+ patterns: `rm -rf`, `sudo`, reverse shells)
- Unicode NFKC normalization + zero-width character stripping
- API key stripping from child processes
- MCP env var validation (LD_PRELOAD blocked)
- SSRF URL scheme validation (http/https only)
- YAML bomb protection (serde-saphyr Budget limits)

## ARMADA Quality System

Every commit passes 10 stations:

```
Format → Lint → Tests → Coverage → Docs
Security → Schema → AI Review → Conventional → Version Lock
```

**Captain's Orders:** Nika will NEVER be version 1.0.0.

## License

AGPL-3.0-or-later
