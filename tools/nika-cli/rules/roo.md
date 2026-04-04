---
description: "Nika YAML workflow engine syntax and patterns"
globs: ["*.nika.yaml"]
---

# Nika Workflow Engine

Schema: `nika/workflow@0.12` | Extension: `.nika.yaml`

## 5 Verbs

| Verb | Purpose | Short form | Full form key fields |
|------|---------|------------|----------------------|
| `infer:` | LLM generation | `infer: "prompt"` | `prompt`, `system`, `model`, `temperature`, `max_tokens`, `content` |
| `exec:` | Shell command | `exec: "command"` | `command`, `shell`, `cwd`, `timeout`, `env` |
| `fetch:` | HTTP request | `fetch: "url"` | `url`, `method`, `headers`, `body`, `extract`, `selector`, `response` |
| `invoke:` | MCP / builtin tool | `invoke: "nika:tool"` | `tool`, `params`, `timeout`, `mcp`, `resource` |
| `agent:` | Multi-turn loop | *(no short form)* | `prompt`, `tools`, `max_turns`, `completion`, `guardrails` |

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
schema: "nika/workflow@0.12"     # Required. Always this exact string
workflow: my-workflow              # Optional. Defaults to filename
description: "What it does"       # Optional
provider: anthropic                # Default LLM provider for all tasks
model: claude-sonnet-4-20250514   # Default model for all tasks

inputs:                            # Workflow parameters
  topic: "default value"

context:                           # File context bindings
  files:
    readme: ./README.md

skills:                            # Prompt augmentation files
  writing: ./skills/writing.md

artifacts:                         # Persist outputs to files (see Artifacts section)
  dir: ./output
  format: markdown

include:                           # Include partial workflows (tasks merged into DAG)
  - path: ./partials/setup.nika.yaml
    prefix: setup_                 # Optional prefix for included task IDs
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

## Pipe Transforms (38 available)

**String**: `upper`, `lower`, `trim`, `trim_start`, `trim_end`, `length`, `to_string`
**Array**: `first`, `last`, `flatten`, `reverse`, `sort`, `unique`, `compact`, `keys`, `values`
**Numeric**: `to_number`, `round`, `abs`, `ceil`, `floor`
**Type**: `to_bool`, `to_json`, `parse_json`, `type_of`
**Parametric**: `join(", ")`, `split(",")`, `default("fallback")`
**System**: `shell` (shell-escape value for safe interpolation — NOT command execution)

Usage: `{{with.items | flatten | unique | join(", ")}}`

**Null safety**: 19 transforms fail on null input. Always guard with `default()`:
`{{with.result | default("none") | upper}}`

## Providers (7 Cloud + 1 Local + 1 Mock)

| Provider | Env Var | Models |
|----------|---------|--------|
| `anthropic` | `ANTHROPIC_API_KEY` | claude-opus-4-20250514, claude-sonnet-4-20250514, claude-haiku-4-5 |
| `openai` | `OPENAI_API_KEY` | gpt-4o, gpt-4.1, o3, o4-mini |
| `mistral` | `MISTRAL_API_KEY` | mistral-large-latest, mistral-small-latest |
| `groq` | `GROQ_API_KEY` | llama-3.3-70b-versatile, mixtral-8x7b-32768 |
| `deepseek` | `DEEPSEEK_API_KEY` | deepseek-chat, deepseek-reasoner |
| `gemini` | `GEMINI_API_KEY` | gemini-2.5-pro, gemini-2.5-flash |
| `xai` | `XAI_API_KEY` | grok-3 |
| `native` | (none) | Local GGUF via mistral.rs (text only — no vision) |
| `mock` | (none) | Deterministic test responses — no API calls, no keys needed |

## Infer Verb (Full Form)

```yaml
- id: generate
  infer:
    prompt: "Your prompt here"
    system: "You are a helpful assistant"
    model: claude-sonnet-4-20250514    # Task-level override
    temperature: 0.7                   # 0.0 - 2.0
    max_tokens: 1000                   # Max output tokens
    extended_thinking: true            # Claude extended thinking
    thinking_budget: 10000             # Thinking token budget
    response_format: json              # text, json, markdown
```

### Vision / Multimodal (since v0.34.0)

Use `content:` array instead of `prompt:` for images:

```yaml
- id: analyze_image
  infer:
    content:
      - type: image
        source: "{{with.photo_hash}}"  # CAS hash — auto-converted to base64
        detail: high                   # high | low | auto
      - type: text
        text: "Describe this image in detail"
    # prompt: optional — if present, prepended as first text part
```

**Vision rules:**
- `source:` must be a CAS hash from `nika:import`, `nika:decode`, or `fetch: … response: binary` — NEVER a file path
- Supported providers: anthropic, openai, mistral, groq, gemini, xai
- `provider: native` with GGUF = **text only**; use cloud provider for vision
- `provider: deepseek` = VisionNotSupported error

## Structured Output (since v0.35.0)

`structured:` enforces schema-validated JSON output with automatic retry and repair.
**Different from `output: { format: json }`** which is formatting only — no validation, no repair.

```yaml
- id: extract
  infer:
    prompt: "Extract product data from: {{with.text}}"
  structured:
    schema:
      type: object
      properties:
        name: { type: string }
        price: { type: number }
        in_stock: { type: boolean }
      required: [name, price]
    enable_repair: true           # LLM auto-repair on violation (default: true)
    max_retries: 3                # Retry attempts before failure (default: 2)
    repair_model: claude-haiku-4-5  # Cheaper model for repair passes (default: task model)
```

**5-layer defense**: tool injection → rig extractor → JSON validation → retry with feedback → LLM repair

## Exec Verb (Full Form)

```yaml
- id: build
  exec:
    command: "npm run build"
    shell: true                        # Run via sh -c (default: false — use for pipes/redirects)
    cwd: "./frontend"
    timeout: 60                        # Seconds
    env:
      NODE_ENV: production
```

## Fetch Verb (Full Form + Extract)

```yaml
- id: scrape
  fetch:
    url: "https://example.com/article"
    method: GET
    headers:
      Authorization: "Bearer {{inputs.token}}"
    body: "raw string body"            # String body (for POST/PUT)
    json:                              # Structured JSON body — auto-serialized (alternative to body:)
      key: value
    follow_redirects: true             # Follow HTTP redirects (default: true)
    extract: markdown                  # Post-processing mode
    selector: "main article"           # CSS selector (for text/selector modes) or JSONPath (for jsonpath mode)
    response: full                     # full | binary | omit for raw body
    timeout: 30
```

**`response: full` returns**: `{ "status": 200, "headers": {...}, "body": "...", "url": "https://..." }`

### 9 Extract Modes

| Mode | Description | `selector:` |
|------|-------------|-------------|
| `markdown` | Clean Markdown from HTML | No |
| `article` | Main article content (Readability) | No |
| `text` | Visible text, optionally filtered | Optional (CSS selector) |
| `selector` | Raw HTML of matching elements | Required (CSS selector) |
| `metadata` | OG, Twitter Cards, JSON-LD, SEO | No |
| `links` | Link classification (internal/external) | No |
| `jsonpath` | JSONPath query on JSON responses | Required (**JSONPath expression**, e.g. `$.data[0].name`) |
| `feed` | RSS/Atom/JSON Feed parsing | No |
| `llm_txt` | AI content discovery (/llms.txt) | No |

**Note**: `extract: jsonpath` uses `selector:` for the JSONPath expression, not a CSS selector.
**Note**: `response: binary` stores raw bytes in CAS, returns a hash for the media pipeline.

## Invoke Verb (MCP + Builtin Tools)

```yaml
- id: search
  retry:                               # Task-level retry — NOT inside invoke block
    max_attempts: 3
    delay_ms: 1000
    backoff: 2.0                       # Exponential backoff multiplier (optional)
  invoke:
    tool: "novanet::novanet_search"    # server::tool_name — ALWAYS double colon
    params:
      query: "{{with.topic}}"
      limit: 10
    timeout: 30
    mcp: novanet                       # Explicit server (alternative to server:: prefix)
    resource: "novanet://entity/123"   # Resource URI (alternative to tool:)
```

**Tool naming rules:**
- `nika:tool_name` — 62 builtin tools (always available, no server needed)
- `server::tool_name` — MCP server tools (double colon `::`, server must be running)
- `mcp: server` + `tool: name` — split form (equivalent to `server::name`)
- Short form for builtins: `invoke: "nika:thumbnail"`
- Single colon or slash separator are **wrong** and will fail

**`retry:` is task-level** — place it alongside `invoke:`, not inside it. Applies to all verbs.

## Agent Verb (Full Reference)

```yaml
- id: assistant
  agent:
    system: "You are a research assistant"
    prompt: "Find and analyze {{inputs.topic}}"
    tools: [novanet::novanet_search, novanet::fetch_node]
    max_turns: 10
    token_budget: 50000
    temperature: 0.5
    tool_choice: auto                  # auto | required | none

    # Completion mode — how the agent signals it is done
    completion:
      mode: explicit                   # explicit (default) | natural | pattern
      # explicit: agent must call nika:complete tool to stop
      # natural: stops when agent makes no more tool calls
      # pattern: stops when output matches a regex

    # Guardrails — 4 types available
    guardrails:
      - type: length
        min_words: 100
        max_words: 2000
        on_failure: retry              # retry | escalate | fail
      - type: schema
        json_schema:
          type: object
          properties:
            findings: { type: array }
          required: [findings]
        on_failure: escalate
      - type: regex
        pattern: "^## (Findings|Summary)"
        message: "Response must start with ## Findings or ## Summary"
        on_failure: retry
      - type: llm
        judge_prompt: "Is this response factually accurate? Reply PASS or FAIL."
        pass_pattern: "^PASS"
        on_failure: retry

    # Cost / time limits
    limits:
      max_cost_usd: 2.0
      max_duration_secs: 120

    # Advanced
    from: researcher                   # Reuse a named agent preset from agents: header section
    skills: [writing, code-review]     # Inject skill files into system prompt
    stop_sequences: ["DONE", "---"]    # Custom generation stop tokens
    mcp: [novanet, filesystem]         # Explicit MCP server list for this agent
```

**Max turns**: exceeding `max_turns` causes a **graceful stop with partial result** — NOT an error.

## For Each (Parallel Loop)

```yaml
- id: process
  for_each:
    items: "{{with.data}}"
    as: item
    concurrency: 3
    fail_fast: false                   # false = continue all; true (default) = stop on first failure
  infer: "Process: {{with.item}}"
```

**CRITICAL: `for_each` output is a JSON array**, not a scalar. Downstream tasks must use array access:

```yaml
- id: consume
  depends_on: [process]
  with:
    results: $process                  # Value::Array([result_0, result_1, ...])
    first: "{{with.results | first}}"
    count: "{{with.results | length}}"
  infer: "Processed {{with.count}} items. First result: {{with.first}}"
```

**Wrong**: `{{with.results.field}}` — results is an array.
**Right**: `{{with.results[0].field}}` or `{{with.results | first}}`.

## Artifacts (Full Form)

Workflow-level defaults:
```yaml
artifacts:
  dir: ./output
  format: markdown                     # text | json | yaml | binary
  mode: overwrite                      # overwrite | append | unique | fail
  manifest: true                       # Write artifacts.json index at end
  max_size: 104857600                  # Max bytes per file (default: 100MB)
```

Task-level (single):
```yaml
- id: report
  infer: "Generate report"
  artifact:
    path: report.md
    format: markdown
    mode: unique
```

Task-level (source binding — save upstream data, not task output):
```yaml
- id: save_raw
  artifact:
    path: data.json
    source: raw_data                   # Bind from with: alias
    format: json
```

Binary artifact (media pipeline):
```yaml
- id: convert_image
  invoke: nika:convert
  params: { input: "photo.png", format: webp }
  artifact:
    path: output.webp
    format: binary                     # Store raw CAS bytes directly
```

## 62 Builtin Tools (nika:*)

**Core (7)**: `nika:sleep`, `nika:log`, `nika:emit`, `nika:assert`, `nika:prompt`, `nika:run`, `nika:complete`
**File (5)**: `nika:read`, `nika:write`, `nika:edit`, `nika:glob`, `nika:grep`
**Introspection (6)**: `nika:dag_info`, `nika:task_status`, `nika:threads`, `nika:orchestrate`, `nika:cost`, `nika:records`
**Data (13)**: `nika:json_merge`, `nika:set_diff`, `nika:zip`, `nika:map`, `nika:filter`, `nika:group_by`, `nika:chunk`, `nika:token_count`, `nika:enrich`, `nika:jq`, `nika:tree_data`, `nika:inject`, `nika:json_query`†
**Data Sprint 2 (6)**: `nika:json_verify`, `nika:yaml_validate`, `nika:locale_lookup`, `nika:aggregate`, `nika:json_flatten`, `nika:json_unflatten`
**Media always-on (5)**: `nika:import`, `nika:decode`, `nika:dimensions`, `nika:thumbhash`, `nika:dominant_color`
**Media core (3)**: `nika:thumbnail`, `nika:convert`, `nika:strip`
**Media opt-in (17)**: `nika:metadata`, `nika:optimize`, `nika:svg_render`, `nika:chart`, `nika:phash`, `nika:compare`, `nika:pdf_extract`, `nika:provenance`, `nika:verify`, `nika:qr_validate`, `nika:quality`, `nika:html_to_md`, `nika:css_select`, `nika:extract_metadata`, `nika:extract_links`, `nika:readability`, `nika:pipeline`

† `nika:json_query` is deprecated — use `nika:jq` instead

## Pipeline Patterns

### Fan-Out / Fan-In

```yaml
tasks:
  - id: get_urls
    infer: "List 5 URLs about {{inputs.topic}}"
    structured:
      schema:
        type: object
        properties:
          urls: { type: array, items: { type: string } }

  - id: scrape_all
    with:
      urls: $get_urls
    for_each:
      items: "{{with.urls.urls}}"
      as: url
      concurrency: 5
    fetch:
      url: "{{with.url}}"
      extract: article

  - id: synthesize
    with:
      articles: $scrape_all            # Array of article texts from for_each
    infer: "Synthesize these articles into a report: {{with.articles | join('\n---\n')}}"
```

### Testing Without API Calls

```yaml
schema: "nika/workflow@0.12"
workflow: my-test
provider: mock                         # Returns deterministic responses, no API key needed

tasks:
  - id: step1
    infer: "Test prompt"               # Returns mock JSON instantly
```

Run: `nika run workflow.nika.yaml --provider mock` or `nika run workflow.nika.yaml --dry-run`

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
| `tool: "server/tool"` (slash) | `tool: "server::tool"` (double colon) |
| `output: { format: json }` | `structured: { schema: ... }` for validated JSON |
| `{{with.results.field}}` after for_each | `{{with.results[0].field}}` (for_each = array) |
| Passing file path to vision `source:` | Pass CAS hash from `nika:import` output |
| `provider: native` for vision | GGUF is text-only — use cloud provider |
| `provider: deepseek` for vision | DeepSeek doesn't support vision |
| `retry:` inside `invoke:` block | `retry:` is task-level — place it alongside `invoke:`, not inside |
| `body: {...}` for JSON payloads | Use `json: {...}` — auto-serializes objects (body: is strings only) |
| `invoke: { tool: "...", input: {...} }` | `invoke: { tool: "...", params: {...} }` — field is `params:` not `input:` |
| `retry: { max_retries: N }` | `retry: { max_attempts: N }` — `max_retries` is for `structured:` validation retries |
| `for_each` without `concurrency:` | Default is **sequential** (concurrency: 1) — set `concurrency: N` for parallel |
| `thinking: true` at task level | `infer: { extended_thinking: true, thinking_budget: N }` inside infer block |
| `max_retries: 3` at task level | `retry: { max_attempts: 3 }` — `max_retries` is only valid inside `structured:` |
| `model: haiku` inside `infer:` block | `model: claude-haiku-4-5` at task level — model goes on the task, not the verb |
| `$()` in shell: true commands | NIKA-053 blocks `$(` — use transforms or exec without shell instead |

## Key Error Codes

| Code | Meaning |
|------|---------|
| NIKA-010 | Schema validation error |
| NIKA-020 | DAG cycle detected |
| NIKA-026 | Dependency chain failed — upstream task failed, downstream blocked |
| NIKA-041 | Template resolution error |
| NIKA-045 | Fetch error (SSRF blocked, timeout, invalid URL) |
| NIKA-046 | Extract error (CSS selector failed, unsupported extract mode) |
| NIKA-053 | Blocked command (security) |
| NIKA-071 | Unknown alias — `{{with.alias}}` not declared in `with:` block |
| NIKA-072 | Null value at path (strict mode) — guard with `default()` |
| NIKA-100 | MCP connection error |
| NIKA-101 | MCP server failed to start |
| NIKA-107 | MCP parameter validation failed — missing or invalid tool params |
| NIKA-112 | Agent guardrail violation |
| NIKA-140 | AST analysis failure |
| NIKA-281 | Artifact write failed (path, permissions, disk space) |
| NIKA-300 | Structured output validation failed |

## Architecture

- Nika connects to NovaNet via MCP protocol ONLY. No direct database access.
- Use `invoke:` verb for all MCP and builtin tool calls.
- Errors use `NikaError` with NIKA-XXX codes (see table above).
- Extensions: `.nika.yaml` for workflows.
- Zero Cypher rule: never write raw Cypher/SQL — always use `invoke:` with MCP tools.

## Security

- API keys: env vars only (`$env.API_KEY`). Never hardcode in workflow YAML.
- `fetch:` validates URLs against SSRF (private IP ranges blocked by default).
- `exec:` has command blocklist: `rm -rf /`, `sudo`, fork bombs blocked (NIKA-053).
- `shell: true` enables shell features — use only when pipe/redirect is required.
- File paths validated against directory traversal (`../`) attacks.
- Traces may contain API responses — never commit `.nika/traces/` to git.

## Validation

```bash
nika check workflow.nika.yaml           # Validate syntax + DAG
nika check workflow.nika.yaml --strict  # + test MCP connections
nika run workflow.nika.yaml             # Execute workflow
nika run workflow.nika.yaml --dry-run   # Validate without executing
nika run workflow.nika.yaml --provider mock  # Test without API calls
nika ui                                 # TUI
nika provider list                      # API key status
```
