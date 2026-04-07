## 5 Verbs

| Verb | Purpose | Short form |
|------|---------|------------|
| `infer:` | LLM generation | `infer: "prompt"` |
| `exec:` | Shell command | `exec: "command"` |
| `fetch:` | HTTP request | `fetch: "url"` |
| `invoke:` | MCP / builtin tool | `invoke: "nika:tool"` |
| `agent:` | Multi-turn loop | *(full form only)* |

### infer: — LLM generation

```yaml
- id: generate
  provider: anthropic
  model: claude-sonnet-4-20250514
  infer:
    prompt: "Your prompt here"
    system: "You are a helpful assistant"
    temperature: 0.7
    max_tokens: 1000
```

### exec: — Shell command

```yaml
- id: build
  exec:
    command: "npm run build"
    shell: true
    cwd: "./frontend"
    timeout: 60
    env:
      NODE_ENV: production
```

### fetch: — HTTP request

```yaml
- id: scrape
  fetch:
    url: "https://example.com/article"
    method: GET
    headers:
      Authorization: "Bearer {{inputs.token}}"
    extract: markdown
    timeout: 30
```

Extract modes: `markdown`, `article`, `text`, `selector`, `metadata`, `links`, `jsonpath`, `feed`, `llm_txt`.

### invoke: — MCP / builtin tool

```yaml
- id: search
  invoke:
    tool: "novanet::novanet_search"
    params:
      query: "{{with.topic}}"
      limit: 10
```

Tool naming: `nika:tool_name` (63 builtins), `server::tool_name` (MCP — double colon `::`)

### agent: — Multi-turn loop

```yaml
- id: assistant
  agent:
    prompt: "Find and analyze {{inputs.topic}}"
    tools: [novanet::novanet_search]
    max_turns: 10
    completion:
      mode: explicit
```

### Complete workflow

```yaml
schema: "nika/workflow@0.12"
workflow: research-and-summarize
provider: anthropic
model: claude-sonnet-4-20250514
inputs:
  topic: "AI workflow engines"

tasks:
  - id: research
    infer:
      prompt: "Research {{inputs.topic}} thoroughly"
      temperature: 0.7
  - id: summarize
    depends_on: [research]
    with: { data: $research }
    infer:
      prompt: "Create a concise summary from: {{with.data}}"
      max_tokens: 500
```
