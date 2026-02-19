# Nika

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
cargo run -- run examples/v03-parallel-locales.yaml

# Validate without executing
cargo run -- validate examples/v03-agent-with-tools.yaml

# Interactive TUI
cargo run -- tui examples/invoke-novanet.yaml
```

## Installation

```bash
# From source
git clone https://github.com/supernovae-st/nika-dev
cd nika-dev/tools/nika
cargo build --release

# Binary location
./target/release/nika --help
```

## v0.3 Features

### Parallel for_each

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

### Resilience Patterns

Provider-level resilience:

```yaml
providers:
  claude:
    resilience:
      retry:
        max_attempts: 3
        backoff: exponential
      circuit_breaker:
        failure_threshold: 5
        reset_timeout_ms: 30000
      rate_limiter:
        requests_per_minute: 60
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
schema: "nika/workflow@0.2"
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
| `v03-parallel-locales.yaml` | Parallel generation for 5 locales |
| `v03-agent-with-tools.yaml` | Agent-driven competitive analysis |
| `v03-resilience-demo.yaml` | Retry, circuit breaker, rate limiting |
| `invoke-novanet.yaml` | Basic MCP invoke |
| `agent-novanet.yaml` | Agent with NovaNet tools |
| `uc1-*.yaml` to `uc10-*.yaml` | Production use cases |

## Architecture

```
src/
├── ast/          # YAML → Rust structs
├── dag/          # DAG validation
├── runtime/      # Execution engine
│   ├── executor.rs   # Task dispatch
│   ├── runner.rs     # Workflow orchestration
│   └── agent_loop.rs # Agentic execution
├── mcp/          # MCP client
├── provider/     # LLM providers (Claude, OpenAI)
├── event/        # Observability (16 event types)
├── resilience/   # Retry, circuit breaker, rate limiter
└── tui/          # Terminal UI
```

## Commands

```bash
# Workflow execution
nika run <workflow.yaml>      # Execute workflow
nika validate <workflow.yaml> # Validate syntax
nika tui <workflow.yaml>      # Interactive TUI

# Trace inspection
nika trace list               # List traces
nika trace show <id>          # Show trace events
nika trace export <id>        # Export to JSON
```

## Testing

```bash
cargo test                    # All tests
cargo test mcp                # MCP tests
cargo test --features integration  # Real MCP tests
```

## License

AGPL-3.0-or-later
