# Nika

[![ARMADA](https://github.com/SuperNovae-studio/nika/actions/workflows/armada-checkpoints.yml/badge.svg)](https://github.com/SuperNovae-studio/nika/actions/workflows/armada-checkpoints.yml)
[![Version](https://img.shields.io/badge/version-0.14.3-blue?logo=rust&logoColor=white)](Cargo.toml)
[![Version Lock](https://img.shields.io/badge/0.x.x-forever-orange?logo=semver&logoColor=white)](../../docs/plans/2025-02-25-nika-fortress-design.md)
[![Tests](https://img.shields.io/badge/tests-3211%20passing-brightgreen)](src/)
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
cargo run -- run examples/v09-context-loading.nika.yaml

# Validate without executing
cargo run -- check examples/v03-agent-with-tools.nika.yaml

# Interactive TUI
cargo run -- tui
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

## v0.14.3 Features

- **context: Field (Schema @0.9)** - Load files at workflow start
  ```yaml
  schema: nika/workflow@0.9
  context:
    files:
      brand: ./context/brand.md
      config: ./context/settings.json
  tasks:
    - id: generate
      infer: "Use brand guidelines: {{context.files.brand}}"
  ```
- **include: DAG Fusion** - Merge external workflows with prefix namespacing
  ```yaml
  schema: nika/workflow@0.9
  include:
    - path: ./partials/setup.nika.yaml
      prefix: setup_
  tasks:
    - id: main
      infer: "Main logic"
      depends_on: [setup_init]
  ```
- **Schema @0.9** - Full support for context: and include: features
- **Path Traversal Security** - `validate_path_boundary()` prevents `../../../` attacks
- **3,211 tests passing** (path validation tests added)

## Features

### context: File Loading (v0.14+)

Load external files at workflow start:

```yaml
schema: nika/workflow@0.9
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
schema: nika/workflow@0.9
include:
  - path: ./partials/setup.nika.yaml
    prefix: setup_
  - path: ./partials/teardown.nika.yaml
    prefix: teardown_
tasks:
  - id: main_task
    infer: "Main workflow logic"
    depends_on: [setup_init]
flows:
  - source: main_task
    target: teardown_cleanup
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
      tool: novanet_generate
      params:
        entity: "qr-code"
        locale: "{{use.locale}}"
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
        Use novanet_describe and novanet_traverse.
        Say "DONE" when complete.
      mcp:
        - novanet
      max_turns: 8
      stop_conditions:
        - "DONE"
```

## Semantic Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM generation | `infer: "Summarize this"` |
| `exec:` | Shell command | `exec: { command: "echo hello" }` |
| `fetch:` | HTTP request | `fetch: { url: "https://..." }` |
| `invoke:` | MCP tool call | `invoke: { mcp: novanet, tool: novanet_generate }` |
| `agent:` | Autonomous loop | `agent: { prompt: "...", mcp: [...] }` |

## MCP Integration

Nika connects to MCP servers for tool calling:

```yaml
schema: "nika/workflow@0.9"
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
├── event/        # Observability (22 event types)
├── binding/      # Data flow ({{use.alias}}, {{context.files.*}})
└── tui/          # Terminal UI (6 views, 39 widgets)
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
cargo test                    # All 3,211 tests
cargo test mcp                # MCP tests
cargo test --features integration  # Real MCP tests
cargo test tui                # TUI widget tests
```

### Test Breakdown (v0.14.3)
- **3,211 tests passing** (path validation tests added)
- Zero clippy warnings
- Schema @0.9 validation in CI

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
