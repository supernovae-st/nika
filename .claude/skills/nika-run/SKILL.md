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
nika check workflow.nika.yaml

# Step 2: Run (only after validation passes)
nika workflow.nika.yaml
```

## Direct Execution (Recommended)

```bash
# Run workflow directly (opens Monitor view)
nika workflow.nika.yaml

# Explicit run command (same as above)
nika run workflow.nika.yaml

# With provider override
nika run workflow.nika.yaml --provider openai

# With model override
nika run workflow.nika.yaml --model gpt-4-turbo
```

## Validation

```bash
# Basic validation
nika check workflow.nika.yaml

# Strict validation (includes MCP connection checks)
nika check workflow.nika.yaml --strict
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

- [ ] `nika check workflow.nika.yaml --strict` passes
- [ ] API keys set (`ANTHROPIC_API_KEY` or `OPENAI_API_KEY`)
- [ ] MCP servers running (if using `invoke:`)
- [ ] No NIKA-XXX error codes
- [ ] Workflow file ends with `.nika.yaml`

## Example Workflows

```bash
# List available examples
ls examples/

# Run basic example
nika run examples/basic.nika.yaml

# Run with NovaNet integration
nika run examples/uc1-multi-locale-page.nika.yaml

# Run with monitoring
nika examples/test-parallel-stress.nika.yaml
```

## Monitor View (TUI)

When you run a workflow directly, it opens a 4-panel Monitor view:

```
┌─────────────────────────────────────────────────────┐
│ Task Graph              │ Event Log                 │
├─────────────────────────────────────────────────────┤
│ Status Bar              │ Output / Details          │
└─────────────────────────────────────────────────────┘
```

**Panel controls:**
- `Tab` — Switch between panels
- `j/k` — Scroll up/down
- `q` — Quit
- `c` — Clear output

## Output & Traces

Workflow outputs are stored in:
- **DataStore:** In-memory during execution
- **Traces:** `.nika/traces/<trace_id>.ndjson` (if `NIKA_TRACE_DIR` set)

```bash
# View recent traces (after run)
nika traces list

# View specific trace
nika traces show <id>

# Export as JSON
nika traces export <id> --format json
```

## Troubleshooting

| Issue | Command |
|-------|---------|
| See debug logs | `RUST_LOG=debug nika run workflow.nika.yaml` |
| Check MCP | `curl localhost:3000/health` |
| Validate schema | `nika check workflow.nika.yaml --strict` |
| View trace | `nika traces show <id>` |

## Related Skills

- `/workflow-validate` — Validation details
- `/nika-diagnose` — Debug failures
- `/nika-arch` — Architecture overview
