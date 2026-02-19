---
name: nika-run
description: Run Nika workflows with proper validation and setup
---

# Run Nika Workflows

## Standard Workflow

**ALWAYS validate before running.**

```bash
cd nika-dev/tools/nika

# Step 1: Validate (required)
cargo run -- validate workflow.yaml

# Step 2: Run (only after validation passes)
cargo run -- run workflow.yaml
```

## Run Options

```bash
# Basic run
cargo run -- run workflow.yaml

# With trace output
cargo run -- run workflow.yaml --trace

# With TUI (real-time monitoring)
cargo run -- tui workflow.yaml

# With custom provider
cargo run -- run workflow.yaml --provider openai

# With custom model
cargo run -- run workflow.yaml --model gpt-4-turbo
```

## Environment Setup

```bash
# Required for infer: and agent: verbs
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."

# Optional: trace directory
export NIKA_TRACE_DIR=".nika/traces"
```

## Pre-Run Checklist

- [ ] `cargo run -- validate workflow.yaml` passes
- [ ] API keys set (`ANTHROPIC_API_KEY` or `OPENAI_API_KEY`)
- [ ] MCP servers running (if using `invoke:`)
- [ ] No NIKA-XXX error codes

## Example Workflows

```bash
# List available examples
ls examples/

# Run basic example
cargo run -- run examples/basic.nika.yaml

# Run with NovaNet integration
cargo run -- run examples/uc1-multi-locale-page.nika.yaml
```

## Output

Workflow outputs are stored in:
- **DataStore:** In-memory during execution
- **Traces:** `.nika/traces/<trace_id>.ndjson`

```bash
# View recent traces
cargo run -- trace list

# Export specific trace
cargo run -- trace show <id>
```

## Troubleshooting

| Issue | Command |
|-------|---------|
| See errors | `RUST_LOG=debug cargo run -- run ...` |
| Check MCP | `curl localhost:3000/health` |
| View trace | `cargo run -- trace show <id>` |

## Related Skills

- `/workflow-validate` — Validation details
- `/nika-diagnose` — Debug failures
- `/nika-arch` — Architecture overview
