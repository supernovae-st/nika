---
description: Nika workflow syntax reference for .nika.yaml files
globs: "**/*.nika.yaml"
mode: code
---

# Nika Workflow Rules

Nika is a semantic YAML workflow engine for AI tasks. Schema: `nika/workflow@0.12`.

## Workflow Structure

```yaml
schema: nika/workflow@0.12          # Required. Always @0.12
workflow: my-workflow                # Optional identifier
provider: anthropic                  # Default LLM provider
model: claude-sonnet-4-20250514   # Default model
inputs:                              # Parameters with defaults
  name: "world"
mcp:                                 # MCP server configuration
  server-name:
    command: npx
    args: ["-y", "@some/mcp-server"]
tasks: []                            # Required. Task list
```

## 5 Verbs

Each task uses exactly one verb.

### infer: -- LLM Generation
```yaml
- id: summarize
  infer:
    prompt: "Summarize: {{with.text}}"
    system: "Be concise"
    temperature: 0.7
    max_tokens: 1000
    extended_thinking: true
```

### exec: -- Shell Command
```yaml
- id: build
  exec:
    command: "npm run build"
    shell: true
    cwd: ./frontend
    env: { NODE_ENV: production }
    timeout: 30
```
Shorthand: `exec: "echo hello"` (no shell by default)

### fetch: -- HTTP Request
```yaml
- id: get-data
  fetch:
    url: "https://api.example.com/data"
    method: POST
    headers: { Authorization: "Bearer {{with.token}}" }
    json: { query: "{{with.q}}" }
    extract: markdown
```
Extract modes: `markdown`, `article`, `text`, `selector`, `metadata`, `links`, `feed`, `jsonpath`, `llm_txt`

### invoke: -- MCP Tool Call
```yaml
- id: search
  invoke:
    tool: tool_name
    mcp: server_name
    params: { query: "{{with.q}}" }
```
Builtins use `nika:` prefix: `nika:import`, `nika:thumbnail`, `nika:chart`, etc.

### agent: -- Multi-Turn Agent Loop
```yaml
- id: research
  agent:
    prompt: "Research {{with.topic}}"
    tools: [web_search, read_file]
    max_turns: 20
    max_tokens: 4096
    mcp: [novanet]
```

## Data Flow

```yaml
- id: consumer
  depends_on: [producer]             # Ordering dependency (no data)
  with:                              # Data binding ($ prefix required)
    data: $producer
    clean: $producer | trim | upper  # Pipe transforms
    temp: $api.data.temp ?? 20       # JSONPath + fallback
    key: $env.API_KEY                # Environment variable
  exec: "echo '{{with.data}}'"
```

## Templates

| Pattern | Description |
|---------|-------------|
| `{{with.alias}}` | Bound task output |
| `{{inputs.name}}` | Workflow input |
| `{{item}}` | for_each current item |
| `{{context.file}}` | Context file content |

## Pipe Transforms

- **String**: `upper`, `lower`, `trim`, `trim_start`, `trim_end`
- **Collection**: `length`, `first`, `last`, `first(N)`, `last(N)`, `keys`, `values`, `flatten`, `reverse`, `sort`, `unique`, `compact`
- **Type**: `to_string`, `to_number`, `to_bool`, `to_json`, `parse_json`
- **Numeric**: `round(N)`, `abs`, `ceil`, `floor`
- **Utility**: `default(V)`, `type_of`, `join(S)`, `split(S)`

## Iteration

```yaml
- id: batch
  for_each: "$list_task"
  as: item
  concurrency: 5
  fail_fast: true
  exec: "echo '{{item}}'"
```

## Task-Level Fields (all verbs)

`id` (required), `description`, `provider`, `model`, `with`, `depends_on`, `for_each`, `retry`, `output`, `artifact`, `structured`, `log`, `concurrency`, `fail_fast`

## Example: Full Pipeline

```yaml
schema: nika/workflow@0.12
workflow: content-pipeline
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  url: "https://example.com/article"

tasks:
  - id: scrape
    fetch:
      url: "{{inputs.url}}"
      extract: article

  - id: analyze
    depends_on: [scrape]
    with: { content: $scrape }
    infer:
      prompt: "Extract key takeaways:\n{{with.content}}"
      structured:
        schema:
          type: object
          properties:
            takeaways: { type: array, items: { type: string } }
            sentiment: { type: string, enum: [positive, neutral, negative] }
          required: [takeaways, sentiment]

  - id: report
    depends_on: [analyze]
    with: { data: $analyze | to_json }
    exec: "echo '{{with.data}}'"
    artifact:
      path: analysis.json
      format: json
```

## Common Mistakes

- Missing `$` prefix: `with: { data: task }` must be `with: { data: $task }`
- Wrong extension: always `.nika.yaml`, never `.yaml`
- Missing `shell: true` for pipes/redirects in `exec:`
- `depends_on:` is ordering only; `with:` carries data
- Schema string: `nika/workflow@0.12`, not `0.12`
- `timeout:` is seconds, not milliseconds

## Providers

`anthropic`/`claude`, `openai`/`gpt`, `mistral`, `groq`, `deepseek`, `gemini`/`google`, `xai`/`grok`, `native`/`local`
