# Nika CLI — Claude Code Context

## Overview

Nika is a DAG workflow runner for AI tasks with MCP integration. It's the "body" of the spn-agi architecture, executing workflows that leverage NovaNet's knowledge graph "brain".

## Architecture

```
tools/nika/src/
├── main.rs           # CLI entry point
├── lib.rs            # Public API
├── error.rs          # NikaError with codes
├── ast/              # YAML → Rust structs
│   ├── workflow.rs   # Workflow, Task
│   ├── action.rs     # TaskAction (5 variants)
│   └── output.rs     # OutputSpec
├── dag/              # DAG validation
├── runtime/          # Execution engine
│   ├── executor.rs   # Task dispatch
│   ├── runner.rs     # Workflow orchestration
│   └── agent_loop.rs # Agentic execution (v0.2)
├── mcp/              # MCP client (v0.2)
├── event/            # Event sourcing
│   ├── log.rs        # EventLog
│   └── trace.rs      # NDJSON writer
├── tui/              # Terminal UI (feature-gated)
├── binding/          # Data flow ({{use.alias}})
├── provider/         # LLM providers
└── store/            # DataStore
```

## Key Concepts

- **Workflow:** YAML file with tasks and flows
- **Task:** Single unit of work (infer, exec, fetch, invoke, agent)
- **Flow:** Dependency edge between tasks
- **Verb:** Action type (infer:, exec:, fetch:, invoke:, agent:)
- **Binding:** Data passing via `use:` block and `{{use.alias}}`

## Schema Versions

- `nika/workflow@0.1`: infer, exec, fetch verbs
- `nika/workflow@0.2`: +invoke, +agent verbs, +mcp config

## Resilience Patterns (v0.2)

Provider-level resilience for handling LLM API failures:

```yaml
providers:
  claude:
    api_key: ${ANTHROPIC_API_KEY}
    resilience:
      retry:
        max_attempts: 3
        backoff: exponential
        initial_delay_ms: 1000
      circuit_breaker:
        failure_threshold: 5
        reset_timeout_ms: 30000
      rate_limiter:
        requests_per_minute: 60
```

| Pattern | Purpose | Config |
|---------|---------|--------|
| `retry` | Automatic retry with exponential backoff | `max_attempts`, `backoff`, `initial_delay_ms` |
| `circuit_breaker` | Fail-fast after repeated failures | `failure_threshold`, `reset_timeout_ms` |
| `rate_limiter` | Throttle requests to stay within limits | `requests_per_minute` |

## for_each Parallelism (v0.3)

Parallel iteration over arrays with concurrency control:

```yaml
tasks:
  - id: generate_pages
    for_each:
      items: $pages
      as: page
      concurrency: 5      # Max parallel tasks (default: 1)
      fail_fast: true     # Stop on first error (default: true)
    infer: "Generate content for {{page.title}}"
    use.ctx: page_content
```

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `items` | array/binding | required | Array to iterate over |
| `as` | string | required | Loop variable name |
| `concurrency` | integer | 1 | Max parallel executions |
| `fail_fast` | boolean | true | Stop all on first error |

## Commands

```bash
# Run workflow
cargo run -- run workflow.yaml

# Validate without executing
cargo run -- validate workflow.yaml

# Run with TUI (default feature)
cargo run -- tui workflow.yaml

# Run tests
cargo nextest run

# Run with coverage
cargo llvm-cov nextest
```

## Testing Strategy

- **Unit tests:** In-file `#[cfg(test)]` modules
- **Integration tests:** `tests/` directory
- **Snapshot tests:** insta for YAML/JSON outputs
- **Property tests:** proptest for parser fuzzing

## Error Codes

| Range | Category |
|-------|----------|
| NIKA-000-009 | Workflow errors |
| NIKA-010-019 | Task errors |
| NIKA-020-029 | DAG errors |
| NIKA-030-039 | Provider errors |
| NIKA-040-049 | Binding errors |
| NIKA-100-109 | MCP errors |
| NIKA-110-119 | Agent errors |

## Conventions

- **Imports:** Group by std, external, internal
- **Error handling:** Use `NikaError` with codes, not `anyhow`
- **Logging:** Use `tracing` macros (debug!, info!, warn!, error!)
- **Tests:** TDD - write failing test first
- **Commits:** Conventional commits with scope
