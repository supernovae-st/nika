---
name: nika-validate
description: >-
  Validate and fix Nika YAML workflow errors (.nika.yaml). Runs `nika check`,
  parses NIKA-XXX error codes, diagnoses issues, and auto-fixes common problems
  in nika/workflow@0.12 files. Use when a workflow fails validation, has YAML
  syntax errors, or produces unexpected NIKA error codes.
---

# Validate and Fix Nika Workflows

Diagnose and resolve errors in `.nika.yaml` workflow files.

## Quick Start

```bash
nika check workflow.nika.yaml        # Validate single file
nika check .                          # Validate all .nika.yaml in directory
```

## Error Code Reference

### Workflow Errors (000-009)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-001 | Missing tasks | Add `tasks:` key with task list |
| NIKA-002 | Empty tasks list | Add at least one task to `tasks:` |
| NIKA-003 | Missing task id | Add `id:` to the task |
| NIKA-004 | Duplicate task id | Rename one of the duplicate ids |
| NIKA-005 | Missing verb | Add one verb: `infer:`, `exec:`, `fetch:`, `invoke:`, or `agent:` |
| NIKA-006 | Multiple verbs | Keep only ONE verb per task |

### Schema Errors (010-019)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-010 | Invalid schema version | Use `schema: nika/workflow@0.12` |
| NIKA-011 | Missing schema | Add `schema: nika/workflow@0.12` as first line |

### DAG Errors (020-029)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-020 | Circular dependency | Remove cycle in `depends_on:` chain |
| NIKA-021 | Missing dep reference | `depends_on:` references task id that does not exist |
| NIKA-022 | Self-dependency | Task cannot depend on itself |

### Provider Errors (030-039)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-030 | Unknown provider | Use: claude, openai, mistral, groq, deepseek, gemini, xai, native, mock |
| NIKA-031 | Missing API key | Set env var (e.g., `OPENAI_API_KEY`) |

### Template/Binding Errors (040-049)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-040 | Bad template syntax | Check `{{...}}` delimiters are balanced |
| NIKA-041 | Undefined template var | Variable not in `with:`, `inputs:`, or `context:` |

### With Block Errors (070-089)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-070 | Bad with reference | `with:` value must start with `$` (e.g., `$task_id`) |
| NIKA-071 | Missing with source | Referenced task does not exist |

### MCP Errors (100-109)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-100 | MCP server not found | Define server in `mcp:` block or check it is running |
| NIKA-101 | MCP tool not found | Verify tool name exists on the MCP server |

### Agent Errors (110-119)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-110 | Agent missing prompt | Add `prompt:` inside `agent:` block |
| NIKA-112 | Guardrail violation | Agent attempted a blocked action |

### Structured Output Errors (300-309)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-300 | Invalid JSON schema | Fix the `structured.schema:` definition |
| NIKA-301 | Output validation failed | LLM output did not match schema; add `max_retries:` |

## Diagnostic Process

### Step 1: Run check

```bash
nika check workflow.nika.yaml
```

### Step 2: Read the error message

Nika errors include:
- Error code (NIKA-XXX)
- Description of what went wrong
- Location in the YAML file (task id, line number)

### Step 3: Apply fix based on error code

See the error code tables above.

### Step 4: Re-validate

```bash
nika check workflow.nika.yaml
```

## Common Fix Patterns

### Fix: Missing schema line

```yaml
# BEFORE (NIKA-011)
tasks:
  - id: hello
    exec: "echo hi"

# AFTER
schema: nika/workflow@0.12
tasks:
  - id: hello
    exec: "echo hi"
```

### Fix: Missing dependency declaration

```yaml
# BEFORE (NIKA-071 or runtime error)
- id: step2
  with:
    data: $step1
  exec: "echo {{with.data}}"

# AFTER
- id: step2
  depends_on: [step1]        # Add this
  with:
    data: $step1
  exec: "echo {{with.data}}"
```

### Fix: Missing $ prefix in with:

```yaml
# BEFORE (NIKA-070)
with:
  data: step1

# AFTER
with:
  data: $step1               # Add $ prefix
```

### Fix: Two verbs on one task

```yaml
# BEFORE (NIKA-006)
- id: both
  exec: "echo data"
  infer: "summarize"

# AFTER — split into two tasks
- id: get_data
  exec: "echo data"
- id: summarize
  depends_on: [get_data]
  with: { data: $get_data }
  infer: "summarize: {{with.data}}"
```

### Fix: Circular dependency

```yaml
# BEFORE (NIKA-020)
- id: a
  depends_on: [b]
- id: b
  depends_on: [a]

# AFTER — break the cycle
- id: a
  exec: "echo start"
- id: b
  depends_on: [a]
  exec: "echo next"
```

## Validation Checklist

- [ ] `nika check` passes with no errors
- [ ] Schema version is `nika/workflow@0.12`
- [ ] All task ids are unique
- [ ] Each task has exactly one verb
- [ ] All `with:` bindings use `$` prefix
- [ ] All `with:` sources exist as task ids
- [ ] All `depends_on:` references exist as task ids
- [ ] No circular dependencies
- [ ] `for_each:` always paired with `as:`
- [ ] Provider names are valid
- [ ] Template variables are defined

## Runtime Errors (only visible with `nika run`)

Some errors only appear at execution time:

| Code | Error | Fix |
|------|-------|-----|
| NIKA-031 | Missing API key | Export the env var for the provider |
| NIKA-050 | Command not found | Check `exec:` command exists on PATH |
| NIKA-090 | IO error | Check file paths and permissions |
| NIKA-091 | Execution timeout | Increase `timeout:` value (in seconds) |
| NIKA-120 | Max retries exceeded | Increase `max_attempts:` or fix upstream error |
