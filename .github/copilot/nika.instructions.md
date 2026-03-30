---
applyTo: "**/*.nika.yaml"
---

# Nika Workflow Completion Guide

Nika workflows use schema `nika/workflow@0.12`. File extension: `.nika.yaml`.

## Required Structure

Every workflow must have `schema:` and `tasks:`. All other top-level keys are optional.

```yaml
schema: nika/workflow@0.12
tasks:
  - id: task-name
    exec: "echo hello"
```

## Top-Level Keys

`schema`, `workflow`, `description`, `provider`, `model`, `inputs`, `context`, `include`, `mcp`, `agents`, `skills`, `artifacts`, `goal`, `orchestrate`, `log`, `tasks`

## 5 Verbs -- Each task uses exactly one

| Verb | Required Fields | Purpose |
|------|----------------|---------|
| `infer:` | `prompt:` | LLM generation |
| `exec:` | `command:` | Shell command |
| `fetch:` | `url:` | HTTP request |
| `invoke:` | `tool:` | MCP tool call |
| `agent:` | `prompt:` | Multi-turn agent |

### infer fields
`prompt`, `system`, `temperature`, `max_tokens`, `extended_thinking`, `thinking_budget`, `content`, `response_format`, `guardrails`

### exec fields
`command`, `shell`, `cwd`, `env`, `timeout`

### fetch fields
`url`, `method`, `headers`, `body`, `json`, `timeout`, `follow_redirects`, `response`, `extract`, `selector`

### invoke fields
`tool`, `resource`, `params`, `mcp`, `timeout`

### agent fields
`prompt`, `system`, `tools`, `max_turns`, `max_tokens`, `from`, `skills`, `provider`, `model`, `mcp`, `temperature`, `token_budget`, `extended_thinking`, `thinking_budget`, `depth_limit`, `tool_choice`, `stop_sequences`, `scope`, `guardrails`, `completion`, `limits`

## Task-Level Keys (all verbs)

`id` (required), `description`, `provider`, `model`, `base_url`, `with`, `depends_on`, `for_each`, `as`, `retry`, `output`, `artifact`, `structured`, `record`, `context_budget`, `routing`, `preset`, `log`, `concurrency`, `fail_fast`

## Data Bindings

```yaml
with:
  data: $other_task              # $ prefix required
  clean: $task | trim | upper    # Pipe transforms
  val: $task.field ?? "default"  # JSONPath + fallback
  key: $env.MY_VAR               # Environment variable
```

## Templates

`{{with.alias}}`, `{{inputs.name}}`, `{{with.item}}`, `{{with.item.field}}`, `{{context.file}}`

## Pipe Transforms

String: `upper`, `lower`, `trim`, `trim_start`, `trim_end`
Collection: `length`, `first`, `last`, `first(N)`, `last(N)`, `keys`, `values`, `flatten`, `reverse`, `sort`, `unique`, `compact`
Type: `to_string`, `to_number`, `to_bool`, `to_json`, `parse_json`
Numeric: `round(N)`, `abs`, `ceil`, `floor`
Utility: `default(V)`, `type_of`, `join(S)`, `split(S)`

## Providers

`anthropic`/`claude`, `openai`/`gpt`, `mistral`, `groq`, `deepseek`, `gemini`/`google`, `xai`/`grok`, `native`/`local`

## Shorthand Forms

- `exec: "echo hello"` -- string shorthand (shell: false)
- `invoke: "nika:dimensions"` -- tool-only shorthand
- `structured: ./schema.json` -- file path shorthand

## Key Rules

1. Schema is always `nika/workflow@0.12` (full string, not just version number)
2. File extension must be `.nika.yaml`
3. `with:` values use `$` prefix for task references
4. `depends_on:` is ordering only; `with:` is for data flow
5. `timeout:` is in seconds
6. `exec` needs `shell: true` for pipes, redirects, and shell features
7. DAG must be acyclic (no circular with/depends_on references)
8. `for_each:` requires `items:` field (or inline array at task level)
