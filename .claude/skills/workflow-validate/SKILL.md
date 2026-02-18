---
name: workflow-validate
description: Validate Nika YAML workflow syntax and DAG structure
---

# Nika Workflow Validation

## ALWAYS Validate Before Run

**Validation is MANDATORY before running any workflow.**

```bash
# Step 1: REQUIRED validation (30 sec)
cd tools/nika && cargo run -- validate workflow.yaml

# Step 2: Only run AFTER validation passes
cargo run -- run workflow.yaml
```

| Skip Validation? | Cost |
|------------------|------|
| ❌ Run validate (30 sec) | Fast, gives clear NIKA-XXX error codes |
| Runtime failure | Unclear errors during execution |
| Demo failure | Client sees crash instead of clean error |
| Debug loop | 10+ min if MCP server missing or DAG invalid |

**Why validation is non-negotiable:**
- **MCP Server Declaration**: `invoke:` tasks referencing undeclared servers fail at runtime with cryptic errors. Validation catches this with NIKA-100-109 codes.
- **DAG Cycles**: Circular dependencies cause infinite loops. Validation detects cycles before execution.
- **"Looks correct" is not a test**: Manual YAML review misses task ID typos, missing flow references, and verb syntax errors.

## Pre-Run Checklist

- [ ] `cargo run -- validate workflow.yaml` (no errors)
- [ ] All `mcp:` servers declared match `invoke:` server references
- [ ] No NIKA-XXX error codes in output

---

## Validate a Workflow

```bash
cd tools/nika && cargo run -- validate workflow.yaml
```

## Run a Workflow (only after validation)

```bash
cargo run -- run workflow.yaml
```

## Workflow Structure

```yaml
schema: nika/workflow@0.2
workflow: my-workflow
description: "What this workflow does"

env:
  MODEL: claude-3-opus

mcp:
  novanet:
    command: npx
    args: ["@novanet/mcp-server"]

tasks:
  - id: step1
    infer:
      prompt: "Generate something"
      model: $MODEL
    output:
      use.ctx: result1

  - id: step2
    invoke:
      tool: novanet_generate
      server: novanet
      params:
        entity: "qr-code"
        locale: "fr-FR"
    flow:
      - step1
    output:
      use.ctx: result2

  - id: step3
    agent:
      goal: "Complete the task"
      tools:
        - novanet_generate
        - novanet_describe
      max_turns: 10
    flow:
      - step2
```

## Semantic Verbs (v0.2)

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM generation | `infer: { prompt: "Summarize this" }` |
| `exec:` | Shell command | `exec: { command: "npm run build" }` |
| `fetch:` | HTTP request | `fetch: { url: "...", method: GET }` |
| `invoke:` | MCP tool call | `invoke: { tool: novanet_generate, server: novanet }` |
| `agent:` | Agentic loop | `agent: { goal: "...", tools: [...] }` |

## DAG Rules

- `flow: [task_ids]` — explicit dependencies
- `output.use.ctx: var_name` — output variable binding
- `{{use.alias}}` — reference previous output in templates
- Cycles are automatically detected and rejected

## Validation Checks

| Check | Description |
|-------|-------------|
| Schema version | Must be `nika/workflow@0.1` or `nika/workflow@0.2` |
| Task IDs | Must be unique and valid identifiers |
| DAG validation | No cycles allowed |
| Verb syntax | Each task has exactly one verb |
| Flow references | All `flow:` targets must exist |
| MCP servers | Referenced servers must be declared in `mcp:` |

## Error Codes

| Code | Category |
|------|----------|
| NIKA-000-009 | Workflow errors |
| NIKA-010-019 | Task errors |
| NIKA-020-029 | DAG errors |
| NIKA-030-039 | Provider errors |
| NIKA-040-049 | Binding errors |
| NIKA-100-109 | MCP errors |
| NIKA-110-119 | Agent errors |

## Example Workflows

```bash
# List examples
ls tools/nika/examples/

# Run a basic workflow
cargo run -- run examples/basic.nika.yaml

# Run with trace output
cargo run -- run examples/basic.nika.yaml --trace

# Run with TUI
cargo run -- tui examples/basic.nika.yaml
```

## Related Commands

| Command | Description |
|---------|-------------|
| `cargo run -- validate <file>` | Validate workflow syntax |
| `cargo run -- run <file>` | Execute workflow |
| `cargo run -- tui <file>` | Run with TUI interface |
| `cargo test` | Run test suite |
