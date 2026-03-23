# Nika Conventions

## Workflow Files

- Extension: `.nika.yaml` (never `.yaml` or `.yml` alone)
- Schema: `nika/workflow@0.12` (always the full string)
- Validate before running: `nika check workflow.nika.yaml`

## Workflow Structure

Every workflow requires `schema:` and `tasks:`. All other top-level keys are optional.

```yaml
schema: nika/workflow@0.12
workflow: optional-name
description: "Optional description"
provider: anthropic                  # Default LLM provider
model: claude-sonnet-4-20250514          # Default model
inputs:                              # Parameters with defaults
  key: "default_value"
tasks:
  - id: unique-task-id
    # exactly one verb: infer: | exec: | fetch: | invoke: | agent:
```

## 5 Verbs

| Verb | Purpose | Required Field |
|------|---------|---------------|
| `infer:` | LLM generation | `prompt:` |
| `exec:` | Shell command | `command:` (or string shorthand) |
| `fetch:` | HTTP request | `url:` |
| `invoke:` | MCP tool call | `tool:` |
| `agent:` | Multi-turn loop | `prompt:` |

## Data Flow Conventions

### with: bindings (data flow)

```yaml
with:
  data: $other_task                # Task reference ($ prefix required)
  clean: $task | trim | upper      # Pipe transforms
  val: $task.path.to.field ?? 20   # JSONPath + fallback operator
  key: $env.API_KEY                # Environment variable
```

### depends_on: (ordering only, no data)

```yaml
depends_on: [task_a, task_b]
```

### Templates

```yaml
prompt: "Hello {{with.name}}, input is {{inputs.param}}"
command: "echo {{item.field}}"
url: "https://api.com/{{with.id}}"
```

Template variables: `{{with.alias}}`, `{{inputs.key}}`, `{{item}}`, `{{context.file}}`

## Pipe Transforms

Chain with `|` in `with:` bindings:

- **String**: `upper`, `lower`, `trim`, `trim_start`, `trim_end`
- **Collection**: `length`, `first`, `last`, `first(N)`, `last(N)`, `keys`, `values`, `flatten`, `reverse`, `sort`, `unique`, `compact`
- **Type**: `to_string`, `to_number`, `to_bool`, `to_json`, `parse_json`
- **Numeric**: `round(N)`, `abs`, `ceil`, `floor`
- **Utility**: `default(V)`, `type_of`, `join(S)`, `split(S)`

## Iteration

```yaml
- id: process-batch
  for_each: "$source_task"
  as: item
  concurrency: 5
  fail_fast: true
  exec: "echo '{{item}}'"
```

## DAG Patterns

- **Sequential**: `depends_on: [previous]` or `with: { data: $previous }`
- **Diamond**: Multiple tasks depend on same source, then merge
- **Fan-out**: `for_each:` on an array
- DAG must be acyclic -- no circular references

## Providers

LLM: `anthropic` (claude), `openai` (gpt), `mistral`, `groq`, `deepseek`, `gemini` (google), `xai` (grok), `native` (local)

## Common Mistakes to Avoid

| Wrong | Right |
|-------|-------|
| `data: other_task` | `data: $other_task` |
| `file.yaml` | `file.nika.yaml` |
| `schema: 0.12` | `schema: nika/workflow@0.12` |
| `exec: "cmd1 \| cmd2"` | `exec: { command: "cmd1 \| cmd2", shell: true }` |
| `depends_on` for data | Use `with:` for data, `depends_on:` for ordering |
| `timeout: 30` (ms?) | `timeout: 30` means 30 seconds |

## Resilience

```yaml
retry:
  max_attempts: 3
  delay_ms: 1000
  backoff: 2.0
```

## Structured Output

```yaml
structured:
  schema: ./schemas/output.json      # Or inline JSON Schema
  max_retries: 3
```

## MCP Configuration

```yaml
mcp:
  server-name:
    command: npx
    args: ["-y", "@modelcontextprotocol/server"]
    env:
      API_KEY: "{{$env.API_KEY}}"
```

Invoke with `tool: tool_name` + `mcp: server_name`, or builtin `tool: "nika:import"`.
