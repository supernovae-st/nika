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
│   ├── executor.rs   # Task dispatch (infer/exec/fetch/invoke)
│   ├── runner.rs     # Workflow orchestration
│   ├── agent_loop.rs # Legacy agentic execution (v0.2, deprecated)
│   └── rig_agent_loop.rs # NEW: rig-core AgentBuilder (v0.3.1)
├── mcp/              # MCP client (v0.2)
├── event/            # Event sourcing
│   ├── log.rs        # EventLog
│   └── trace.rs      # NDJSON writer
├── tui/              # Terminal UI (feature-gated)
├── binding/          # Data flow ({{use.alias}})
├── provider/         # LLM providers
├── resilience/       # Retry, circuit breaker, rate limiter (v0.2)
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
- `nika/workflow@0.3`: +for_each parallelism, rig-core integration

## rig-core Migration (v0.3.1)

Nika is migrating from custom providers to [rig-core](https://github.com/0xPlaygrounds/rig).

| Component | Status | Implementation |
|-----------|--------|----------------|
| `agent:` verb | ✅ Done | `RigAgentLoop` uses rig's AgentBuilder |
| `infer:` verb | ⏳ Pending | Still uses executor.rs + Provider trait |
| MCP tools | ✅ Done | `NikaMcpTool` implements rig's `ToolDyn` |

### Using RigAgentLoop (Recommended for agent:)

```rust
use nika::runtime::RigAgentLoop;
use nika::ast::AgentParams;
use nika::event::EventLog;

let params = AgentParams {
    prompt: "Generate a landing page".to_string(),
    mcp: vec!["novanet".to_string()],
    max_turns: Some(5),
    ..Default::default()
};
let agent = RigAgentLoop::new("task-1".into(), params, EventLog::new(), mcp_clients)?;
let result = agent.run_mock().await?;  // or run_claude() for real execution
```

### Deprecated Providers

These are deprecated and will be removed in v0.4:
- `ClaudeProvider` → Use `RigAgentLoop`
- `OpenAIProvider` → Use `RigAgentLoop`
- `provider::types` → Use rig-core types

## Resilience Patterns (v0.2)

Provider-level resilience for handling LLM API failures.

### Configuration

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

### Patterns

| Pattern | Purpose | Config | Tests |
|---------|---------|--------|-------|
| `retry` | Automatic retry with exponential backoff + jitter | `max_attempts`, `backoff`, `initial_delay_ms` | 21 |
| `circuit_breaker` | Fail-fast after repeated failures | `failure_threshold`, `reset_timeout_ms` | 12 |
| `rate_limiter` | Throttle requests to stay within API limits | `requests_per_minute` | 11 |

### Circuit Breaker States

```
┌────────┐  failure_threshold  ┌──────┐  reset_timeout  ┌──────────┐
│ Closed │ ─────────────────►  │ Open │ ─────────────►  │ HalfOpen │
└────────┘                     └──────┘                 └──────────┘
    ▲                                                        │
    │                     success                            │
    └────────────────────────────────────────────────────────┘
```

### Implementation

- `resilience/retry.rs` — Exponential backoff with jitter
- `resilience/circuit_breaker.rs` — State machine (Closed/Open/HalfOpen)
- `resilience/rate_limiter.rs` — Token bucket algorithm
- `resilience/metrics.rs` — Performance metrics collection

## for_each Parallelism (v0.3)

Parallel iteration over arrays with concurrency control.

### Configuration

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

### Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `items` | array/binding | required | Array to iterate over |
| `as` | string | required | Loop variable name |
| `concurrency` | integer | 1 | Max parallel executions |
| `fail_fast` | boolean | true | Stop all on first error |

### Implementation

Uses `tokio::spawn` with `JoinSet` for true concurrent execution:

```
concurrency=1:  [Task1] → [Task2] → [Task3]  (sequential)
concurrency=3:  [Task1]
                [Task2]  → (all in parallel)
                [Task3]
```

- Each iteration spawns as a separate tokio task
- `JoinSet` manages concurrent task lifecycle
- Results collected in original order
- `fail_fast: true` aborts remaining tasks on first error

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
