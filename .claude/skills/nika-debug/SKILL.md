---
name: nika-debug
description: Debug Nika workflows with traces and logging
---

# Debug Nika Workflows

## Debug Levels

```bash
# Minimal (errors only)
cargo run -- run workflow.yaml

# Verbose (debug level)
RUST_LOG=debug cargo run -- run workflow.yaml

# Maximum (trace level)
RUST_LOG=trace cargo run -- run workflow.yaml

# Specific module
RUST_LOG=nika::mcp=debug cargo run -- run workflow.yaml
```

## Trace Commands

```bash
# List all traces
cargo run -- trace list

# Show trace details
cargo run -- trace show <trace_id>

# Export as JSON
cargo run -- trace export <trace_id> --format json > debug.json

# Export as YAML
cargo run -- trace export <trace_id> --format yaml > debug.yaml
```

## Trace Event Types

| Event | What It Shows |
|-------|---------------|
| `WorkflowStarted` | Workflow ID, hash, timestamp |
| `TaskStarted` | Task ID, action type |
| `InferStarted/Completed` | Provider, model, tokens |
| `McpToolCalled/Responded` | Tool name, params, response |
| `AgentTurnStarted/Completed` | Turn index, tools used |
| `TaskFailed` | Error code, reason |

## Debug Workflow Structure

```yaml
schema: nika/workflow@0.2
workflow: debug-example

tasks:
  - id: step1
    infer:
      prompt: "Test prompt"
      model: claude-3-haiku  # Use cheaper model for debug
    output:
      use.ctx: result

  - id: debug_check
    exec:
      command: "echo 'Result: {{use.result}}'"  # Print intermediate
    flow: [step1]
```

## MCP Debugging

```bash
# Check MCP server health
curl -s http://localhost:3000/health

# Test MCP tool directly
curl -X POST http://localhost:3000/tools/novanet_describe \
  -H "Content-Type: application/json" \
  -d '{"entity": "qr-code"}'

# Watch MCP logs
cd ../novanet-dev/tools/novanet-mcp
RUST_LOG=debug cargo run
```

## Common Debug Scenarios

### Scenario: Task silently produces empty output

```bash
# 1. Enable debug logging
RUST_LOG=debug cargo run -- run workflow.yaml

# 2. Look for InferCompleted or McpResponse events
cargo run -- trace show <id> | grep -E "(Completed|Response)"

# 3. Check if binding was captured
cargo run -- trace show <id> | grep "use.ctx"
```

### Scenario: Agent loops forever

```bash
# 1. Run with TUI to watch turns
cargo run -- tui workflow.yaml

# 2. Check agent turn count
cargo run -- trace show <id> | grep "AgentTurn"

# 3. Add turn limit
agent:
  goal: "..."
  max_turns: 5  # Limit iterations
```

### Scenario: MCP call fails

```bash
# 1. Check MCP server is running
pgrep -f novanet-mcp || echo "Not running"

# 2. Verify server config in workflow
grep -A5 "mcp:" workflow.yaml

# 3. Test tool independently
cargo run -- trace show <id> | grep "McpTool"
```

## TUI Debug Mode

```bash
cargo run -- tui workflow.yaml
```

| Key | Action |
|-----|--------|
| `Tab` | Switch panels |
| `j/k` | Scroll up/down |
| `q` | Quit |
| `?` | Help |

## Performance Profiling

```bash
# Time execution
time cargo run -- run workflow.yaml

# With flamegraph (requires cargo-flamegraph)
cargo flamegraph -- run workflow.yaml
```

## Related Skills

- `/nika-diagnose` — Systematic diagnosis
- `/nika-run` — Run workflows
- `/nika-arch` — Architecture overview
