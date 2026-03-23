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
| NIKA-001 | Failed to parse workflow | Fix YAML syntax errors in the workflow file |
| NIKA-002 | Invalid schema version | Use `schema: nika/workflow@0.12` |
| NIKA-003 | Workflow file not found | Check the file path exists |
| NIKA-004 | Workflow validation failed | Read the detailed validation message |
| NIKA-005 | Schema validation failed | Fix schema structure issues |
| NIKA-006 | Could not determine home directory | Check home directory configuration |

### Schema Errors (010-019)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-013 | Schema file not found | Check the schema file path |

### DAG Errors (020-029)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-020 | Circular dependency | Remove cycle in `depends_on:` chain |
| NIKA-021 | Missing dep reference | `depends_on:` references task id that does not exist |
| NIKA-022 | Duplicate task ID | Rename one of the duplicate ids |

### Provider Errors (030-039)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-030 | Provider not supported | Use: claude, openai, mistral, groq, deepseek, gemini, xai, native, mock |
| NIKA-031 | Provider API error | Check API key and provider availability |
| NIKA-032 | Missing API key | Set env var (e.g., `OPENAI_API_KEY`) |
| NIKA-033 | Model not found | Check the model name is valid for the provider |

### Template/Binding Errors (040-049)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-041 | Template resolution error | Check `{{...}}` delimiters, variable names, and binding sources |

### Path/Security Errors (050-059)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-050 | Invalid path syntax | Check the path format |
| NIKA-053 | Command blocked by security | Command is on the security blocklist |

### With Block Errors (070-089)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-071 | Unknown alias in with: block | `with:` references an alias that does not exist |
| NIKA-080 | Unknown task in with: reference | Referenced task id does not exist |

### Execution Errors (090-099)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-090 | JSONPath unsupported | Check JSONPath expression syntax |
| NIKA-093 | IO error | Check file paths and permissions |
| NIKA-096 | Execution error | Catch-all runtime error; read the detailed message |

### MCP Errors (100-109)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-100 | MCP server not declared | Define server in `mcp:` block |
| NIKA-101 | MCP tool not found | Verify tool name exists on the MCP server |

### Agent Errors (110-119)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-110 | Agent error | Check agent configuration and prompt |
| NIKA-113 | Agent validation error | Fix agent configuration issues |

### Resilience Errors (120-129)

| Code | Error | Fix |
|------|-------|-----|
| NIKA-121 | Timeout / resilience error | Increase `timeout:` or `max_attempts:` |

### Structured Output Errors (300-309)

| Code | Error | Fix |
|------|-------|-----|
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
# BEFORE (NIKA-002)
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
# BEFORE (NIKA-071)
with:
  data: step1

# AFTER
with:
  data: $step1               # Add $ prefix
```

### Fix: Two verbs on one task

```yaml
# BEFORE (NIKA-004)
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
| NIKA-032 | Missing API key | Export the env var for the provider |
| NIKA-050 | Invalid path syntax | Check path format and command exists on PATH |
| NIKA-053 | Command blocked | Command is on the security blocklist |
| NIKA-093 | IO error | Check file paths and permissions |
| NIKA-096 | Execution error | Read the detailed error message |
| NIKA-121 | Timeout / resilience error | Increase `timeout:` or `max_attempts:` |
