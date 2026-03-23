# Nika Workflow Rules

Schema: `nika/workflow@0.12` | Extension: `.nika.yaml`

## 5 Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM generation | `infer: "Summarize this"` |
| `exec:` | Shell command | `exec: "echo hello"` |
| `fetch:` | HTTP request | `fetch: "https://api.example.com"` |
| `invoke:` | MCP tool call | `invoke:` block with `tool:` + `params:` |
| `agent:` | Multi-turn loop | `agent:` block with `tools:` + `max_turns:` |

## Complete Workflow Example

```yaml
schema: "nika/workflow@0.12"
workflow: research-and-summarize
description: "Research a topic and create a summary"
provider: anthropic
model: claude-sonnet-4-20250514

inputs:
  topic: "AI workflow engines"

tasks:
  - id: research
    infer:
      prompt: |
        Research the following topic thoroughly: {{inputs.topic}}
        Provide key findings, trends, and notable projects.
      temperature: 0.7

  - id: summarize
    depends_on: [research]
    with:
      data: $research
    infer:
      prompt: |
        Create a concise executive summary from this research:
        {{with.data}}
      max_tokens: 500
```

## Workflow Header Fields

```yaml
schema: "nika/workflow@0.12"               # Required. Always "nika/workflow@0.12"
workflow: my-workflow          # Optional. Defaults to filename
description: "What it does"   # Optional
provider: anthropic            # Default LLM provider for all tasks
model: claude-sonnet-4-20250514  # Default model for all tasks

inputs:                        # Workflow parameters
  topic: "default value"

context:                       # File context bindings
  files:
    readme: ./README.md

skills:                        # Prompt augmentation files
  writing: ./skills/writing.md

artifacts:                     # Persist outputs to files
  dir: ./output
  format: markdown
```

## Data Flow

- **Bindings**: `with: { alias: $task_id }` then `{{with.alias}}`
- **Path access**: `with: { temp: $weather.data.temperature }`
- **Defaults**: `with: { val: $task.path ?? "fallback" }`
- **Env vars**: `with: { key: $env.API_KEY }`
- **Transforms**: `{{with.data | upper | trim}}`
- **Dependencies**: `depends_on: [task_id]` for ordering without data
- **Inputs**: `{{inputs.param}}` for workflow parameters
- **Context files**: `{{context.readme}}` for loaded file content

## Pipe Transforms (31 available)

**String**: `upper`, `lower`, `trim`, `trim_start`, `trim_end`, `length`, `to_string`
**Array**: `first`, `last`, `flatten`, `reverse`, `sort`, `unique`, `compact`, `keys`, `values`
**Numeric**: `to_number`, `round`, `abs`, `ceil`, `floor`
**Type**: `to_bool`, `to_json`, `parse_json`, `type_of`
**Parametric**: `join(", ")`, `split(",")`, `default("fallback")`
**System**: `shell` (execute as shell command)

Usage: `{{with.items | flatten | unique | join(", ")}}`

## Providers (7 Cloud + 1 Local)

| Provider | Env Var | Models |
|----------|---------|--------|
| `anthropic` | `ANTHROPIC_API_KEY` | claude-opus-4-20250514, claude-sonnet-4-20250514, claude-haiku-3.5 |
| `openai` | `OPENAI_API_KEY` | gpt-4o, gpt-4.1, o3, o4-mini |
| `mistral` | `MISTRAL_API_KEY` | mistral-large-latest, mistral-small-latest |
| `groq` | `GROQ_API_KEY` | llama-4-maverick, mixtral-8x7b |
| `deepseek` | `DEEPSEEK_API_KEY` | deepseek-chat, deepseek-reasoner |
| `gemini` | `GEMINI_API_KEY` | gemini-2.5-pro, gemini-2.5-flash |
| `xai` | `XAI_API_KEY` | grok-3 |
| `native` | (none) | Local GGUF via mistral.rs |

## For Each (Parallel Loop)

```yaml
- id: process
  for_each:
    items: "{{with.data}}"
    as: item
    concurrency: 3
  infer: "Process: {{with.item}}"
```

Access loop variable via `with:` prefix: `{{with.item}}` (same as all bindings).

## 24 Builtin Tools (nika:*)

**Always-on**: `nika:import`, `nika:dimensions`, `nika:thumbhash`, `nika:dominant_color`, `nika:pipeline`
**Media core**: `nika:thumbnail`, `nika:convert`, `nika:strip`, `nika:metadata`, `nika:optimize`, `nika:svg_render`
**Opt-in**: `nika:phash`, `nika:compare`, `nika:pdf_extract`, `nika:chart`, `nika:provenance`, `nika:verify`, `nika:qr_validate`, `nika:quality`, `nika:html_to_md`, `nika:css_select`, `nika:extract_metadata`, `nika:extract_links`, `nika:readability`

## Fetch Extract Modes (9)

| Mode | Description |
|------|-------------|
| `markdown` | Clean Markdown from HTML |
| `article` | Main article content (Readability) |
| `text` | Visible text, optionally filtered by `selector:` |
| `selector` | Raw HTML of matching elements (requires `selector:`) |
| `metadata` | OG, Twitter Cards, JSON-LD, SEO tags |
| `links` | Link classification (internal/external) |
| `jsonpath` | JSONPath query on JSON responses (requires `selector:` for path) |
| `feed` | RSS/Atom/JSON Feed parsing |
| `llm_txt` | AI content discovery (/llms.txt) |

## Common Mistakes

| Wrong | Right |
|-------|-------|
| `timeout: 30000` (ms) | `timeout: 30` (always seconds) |
| `use: { data: step1 }` | `with: { data: $step1 }` ($ prefix required) |
| `{{data}}` | `{{with.data}}` (always with. prefix) |
| `{{item}}` in for_each | `{{with.item}}` (loop var uses with. prefix) |
| `retry: 3` | `retry: { max_attempts: 3, delay_ms: 2000 }` |
| `.yaml` extension | `.nika.yaml` extension |
| Direct Cypher/SQL | Use `invoke:` with MCP tools |
| `shell: bash` | `shell: true` (boolean, not shell name) |
| Missing `schema:` line | Always start with `schema: "nika/workflow@0.12"` |
| `depends_on: task_id` | `depends_on: [task_id]` (always array) |

## Validation

```bash
nika check workflow.nika.yaml          # Validate syntax + DAG
nika check workflow.nika.yaml --strict # + test MCP connections
nika run workflow.nika.yaml            # Execute workflow
nika run workflow.nika.yaml --dry-run  # Validate without executing
nika ui                                # TUI
nika provider list                     # API key status
```
