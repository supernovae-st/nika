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
│   ├── decompose.rs  # ✅ DecomposeSpec (v0.5 MVP 8)
│   └── output.rs     # OutputSpec
├── dag/              # DAG validation
├── runtime/          # Execution engine
│   ├── executor.rs   # Task dispatch + decompose expansion
│   ├── runner.rs     # Workflow orchestration
│   ├── output.rs     # Output format handling
│   ├── spawn.rs      # ✅ SpawnAgentTool (v0.5 MVP 8)
│   └── rig_agent_loop.rs # ✅ rig-core AgentBuilder (v0.4+)
├── mcp/              # MCP client (rmcp v0.16)
├── event/            # Event sourcing
│   ├── log.rs        # EventLog (17 variants)
│   └── trace.rs      # NDJSON writer
├── tui/              # Terminal UI (feature-gated)
├── binding/          # Data flow ({{use.alias}}) + lazy bindings
│   ├── entry.rs      # UseEntry with lazy flag (v0.5)
│   └── resolve.rs    # LazyBinding enum (v0.5)
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
- `nika/workflow@0.5`: +decompose, +lazy bindings, +spawn_agent (MVP 8)

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

## v0.4.1 Changes (Token Tracking Fix)

Token tracking in streaming mode (extended thinking) now works correctly:

| Before (v0.4.0) | After (v0.4.1) |
|-----------------|----------------|
| `input_tokens: 0` (always) | `input_tokens: <actual>` |
| `output_tokens: 0` (always) | `output_tokens: <actual>` |
| `total_tokens: 0` (always) | `total_tokens: <actual>` |

**Technical fix:** `run_claude_with_thinking()` now extracts token usage from `StreamedAssistantContent::Final` via rig's `GetTokenUsage` trait.

**Affected files:**
- `runtime/rig_agent_loop.rs` - Token extraction from streaming response
- `tests/thinking_capture_test.rs` - Integration tests for token capture

**AgentTurnMetadata** now contains accurate token counts when using extended thinking:

```rust
if let EventKind::AgentTurn { metadata: Some(metadata), .. } = event {
    println!("Input tokens: {}", metadata.input_tokens);   // Now > 0
    println!("Output tokens: {}", metadata.output_tokens); // Now > 0
    println!("Thinking: {:?}", metadata.thinking);         // Claude's reasoning
}
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

## v0.5 MVP 8: RLM Enhancements

### Lazy Bindings

Defer binding resolution until first access:

```yaml
use:
  # Eager (default) - resolved immediately
  eager_val: task1.result

  # Lazy (v0.5) - resolved on access
  lazy_val:
    path: future_task.result
    lazy: true
    default: "fallback"
```

### Decompose Modifier

Runtime DAG expansion via MCP traversal:

```yaml
tasks:
  - id: expand_entities
    decompose:
      strategy: semantic    # semantic | static | nested
      traverse: HAS_CHILD   # Arc to follow
      source: $entity       # Starting node
      max_items: 10         # Optional limit
    infer: "Generate for {{use.item}}"
```

### Nested Agents (spawn_agent) ✅ IMPLEMENTED

Internal tool for recursive agent spawning with depth protection.
Implements `rig::ToolDyn` for seamless integration with `RigAgentLoop`.

**Usage in workflow:**
```yaml
tasks:
  - id: orchestrator
    agent:
      prompt: "Decompose and delegate sub-tasks"
      depth_limit: 3  # Prevents infinite recursion (default: 3, max: 10)
```

**spawn_agent tool parameters:**
```json
{
  "task_id": "subtask-1",      // Unique ID for child task
  "prompt": "Generate header", // Child agent goal
  "context": {"entity": "qr"}, // Optional context data
  "max_turns": 5               // Optional max turns (default: 10)
}
```

**Implementation:**
- `SpawnAgentTool` in `runtime/spawn.rs` (implements `rig::ToolDyn`)
- Automatically added to agents when `depth_limit > current_depth`
- Child agents inherit MCP clients from parent
- Emits `AgentSpawned` event for observability
- 13 unit tests + 4 ToolDyn integration tests

## for_each Parallelism (v0.3)

Parallel iteration over arrays with concurrency control.

### Configuration (Flat Format)

```yaml
tasks:
  - id: generate_pages
    for_each: ["fr-FR", "en-US", "de-DE"]  # Array or binding expression
    as: page                                # Loop variable name
    concurrency: 5                          # Max parallel tasks (default: 1)
    fail_fast: true                         # Stop on first error (default: true)
    infer: "Generate content for {{use.page}}"
    use.ctx: page_content
```

Binding expressions are also supported:
```yaml
    for_each: "{{use.items}}"  # Resolved at runtime
    for_each: "$items"         # Alternative binding syntax
```

### Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `for_each` | array/binding | required | Array or binding expression |
| `as` | string | "item" | Loop variable name |
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
