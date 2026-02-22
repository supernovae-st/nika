# nika-runtime

Runtime execution engine for Nika workflow engine.

## Overview

This crate provides the execution layer for Nika:

- **Runner** - DAG workflow orchestration
- **Executor** - Individual task execution (5 verbs + for_each)
- **RigAgentLoop** - Multi-turn agentic execution with rig-core
- **SpawnAgentTool** - Nested agent spawning (recursive agents)
- **Tools** - Built-in file tools (read, write, edit, grep, glob)

## Architecture

```
nika-runtime/
├── runner.rs         # Workflow orchestration, DAG execution
├── executor.rs       # Task dispatch (5 verbs + for_each)
├── rig_agent_loop.rs # Agentic execution (rig-core AgentBuilder)
├── spawn.rs          # SpawnAgentTool (rig::ToolDyn)
├── output.rs         # Output format handling, schema validation
├── test_utils.rs     # Test builders and fixtures
└── tools/            # Built-in agent tools
    ├── read.rs       # File reading with line numbers
    ├── write.rs      # File writing
    ├── edit.rs       # Search-replace editing
    ├── grep.rs       # Content search (ripgrep-style)
    ├── glob.rs       # File pattern matching
    ├── context.rs    # Tool context and permissions
    └── rig_adapter.rs # Rig ToolDyn integration
```

## 5 Semantic Verbs

| Verb | Handler | Description |
|------|---------|-------------|
| `infer:` | `execute_infer()` | LLM text generation |
| `exec:` | `execute_exec()` | Shell command execution |
| `fetch:` | `execute_fetch()` | HTTP requests |
| `invoke:` | `execute_invoke()` | MCP tool calls |
| `agent:` | `RigAgentLoop` | Multi-turn agentic loops |

## Usage

```rust
use nika_runtime::{Runner, TaskExecutor};
use nika_core::{Workflow, EventLog};

// Create runner from workflow
let runner = Runner::new(workflow);

// Execute workflow
let result = runner.run().await?;

// Or use executor directly
let executor = TaskExecutor::new("claude", None, mcp_configs, event_log);
let output = executor.execute(&task_id, &action, &bindings, &store).await?;
```

## Agent Tools

The runtime provides 5 built-in tools for agents:

- **read** - Read files with line numbers
- **write** - Write file contents
- **edit** - Search-replace editing
- **grep** - Search file contents
- **glob** - Find files by pattern

## License

MIT
