# Nika

[![ARMADA](https://github.com/SuperNovae-studio/nika/actions/workflows/armada-checkpoints.yml/badge.svg)](https://github.com/SuperNovae-studio/nika/actions/workflows/armada-checkpoints.yml)
[![Version](https://img.shields.io/badge/version-0.27.0-blue?logo=rust&logoColor=white)](Cargo.toml)
[![Version Lock](https://img.shields.io/badge/0.x.x-forever-orange?logo=semver&logoColor=white)](CHANGELOG.md)
[![Tests](https://img.shields.io/badge/tests-6264%20passing-brightgreen)](src/)
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

## v0.27.0 Features

- **Language Improvements**
  - `exec.env` - Environment variable injection for exec tasks
  - `fetch.json` - Auto-serialize JSON body (sets Content-Type automatically)
  - `inputs.xxx` references - Access workflow inputs in `use:` blocks
  - `$inputs` binding - for_each accepts `$inputs.items` expressions
- **TUI Panels Module** - TaskListPanel, TaskBoxFlow, BrowserPanel
- **New Task Status** - `TaskStatus::Queued` and `TaskStatus::Skipped` variants
- **6,264 tests passing** (full regression coverage)

## v0.22.0+ Features

- **4-View TUI Architecture** - Studio, Runner, Chat, Settings
- **Two-Phase IR** - Raw AST → Analyzed AST pipeline
- **spn Daemon Integration** - Unified secret management

## v0.15.0 Features

- **Security Hardening: Shell-Free Execution** (BREAKING)
  - `exec:` now defaults to `shell: false` for security
  - Command parsing via shlex (no shell injection)
  - New error code: `NIKA-053 BlockedCommand`
  ```yaml
  # Default: shell-free (v0.15.0)
  - id: safe_exec
    exec: "echo 'Hello World'"  # Parsed via shlex

  # Opt-in shell mode for pipes/redirects
  - id: pipeline
    exec:
      command: "cat file.txt | grep pattern"
      shell: true
  ```
- **Infer LLM Control Parity** - `temperature`, `system`, `max_tokens` support
  ```yaml
  - id: creative
    infer:
      prompt: "Generate tagline"
      temperature: 0.9
      system: "You are a marketing expert"
      max_tokens: 100
  ```
- **Gemini Provider (7th provider)** - `RigProvider::gemini()`, full streaming
- **File Tools (5 new builtin)** - `nika:read`, `nika:write`, `nika:edit`, `nika:glob`, `nika:grep`
- **11 builtin tools total** (6 core + 5 file)

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

### Test Breakdown (v0.27.0)
- **6,264 tests passing**
- Zero clippy warnings
- Schema @0.11 validation in CI

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
