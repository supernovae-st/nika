# Nika Skills Index

**Nika Version:** v0.4.1 | **5 verbs** | for_each parallelism | rig-core v0.31 | Extended thinking

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

## v0.3 Features (Reference)

Features added in v0.2 and v0.3 are documented in:

| Feature | Documentation |
|---------|--------------|
| `invoke:` verb (MCP) | `README.md` → "invoke (MCP)" |
| `agent:` verb (agentic loop) | `README.md` → "agent (Agentic Loop)" |
| `for_each:` parallelism | `README.md` → "for_each Parallelism" |
| Resilience (retry, circuit breaker) | `CLAUDE.md` → "Resilience Patterns" |
| MCP configuration | `README.md` → "MCP Configuration" |

## Quick Reference

### Starting a Workflow

```bash
# Always validate first
cargo run -- validate workflow.yaml

# Then run
cargo run -- run workflow.yaml
```

### Debugging

```bash
# Verbose logging
RUST_LOG=debug cargo run -- run workflow.yaml

# View traces
cargo run -- trace list
cargo run -- trace show <id>
```

### TUI Mode

```bash
cargo run -- tui workflow.yaml
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
