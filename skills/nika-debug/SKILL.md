---
name: nika-debug
description: >-
  Debug failing Nika YAML workflows (.nika.yaml). Trace analysis, NIKA-XXX error
  code lookup, step-by-step diagnosis, binding resolution issues, provider
  failures, timeout debugging, DAG cycle detection, and runtime error analysis.
  Use when a .nika.yaml workflow fails, produces wrong output, or behaves
  unexpectedly with the Nika engine (schema nika/workflow@0.12).
---

# Debug Nika Workflows

Systematic diagnosis of failing `.nika.yaml` workflows.

## Debugging Process

### Step 1: Reproduce

```bash
nika check workflow.nika.yaml              # Static validation
nika run workflow.nika.yaml                # Execute and observe
nika run workflow.nika.yaml --trace t.ndjson  # With execution trace
```

### Step 2: Classify the Error

| Symptom | Category | Start Here |
|---------|----------|------------|
| `nika check` fails | Static error | Error code table below |
| `nika run` crashes | Runtime error | Check error code + trace |
| Task succeeds but wrong output | Silent failure | Inspect bindings |
| Task hangs | Timeout | Check `timeout:` value |
| Works sometimes, fails sometimes | Flaky | Check provider/network |
| Empty output file | Artifact issue | Check `artifact:` config |

### Step 3: Lookup Error Code

## Error Code Quick Reference

### Static Errors (found by `nika check`)

| Code | Meaning | Common Fix |
|------|---------|------------|
| NIKA-001 | Missing `tasks:` | Add tasks list |
| NIKA-003 | Missing task `id:` | Add id to every task |
| NIKA-004 | Duplicate task id | Rename duplicates |
| NIKA-005 | Missing verb | Add infer/exec/fetch/invoke/agent |
| NIKA-006 | Multiple verbs on task | Keep only one verb per task |
| NIKA-010 | Bad schema version | Use `nika/workflow@0.12` |
| NIKA-020 | Circular dependency | Break the cycle |
| NIKA-021 | Missing dep reference | Fix task id in depends_on |
| NIKA-040 | Bad template `{{...}}` | Check balanced delimiters |
| NIKA-070 | Bad with: reference | Add `$` prefix to task ref |
| NIKA-071 | Missing with: source | Referenced task doesn't exist |

### Runtime Errors (only at `nika run`)

| Code | Meaning | Common Fix |
|------|---------|------------|
| NIKA-031 | Missing API key | Export `PROVIDER_API_KEY` |
| NIKA-050 | Command not found | Check PATH, install dependency |
| NIKA-051 | Security violation | Command is blocklisted |
| NIKA-090 | IO error | Check file path, permissions |
| NIKA-091 | Timeout exceeded | Increase `timeout:` (in seconds) |
| NIKA-100 | MCP server not found | Check `mcp:` config |
| NIKA-101 | MCP tool not found | Verify tool name |
| NIKA-112 | Guardrail violation | Agent used a blocked tool |
| NIKA-120 | Max retries exceeded | Fix root cause or increase retries |
| NIKA-301 | Structured output mismatch | Simplify schema, add retry |

## Debugging Techniques

### 1. Trace Analysis

```bash
nika run workflow.nika.yaml --trace trace.ndjson
```

Read the trace to see exact execution order and outputs:

```bash
# See all events
cat trace.ndjson | jq .

# See failures only
cat trace.ndjson | jq 'select(.event == "task_failed")'

# See task outputs
cat trace.ndjson | jq 'select(.event == "task_completed") | {task: .task_id, output: .output}'
```

### 2. Isolate the Failing Task

Create a minimal workflow with just the failing task:

```yaml
schema: nika/workflow@0.12
tasks:
  - id: test
    exec: "echo 'test input'"    # Hardcode the input
```

### 3. Check Binding Resolution

If `{{with.x}}` produces wrong values:

```yaml
# Add a debug task to print the binding
- id: debug
  depends_on: [suspect_task]
  with:
    check: $suspect_task
  exec: "echo 'DEBUG: {{with.check}}'"
```

### 4. Provider Testing

```yaml
# Test with mock provider (no API key needed)
- id: test
  infer: "Say hello"
  provider: mock
```

### 5. Increase Logging

```yaml
log:
  level: debug                   # At workflow level
tasks:
  - id: verbose
    log:
      level: debug               # At task level
    exec: "echo test"
```

## Common Silent Failures

### Empty Artifact Files

The task "succeeded" but the artifact file is empty.

Causes:
1. Task output was actually empty string
2. `artifact.source:` references wrong alias
3. Binding `with:` not resolving correctly

Fix: Add debug exec to print the output before artifact write.

### Wrong Provider Used

The workflow specifies `provider: openai` but Claude responds.

Causes:
1. `provider:` not at the right YAML level
2. Task-level provider overridden by workflow default
3. Agent `provider:` must be inside `agent:` block

Fix: Check YAML indentation carefully.

### Binding Returns Raw Template String

`{{with.data}}` appears literally in output instead of resolved value.

Causes:
1. Missing `with:` block
2. Missing `depends_on:` (data not ready)
3. Wrong alias name
4. Typo in template variable

Fix: Verify the `with:` alias matches the template variable exactly.

### for_each Produces Wrong Count

Expected 3 iterations but got 1 (or 0).

Causes:
1. Source data is not a JSON array
2. `for_each: "$task"` but task output is a string, not array
3. Missing `as:` field

Fix: Ensure source outputs valid JSON array. Add `as:` field.

## Debugging Decision Tree

```
nika check fails?
  YES -> Read error code -> Fix -> Recheck
  NO  -> nika run fails?
    YES -> Read runtime error code -> Fix
    NO  -> Output correct?
      YES -> Done!
      NO  -> Silent failure:
        1. Add debug exec tasks to print bindings
        2. Check artifact config
        3. Check provider/model
        4. Run with --trace
        5. Isolate failing task
```

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| Not running `nika check` first | Always validate before running |
| Assuming compile = works | Runtime behavior can differ |
| Ignoring empty output | Empty string is a valid "success" |
| Wrong YAML indentation | Use 2-space indent consistently |
| `timeout:` in ms | It is in seconds |

## Validation

```bash
nika check workflow.nika.yaml    # Static check
nika run workflow.nika.yaml      # Runtime test
```
