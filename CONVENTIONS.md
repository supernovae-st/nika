# Nika Conventions

This document covers conventions for **authoring Nika workflows**. For contributing to the Nika codebase itself, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Workflow Files

- Extension: `.nika.yaml` (never `.yaml` or `.yml` alone)
- Schema: `schema: "nika/workflow@0.12"` (required, always the full string)
- Validate before running: `nika check workflow.nika.yaml`
- Validate + test MCP: `nika check workflow.nika.yaml --strict`

## Workflow Structure

Every workflow requires `schema:` and `tasks:`. All other top-level keys are optional.

```yaml
schema: "nika/workflow@0.12"
workflow: optional-name              # Defaults to filename
description: "Optional description"
provider: anthropic                  # Default LLM provider for all tasks
model: claude-sonnet-4-20250514      # Default model for all tasks

inputs:                              # Workflow parameters with defaults
  topic: "default value"

context:                             # Load file content into bindings
  files:
    readme: ./README.md

skills:                              # Prompt augmentation (injected into all infer tasks)
  writing: ./skills/writing-style.md

artifacts:                           # Output configuration
  dir: ./output
  format: markdown

tasks:
  - id: unique-task-id
    # Exactly one verb per task: infer: | exec: | fetch: | invoke: | agent:
```

## 5 Verbs

Nika has exactly 5 verbs. No more will be added.

| Verb | Purpose | Short form | Full form key fields |
|------|---------|------------|----------------------|
| `infer:` | LLM generation | `infer: "prompt"` | `prompt`, `system`, `model`, `temperature`, `max_tokens`, `content` |
| `exec:` | Shell command | `exec: "command"` | `command`, `shell`, `cwd`, `timeout`, `env` |
| `fetch:` | HTTP request | `fetch: "url"` | `url`, `method`, `headers`, `body`, `json`, `extract`, `selector`, `response` |
| `invoke:` | MCP / builtin tool | `invoke: "nika:tool"` | `tool`, `params`, `timeout`, `mcp`, `resource` |
| `agent:` | Multi-turn loop | *(no short form)* | `prompt`, `tools`, `max_turns`, `completion`, `guardrails` |

## Data Flow

### with: bindings (pass data between tasks)

```yaml
with:
  data: $other_task                # Task output reference ($ prefix required)
  clean: $task | trim | upper      # Pipe transforms
  val: $task.path.to.field ?? 20   # JSONPath + fallback operator
  key: $env.API_KEY                # Environment variable
  file: $context.readme            # Loaded file content
```

### depends_on: (ordering without data)

```yaml
depends_on: [task_a, task_b]       # Always an array
```

### Templates

```yaml
prompt: "Hello {{with.name}}, input is {{inputs.param}}"
command: "echo {{with.item.field}}"
url: "https://api.com/{{with.id}}"
```

Template variables: `{{with.alias}}`, `{{inputs.key}}`, `{{context.file}}`

## Pipe Transforms (31 available)

Chain with `|` in `with:` bindings or template expressions:

- **String**: `upper`, `lower`, `trim`, `trim_start`, `trim_end`, `length`, `to_string`
- **Array**: `first`, `last`, `flatten`, `reverse`, `sort`, `unique`, `compact`, `keys`, `values`
- **Numeric**: `to_number`, `round`, `abs`, `ceil`, `floor`
- **Type**: `to_bool`, `to_json`, `parse_json`, `type_of`
- **Parametric**: `join(", ")`, `split(",")`, `default("fallback")`
- **Advanced**: `pluck(field)`, `where(field, val)`, `pick(f1, f2)`, `omit(f1, f2)`, `sort_by(field)`, `merge`, `regex(pattern)`, `group_by(field)`
- **String tests**: `starts_with(str)`, `ends_with(str)`, `contains(str)`
- **Encoding**: `base64_encode`, `base64_decode`, `content_hash`, `unique_urls`
- **System**: `shell` (shell-escape for safe interpolation, NOT command execution)

**Null safety**: Many transforms fail on null input. Guard with `default()`:

```yaml
data: $task.result | default("none") | upper
```

## Iteration

```yaml
- id: process-batch
  for_each: "$source_task.items"     # Only $binding_ref, NOT {{template}}
  as: item
  concurrency: 5                     # Default: 1 (sequential)
  fail_fast: false                   # Default: true
  infer: "Process: {{with.item}}"
```

**for_each output is always an array**. Downstream tasks must handle it:

```yaml
- id: use_results
  with:
    all: $process_batch              # Array
    first: "{{with.all | first}}"
    count: "{{with.all | length}}"
```

## Structured Output

Enforces schema-validated JSON with automatic retry and repair. The prompt must be natural language -- never mention JSON or the schema.

```yaml
- id: extract
  infer: "Tell me about Alice, 30 years old, Rust and Python developer"
  structured:
    schema:
      type: object
      properties:
        name: { type: string }
        age: { type: number, minimum: 0 }
        skills: { type: array, items: { type: string }, minItems: 1 }
      required: [name, age, skills]
    enable_repair: true              # LLM auto-repair on violation (default: true)
    max_retries: 3                   # Schema validation retries (default: 2)
```

## Resilience

Task-level retry works on ALL verbs:

```yaml
- id: flaky_api
  retry:
    max_attempts: 3
    delay_ms: 1000
    backoff: 2.0                     # Exponential backoff multiplier
  fetch: "https://unreliable-api.com/data"
```

Note: `retry:` goes at task level (alongside the verb), not inside it.

## Providers (7 Cloud + 1 Local + 1 Mock)

| Provider | Aliases | Env Var |
|----------|---------|---------|
| `anthropic` | `claude` | `ANTHROPIC_API_KEY` |
| `openai` | `gpt` | `OPENAI_API_KEY` |
| `mistral` | | `MISTRAL_API_KEY` |
| `groq` | | `GROQ_API_KEY` |
| `deepseek` | `deep-seek` | `DEEPSEEK_API_KEY` |
| `gemini` | `google` | `GEMINI_API_KEY` |
| `xai` | `grok` | `XAI_API_KEY` |
| `native` | `local` | *(none)* |
| `mock` | | *(none)* |

## MCP Configuration

Define MCP servers in `mcp:` block or in `.mcp.json`:

```yaml
mcp:
  server-name:
    command: npx
    args: ["-y", "@modelcontextprotocol/server"]
    env:
      API_KEY: "{{$env.API_KEY}}"
```

Invoke tools with double colon: `tool: "server::tool_name"` or `tool: "nika:builtin_name"`.

## DAG Patterns

- **Sequential**: `depends_on: [previous]` or implicit via `with: { data: $previous }`
- **Diamond**: Multiple tasks depend on same source, then merge
- **Fan-out / fan-in**: `for_each:` on an array, then merge results
- **Must be acyclic**: No circular references (NIKA-020)

## Common Mistakes

| Wrong | Right |
|-------|-------|
| `data: other_task` | `data: $other_task` ($ prefix required) |
| `file.yaml` | `file.nika.yaml` |
| `schema: 0.12` | `schema: "nika/workflow@0.12"` |
| `depends_on: task_id` | `depends_on: [task_id]` (always array) |
| `timeout: 30` meaning 30ms | `timeout: 30` means 30 seconds |
| `retry: 3` | `retry: { max_attempts: 3, delay_ms: 2000 }` |
| `tool: "server/tool"` | `tool: "server::tool"` (double colon) |
| `body: {...}` for JSON | `json: {...}` (auto-serializes) |
| `invoke: { tool: "...", input: {...} }` | `invoke: { tool: "...", params: {...} }` |
| `for_each: "{{with.items}}"` | `for_each: "$task.items"` (only $ref) |
| `{{with.results.field}}` after for_each | `{{with.results[0].field}}` (for_each = array) |
| `retry:` inside `invoke:` block | `retry:` at task level, alongside the verb |
| `model:` inside `infer:` block | `model:` at task level |

## Timeouts

All `timeout:` values are in **seconds** (the engine converts internally).

## Path Resolution

All relative paths (context, skills, exec commands) resolve from the **project root** -- the directory containing `nika.toml`, or the directory from which `nika run` is invoked.

## Security

- API keys via env vars only (`$env.API_KEY`). Never hardcode in YAML.
- `fetch:` validates URLs against SSRF (private IP ranges blocked).
- `exec:` has a command blocklist (NIKA-053).
- Use `| shell` transform when interpolating dynamic data in `shell: true` commands.
