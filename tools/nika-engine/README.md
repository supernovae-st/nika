# nika-engine

Embeddable workflow execution engine for Nika.

## Purpose

The core runtime that executes `.nika.yaml` workflows. Can be used as a library — no CLI, no UI dependencies.

## Key Modules

| Module | Purpose |
|--------|---------|
| `ast/` | Phase 3: Analyzed → Runtime types (lowering) |
| `runtime/` | DAG execution (Runner, TaskExecutor, verb dispatch) |
| `dag/` | DAG validation, cycle detection, topological sort |
| `provider/` | LLM providers (rig-core cloud + mistral.rs native) |
| `binding/` | Template resolution, 27 pipe transforms, JSONPath |
| `builtin/` | 12 core tools + 24 media/fetch tools |
| `init/` | Scaffold generators (minimal, showcase) |
| `secrets/` | OS Keychain + daemon IPC |

## Usage

```rust
use nika_engine::{WorkflowRunner, Config};
```

## License

AGPL-3.0-or-later
