# 03 -- YAML Schema Reference

## Schema Version

Every Nika workflow file must begin with the schema declaration:

```yaml
schema: "nika/workflow@0.12"
```

The current and only supported schema version is `0.12`. Files must use the `.nika.yaml` extension.

---

## Top-Level Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `schema` | string | **Yes** | Schema version: `"nika/workflow@0.12"` |
| `workflow` | string | No | Workflow name (defaults to filename) |
| `description` | string | No | Human-readable description |
| `provider` | string | No | Default LLM provider (e.g., `"claude"`, `"openai"`) |
| `model` | string | No | Default model (e.g., `"claude-sonnet-4-6"`) |
| `mcp` | object | No | MCP server configuration |
| `context` | object | No | Context files loaded at workflow start |
| `imports` | array | No | External workflow/module imports |
| `inputs` | object | No | Input parameters with defaults |
| `agents` | object | No | Reusable agent definitions |
| `skills` | object | No | Workflow-level skills mapping |
| `artifacts` | object | No | Artifact output configuration |
| `log` | object | No | Logging configuration |
| `pkg` | object | No | Package includes |
| `tasks` | array | **Yes** | Task definitions (order matters) |

### Minimal Example

```yaml
schema: "nika/workflow@0.12"
workflow: minimal
tasks:
  - id: hello
    exec:
      command: echo "Hello, World!"
```

### Full Example

```yaml
schema: "nika/workflow@0.12"
workflow: research-pipeline
description: "Multi-step research pipeline with LLM analysis"
provider: claude
model: claude-sonnet-4-6

mcp:
  novanet:
    command: cargo
    args: ["run", "--", "mcp"]
    cwd: ../novanet

context:
  files:
    guidelines: ./context/style-guide.md
    template: ./context/report-template.md

inputs:
  topic:
    default: "AI safety"
  depth:
    default: 3

agents:
  researcher:
    system: "You are a thorough research analyst."
    max_turns: 10
    tools: ["nika:read", "nika:write", "nika:grep"]

skills:
  writing: ./skills/technical-writing.md
  analysis: pkg:@nika/skills@1.0/analysis.md

artifacts:
  dir: ./output
  max_size: 10485760

tasks:
  - id: gather
    fetch:
      url: "https://api.example.com/search?q={{inputs.topic}}"
      extract: markdown

  - id: analyze
    infer:
      prompt: "Analyze the following research data:\n\n{{with.data}}"
      system: "{{context.guidelines}}"
      temperature: 0.3
      max_tokens: 4000
    with:
      data: $gather
    output:
      format: json
      schema:
        type: object
        properties:
          summary: { type: string }
          key_findings: { type: array, items: { type: string } }
          confidence: { type: number }
        required: [summary, key_findings, confidence]

  - id: report
    agent:
      prompt: "Write a detailed report on: {{with.analysis.summary}}"
      from: researcher
      skills: [writing]
    with:
      analysis: $analyze
    artifact:
      path: "report-{{inputs.topic}}.md"
      format: text
```

---

## MCP Configuration

```yaml
mcp:
  <server_name>:
    command: <string>     # Command to spawn the server
    args: [<string>...]   # Command arguments
    env:                  # Environment variables
      KEY: value
    cwd: <path>           # Working directory (optional)
```

Example:

```yaml
mcp:
  novanet:
    command: cargo
    args: ["run", "--", "mcp"]
    cwd: ../novanet
  github:
    command: npx
    args: ["-y", "@modelcontextprotocol/server-github"]
    env:
      GITHUB_TOKEN: $GITHUB_TOKEN
```

---

## Context Configuration

Load files at workflow start for use in templates:

```yaml
context:
  files:
    <alias>: <path>
```

Access via `{{context.<alias>}}` in any template.

---

## Imports

Import external workflows for DAG fusion or code reuse:

```yaml
imports:
  - path: ./partials/setup.nika.yaml
    prefix: setup_
  - path: pkg:@nika/core@1.0/seo.nika.yaml
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | **Yes** | Local path or `pkg:` URI |
| `prefix` | string | No | Task ID prefix for namespace isolation |

---

## Inputs

Define workflow parameters with defaults:

```yaml
inputs:
  topic:
    default: "AI safety"
  max_results:
    default: 10
  verbose:
    default: false
```

Access via `{{inputs.<name>}}` in templates.

---

## Agent Definitions

Reusable agent configurations referenced by `from:`:

```yaml
agents:
  researcher:
    system: "You are a research analyst."
    max_turns: 15
    tools: ["nika:read", "nika:write", "nika:glob", "nika:grep"]
    temperature: 0.7
  coder:
    system: "You are an expert programmer."
    max_turns: 20
    tools: ["builtin"]
    extended_thinking: true
```

---

## Skills

Map skill aliases to file paths:

```yaml
skills:
  writing: ./skills/technical-writing.md
  analysis: pkg:@nika/skills@1.0/analysis.md
```

Skills are markdown files whose content is prepended to agent system prompts.

---

## Artifacts Configuration

```yaml
artifacts:
  dir: ./output        # Output directory
  max_size: 10485760   # Max artifact size in bytes (10 MB)
```

---

## Task Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | **Yes** | Unique task identifier |
| `description` | string | No | Human-readable description |
| `<verb>` | object | **Yes** | One of: `infer:`, `exec:`, `fetch:`, `invoke:`, `agent:` |
| `provider` | string | No | Task-level provider override |
| `model` | string | No | Task-level model override |
| `with` | object | No | Data bindings from other tasks |
| `depends_on` | array | No | Explicit ordering dependencies |
| `output` | object | No | Output format and schema validation |
| `for_each` | object | No | Iteration over arrays |
| `retry` | object | No | Retry configuration |
| `decompose` | object | No | Runtime DAG expansion |
| `concurrency` | integer | No | Max parallel iterations (with decompose) |
| `fail_fast` | boolean | No | Stop on first error (with decompose) |
| `structured` | object | No | Structured output JSON schema |
| `artifact` | object | No | Persist output to file |
| `log` | object | No | Task-level log configuration |

---

## Verb: `infer:`

LLM text generation.

```yaml
infer:
  prompt: <string>               # Required (unless content: present)
  system: <string>               # System prompt override
  temperature: <float>           # 0.0 - 2.0 (default: provider-specific)
  max_tokens: <integer>          # Max tokens to generate
  extended_thinking: <boolean>   # Enable extended thinking (Claude)
  thinking_budget: <integer>     # Thinking budget tokens
  response_format: <string>     # text, json, markdown
  content:                       # Multimodal content (vision)
    - type: image
      source: "{{with.photo.media[0].hash}}"
      detail: high
    - type: text
      text: "Describe this image"
  guardrails:                    # Output validation
    - type: regex
      pattern: "^\\{.*\\}$"
      message: "Must be JSON"
```

### Vision Support (content:)

When `content:` is present, `prompt:` becomes optional. If both are provided, the prompt is prepended as the first text part.

Content part types:

| Type | Fields | Description |
|------|--------|-------------|
| `image` | `source`, `detail` | CAS hash auto-resolved to base64 |
| `image_url` | `url`, `detail` | Direct image URL |
| `text` | `text` | Text content |

Supported providers for vision: Claude, OpenAI, Mistral, Groq, Gemini, xAI. Not supported: DeepSeek (returns VisionNotSupported error).

---

## Verb: `exec:`

Shell command execution.

```yaml
exec:
  command: <string>              # Required
  shell: <boolean>               # Use sh -c (default: false)
  cwd: <path>                   # Working directory
  env:                           # Environment variables
    KEY: value
  timeout: <integer>             # Timeout in seconds
```

When `shell: false` (default), commands are tokenized via `shlex` and executed directly. When `shell: true`, commands run through `sh -c` with additional blocklist checks (command substitution, backticks).

See [10-security-model.md](./10-security-model.md) for the full command blocklist.

---

## Verb: `fetch:`

HTTP requests with optional extraction.

```yaml
fetch:
  url: <string>                  # Required
  method: <string>               # GET, POST, PUT, DELETE, PATCH, HEAD
  headers:                       # HTTP headers
    Authorization: "Bearer {{env.TOKEN}}"
  body: <string>                 # Request body (POST/PUT)
  json: <object>                 # Request body as JSON
  timeout: <integer>             # Timeout in seconds
  follow_redirects: <boolean>    # Follow redirects (default: true)
  response: <string>             # Output mode: full, binary
  extract: <string>              # Extraction mode (see below)
  selector: <string>             # CSS selector or JSONPath (for extract)
```

### Extract Modes

| Mode | Description | Required Fields |
|------|-------------|----------------|
| `markdown` | Clean Markdown via htmd | -- |
| `article` | Main article content (Readability) | -- |
| `text` | Visible text, optionally filtered | `selector` (optional) |
| `selector` | Raw HTML of matching elements | `selector` (required) |
| `metadata` | OG, Twitter Cards, JSON-LD, SEO tags | -- |
| `links` | Rich link classification | -- |
| `jsonpath` | JSONPath query on JSON responses | `selector` (required, used as path) |
| `feed` | RSS/Atom/JSON Feed parsing | -- |
| `llm_txt` | AI-era content discovery | -- |

### Response Modes

| Mode | Description |
|------|-------------|
| (default) | Raw body text |
| `full` | JSON with status, headers, body, final URL |
| `binary` | Store in CAS, return hash for media pipeline |

---

## Verb: `invoke:`

MCP tool calls and builtin tools.

```yaml
# Simple form
invoke: nika:sleep
  params:
    duration_ms: 1000

# Full form
invoke:
  tool: "server_name::tool_name"
  params:
    key: value
  mcp: server_name               # Alternative server specification
  timeout: <integer>             # Timeout in seconds

# Resource form
invoke:
  resource: "resource://uri"
```

### Tool Name Resolution

- `nika:*` -- Builtin tools (no MCP server needed)
- `tool_name` -- Auto-resolved to first MCP server that has this tool
- `server::tool_name` -- Explicitly specify the MCP server

See [04-five-verbs-deep-dive.md](./04-five-verbs-deep-dive.md) for complete builtin tool documentation.

---

## Verb: `agent:`

Multi-turn autonomous agent loop.

```yaml
agent:
  prompt: <string>               # Required
  system: <string>               # System prompt (agent persona)
  from: <agent_name>             # Reference to agents: definition
  tools: [<string>...]           # Available tools
  skills: [<string>...]          # Skills to inject into system prompt
  max_turns: <integer>           # Max turns (1-100, default: 10)
  max_tokens: <integer>          # Max tokens per response
  temperature: <float>           # Temperature (0.0 - 2.0)
  token_budget: <integer>        # Total token budget
  provider: <string>             # Provider override
  model: <string>                # Model override
  mcp: [<string>...]             # MCP servers for tool access
  extended_thinking: <boolean>   # Enable extended thinking (Claude)
  thinking_budget: <integer>     # Thinking budget tokens
  depth_limit: <integer>         # Max spawn_agent recursion depth
  tool_choice: <string>          # auto, required, none
  stop_sequences: [<string>...]  # Sequences that stop generation
  scope: <string>                # Preset tool set: full, minimal, debug
  guardrails:                    # Output validation
    - type: regex
      pattern: "..."
  completion:                    # Completion behavior configuration
    mode: explicit               # explicit, pattern, natural
    confidence:
      threshold: 0.8
  limits:                        # Execution cost limits
    max_cost: 1.00
    max_tokens: 50000
    max_duration: 300
    on_limit_reached:
      action: stop
```

### Tools Configuration

| Value | Behavior |
|-------|----------|
| `["nika:read", "nika:write"]` | Only specified tools |
| `["builtin"]` | All builtin tools (core + file) |
| `[]` or omitted | All core builtin tools |

---

## with: Bindings

Data flow between tasks:

```yaml
with:
  # Simple task reference
  data: $task_id

  # Path traversal
  name: $task_id.user.name

  # Array indexing
  first: $task_id.items[0]

  # Default values
  temp: $task_id.temp ?? 20
  name: $task_id.name ?? "Anonymous"

  # Pipe transforms
  upper_name: $task_id.name | upper | trim

  # Lazy bindings (deferred resolution)
  lazy_val:
    path: $future_task.result
    lazy: true
    default: "fallback"
```

### Default Value Syntax

| Syntax | Type |
|--------|------|
| `$task.path ?? 42` | Numeric default |
| `$task.path ?? "text"` | String default (quoted) |
| `$task.path ?? true` | Boolean default |
| `$task.path ?? {"key": "val"}` | Object default |
| `$task.path ?? [1, 2, 3]` | Array default |

### Transform Pipes

31 built-in transforms, chained with `|`:

| Category | Transforms |
|----------|-----------|
| String | `upper`, `lower`, `trim`, `trim_start`, `trim_end` |
| Collection | `length`, `first`, `last`, `first(N)`, `last(N)`, `keys`, `values`, `flatten`, `reverse`, `sort`, `unique`, `compact` |
| Type conversion | `to_string`, `to_number`, `to_bool`, `to_json`, `parse_json` |
| Numeric | `round(N)`, `abs`, `ceil`, `floor` |
| Utility | `default(V)`, `type_of`, `join(S)`, `split(S)`, `shell` |

---

## depends_on:

Explicit ordering dependencies (no data flow):

```yaml
- id: deploy
  depends_on: [build, test]
  exec:
    command: ./deploy.sh
```

Data dependencies via `with:` automatically create implicit `depends_on` edges.

---

## output:

Output format and validation:

```yaml
output:
  format: json                   # text, json, yaml
  schema:                        # Inline JSON Schema
    type: object
    properties:
      title: { type: string }
    required: [title]
  schema_ref: ./schemas/output.schema.json  # External schema file
  max_retries: 3                 # Retries on validation failure
```

---

## for_each:

Iterate over arrays:

```yaml
- id: process
  for_each:
    items: "{{with.list}}"        # Task output (must be array)
    as: item                       # Loop variable name (default: "item")
    concurrency: 5                 # Max parallel iterations
  infer:
    prompt: "Process: {{with.item}}"
```

---

## retry:

Retry configuration:

```yaml
retry:
  max_attempts: 3                # Maximum retry attempts
  delay_ms: 1000                 # Delay between retries (ms)
  backoff: 2.0                   # Exponential backoff multiplier
```

---

## structured:

JSON schema enforcement with 5-layer defense:

```yaml
structured:
  schema:
    type: object
    properties:
      result: { type: string }
    required: [result]
  max_retries: 3
  strict: true
```

---

## artifact:

Persist task output to files:

```yaml
artifact:
  path: "output/report.md"      # Output file path
  format: text                   # text, json, yaml, binary
```

---

## log:

Task-level log configuration:

```yaml
log:
  level: debug                   # trace, debug, info, warn, error
  file: ./logs/task.log          # Log file path
```

---

## Template Syntax

Templates use double-brace syntax: `{{expression}}`.

| Expression | Description |
|-----------|-------------|
| `{{with.alias}}` | Binding value |
| `{{with.alias.path.to.field}}` | JSONPath traversal |
| `{{with.alias \| transform}}` | With pipe transform |
| `{{context.alias}}` | Context file content |
| `{{inputs.param}}` | Input parameter value |
| `{{env.VAR_NAME}}` | Environment variable |

Templates are resolved at runtime after binding resolution. The resolution order is: bindings first, then templates, then pipe transforms.
