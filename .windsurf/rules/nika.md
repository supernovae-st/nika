---
trigger: glob
globs: "**/*.nika.yaml"
---

# Nika Workflow Rules

Nika is a semantic YAML workflow engine for AI tasks. Schema: `nika/workflow@0.12`.

## CRITICAL: Common Mistakes

| Wrong | Right |
|-------|-------|
| `timeout: 30000` (ms) | `timeout: 30` (seconds) |
| `data: other_task` | `data: $other_task` ($ prefix) |
| `{{data}}` | `{{with.data}}` (with. prefix) |
| `{{item}}` in for_each | `{{with.item}}` (with. prefix) |
| `retry: 3` | `retry: { max_attempts: 3, delay_ms: 2000 }` |
| `.yaml` extension | `.nika.yaml` extension |
| `shell: bash` | `shell: true` (boolean) |
| Missing `schema:` | `schema: "nika/workflow@0.12"` |
| `depends_on: task_id` | `depends_on: [task_id]` (array) |
| `tool: "server/tool"` | `tool: "server::tool"` (double colon) |
| `retry:` inside `invoke:` | `retry:` at task level |
| `body: {...}` JSON | `json: {...}` (auto-serialized) |
| `{{with.results.field}}` after for_each | `{{with.results[0].field}}` (array) |
| `thinking: true` at task level | `infer: { extended_thinking: true }` inside infer |
| `max_retries: 3` at task level | `retry: { max_attempts: 3 }` at task level |
| `model: haiku` inside `infer:` | `model: claude-haiku-4-5` at task level |

## Which Verb?

- Need LLM output? → `infer:`
- Need HTTP/API call? → `fetch:`
- Need shell command? → `exec:`
- Need MCP/builtin tool? → `invoke:`
- Need multi-turn agent? → `agent:`

## Workflow Skeleton

```yaml
schema: nika/workflow@0.12
workflow: my-workflow
provider: claude                     # or array: [groq, claude] for fallback
model: claude-sonnet-4-20250514
inputs:
  name: "world"
tasks:
  - id: hello
    infer: "Say hello to {{inputs.name}}"
```

## 5 Verbs

### infer: -- LLM generation
```yaml
- id: summarize
  infer:
    prompt: "Summarize: {{with.text}}"
    system: "Be concise"
    temperature: 0.7
    max_tokens: 1000
    extended_thinking: true          # Claude only
    thinking_budget: 10000
    response_format: json            # text | json | markdown
    content:                         # Vision/multimodal (v0.34+)
      - type: image
        source: "{{with.hash}}"      # CAS hash, NOT file path
        detail: high
      - type: text
        text: "Describe this"
    guardrails:
      - type: length
        min_words: 50
        on_failure: retry
```

### exec: -- Shell command
```yaml
- id: build
  exec:
    command: "npm run build"
    shell: true                      # Required for pipes/redirects
    cwd: ./frontend
    env: { NODE_ENV: production }
    timeout: 30                      # Seconds (NOT ms)
```

### fetch: -- HTTP request
```yaml
- id: get-data
  fetch:
    url: "https://api.example.com/data"
    method: POST
    headers: { Authorization: "Bearer {{with.token}}" }
    json: { query: "{{with.q}}" }    # Auto-serialized JSON body
    extract: markdown                # 9 modes: markdown|article|text|selector|metadata|links|jsonpath|feed|llm_txt
    response: full                   # full | binary | (default: raw body)
    timeout: 30
```

### invoke: -- MCP tool call
```yaml
- id: search
  invoke:
    tool: "novanet::novanet_search"  # server::tool (double colon)
    params: { query: "{{with.q}}" }
    timeout: 30
```

**30 builtin nika:* tools** (no MCP needed):
Always-on, Media, Opt-in, Runtime (nika:cost, nika:records, nika:dag_info, nika:task_status, nika:threads, nika:orchestrate)

### agent: -- Multi-turn loop
```yaml
- id: research
  agent:
    prompt: "Research {{with.topic}}"
    tools: [web_search, read_file]
    mcp: [novanet]
    max_turns: 20
    completion:
      mode: explicit                 # explicit | natural | pattern
    guardrails:
      - type: length
        max_words: 500
        on_failure: retry
    limits:
      max_cost_usd: 2.0
```

**Note**: `agent: "think"` (scalar) = preset reference, NOT the agent verb.

## Data Flow

```yaml
- id: consumer
  depends_on: [producer]
  with:
    data: $producer                  # $ prefix required
    clean: $producer | trim | upper  # Pipe transforms
    temp: $api.data.temp ?? 20       # JSONPath + fallback
    key: $env.API_KEY                # Environment variable
  infer: "Process: {{with.data}}"
```

Templates: `{{with.alias}}`, `{{inputs.name}}`, `{{with.item}}`, `{{context.readme}}`

## Transforms (38 pipe-chainable)

- **String**: `upper`, `lower`, `trim`, `trim_start`, `trim_end`, `length`, `to_string`
- **Array**: `first`, `last`, `first(N)`, `last(N)`, `flatten`, `reverse`, `sort`, `unique`, `compact`, `keys`, `values`
- **Numeric**: `to_number`, `round(N)`, `abs`, `ceil`, `floor`
- **Type**: `to_bool`, `to_json`, `parse_json`, `type_of`
- **Parametric**: `join(",")`, `split(",")`, `default("fallback")`
- **System**: `shell`

## Task-Level Fields

```yaml
- id: my-task
  provider: openai                   # Override (or array: [openai, claude])
  model: gpt-4o
  preset: think                      # From agents: block
  record: true                       # Output recording (v0.51+)
  context_budget: 50000              # Token budget for bindings (v0.51+)
  routing:                           # Provider routing (v0.50+)
    fallback: [openai, claude]
    strategy: cost                   # cost | latency | availability
  retry: { max_attempts: 3, delay_ms: 1000, backoff: 2.0 }
  structured:
    schema: { type: object, properties: { name: { type: string } } }
    max_retries: 3
    enable_repair: true
  artifact: { path: out.md, format: markdown }
```

## Structured Output (v0.35+)

5-layer defense: tool injection → extractor → JSON validation → retry → LLM repair

```yaml
structured:
  schema:
    type: object
    properties:
      name: { type: string }
    required: [name]
  enable_repair: true
  max_retries: 3
  repair_model: claude-haiku-4-5
```

## Agent Presets

```yaml
agents:
  think:
    system: "Deep reasoning assistant"
    provider: claude
    model: claude-sonnet-4-20250514
    temperature: 0.3
tasks:
  - id: plan
    preset: think                    # Inherits from agents: block
    infer: "Plan the architecture"
```

## Security

- API keys: env vars only. Never hardcode.
- `fetch:` blocks SSRF (private IPs).
- `exec:` blocklist: `rm -rf /`, `sudo`, `$()`, backticks (NIKA-053).
- Never commit `.nika/traces/` to git.

## Providers

`anthropic`/`claude`, `openai`/`gpt`, `mistral`, `groq`, `deepseek`/`deep-seek`, `gemini`/`google`, `xai`/`grok`, `native`/`local`, `mock`

Provider array for fallback: `provider: [groq, claude, openai]`
