---
name: nika-diagnose
description: Diagnose failing Nika workflows with systematic checklist
---

# Nika Workflow Diagnosis

## Quick Diagnosis Checklist

When a workflow fails, run through this checklist:

### 1. Validation First (30 sec)

```bash
cd nika-dev/tools/nika
cargo run -- validate <workflow.yaml>
```

| Error Code | Category | Common Fix |
|------------|----------|------------|
| NIKA-000-009 | Workflow | Check `schema:` version |
| NIKA-010-019 | Task | Check task ID uniqueness |
| NIKA-020-029 | DAG | Remove circular `flow:` deps |
| NIKA-100-109 | MCP | Declare server in `mcp:` block |
| NIKA-110-119 | Agent | Check `tools:` list validity |

### 2. MCP Connection

```bash
# Check if NovaNet MCP is running
curl -s http://localhost:3000/health || echo "MCP not running"

# Start NovaNet MCP
cd ../novanet-dev/tools/novanet-mcp
cargo run
```

### 3. Environment Variables

```bash
# Required for infer: and agent: verbs
echo $ANTHROPIC_API_KEY
echo $OPENAI_API_KEY

# Check they're set
[ -z "$ANTHROPIC_API_KEY" ] && echo "Missing ANTHROPIC_API_KEY"
```

### 4. Trace Analysis

```bash
# List recent traces
cargo run -- trace list

# Show specific trace
cargo run -- trace show <trace_id>

# Export for debugging
cargo run -- trace export <trace_id> --format json > debug.json
```

---

## Error Pattern Matching

### Pattern: "MCP server not connected"

```
Error: [NIKA-100] MCP server 'novanet' not connected
```

**Diagnosis:**
1. Is server declared in `mcp:` block?
2. Is the binary path correct?
3. Is the server process running?

**Fix:**
```yaml
mcp:
  novanet:
    command: cargo
    args: ["run", "--manifest-path", "../novanet-dev/tools/novanet-mcp/Cargo.toml"]
```

### Pattern: "Task dependency not found"

```
Error: [NIKA-021] Task 'step2' references unknown task 'step1x'
```

**Diagnosis:** Typo in `flow:` array.

**Fix:** Check task IDs match exactly.

### Pattern: "Provider returned empty response"

```
Error: [NIKA-031] Provider 'claude' returned empty response
```

**Diagnosis:**
1. API key valid?
2. Model exists? (e.g., `claude-3-opus` not `claude-opus`)
3. Rate limited?

**Fix:** Check provider config and API status.

### Pattern: "Binding not found"

```
Error: [NIKA-040] Binding 'entity_context' not found
```

**Diagnosis:** Task output not captured.

**Fix:**
```yaml
- id: step1
  infer: "..."
  output:
    use.ctx: entity_context  # This captures the output
```

### Pattern: "Agent exceeded max turns"

```
Error: [NIKA-110] Agent exceeded max_turns (10)
```

**Diagnosis:** Agent stuck in loop or goal too complex.

**Fix:**
1. Increase `max_turns`
2. Simplify goal
3. Add better tools

---

## Debug Mode

Run with verbose logging:

```bash
RUST_LOG=debug cargo run -- run workflow.yaml
```

Log levels:
- `error` — Only failures
- `warn` — Warnings + errors
- `info` — Progress updates
- `debug` — Detailed execution
- `trace` — Everything

---

## Common Workflow Issues

| Symptom | Likely Cause | Fix |
|---------|--------------|-----|
| Hangs forever | MCP server not responding | Check server process |
| Empty output | Binding not captured | Add `output.use.ctx:` |
| Wrong data | Template not resolved | Check `{{use.alias}}` syntax |
| DAG error | Circular dependency | Remove flow cycle |
| 401 error | Invalid API key | Check env vars |

---

## TUI Debugging

Run with TUI for real-time visibility:

```bash
cargo run -- tui workflow.yaml
```

| Panel | Shows |
|-------|-------|
| Progress | Task execution timeline |
| Graph | DAG visualization |
| Context | Current bindings |
| Reasoning | Agent thought process |

---

## Related Skills

- `/nika-arch` — Architecture overview
- `/nika-run` — Run workflows
- `/workflow-validate` — Validation details
