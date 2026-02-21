# Nika Skills Index

**Nika Version:** v0.5.2 | **5 verbs** | decompose + lazy bindings + spawn_agent | rig-core v0.31 | Extended thinking

## Available Skills

| Skill | Command | Purpose |
|-------|---------|---------|
| **nika-yaml** | `/nika-yaml` | **NEW** Complete YAML authoring guide (verbs, for_each, bindings) |
| **nika-arch** | `/nika-arch` | Architecture diagram and module structure |
| **nika-run** | `/nika-run` | Run workflows with validation |
| **nika-diagnose** | `/nika-diagnose` | Systematic workflow diagnosis |
| **nika-debug** | `/nika-debug` | Debug with traces and logging |
| **nika-binding** | `/nika-binding` | Data binding syntax reference |
| **workflow-validate** | `/workflow-validate` | Validate YAML syntax and DAG |
| **nika-spec** | `/nika-spec` | Workflow specification reference |

## v0.5.2 MVP 8 Features

Features added in v0.5 MVP 8:

| Feature | Documentation |
|---------|--------------|
| `decompose:` modifier | `CLAUDE.md` → "Decompose Modifier" |
| Lazy bindings | `CLAUDE.md` → "Lazy Bindings" |
| `spawn_agent` tool | `CLAUDE.md` → "Nested Agents" |
| Shorthand syntax | `CLAUDE.md` → "Verb Shorthand Syntax" |
| Event sourcing | `CLAUDE.md` → "Event Sourcing" |

## v0.3-0.4 Features (Reference)

Features from earlier versions:

| Feature | Documentation |
|---------|--------------|
| `invoke:` verb (MCP) | `CLAUDE.md` → "MCP Integration" |
| `agent:` verb (agentic loop) | `CLAUDE.md` → "rig-core Integration" |
| `for_each:` parallelism | `CLAUDE.md` → "for_each Parallelism" |
| Token tracking fix | `CLAUDE.md` → "v0.4.1 Changes" |

## Quick Reference

### CLI Commands (v0.5.2)

```bash
# TUI Home view (default)
cargo run -- nika

# TUI Chat mode
cargo run -- nika chat

# TUI Studio editor
cargo run -- nika studio

# Run workflow directly
cargo run -- nika workflow.nika.yaml

# Explicit run command
cargo run -- run workflow.nika.yaml

# Validate without executing
cargo run -- check workflow.nika.yaml

# Initialize project
cargo run -- init
```

### Debugging & Observation

```bash
# Verbose logging with run
RUST_LOG=debug cargo run -- run workflow.nika.yaml

# View event trace (NDJSON)
cat .nika/trace.ndjson | jq .

# Check syntax and DAG
cargo run -- check workflow.nika.yaml
```

## Skill Categories

### Development
- `/nika-arch` — Understand the codebase
- `/nika-binding` — Data flow between tasks

### Execution
- `/nika-run` — Run workflows
- `/workflow-validate` — Validate before running

### Debugging
- `/nika-diagnose` — Systematic checklist
- `/nika-debug` — Traces and logging

### Reference
- `/nika-spec` — Full workflow specification
