---
name: nika-workflow-syntax
description: >-
  Background knowledge for Nika YAML workflow engine (.nika.yaml files).
  Complete syntax reference for schema nika/workflow@0.12 — 5 verbs (infer,
  exec, fetch, invoke, agent), with: bindings, depends_on DAG, for_each
  parallelism, structured output, pipe transforms, artifacts, inputs, context,
  MCP config, retry, and logging. Auto-loaded when .nika.yaml files are present.
user_invocable: false
globs:
  - "**/*.nika.yaml"
---

# Nika Workflow Syntax Reference

Nika is a semantic YAML workflow engine for AI tasks. Schema: `nika/workflow@0.12`.
File extension: `.nika.yaml` (required).

## Minimal Workflow

```yaml
schema: nika/workflow@0.12
tasks:
  - id: hello
    exec: "echo Hello"
```

## Workflow-Level Fields

```yaml
schema: nika/workflow@0.12    # Required
workflow: my-workflow          # Optional name
description: "What it does"   # Optional
model: gpt-4.1-mini           # Default model for all tasks
provider: openai               # Default provider for all tasks
log:
  level: info                  # debug | info | warn | error
inputs:                        # User-supplied parameters
  topic:
    default: "AI"
context:                       # External file context
  files:
    data: ./data.json
artifacts:                     # Output file configuration
  dir: ./output
mcp:                           # MCP server config
  server-name:
    command: npx
    args: ["-y", "@org/mcp-server"]
    env:
      API_KEY: "${API_KEY}"    # Shell-expand syntax for env vars
tasks: []                      # Required: list of tasks
```

## Task-Level Fields

```yaml
- id: task_id                  # Required: unique identifier
  # --- Exactly ONE verb ---
  infer: "prompt"              # LLM generation
  exec: "command"              # Shell command
  fetch: { url: "..." }       # HTTP request
  invoke: { tool: "..." }     # MCP tool call
  agent: { prompt: "..." }    # Multi-turn autonomous loop
  # --- Optional modifiers ---
  depends_on: [other_task]     # DAG dependencies
  with:                        # Data bindings
    alias: $other_task         # $ prefix required
  for_each: ["a", "b", "c"]   # Parallel iteration
  as: item                     # Iterator variable name
  provider: openai             # Override provider
  model: gpt-4.1-mini          # Override model
  system: "You are..."         # System prompt (infer/agent)
  temperature: 0.7             # 0.0-2.0
  max_tokens: 1000             # Max output tokens
  timeout: 30                  # Seconds (not milliseconds)
  retry:
    max_attempts: 3            # Retry on failure
    delay: 2                   # Seconds between retries
  structured:                  # JSON schema output
    schema:
      type: object
      properties:
        name: { type: string }
      required: [name]
  artifact:                    # Write output to file
    path: result.txt
    format: text               # text | json | markdown
    source: alias              # Use binding instead of task output
    append: true               # Append instead of overwrite
    template: "# {{output}}"   # Wrap output in template
  log:
    level: debug               # Task-level log override
```

## The 5 Verbs

### infer: (LLM Generation)

```yaml
# Short form
- id: ask
  infer: "What is the capital of France?"

# Long form
- id: ask
  infer:
    prompt: "Describe this image"
    content:                    # Multimodal (vision)
      - type: image
        source: "{{with.photo.media[0].hash}}"
        detail: high
      - type: text
        text: "What do you see?"
```

### exec: (Shell Command)

```yaml
# Short form
- id: run
  exec: "echo hello"

# Long form
- id: run
  exec:
    command: "ls -la"
    shell: true                # Use shell (default: true)
    cwd: ./subdir              # Working directory
    env:
      MY_VAR: value
```

### fetch: (HTTP Request)

```yaml
- id: api
  fetch:
    url: "https://api.example.com/data"
    method: POST               # GET | POST | PUT | DELETE | PATCH | HEAD
    headers:
      Authorization: "Bearer {{with.token}}"
    json:                      # Auto-sets Content-Type
      key: value
    body: "raw body"           # Alternative to json
    extract: markdown          # Post-processing mode
    selector: "h1, p"         # CSS selector (for text/selector extract)
    response: full             # full | binary (default: raw body)
```

Extract modes: `markdown`, `article`, `text`, `selector`, `metadata`, `links`, `jsonpath`, `feed`, `llm_txt`

### invoke: (MCP Tool Call)

```yaml
- id: call
  invoke:
    tool: tool_name            # MCP tool or nika:* builtin
    mcp: server-name           # MCP server (omit for builtins)
    params:
      input: "{{with.data}}"
```

### agent: (Multi-Turn Loop)

```yaml
- id: auto
  agent:
    prompt: "Research and summarize topic X"
    tools: [nika_read, nika_glob, nika_complete]
    max_turns: 10
    max_tokens: 2000
    provider: openai
    model: gpt-4.1
    guardrails:
      blocked_tools: [nika_write]   # Safety constraints
    completion:
      signal: nika_complete         # Tool that ends the loop
```

## Data Flow: Bindings

### with: block (bind upstream task output)

```yaml
- id: step1
  exec: "echo data"
- id: step2
  depends_on: [step1]
  with:
    result: $step1                    # $ prefix required
  exec: "echo {{with.result}}"
```

### Fallback operator

```yaml
with:
  val: $task.missing ?? "default"     # Use default if path is null
```

### JSONPath access

```yaml
with:
  name: $task.users[0].name          # Nested JSON access
```

### Pipe transforms (27 built-in)

```yaml
exec: "echo {{with.data | upper | trim}}"
```

| Category | Transforms |
|----------|-----------|
| String | `upper`, `lower`, `trim`, `trim_start`, `trim_end` |
| Collection | `length`, `first`, `last`, `first(N)`, `last(N)`, `keys`, `values`, `flatten`, `reverse`, `sort`, `unique`, `compact` |
| Type | `to_string`, `to_number`, `to_bool`, `to_json`, `parse_json` |
| Numeric | `round(N)`, `abs`, `ceil`, `floor` |
| Utility | `default(V)`, `type_of`, `join(S)`, `split(S)`, `shell` |

## Template Contexts

Templates use `{{...}}` syntax. Available contexts:

- `{{with.alias}}` -- Binding values
- `{{inputs.name}}` -- Input parameters
- `{{context.files.name}}` -- Context file contents
- `{{output}}` -- Current task output (in artifact templates only)
- `{{$env.VAR}}` -- Environment variables in templates

## for_each: (Parallel Iteration)

```yaml
- id: process
  for_each: ["en", "fr", "de"]       # Inline list
  as: lang                            # Iterator alias
  exec: "echo {{with.lang}}"

# From upstream task
- id: process
  depends_on: [source]
  for_each: "$source"                 # Dynamic from task output (must be JSON array)
  as: item
  exec: "echo {{with.item}}"

# Over objects
- id: greet
  for_each:
    - name: Alice
      role: dev
    - name: Bob
      role: ops
  as: person
  exec: "echo {{with.person}}"
```

## DAG Patterns

```yaml
# Sequential chain
- id: a
  exec: "echo 1"
- id: b
  depends_on: [a]
  exec: "echo 2"

# Fan-out (1 -> N parallel)
- id: source
  exec: "echo data"
- id: w1
  depends_on: [source]
  exec: "echo worker1"
- id: w2
  depends_on: [source]
  exec: "echo worker2"

# Fan-in (N -> 1 merge)
- id: merge
  depends_on: [w1, w2]
  with:
    r1: $w1
    r2: $w2
  exec: "echo {{with.r1}} + {{with.r2}}"

# Diamond (fan-out + fan-in)
# source -> left + right -> merge
```

## Common Mistakes

| Mistake | Correct |
|---------|---------|
| `.yaml` extension | `.nika.yaml` |
| Missing `schema:` line | `schema: nika/workflow@0.12` |
| `with: { x: task_id }` | `with: { x: $task_id }` ($ prefix) |
| `timeout: 30` meaning ms | `timeout: 30` means 30 seconds |
| Two verbs on one task | Exactly ONE verb per task |
| `for_each` without `as` | Always pair `for_each:` with `as:` |
| Circular `depends_on` | DAG must be acyclic |
| `{{env.VAR}}` in templates | `{{$env.VAR}}` ($ prefix) |

## CLI Commands

```bash
nika check file.nika.yaml     # Validate without running
nika run file.nika.yaml       # Execute workflow
nika ui                       # Terminal UI
nika provider list             # Show configured providers
nika init                      # Interactive project setup
nika course next               # Learning course
```

## Providers

Supported: `claude` (Anthropic), `openai`, `mistral`, `groq`, `deepseek`, `gemini`, `xai`, `native` (local GGUF), `mock` (testing).

Set API keys as environment variables: `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `MISTRAL_API_KEY`, `GROQ_API_KEY`, `DEEPSEEK_API_KEY`, `GEMINI_API_KEY`, `XAI_API_KEY`.
