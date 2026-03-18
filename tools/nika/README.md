# Nika

[![ARMADA](https://github.com/SuperNovae-studio/nika/actions/workflows/armada-checkpoints.yml/badge.svg)](https://github.com/SuperNovae-studio/nika/actions/workflows/armada-checkpoints.yml)
[![Version](https://img.shields.io/badge/version-0.30.8-blue?logo=rust&logoColor=white)](Cargo.toml)
[![Version Lock](https://img.shields.io/badge/0.x.x-forever-orange?logo=semver&logoColor=white)](CHANGELOG.md)
[![Tests](https://img.shields.io/badge/tests-5212%20passing-brightgreen)](src/)
[![License](https://img.shields.io/badge/license-AGPL--3.0-green)](../../LICENSE)

DAG workflow runner for AI tasks with MCP integration.

```
┌─────────────────────────────────────────────────────────────────────┐
│  YAML Workflow → DAG Validation → Parallel Execution → Results     │
│                                                                     │
│  Verbs: infer | exec | fetch | invoke | agent                      │
│                                                                     │
│  Features: for_each parallelism | MCP tools | TUI | Observability  │
└─────────────────────────────────────────────────────────────────────┘
```

## Quick Start

```bash
# Run a workflow
nika run examples/use-cases/blog-pipeline.nika.yaml

# Validate without executing
nika check examples/gates/feature/infer-basic.nika.yaml

# Interactive TUI
nika ui
```

## Installation

```bash
# From source
git clone https://github.com/supernovae-st/nika
cd nika/tools/nika
cargo build --release

# Binary location
./target/release/nika --help
```

## v0.30 Features

- **Structured Output** — DynamicSubmitTool + 5-layer architecture (v0.30.0)
- **LSP Intelligence** — Semantic tokens, Go to Definition, VS Code extension (v0.30.1)
- **Security Hardening** — Block LD_PRELOAD, strip API keys, AST hardening (v0.30.2)
- **A+++ Quality Pass** — CRLF compliance, NaN fixes, 34 property tests (v0.30.3)
- **Three-Phase AST** — YAML → Raw → Analyzed → Lower → Runtime (v0.28.1)
- **IndexedDag** — Vec-based adjacency list with Kahn's topological sort (v0.29.0)
- **use→with Migration** — Complete binding syntax migration to `with:` (v0.29.1)
- **Deep Audit** — 519 gate workflows, 15-agent swarm, 44+ bug fixes (v0.30.5)
- **5,219+ lib tests passing** | 34 proptests | Zero clippy warnings

## Architecture

- **Three-Phase AST** — YAML → Raw → Analyzed → Lower → Runtime
- **4-View TUI** — Studio, Runner, Chat, Settings
- **IndexedDag** — Vec-based adjacency with Kahn's topological sort
- **8 LLM Providers** — Claude, OpenAI, Mistral, Groq, DeepSeek, Gemini, xAI, Native

## Security

- `exec:` defaults to `shell: false` (no shell injection)
- Command blocklist (30+ patterns: `rm -rf`, `sudo`, reverse shells)
- Unicode NFKC normalization + zero-width character stripping
- API key stripping from child processes
- MCP env var validation (LD_PRELOAD blocked)

## Features

### context: File Loading (v0.14+)

Load external files at workflow start:

```yaml
schema: nika/workflow@0.12
context:
  files:
    brand: ./context/brand-guidelines.md
    data: ./context/config.json
    templates: ./context/*.yaml
  session: .nika/sessions/previous.json
tasks:
  - id: generate
    infer: |
      Using brand guidelines: {{context.files.brand}}
      With data: {{context.files.data}}
```

### include: DAG Fusion (v0.14+)

Merge tasks from external workflows:

```yaml
schema: nika/workflow@0.12
include:
  - path: ./partials/setup.nika.yaml
    prefix: setup_
  - path: ./partials/teardown.nika.yaml
    prefix: teardown_
tasks:
  - id: main_task
    infer: "Main workflow logic"
    depends_on: [setup_init]
    # teardown_cleanup runs after via depends_on in the included partial
```

### Parallel for_each (v0.3+)

Execute tasks in parallel with `for_each`:

```yaml
tasks:
  - id: generate_all
    for_each: ["fr-FR", "en-US", "es-ES", "de-DE", "ja-JP"]
    as: locale
    invoke:
      mcp: novanet
      tool: novanet_context
      params:
        focus_key: "entity:qr-code"
        locale: "{{with.locale}}"
        mode: block
```

Each iteration runs via `tokio::spawn` for true concurrency.

### Agent with Tools

Autonomous multi-turn execution with MCP tools:

```yaml
tasks:
  - id: analysis
    agent:
      prompt: |
        Analyze "qr-code" using NovaNet tools.
        Use novanet_describe and novanet_search.
        Say "DONE" when complete.
      mcp:
        - novanet
      max_turns: 8
```

## Semantic Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM generation | `infer: "Summarize this"` |
| `exec:` | Shell command | `exec: { command: "echo hello" }` |
| `fetch:` | HTTP request | `fetch: { url: "https://..." }` |
| `invoke:` | MCP tool call | `invoke: { mcp: novanet, tool: novanet_context }` |
| `agent:` | Autonomous loop | `agent: { prompt: "...", mcp: [...] }` |

## MCP Integration

Nika connects to MCP servers for tool calling:

```yaml
schema: "nika/workflow@0.12"
provider: claude

mcp:
  novanet:
    command: cargo
    args: [run, --manifest-path, path/to/novanet-mcp/Cargo.toml]
    env:
      NOVANET_MCP_NEO4J_URI: bolt://localhost:7687
```

## Examples

| Example | Description |
|---------|-------------|
| `v09-context-loading.nika.yaml` | context: field demo (v0.14+) |
| `v03-parallel-locales.nika.yaml` | Parallel generation for 5 locales |
| `v03-agent-with-tools.nika.yaml` | Agent-driven competitive analysis |
| `v05-lazy-bindings.nika.yaml` | Lazy bindings with defaults |
| `v05-spawn-agent.nika.yaml` | Nested agent spawning |
| `invoke-novanet.nika.yaml` | Basic MCP invoke |
| `agent-novanet.nika.yaml` | Agent with NovaNet tools |

## Architecture

```
src/
├── ast/          # YAML → Rust structs
├── dag/          # DAG validation
├── runtime/      # Execution engine
│   ├── executor.rs       # Task dispatch (5 verbs + for_each)
│   ├── runner.rs         # Workflow orchestration
│   ├── context_loader.rs # context: file loading
│   ├── include_loader.rs # include: DAG fusion
│   └── rig_agent_loop.rs # RigAgentLoop with rig::AgentBuilder
├── mcp/          # MCP client (rmcp v0.16)
├── provider/     # rig-core provider (RigProvider wrapper)
├── event/        # Observability (34 event types)
├── binding/      # Data flow ({{with.alias}}, {{context.files.*}})
└── tui/          # Terminal UI (4 views, 40+ widgets)
```

## Commands

```bash
# Workflow execution
nika run <workflow.yaml>      # Execute workflow
nika check <workflow.yaml>    # Validate syntax
nika <workflow.yaml>          # Direct execution (positional)

# Interactive modes
nika                          # Home view (browse workflows)
nika chat                     # Chat view (conversational agent)
nika studio                   # Studio view (YAML editor)

# Trace inspection
nika trace list               # List traces
nika trace show <id>          # Show trace events
nika trace export <id>        # Export to JSON
```

## Testing

```bash
cargo test                    # All 6,264 tests
cargo test mcp                # MCP tests
cargo test --features integration  # Real MCP tests
cargo test tui                # TUI widget tests
```

### Test Breakdown (v0.30.3)
- **5,204+ lib tests** + 34 property tests
- Zero clippy warnings
- Schema @0.12 validation in CI

## ARMADA Quality System

Every contribution passes through the 10-station ARMADA checkpoint:

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  🏴‍☠️ ARMADA — 10 QUALITY STATIONS                                             ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║   Station 1: 🔧 Format       | Station 6: 🔒 Security                          ║
║   Station 2: 📎 Lint         | Station 7: 📐 Schema Validation (v0.1-v0.9)     ║
║   Station 3: 🧪 Tests        | Station 8: 🧠 Claude AI                         ║
║   Station 4: 📊 Coverage     | Station 9: 📝 Conventional                      ║
║   Station 5: 📖 Docs         | Station 10: ⚓ Version Lock                     ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Captain's Orders:** Nika will NEVER be version 1.0.0. See [CONTRIBUTING.md](../../CONTRIBUTING.md).

## License

AGPL-3.0-or-later
