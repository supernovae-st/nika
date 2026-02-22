---
name: nika-yaml
description: Comprehensive guide for writing perfect Nika workflow YAML files. Use when creating, editing, or debugging Nika workflows. Covers all 5 verbs, for_each parallelism, bindings, MCP configuration, and common mistakes to avoid.
user-invocable: true
---

# Nika YAML Workflow Authoring

> Perfect YAML workflows every time. No guessing, no debugging.

## File Naming

All Nika workflow files **MUST** use the `.nika.yaml` extension:

```
workflow.nika.yaml     ✅ Correct
workflow.yaml          ❌ Wrong (ambiguous)
workflow.nika          ❌ Wrong (not YAML)
```

## Schema Versions

```yaml
schema: nika/workflow@0.1  # Original (infer, exec, fetch only)
schema: nika/workflow@0.2  # +invoke, +agent verbs, +mcp config
schema: nika/workflow@0.3  # +for_each parallelism
schema: nika/workflow@0.4  # +extended_thinking, +thinking_budget
```

**Always use the latest version unless you have a specific reason not to.**

---

## Complete Workflow Structure

```yaml
# Required
schema: nika/workflow@0.4
tasks:
  - id: task_id
    # exactly ONE verb

# Optional
workflow: workflow-name
description: "What this workflow does"
provider: claude       # or openai, mock
model: claude-3-opus   # default model for all tasks
mcp:                   # MCP server configurations
  server_name:
    command: "path/to/server"
    args: ["--flag"]
    env: { "KEY": "value" }
flows:                 # Explicit DAG edges (optional)
  - source: task_a
    target: task_b
```

---

## The 5 Semantic Verbs

Every task MUST have exactly ONE verb. Never mix verbs.

### 1. infer: — LLM Text Generation

```yaml
# Simple form (string)
- id: summarize
  infer: "Summarize this text: {{use.ctx}}"

# Extended form (object)
- id: summarize
  infer:
    prompt: "Summarize this text: {{use.ctx}}"
    provider: openai     # Override workflow provider
    model: gpt-4-turbo   # Override workflow model
```

### 2. exec: — Shell Command Execution

```yaml
# Simple form (string)
- id: build
  exec: "npm run build"

# Extended form (object)
- id: build
  exec:
    command: "npm run build"
```

### 3. fetch: — HTTP Request

```yaml
- id: get_data
  fetch:
    url: "https://api.example.com/data"
    method: GET          # GET, POST, PUT, DELETE, PATCH
    headers:
      Authorization: "Bearer {{use.token}}"
    body: '{"key": "value"}'  # For POST/PUT/PATCH
```

### 4. invoke: — MCP Tool Call

```yaml
# Tool invocation
- id: get_context
  invoke:
    mcp: novanet           # Server name from workflow mcp config
    tool: novanet_generate
    params:
      entity: "qr-code"
      locale: "{{use.locale}}"

# Resource read
- id: read_schema
  invoke:
    mcp: novanet
    resource: "schema://nodes/Entity"
```

### 5. agent: — Multi-turn Agentic Loop

```yaml
- id: research
  agent:
    prompt: "Research the topic and create a summary"
    system: "You are a research assistant..."     # Optional
    provider: claude                               # Optional
    model: claude-3-opus                          # Optional
    mcp: [novanet, filesystem]                    # MCP servers the agent can use
    max_turns: 10                                  # Maximum agentic turns (default: 10)
    token_budget: 50000                           # Total token budget (optional)
    stop_conditions: ["DONE", "COMPLETE"]         # Early stop strings (optional)
    scope: full                                    # full, minimal, debug
    extended_thinking: true                        # Enable Claude extended thinking (v0.4+)
    thinking_budget: 8192                          # Thinking token budget (default: 4096)
```

---

## for_each Parallelism (v0.3+)

Execute tasks in parallel over an array.

### CRITICAL: Use FLAT format, NOT nested

```yaml
# ✅ CORRECT - Flat format
- id: generate_pages
  for_each: ["fr-FR", "en-US", "de-DE"]  # Array at task level
  as: locale                              # Loop variable (default: "item")
  concurrency: 5                          # Max parallel tasks (default: 1)
  fail_fast: true                         # Stop on first error (default: true)
  invoke:
    mcp: novanet
    tool: novanet_generate
    params:
      locale: "{{use.locale}}"

# ❌ WRONG - Nested format (will fail validation)
- id: generate_pages
  for_each:
    items: ["fr-FR", "en-US", "de-DE"]   # WRONG!
    as: locale
    concurrency: 5
```

### Binding Expressions for for_each

```yaml
# Using output from previous task
- id: get_locales
  invoke:
    mcp: novanet
    tool: novanet_describe
    params:
      describe: locales

- id: generate_all
  for_each: "{{use.locales}}"  # Binding expression
  as: locale
  concurrency: 3
  use:
    locales: get_locales       # Wire in the locales array
  infer: "Generate content for {{use.locale}}"

# Alternative binding syntax
- id: process_items
  for_each: "$items"           # $ prefix also works
```

### for_each Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `for_each` | array/string | required | Literal array or binding expression |
| `as` | string | `"item"` | Loop variable name (access via `{{use.<as>}}`) |
| `concurrency` | integer | `1` | Max parallel executions |
| `fail_fast` | boolean | `true` | Stop all iterations on first error |

---

## Data Binding (use: and {{use.alias}})

Wire data between tasks with explicit dependencies.

### Declaring Dependencies

```yaml
- id: get_entity
  invoke:
    mcp: novanet
    tool: novanet_describe
    params:
      entity: "qr-code"

- id: generate_page
  use:
    entity_data: get_entity    # Alias: task_id
  infer: |
    Generate a page using this context:
    {{use.entity_data}}
```

### Alternative: depends_on

```yaml
- id: step2
  depends_on: [step1]   # Explicit dependency without data wiring
  exec: "echo 'after step1'"
```

### Binding in for_each

```yaml
- id: process_batch
  for_each: ["a", "b", "c"]
  as: item
  use:
    shared_context: setup_task
  infer: "Process {{use.item}} with {{use.shared_context}}"
```

---

## MCP Configuration

Define MCP servers at workflow level, use in invoke: tasks.

```yaml
mcp:
  novanet:
    command: "cargo"
    args: ["run", "--manifest-path", "../novanet-dev/tools/novanet-mcp/Cargo.toml"]
    env:
      NEO4J_URI: "bolt://localhost:7687"
    cwd: "/path/to/working/dir"

  filesystem:
    command: "npx"
    args: ["-y", "@anthropic/filesystem-mcp"]
```

---

## Output Configuration

Control output format and validation.

```yaml
- id: structured_output
  output:
    format: json           # text (default) or json
    schema: "./schemas/response.json"  # JSON Schema validation
  infer: "Generate JSON response"
```

---

## Common Mistakes and Fixes

### 1. Nested for_each (WRONG)

```yaml
# ❌ WRONG
for_each:
  items: [1, 2, 3]
  concurrency: 5

# ✅ CORRECT
for_each: [1, 2, 3]
concurrency: 5
```

### 2. Missing verb

```yaml
# ❌ WRONG - no verb
- id: step1
  use:
    ctx: prev_task

# ✅ CORRECT - has verb
- id: step1
  use:
    ctx: prev_task
  infer: "Process {{use.ctx}}"
```

### 3. Multiple verbs

```yaml
# ❌ WRONG - two verbs
- id: step1
  infer: "Generate"
  exec: "echo done"

# ✅ CORRECT - split into two tasks
- id: generate
  infer: "Generate"

- id: notify
  depends_on: [generate]
  exec: "echo done"
```

### 4. Wrong binding syntax

```yaml
# ❌ WRONG
infer: "Use ${ctx}"
infer: "Use {ctx}"
infer: "Use {{ctx}}"

# ✅ CORRECT
infer: "Use {{use.ctx}}"
```

### 5. Invalid task ID format

```yaml
# ❌ WRONG - must be snake_case, start with letter
- id: 1_task
- id: Task-Name
- id: task name

# ✅ CORRECT
- id: task_1
- id: generate_page
- id: fetch_data
```

---

## Complete Example (v0.4)

```yaml
schema: nika/workflow@0.4
workflow: multi-locale-content
description: "Generate native content for multiple locales"
provider: claude
model: claude-sonnet-4-20250514

mcp:
  novanet:
    command: "cargo"
    args: ["run", "--manifest-path", "../novanet-dev/tools/novanet-mcp/Cargo.toml"]

tasks:
  # 1. Get available locales
  - id: get_locales
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        describe: locales

  # 2. Get entity context
  - id: get_entity
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        entity: "qr-code"
        forms: ["text", "title", "abbrev"]

  # 3. Generate content for each locale (parallel)
  - id: generate_content
    for_each: ["fr-FR", "en-US", "de-DE", "es-MX", "ja-JP"]
    as: locale
    concurrency: 3
    fail_fast: false
    use:
      entity: get_entity
    agent:
      prompt: |
        Generate native landing page content for locale {{use.locale}}.
        Entity context: {{use.entity}}
      mcp: [novanet]
      max_turns: 5
      extended_thinking: true
      thinking_budget: 4096

  # 4. Aggregate results
  - id: aggregate
    use:
      content: generate_content
    infer: |
      Create a summary of all generated content:
      {{use.content}}

flows:
  - source: [get_locales, get_entity]
    target: generate_content
  - source: generate_content
    target: aggregate
```

---

## Validation

Always validate before running:

```bash
# Validate single file
cargo run -- validate workflow.nika.yaml

# Validate directory
cargo run -- validate examples/

# Run with verbose logging
RUST_LOG=debug cargo run -- run workflow.nika.yaml
```

---

## Quick Reference Card

| Element | Syntax | Notes |
|---------|--------|-------|
| Schema | `schema: nika/workflow@0.4` | Required, first line |
| Task ID | `id: snake_case_name` | Must match `^[a-z][a-z0-9_]*$` |
| Infer | `infer: "prompt"` or `infer: { prompt: "..." }` | LLM generation |
| Exec | `exec: "command"` or `exec: { command: "..." }` | Shell execution |
| Fetch | `fetch: { url, method, headers, body }` | HTTP request |
| Invoke | `invoke: { mcp, tool/resource, params }` | MCP call |
| Agent | `agent: { prompt, mcp, max_turns, ... }` | Agentic loop |
| for_each | `for_each: [array]` + `as` + `concurrency` | FLAT format only |
| Binding | `use: { alias: task_id }` + `{{use.alias}}` | Data wiring |
| Output | `output: { format: json, schema: path }` | Format control |
