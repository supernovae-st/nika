# Nika Cheat Sheet

> Everything you need on one page. Print it. Pin it. Reference it.

---

## CLI Commands

### Core Commands
```bash
nika run <workflow.nika.yaml>              # Execute a workflow
nika run workflow.nika.yaml --input key=val # Execute with input overrides
nika check <workflow.nika.yaml>            # Validate without executing
nika ui                                     # Launch the Terminal UI
nika provider list                          # Show configured providers
nika keys set <provider>                # Set active provider
```

### Course Commands
```bash
nika init --course                          # Generate 12-level course (44 exercises)
nika course status                          # Show progress constellation map
nika course next                            # Open next exercise
nika course check [level]                   # Validate exercises for a level
nika course hint [exercise]                 # Progressive hints (3 tiers)
nika course run <exercise>                  # Run a course exercise
nika course info [level]                    # Show course/level details
nika course reset <level>                   # Reset a level
nika course watch                           # Auto-check on file save
```

### Showcase Commands
```bash
nika showcase list                          # Browse 115 showcase workflows
nika showcase extract <name>                # Extract a showcase to current dir
```

### Init Commands
```bash
nika init                                   # Interactive project setup wizard
```

---

## Workflow Structure

```yaml
schema: "nika/workflow@0.12"     # Required -- always this version
workflow: my-workflow-name        # Required -- human-readable name
description: "What it does"      # Optional

provider: anthropic               # Default LLM provider
model: claude-sonnet-4-6          # Default model

inputs:                           # CLI-overridable parameters
  key: "default_value"

artifacts:                        # File output configuration
  dir: ./output
  format: text
  manifest: true

mcp:                              # MCP server connections
  name:
    command: "npx"
    args: ["-y", "server-pkg"]

tasks:                            # The work to do
  - id: unique_task_id
    depends_on: [other_task]
    with:
      alias: $other_task
    <verb>: <verb_config>
    on_error: continue            # Or: fail (default)
    timeout: 30                   # Seconds
    retry:
      max_attempts: 3
    artifact:
      path: output/file.txt
    for_each: [a, b, c]
    concurrency: 3
```

---

## The 5 Verbs

### exec: -- Shell Commands
```yaml
# Shorthand
- id: quick
  exec: "ls -la"

# Full form
- id: full
  exec:
    command: "echo $GREETING | tr '[:lower:]' '[:upper:]'"
    shell: true           # Required for pipes, &&, $VAR
    timeout: 30           # Seconds
    cwd: "/tmp"           # Working directory
    env:                  # Environment variables
      GREETING: "hello"
```

### fetch: -- HTTP Requests
```yaml
# Shorthand (GET)
- id: quick
  fetch: "https://api.example.com/data"

# Full form
- id: full
  fetch:
    url: "https://api.example.com/data"
    method: POST                    # GET (default), POST, PUT, DELETE, PATCH
    headers:
      Authorization: "Bearer {{with.token}}"
      Accept: "application/json"
    json:                           # JSON body (auto-sets Content-Type)
      key: "value"
    extract: markdown               # Post-processing mode
    selector: "article.main"        # CSS or JSONPath selector
    response: full                  # full | binary | (default: raw body)
    timeout: 30
```

### infer: -- LLM Generation
```yaml
# Shorthand
- id: quick
  infer: "Explain recursion in one sentence."

# Full form
- id: full
  infer:
    prompt: "Summarize: {{with.article}}"
    system: "You are a concise technical writer."
    temperature: 0.3                # 0.0 = deterministic, 1.0 = creative
    max_tokens: 500                 # Response length limit
    provider: anthropic             # Override workflow default
    model: claude-sonnet-4-6        # Override workflow default
    output:
      format: json_schema           # json | json_schema | (default: text)
      schema:
        type: object
        properties:
          summary: { type: string }
        required: [summary]
      enable_retry: true
      max_retry_attempts: 3

# Vision (multimodal)
- id: vision
  infer:
    content:
      - type: image
        source: "{{with.photo.media[0].hash}}"
        detail: high
      - type: text
        text: "Describe this image."
```

### invoke: -- Tool Calls
```yaml
- id: tool_call
  invoke:
    tool: "nika:write"              # nika:* for builtins, server:tool for MCP
    params:
      file_path: "output.txt"
      content: "Hello {{with.data}}"
```

### agent: -- Autonomous LLM Loop
```yaml
- id: agent_task
  agent:
    prompt: "Research AI trends and report findings."
    tools: [builtin]                # [builtin], [nika:log, nika:complete], [builtin, mcp:server]
    max_turns: 10                   # Safety: max loop iterations
    max_tokens: 800                 # Per-response token limit
    token_budget: 8000              # Total across all turns
    tool_choice: auto               # auto | required | none
    completion:
      mode: explicit                # explicit | natural | pattern
      signal:
        tool: nika:complete
        fields:
          required: [result]
    guardrails:
      - type: length
        min_words: 100
        max_words: 500
        on_failure: retry           # retry | fail | escalate
      - type: regex
        pattern: "^## "
        message: "Must start with heading"
        on_failure: retry
      - type: schema
        schema: { type: object, required: [title] }
        on_failure: fail
    limits:
      max_turns: 20
      max_cost_usd: 0.50
      max_duration_secs: 120
```

---

## Data Flow: Bindings and Templates

### With Block
```yaml
with:
  alias: $task_id                   # Task output reference ($ required)
  env_var: $env.API_KEY             # Environment variable
```

### Template Syntax
```yaml
"{{with.alias}}"                    # Direct value
"{{with.alias.field}}"              # Nested object field
"{{with.alias.items[0].name}}"      # Array index + field
"{{with.alias | transform}}"        # Pipe transform
"{{with.alias | trim | upper}}" # Chained transforms
"{{inputs.key}}"                    # Input parameter
"{{with.item}}"                     # for_each current item
```

---

## Pipe Transforms

### String Transforms
| Transform | Description | Example |
|-----------|-------------|---------|
| `upper` | UPPERCASE | `{{x \| upper}}` |
| `lower` | lowercase | `{{x \| lower}}` |
| `trim` | Strip whitespace | `{{x \| trim}}` |
| `trim_start` | Strip leading whitespace | `{{x \| trim_start}}` |
| `trim_end` | Strip trailing whitespace | `{{x \| trim_end}}` |
| `length` | Character count | `{{x \| length}}` |
| `reverse` | Reverse string | `{{x \| reverse}}` |
| `shell` | Shell-escape | `{{x \| shell}}` |

### Type Transforms
| Transform | Description |
|-----------|-------------|
| `to_string` | Convert to string |
| `to_number` | Convert to number |
| `to_bool` | Convert to boolean |
| `to_json` | Pretty-print as JSON |
| `parse_json` | Parse JSON string |
| `type_of` | Return type name |

### Array Transforms
| Transform | Description |
|-----------|-------------|
| `first` | First element |
| `last` | Last element |
| `flatten` | Flatten nested arrays |
| `reverse` | Reverse order |
| `sort` | Sort elements |
| `unique` | Deduplicate |
| `compact` | Remove nulls/empty |
| `keys` | Object keys as array |
| `values` | Object values as array |

### Math Transforms
| Transform | Description |
|-----------|-------------|
| `round` | Round to integer |
| `abs` | Absolute value |
| `ceil` | Round up |
| `floor` | Round down |

### Parameterized Transforms
| Transform | Description | Example |
|-----------|-------------|---------|
| `join(sep)` | Join array | `{{x \| join(", ")}}` |
| `split(sep)` | Split string | `{{x \| split(",")}}` |
| `default(val)` | Default value | `{{x \| default("N/A")}}` |
| `round(n)` | Round to n decimals | `{{x \| round(2)}}` |

---

## Extract Modes (fetch:)

| Mode | Description | Requires `selector:` |
|------|-------------|---------------------|
| `markdown` | HTML to clean Markdown | No |
| `article` | Main article (Readability) | No |
| `text` | Visible text | Optional (CSS) |
| `selector` | Raw HTML elements | Yes (CSS) |
| `metadata` | OG/Twitter/JSON-LD/SEO | No |
| `links` | Link classification | No |
| `jsonpath` | JSONPath query on JSON | Yes (JSONPath) |
| `feed` | RSS/Atom/JSON Feed | No |
| `llm_txt` | AI content discovery | No |

---

## Builtin Tools (nika:*)

### Core Tools
| Tool | Parameters |
|------|-----------|
| `nika:log` | `level` (trace/debug/info/warn/error), `message` |
| `nika:emit` | `name`, `payload` (JSON) |
| `nika:assert` | `condition` (bool), `message` |
| `nika:sleep` | `duration` (humantime: "1s", "500ms", "2m") |
| `nika:complete` | `result` (any) |
| `nika:run` | `workflow` (path to .nika.yaml) |

### File Tools
| Tool | Parameters |
|------|-----------|
| `nika:read` | `file_path` |
| `nika:write` | `file_path`, `content` |
| `nika:edit` | `file_path`, `old_string`, `new_string` |
| `nika:glob` | `pattern`, `path` (optional) |
| `nika:grep` | `pattern`, `path` |

### Media Tools (Tier 1 -- Always-on)
| Tool | Parameters |
|------|-----------|
| `nika:import` | `path` |
| `nika:dimensions` | `hash` |
| `nika:thumbhash` | `hash` |
| `nika:dominant_color` | `hash` |
| `nika:pipeline` | `input` (hash), `operations` (array) |

### Media Tools (Tier 2 -- Default)
| Tool | Parameters |
|------|-----------|
| `nika:thumbnail` | `hash`, `width` |
| `nika:convert` | `hash`, `format` (png/jpeg/webp) |
| `nika:strip` | `hash` |
| `nika:metadata` | `hash` |
| `nika:optimize` | `hash` |
| `nika:svg_render` | `hash`, `width` (optional) |

---

## Error Code Ranges

| Range | Category |
|-------|----------|
| 000-009 | Workflow (parse, schema, not found) |
| 010-019 | Schema/validation |
| 020-029 | DAG (cycles, missing deps, duplicates) |
| 030-039 | Provider (not configured, API error, missing key) |
| 040-049 | Template/binding (template error, alias not found) |
| 050-059 | Path/task/security (blocked command, invalid ID) |
| 060-069 | Output (invalid JSON, schema validation) |
| 070-079 | With block validation (unknown alias, null value) |
| 080-089 | DAG validation (unknown task ref, not upstream, circular) |
| 090-099 | JSONPath/IO/Execution |
| 100-109 | MCP (not connected, tool failed, timeout) |
| 110-119 | Agent + Guardrails |
| 120-129 | Resilience |
| 200-219 | File tools + Builtin tools |
| 251-259 | Media pipeline |
| 280-297 | Artifact + Media tools |
| 300-309 | Structured output |
| 310-319 | Course |

---

## Provider Setup

```bash
# Set API keys as environment variables
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."
export MISTRAL_API_KEY="..."
export GROQ_API_KEY="gsk_..."
export DEEPSEEK_API_KEY="..."
export GEMINI_API_KEY="..."     # Gemini
export XAI_API_KEY="..."        # Grok

# Verify configuration
nika provider list

# Auto-detect (uses first available)
provider: auto
```

### Provider/Model Quick Reference
| Provider | Example Model |
|----------|--------------|
| `anthropic` | `claude-sonnet-4-6`, `claude-opus-4`, `claude-haiku-3` |
| `openai` | `gpt-4o`, `gpt-4o-mini`, `o1` |
| `gemini` | `gemini-2.5-flash` |
| `groq` | `llama-4-maverick`, `mixtral-8x7b` |
| `mistral` | `mistral-small-latest`, `mistral-large-latest` |
| `deepseek` | `deepseek-chat`, `deepseek-reasoner` |
| `xai` | `grok-2`, `grok-3` |
| `native` | Local GGUF file path |

---

## DAG Patterns

### Sequential
```yaml
tasks:
  - id: a
    exec: echo "1"
  - id: b
    depends_on: [a]
    exec: echo "2"
```

### Parallel (no depends_on = parallel)
```yaml
tasks:
  - id: a
    exec: echo "1"
  - id: b
    exec: echo "2"     # Runs simultaneously with a
```

### Diamond (fan-out, fan-in)
```yaml
tasks:
  - id: start
    exec: echo "begin"
  - id: left
    depends_on: [start]
    exec: echo "left"
  - id: right
    depends_on: [start]
    exec: echo "right"
  - id: merge
    depends_on: [left, right]
    exec: echo "done"
```

### For Each (parallel iteration)
```yaml
- id: batch
  for_each: [a, b, c]
  concurrency: 3
  exec: echo "{{with.item}}"
```

---

## Common One-Liners

```yaml
# Fetch and display JSON
- id: api
  fetch:
    url: "https://httpbin.org/json"
    extract: jsonpath
    selector: "$.slideshow.title"

# Scrape a page to markdown
- id: scrape
  fetch:
    url: "https://example.com"
    extract: markdown

# Quick LLM call
- id: think
  infer: "Explain X in one sentence."

# Write a file
- id: save
  invoke:
    tool: "nika:write"
    params:
      file_path: "output.txt"
      content: "{{with.data}}"

# Download binary
- id: download
  fetch:
    url: "https://example.com/image.png"
    response: binary
```

---

## Guardrail Types

| Type | Parameters | Description |
|------|-----------|-------------|
| `length` | `min_words`, `max_words` | Word count bounds |
| `regex` | `pattern`, `message` | Regex match check |
| `schema` | `schema` (JSON Schema) | JSON structure validation |
| `llm` | `prompt` | Secondary LLM evaluates quality |

### on_failure Actions
- `retry` -- Feed failure back to agent, ask to fix
- `fail` -- Stop agent with error
- `escalate` -- Flag for human review

---

## Agent Completion Modes

| Mode | Behavior |
|------|----------|
| `explicit` | Agent must call `nika:complete` |
| `natural` | Stops when LLM stops calling tools |
| `pattern` | Stops when output matches regex |

---

## File Extensions and Naming

| Item | Convention |
|------|-----------|
| Workflow files | `.nika.yaml` |
| Task IDs | `snake_case` |
| Workflow names | `kebab-case` |
| Input keys | `snake_case` |

---

## Environment Variables for Providers

| Provider | Environment Variable |
|----------|---------------------|
| Anthropic | `ANTHROPIC_API_KEY` |
| OpenAI | `OPENAI_API_KEY` |
| Mistral | `MISTRAL_API_KEY` |
| Groq | `GROQ_API_KEY` |
| DeepSeek | `DEEPSEEK_API_KEY` |
| Google (Gemini) | `GEMINI_API_KEY` |
| xAI | `XAI_API_KEY` |
| Cohere | `COHERE_API_KEY` |

---

## Common Debugging Steps

```bash
# 1. Validate syntax
nika check my-workflow.nika.yaml

# 2. Check provider setup
nika provider list

# 3. Run with default verbosity
nika run my-workflow.nika.yaml

# 4. Check course progress
nika course status

# 5. Get hints for stuck exercises
nika course hint
```

---

## Template Quick Reference

```yaml
# Task output reference
"{{with.alias}}"

# Nested field access
"{{with.alias.data.users[0].name}}"

# Pipe transforms (chain with |)
"{{with.alias | trim | upper | length}}"

# Environment variable
"{{with.env_alias}}"   # where with: { env_alias: $env.VAR }

# Input parameter
"{{inputs.key}}"

# For-each current item
"{{with.item}}"
"{{with.item.field}}"
```

---

## Safety Limits Reference

| Limit | Scope | Type | Purpose |
|-------|-------|------|---------|
| `timeout:` | Task | Seconds | Kill task after N seconds |
| `max_turns:` | Agent | Integer | Max loop iterations |
| `token_budget:` | Agent | Integer | Total tokens across all turns |
| `max_tokens:` | Agent/Infer | Integer | Tokens per LLM response |
| `max_cost_usd:` | Agent | Float | Dollar cost ceiling |
| `max_duration_secs:` | Agent | Integer | Wall-clock timeout |

---

*Keep this page open. You will need it.*
