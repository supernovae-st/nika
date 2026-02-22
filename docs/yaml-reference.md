# YAML Reference

Complete reference for Nika workflow syntax and the 5 action verbs.

## Workflow Structure

```yaml
# Required
schema: "nika/workflow@0.4"

# Optional defaults
workflow: workflow-name
description: "What this workflow does"
provider: claude                    # Default LLM provider
model: claude-sonnet-4-20250514     # Default model

# MCP server configuration (for invoke/agent)
mcp:
  server_name:
    command: "path/to/server"
    args: ["--flag"]
    env:
      KEY: "value"
    cwd: "/working/directory"

# Tasks (required)
tasks:
  - id: task_id
    # exactly ONE verb per task

# Explicit DAG edges (optional)
flows:
  - source: task_a
    target: task_b
```

## Schema Versions

| Version | Features |
|---------|----------|
| `@0.1` | `infer`, `exec`, `fetch` |
| `@0.2` | + `invoke`, `agent`, MCP config |
| `@0.3` | + `for_each` parallelism |
| `@0.4` | + `extended_thinking`, `thinking_budget` |

Always use the latest version unless you have a specific compatibility requirement.

---

## The 5 Action Verbs

Every task must have exactly one verb. Never combine verbs in a single task.

### 1. infer - LLM Text Generation

Generate text using a language model.

**Simple form:**

```yaml
- id: summarize
  infer: "Summarize this text: {{use.ctx}}"
```

**Extended form:**

```yaml
- id: summarize
  infer:
    prompt: "Summarize this text: {{use.ctx}}"
    provider: openai          # Override workflow provider
    model: gpt-4o             # Override workflow model
```

**Properties:**

| Property | Type | Required | Description |
|----------|------|----------|-------------|
| `prompt` | string | Yes | The prompt to send to the LLM |
| `provider` | string | No | Override the workflow's default provider |
| `model` | string | No | Override the workflow's default model |

---

### 2. exec - Shell Command Execution

Execute shell commands.

**Simple form:**

```yaml
- id: build
  exec: "npm run build"
```

**Extended form:**

```yaml
- id: build
  exec:
    command: "npm run build"
```

**Properties:**

| Property | Type | Required | Description |
|----------|------|----------|-------------|
| `command` | string | Yes | The shell command to execute |

**Output:** The task result contains stdout, stderr, and exit code.

---

### 3. fetch - HTTP Request

Make HTTP requests to external APIs.

```yaml
- id: get_data
  fetch:
    url: "https://api.example.com/data"
    method: GET
    headers:
      Authorization: "Bearer {{use.token}}"
    body: '{"key": "value"}'
```

**Properties:**

| Property | Type | Required | Description |
|----------|------|----------|-------------|
| `url` | string | Yes | The URL to request |
| `method` | string | No | HTTP method: GET, POST, PUT, DELETE, PATCH (default: GET) |
| `headers` | object | No | HTTP headers as key-value pairs |
| `body` | string | No | Request body (for POST/PUT/PATCH) |

**Output:** The response body as text or JSON.

---

### 4. invoke - MCP Tool Call

Call tools from MCP (Model Context Protocol) servers.

**Tool invocation:**

```yaml
- id: get_context
  invoke:
    server: novanet            # Server name from workflow mcp config
    tool: novanet_generate
    params:
      entity: "qr-code"
      locale: "{{use.locale}}"
```

**Resource read:**

```yaml
- id: read_schema
  invoke:
    server: novanet
    resource: "schema://nodes/Entity"
```

**Properties:**

| Property | Type | Required | Description |
|----------|------|----------|-------------|
| `server` | string | Yes | MCP server name (defined in workflow `mcp:` section) |
| `tool` | string | One of tool/resource | Tool name to invoke |
| `resource` | string | One of tool/resource | Resource URI to read |
| `params` | object | No | Parameters to pass to the tool |

---

### 5. agent - Multi-turn Agentic Loop

Execute an autonomous agent that can make multiple LLM calls and use tools.

```yaml
- id: research
  agent:
    prompt: "Research the topic and create a summary"
    system: "You are a research assistant..."
    provider: claude
    model: claude-sonnet-4-20250514
    mcp: [novanet, filesystem]
    max_turns: 10
    token_budget: 50000
    stop_conditions: ["DONE", "COMPLETE"]
    scope: full
    extended_thinking: true
    thinking_budget: 8192
```

**Properties:**

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `prompt` | string | Required | The agent's goal or task |
| `system` | string | None | System prompt for the agent |
| `provider` | string | Workflow default | LLM provider to use |
| `model` | string | Workflow default | Model to use |
| `mcp` | array | `[]` | List of MCP servers the agent can use |
| `max_turns` | integer | 10 | Maximum number of agent turns |
| `token_budget` | integer | None | Total token budget for the agent |
| `stop_conditions` | array | `[]` | Strings that trigger early stop |
| `scope` | string | "full" | Context scope: full, minimal, debug |
| `extended_thinking` | boolean | false | Enable extended thinking (Claude) |
| `thinking_budget` | integer | 4096 | Token budget for thinking |

---

## Parallel Execution (for_each)

Execute tasks in parallel over an array. Available in schema `@0.3` and later.

**Important:** Use FLAT format, not nested.

```yaml
# Correct - flat format
- id: generate_pages
  for_each: ["fr-FR", "en-US", "de-DE"]
  as: locale
  concurrency: 5
  fail_fast: true
  invoke:
    server: novanet
    tool: novanet_generate
    params:
      locale: "{{use.locale}}"

# Wrong - nested format (will fail validation)
- id: generate_pages
  for_each:
    items: ["fr-FR", "en-US", "de-DE"]   # WRONG!
    as: locale
```

**Using binding expressions:**

```yaml
- id: get_items
  infer: "Return a JSON array of 5 topics"
  output:
    format: json

- id: process_all
  for_each: "{{use.items}}"     # Reference previous task output
  as: topic
  use:
    items: get_items
  infer: "Write about {{use.topic}}"
```

**Properties:**

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `for_each` | array/string | Required | Literal array or binding expression |
| `as` | string | "item" | Loop variable name (access via `{{use.<as>}}`) |
| `concurrency` | integer | 1 | Maximum parallel executions |
| `fail_fast` | boolean | true | Stop all iterations on first error |

---

## Data Binding

### The use: Block

Wire data from previous tasks using explicit dependencies.

```yaml
- id: fetch_data
  invoke:
    server: api
    tool: get_users

- id: process_data
  use:
    users: fetch_data           # Alias: task_id
  infer: |
    Process these users:
    {{use.users}}
```

### Template Syntax

Access bound data in prompts and parameters:

```yaml
# Basic access
{{use.alias}}

# In strings
infer: "Process {{use.data}} now"

# In parameters
params:
  value: "{{use.computed}}"
```

### depends_on

Declare dependencies without data binding:

```yaml
- id: setup
  exec: "npm install"

- id: build
  depends_on: [setup]           # Wait for setup, no data passed
  exec: "npm run build"
```

---

## Output Configuration

Control task output format and validation.

```yaml
- id: structured_output
  output:
    format: json                 # text (default), json, or yaml
    schema: "./schemas/response.json"   # Optional JSON Schema validation
  infer: "Return a JSON object with name and age fields"
```

**Properties:**

| Property | Type | Description |
|----------|------|-------------|
| `format` | string | Output format: `text`, `json`, `yaml` |
| `schema` | string | Path to JSON Schema file for validation |

---

## MCP Configuration

Define MCP servers at the workflow level for use with `invoke` and `agent`.

```yaml
mcp:
  novanet:
    command: "cargo"
    args: ["run", "--manifest-path", "path/to/Cargo.toml"]
    env:
      NEO4J_URI: "bolt://localhost:7687"
    cwd: "/path/to/working/dir"

  filesystem:
    command: "npx"
    args: ["-y", "@modelcontextprotocol/server-filesystem", "/allowed/path"]
```

**Server properties:**

| Property | Type | Required | Description |
|----------|------|----------|-------------|
| `command` | string | Yes | Executable to run |
| `args` | array | No | Command-line arguments |
| `env` | object | No | Environment variables |
| `cwd` | string | No | Working directory |

---

## Complete Example

```yaml
schema: "nika/workflow@0.4"
workflow: content-pipeline
description: "Generate content for multiple locales"
provider: claude
model: claude-sonnet-4-20250514

mcp:
  content_api:
    command: "./mcp-server"
    args: ["--port", "3000"]

tasks:
  # 1. Fetch entity data
  - id: get_entity
    invoke:
      server: content_api
      tool: get_entity
      params:
        id: "article-123"

  # 2. Generate for each locale in parallel
  - id: generate_content
    for_each: ["en-US", "fr-FR", "de-DE", "es-ES"]
    as: locale
    concurrency: 4
    fail_fast: false
    use:
      entity: get_entity
    agent:
      prompt: |
        Generate native content for locale {{use.locale}}.
        Source: {{use.entity}}
      mcp: [content_api]
      max_turns: 5

  # 3. Aggregate results
  - id: aggregate
    use:
      content: generate_content
    infer: |
      Create a summary report of all generated content:
      {{use.content}}
    output:
      format: json

flows:
  - source: get_entity
    target: generate_content
  - source: generate_content
    target: aggregate
```

---

## Task ID Rules

Task IDs must follow these rules:

- Start with a lowercase letter
- Contain only lowercase letters, numbers, and underscores
- Match pattern: `^[a-z][a-z0-9_]*$`

```yaml
# Valid
- id: fetch_data
- id: step_1
- id: generate_page_v2

# Invalid
- id: 1_task        # Cannot start with number
- id: Task-Name     # No hyphens or uppercase
- id: task name     # No spaces
```

---

## Common Mistakes

### 1. Multiple verbs in one task

```yaml
# Wrong
- id: step1
  infer: "Generate"
  exec: "echo done"

# Correct - split into two tasks
- id: generate
  infer: "Generate"

- id: notify
  depends_on: [generate]
  exec: "echo done"
```

### 2. Missing verb

```yaml
# Wrong - no action verb
- id: step1
  use:
    ctx: prev_task

# Correct
- id: step1
  use:
    ctx: prev_task
  infer: "Process {{use.ctx}}"
```

### 3. Wrong binding syntax

```yaml
# Wrong
infer: "Use ${ctx}"
infer: "Use {ctx}"
infer: "Use {{ctx}}"

# Correct
infer: "Use {{use.ctx}}"
```

### 4. Nested for_each

```yaml
# Wrong
for_each:
  items: [1, 2, 3]
  concurrency: 5

# Correct
for_each: [1, 2, 3]
concurrency: 5
```

---

## Validation

Always validate workflows before running:

```bash
# Validate a workflow
nika check workflow.nika.yaml

# Validate all files in a directory
nika check examples/

# Run with verbose logging
RUST_LOG=debug nika workflow.nika.yaml
```

---

## Quick Reference

| Element | Syntax | Notes |
|---------|--------|-------|
| Schema | `schema: "nika/workflow@0.4"` | Required, first line |
| Task ID | `id: snake_case_name` | Must match `^[a-z][a-z0-9_]*$` |
| Infer | `infer: "prompt"` | LLM generation |
| Exec | `exec: "command"` | Shell execution |
| Fetch | `fetch: { url, method, headers, body }` | HTTP request |
| Invoke | `invoke: { server, tool, params }` | MCP tool call |
| Agent | `agent: { prompt, mcp, max_turns }` | Agentic loop |
| for_each | `for_each: [array]` + `as` + `concurrency` | FLAT format only |
| Binding | `use: { alias: task_id }` + `{{use.alias}}` | Data wiring |
| Output | `output: { format: json }` | Format control |
