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
│   ├── executor.rs   # Task dispatch (infer/exec/fetch/invoke/agent)
│   ├── runner.rs     # Workflow orchestration
│   ├── output.rs     # Output format handling
│   └── rig_agent_loop.rs # ✅ rig-core AgentBuilder (v0.4+)
├── mcp/              # MCP client (rmcp v0.16)
├── event/            # Event sourcing
│   ├── log.rs        # EventLog (16 variants)
│   └── trace.rs      # NDJSON writer
├── tui/              # Terminal UI (feature-gated)
├── binding/          # Data flow ({{use.alias}})
├── provider/         # LLM providers (rig-core only)
│   └── rig.rs        # ✅ RigProvider + NikaMcpTool (rig-core v0.31)
└── store/            # DataStore
```

## Key Concepts

- **Workflow:** YAML file with tasks and flows
- **Task:** Single unit of work (infer, exec, fetch, invoke, agent)
- **Flow:** Dependency edge between tasks
- **Verb:** Action type (infer:, exec:, fetch:, invoke:, agent:)
- **Binding:** Data passing via `use:` block and `{{use.alias}}`

## File Conventions

### Workflow File Extension

All Nika workflow files **MUST** use the `.nika.yaml` extension:

```
workflow.nika.yaml     ✅ Correct
workflow.yaml          ❌ Wrong (ambiguous)
workflow.nika          ❌ Wrong (not YAML)
```

### JSON Schema Validation

Workflows are validated against `schemas/nika-workflow.schema.json`:

```bash
# Validate single file
cargo run -- validate workflow.nika.yaml

# Validate directory
cargo run -- validate examples/
```

### VS Code Integration

Schema auto-completion is enabled via `.vscode/settings.json`:

```json
{
  "yaml.schemas": {
    "./schemas/nika-workflow.schema.json": "*.nika.yaml"
  }
}
```

### yamllint

YAML linting uses `.yamllint.yaml` configuration:

```bash
yamllint -c .yamllint.yaml **/*.nika.yaml
```

## Schema Versions

- `nika/workflow@0.1`: infer, exec, fetch verbs
- `nika/workflow@0.2`: +invoke, +agent verbs, +mcp config
- `nika/workflow@0.3`: +for_each parallelism, rig-core integration

## rig-core Integration (v0.4)

Nika uses [rig-core](https://github.com/0xPlaygrounds/rig) for LLM providers.

| Component | Status | Implementation |
|-----------|--------|----------------|
| `agent:` verb | ✅ Done | `RigAgentLoop` uses rig's `AgentBuilder` |
| `infer:` verb | ✅ Done | `RigProvider.infer()` (rig-core v0.31) |
| MCP tools | ✅ Done | `NikaMcpTool` implements rig's `ToolDyn` |

### Using RigProvider (v0.3.1+)

```rust
use nika::provider::rig::RigProvider;
use rig::client::CompletionClient;  // Required trait import

// Create provider from environment
let provider = RigProvider::claude()?;  // or RigProvider::openai()?

// Simple text completion via rig-core
let result = provider.infer("Summarize this text", None).await?;
```

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
let result = agent.run_claude().await?;  // or run_mock() for testing
```

## v0.4 Changes (Removed Deprecated Code)

The following were **removed in v0.4**:

| Removed | Replacement | Notes |
|---------|-------------|-------|
| `ClaudeProvider` | `RigProvider::claude()` | Deleted `provider/claude.rs` |
| `OpenAIProvider` | `RigProvider::openai()` | Deleted `provider/openai.rs` |
| `provider::types` | `rig::completion::*` | Moved to minimal compat types in `mod.rs` |
| `AgentLoop` | `RigAgentLoop` | Deleted `runtime/agent_loop.rs` |
| `UseWiring` | `WiringSpec` | Alias removed |
| `from_use_wiring()` | `from_wiring_spec()` | Method removed |
| `resilience/` module | None | Entire module deleted (was never wired) |

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
