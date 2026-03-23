---
trigger: glob
globs: "**/*.nika.yaml"
---

# Nika Workflow Rules

Nika is a semantic YAML workflow engine for AI tasks. Schema: `nika/workflow@0.12`.

## Workflow Skeleton

```yaml
schema: nika/workflow@0.12          # Required. Always @0.12
workflow: my-workflow                # Optional identifier
provider: anthropic                  # Default LLM provider
model: claude-sonnet-4-6          # Default model
inputs:                              # Parameters with defaults
  name: "world"
tasks: []                            # Required. Task list
```

## 5 Verbs (each task uses exactly one)

### infer: -- LLM generation
```yaml
- id: summarize
  infer:
    prompt: "Summarize: {{with.text}}"
    system: "Be concise"
    temperature: 0.7
    max_tokens: 1000
    extended_thinking: true          # Claude only
```

### exec: -- Shell command
```yaml
- id: build
  exec:
    command: "npm run build"
    shell: true                      # Required for pipes/redirects
    cwd: ./frontend
    env: { NODE_ENV: production }
    timeout: 30                      # seconds
```
Shorthand: `exec: "echo hello"`

### fetch: -- HTTP request
```yaml
- id: get-data
  fetch:
    url: "https://api.example.com/data"
    method: POST
    headers: { Authorization: "Bearer {{with.token}}" }
    json: { query: "{{with.q}}" }
    extract: markdown                # markdown|article|text|selector|metadata|links|feed|jsonpath|llm_txt
    response: full                   # full|binary|(default: raw body)
```

### invoke: -- MCP tool call
```yaml
- id: search
  invoke:
    tool: "server::tool_name"
    params: { query: "{{with.q}}" }
```
Builtins: `nika:import`, `nika:thumbnail`, `nika:chart`, `nika:metadata`, etc.

### agent: -- Multi-turn agent loop
```yaml
- id: research
  agent:
    prompt: "Research {{with.topic}}"
    tools: [web_search, read_file]
    max_turns: 20
    max_tokens: 4096
```

## Data Flow

```yaml
- id: consumer
  depends_on: [producer]             # Ordering dependency
  with:                              # Data binding ($ prefix required)
    data: $producer
    clean: $producer | trim | upper  # Pipe transforms
    temp: $api.data.temp ?? 20       # JSONPath + fallback
    key: $env.API_KEY                # Environment variable
  exec: "echo '{{with.data}}'"
```

## Templates

- `{{with.alias}}` -- Bound task output
- `{{inputs.name}}` -- Workflow input parameter
- `{{item}}` -- Current for_each item
- `{{context.file}}` -- Loaded context file

## Transforms (pipe-chained in with: bindings)

- **String**: `upper`, `lower`, `trim`, `trim_start`, `trim_end`
- **Collection**: `length`, `first`, `last`, `first(N)`, `last(N)`, `keys`, `values`, `flatten`, `reverse`, `sort`, `unique`, `compact`
- **Type**: `to_string`, `to_number`, `to_bool`, `to_json`, `parse_json`
- **Numeric**: `round(N)`, `abs`, `ceil`, `floor`
- **Utility**: `default(V)`, `type_of`, `join(S)`, `split(S)`

## Iteration

```yaml
- id: batch
  for_each:
    items: $list_task
    as: item
    concurrency: 5
    fail_fast: true
  exec: "echo '{{item}}'"
```

## Task-Level Fields (all verbs)

```yaml
- id: my-task                        # Required, unique
  description: "..."                 # Optional
  provider: openai                   # Override workflow default
  model: gpt-4o                      # Override workflow default
  with: { alias: $task_id }          # Data bindings
  depends_on: [task_a]               # Ordering-only deps
  for_each: { items: $src, as: x }   # Iteration
  retry: { max_attempts: 3, delay_ms: 1000, backoff: 2.0 }
  output: { format: json }           # text | json | yaml
  artifact: { path: out.json }       # Persist output to file
  structured: ./schema.json          # JSON schema enforcement
  log: debug                         # Log level override
```

## Example: Diamond DAG

```yaml
schema: nika/workflow@0.12
workflow: diamond
tasks:
  - id: source
    exec: "echo 'data'"
  - id: left
    depends_on: [source]
    with: { data: $source | trim | upper }
    infer: { prompt: "Analyze: {{with.data}}" }
  - id: right
    depends_on: [source]
    with: { data: $source | trim }
    infer: { prompt: "Summarize: {{with.data}}" }
  - id: merge
    depends_on: [left, right]
    with: { l: $left, r: $right }
    infer: { prompt: "Combine:\n{{with.l}}\n{{with.r}}" }
```

## Example: Fetch + Structured Output

```yaml
schema: nika/workflow@0.12
workflow: web-research
tasks:
  - id: scrape
    fetch:
      url: "https://example.com/article"
      extract: article
  - id: analyze
    depends_on: [scrape]
    with: { content: $scrape }
    infer:
      prompt: "Key takeaways:\n{{with.content}}"
      structured:
        schema:
          type: object
          properties:
            takeaways: { type: array, items: { type: string } }
          required: [takeaways]
```

## Common Mistakes

- Missing `$` prefix in `with:` bindings: use `$other_task`, not `other_task`
- Wrong extension: use `.nika.yaml`, never `.yaml` alone
- Missing `shell: true` for pipes/redirects in exec
- Using `depends_on:` for data flow -- use `with:` instead
- Schema must be `nika/workflow@0.12`, not just `0.12`
- Circular references in `with:` are not allowed

## LLM Providers

`anthropic`/`claude`, `openai`/`gpt`, `mistral`, `groq`, `deepseek`, `gemini`/`google`, `xai`/`grok`, `native`/`local`
