# nika-core

Core types and AST for Nika workflow engine.

## Overview

This crate provides the foundational types for Nika:

- **AST** - YAML to Rust type mapping (Workflow, Task, TaskAction)
- **DAG** - Dependency graph structure and validation
- **Binding** - Data binding and wiring resolution
- **Store** - Runtime state management (DataStore, TaskResult)
- **Event** - Event sourcing (EventLog, EventKind)
- **Config** - Configuration and environment handling
- **Error** - Error types with 40+ variants

## Architecture

```
nika-core/
├── ast/           # Domain model (Workflow, Task, 5 verbs)
│   ├── workflow.rs
│   ├── action.rs  # TaskAction: Infer, Exec, Fetch, Invoke, Agent
│   └── ...
├── dag/           # FlowGraph, validation
├── binding/       # WiringSpec, ResolvedBindings
├── store/         # DataStore, TaskResult, TaskStatus
├── event/         # EventLog (22 variants), TraceWriter
├── config.rs      # NikaConfig
├── error.rs       # NikaError (40+ variants)
└── util/          # Interner, JSONPath helpers
```

## 5 Semantic Verbs

| Verb | Purpose |
|------|---------|
| `infer:` | LLM text generation |
| `exec:` | Shell command execution |
| `fetch:` | HTTP requests |
| `invoke:` | MCP tool calls |
| `agent:` | Multi-turn agentic loops |

## Usage

```rust
use nika_core::{Workflow, NikaError, DataStore, EventLog};

// Parse workflow from YAML
let yaml = std::fs::read_to_string("workflow.nika.yaml")?;
let workflow: Workflow = serde_yaml::from_str(&yaml)?;

// Create runtime state
let store = DataStore::new();
let event_log = EventLog::new();
```

## License

MIT
