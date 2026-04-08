# Nika Workflow Syntax Reference

Nika is a semantic YAML workflow engine for AI tasks. Current schema: `nika/workflow@0.12`.

## Workflow Structure

```yaml
schema: nika/workflow@0.12        # Required. Always use @0.12
workflow: my-workflow              # Optional. Defaults to filename
description: "What this does"     # Optional
provider: anthropic               # Default LLM provider
model: claude-sonnet-4-20250514        # Default model

inputs:                            # Workflow parameters with defaults
  name: "world"
  count: 5

context:                           # File bindings loaded at boot
  files:
    readme: ./README.md

include:                           # Include partial workflows (tasks merged into DAG)
  - path: ./partials/setup.nika.yaml
    prefix: setup_

goal: "High-level objective"       # P-ORCHESTRATE: orchestrator intent
orchestrate:                       # Multi-workflow orchestration (v0.52+)
  workflows:
    - name: sub_flow
      path: ./sub.nika.yaml

mcp:                               # MCP server configuration
  server-name:
    command: npx
    args: ["-y", "@modelcontextprotocol/server-name"]
    env:
      API_KEY: "{{$env.API_KEY}}"

agents:                            # Reusable agent definitions
  researcher:
    system: "You are a research assistant"
    tools: [web_search, read_file]
    max_turns: 10

skills:                            # Prompt augmentation files
  writing: ./skills/writing-style.md

artifacts:                         # Output configuration
  dir: ./output

tasks: []                          # Required. Task list (see below)
```

## 5 Verbs

Every task uses exactly one verb.

### infer: -- LLM Generation

```yaml
- id: summarize
  model: claude-sonnet-4-20250514
  infer:
    prompt: "Summarize: {{with.text}}"
    system: "You are a concise summarizer"
    temperature: 0.7
    max_tokens: 1000
    response_format: json           # text | json | markdown
    extended_thinking: true         # Claude only
    thinking_budget: 10000          # tokens for thinking
    content:                        # Vision/multimodal (optional)
      - type: image
        source: "{{with.photo.media[0].hash}}"
        detail: high
      - type: text
        text: "Describe this image"
    guardrails:                     # Output validation
      - type: length
        max_words: 200
  structured:                       # JSON schema enforcement
    schema: ./schemas/response.json
    max_retries: 3
```

### exec: -- Shell Command

```yaml
- id: build
  exec:
    command: "npm run build"
    shell: true                     # Run through sh -c (default: false)
    cwd: ./frontend
    env:
      NODE_ENV: production
    timeout: 30                     # seconds
```

Shorthand: `exec: "echo hello"` (no shell by default).

### fetch: -- HTTP Request

```yaml
- id: get-data
  fetch:
    url: "https://api.example.com/data"
    method: POST                    # GET | POST | PUT | DELETE
    headers:
      Authorization: "Bearer {{with.token}}"
      Content-Type: application/json
    json:                           # Request body as JSON
      query: "{{with.search_term}}"
    body: "raw string body"         # Alternative to json:
    timeout: 30                     # seconds
    follow_redirects: true
    response: full                  # full | binary | (default: raw body)
    extract: markdown               # Post-processing mode
    selector: "div.content"         # CSS selector or JSONPath
```

Extract modes: `markdown`, `article`, `text`, `selector`, `metadata`, `links`, `feed`, `jsonpath`, `llm_txt`.

### invoke: -- MCP Tool Call

```yaml
- id: search
  invoke:
    tool: tool_name                  # Tool name
    mcp: server_name                 # MCP server name (from mcp: block)
    params:
      query: "{{with.search_term}}"
    timeout: 30                     # seconds
```

Shorthand: `invoke: "nika:dimensions"` (no params needed).

Builtin tools use `nika:` prefix: `nika:import`, `nika:thumbnail`, `nika:chart`, etc.

### agent: -- Multi-Turn Agent Loop

```yaml
- id: research
  agent:
    prompt: "Research {{with.topic}} and write a report"
    system: "You are a thorough researcher"
    tools: [web_search, read_file, write_file]
    max_turns: 20
    max_tokens: 4096
    temperature: 0.7
    model: claude-sonnet-4-20250514
    provider: anthropic
    mcp: [novanet]                  # MCP servers to expose
    tool_choice: auto               # auto | required | none
    scope: full                     # full | minimal | debug
    depth_limit: 3                  # Max spawn_agent recursion
    token_budget: 100000
    extended_thinking: true
    thinking_budget: 10000
    from: researcher                # Reference agents: definition
    skills: [writing]               # Inject skills into system prompt
    guardrails:
      - type: length
        max_words: 500
    completion:                     # When to stop
      on_tool: final_answer
    limits:                         # Cost control
      max_cost_usd: 1.0
```

## Task-Level Fields

Fields available on any task (all verbs):

```yaml
- id: my-task                       # Required. Unique identifier
  description: "What this task does" # Optional

  # Provider/model override
  provider: openai
  model: gpt-4o

  # Data flow
  with:                             # Bind data from other tasks
    data: $other_task               # Reference task output ($ prefix)
    clean: $source | trim | upper   # With pipe transforms
    temp: $api.data.temp ?? 20      # JSONPath + fallback
    key: $env.API_KEY               # Environment variable
  depends_on: [task_a, task_b]      # Ordering-only dependencies

  # Iteration
  for_each: "$list_task"            # Array to iterate over
  as: item                          # Loop variable (default: "item")
  concurrency: 5                    # Max parallel iterations
  fail_fast: true                   # Stop on first error

  # Output
  output:
    format: json                    # text | json | yaml
    schema: { type: object }        # JSON Schema validation
    max_retries: 2
  artifact:
    path: result.json
    format: json

  # Custom endpoint (v0.50+)
  base_url: "http://localhost:8000/v1"  # OpenAI-compatible endpoint

  # Provider fallback (v0.51+)
  # provider: [groq, claude, openai]   # At task or workflow level

  # Recording & context (v0.51+)
  record: true                      # Output recording to NDJSON
  context_budget: 50000             # Token budget for bindings

  # Resilience
  retry:
    max_attempts: 3
    delay_ms: 1000
    backoff: 2.0                    # Exponential multiplier

  # Structured output (JSON schema enforcement)
  structured:
    schema: ./schemas/output.json
    max_retries: 3

  # Logging
  log: debug                        # Override workflow log level
```

## Template Syntax

Templates use `{{...}}` delimiters inside any string value.

### References

| Pattern | Description |
|---------|-------------|
| `{{with.alias}}` | Bound task output |
| `{{with.alias.field}}` | Nested field access |
| `{{inputs.name}}` | Workflow input parameter |
| `{{context.readme}}` | Context file content |
| `{{with.item}}` | Current for_each item (default `as: item`) |
| `{{with.item.field}}` | Field on for_each item |

### Pipe Transforms

Use `|` to chain transforms in `with:` bindings or templates:

```yaml
with:
  clean: $source | trim | upper
  items: $data | sort | unique | first(5)
  name: $user | default("anonymous")
```

Available transforms:

| Category | Transforms |
|----------|------------|
| String | `upper`, `lower`, `trim`, `trim_start`, `trim_end` |
| Collection | `length`, `first`, `last`, `first(N)`, `last(N)`, `keys`, `values`, `flatten`, `reverse`, `sort`, `unique`, `compact` |
| Type | `to_string`, `to_number`, `to_bool`, `to_json`, `parse_json` |
| Numeric | `round(N)`, `abs`, `ceil`, `floor` |
| Utility | `default(V)`, `type_of`, `join(S)`, `split(S)`, `shell` |

### Fallback Operator

```yaml
with:
  temp: $weather.data.temp ?? 20    # Use 20 if path resolves to null
```

## Providers

### LLM Providers (7)

| Provider | Aliases | Env Var | Models |
|----------|---------|---------|--------|
| `anthropic` | `claude` | `ANTHROPIC_API_KEY` | Claude Opus, Sonnet, Haiku |
| `openai` | `gpt` | `OPENAI_API_KEY` | GPT-4o, GPT-4, o1 |
| `mistral` | -- | `MISTRAL_API_KEY` | Mistral Large, Medium, Small |
| `groq` | -- | `GROQ_API_KEY` | Llama, Mixtral (fast) |
| `deepseek` | `deep-seek` | `DEEPSEEK_API_KEY` | DeepSeek Chat, Coder |
| `gemini` | `google` | `GEMINI_API_KEY` | Gemini Pro, Flash, Ultra |
| `xai` | `grok` | `XAI_API_KEY` | Grok-3, Grok-4 |

### Local: `native` (alias: `local`) -- GGUF models via mistral.rs

## DAG Patterns

### Sequential Chain

```yaml
tasks:
  - id: fetch-data
    fetch: { url: "https://api.example.com/data" }
  - id: process
    depends_on: [fetch-data]
    with: { data: $fetch-data }
    exec: "echo '{{with.data}}'"
```

### Diamond DAG

```yaml
tasks:
  - id: source
    exec: "echo 'data'"
  - id: left
    depends_on: [source]
    with: { data: $source }
    infer: { prompt: "Left: {{with.data}}" }
  - id: right
    depends_on: [source]
    with: { data: $source }
    infer: { prompt: "Right: {{with.data}}" }
  - id: merge
    depends_on: [left, right]
    with: { l: $left, r: $right }
    infer: { prompt: "Merge: {{with.l}} + {{with.r}}" }
```

### Fan-Out with for_each

```yaml
tasks:
  - id: get-urls
    exec: "echo '[\"url1\",\"url2\",\"url3\"]'"
    output: { format: json }
  - id: scrape
    depends_on: [get-urls]
    for_each: "$get-urls"
    as: url
    concurrency: 3
    fetch:
      url: "{{with.url}}"
      extract: markdown
```

## Common Mistakes

| Wrong | Right |
|-------|-------|
| `schema: 0.12` | `schema: nika/workflow@0.12` |
| `with: { data: other_task }` | `with: { data: $other_task }` ($ prefix) |
| `workflow.yaml` | `workflow.nika.yaml` (.nika.yaml extension) |
| Using `depends_on` for data | Use `with:` for data flow, `depends_on:` for ordering only |
| `timeout: 30` (ambiguous) | `timeout: 30` means 30 seconds (not ms) |
| Circular `with:` references | DAG must be acyclic |
| `for_each: { items: $src, as: x }` | `for_each: "$src"` + `as: x` as flat siblings at task level |
| `invoke: tool_name` without MCP | Configure `mcp:` block or use `nika:` builtins |
| Missing `shell: true` for pipes | `exec: { command: "cmd1 | cmd2", shell: true }` |

## File Extension

Always use `.nika.yaml` for workflow files. Never `.yaml` or `.yml` alone.

## Validation

```bash
nika check workflow.nika.yaml      # Validate without running
nika run workflow.nika.yaml        # Execute
```
