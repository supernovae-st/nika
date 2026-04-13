# Nika Concepts Glossary

> Every concept in the Nika workflow engine, from A to Z.

Each entry includes a definition, a concrete YAML example, and cross-references to related concepts. This glossary covers the full system as of schema `nika/workflow@0.12`.

---

## A

### Agent

An autonomous LLM loop that calls tools iteratively until a completion condition is met. The `agent:` verb creates a multi-turn conversation where the LLM decides which tools to call, receives results, and continues until it signals completion or hits a safety limit.

```yaml
tasks:
  - id: researcher
    agent:
      prompt: "Research the topic and summarize findings."
      tools: [builtin]
      max_turns: 10
      token_budget: 8000
      completion:
        mode: explicit
```

**Related**: Completion Mode, Guardrails, Limits, Tool Choice, Token Budget

### Alias

A name given to a task's output inside a `with:` block, making it available as a template variable. Aliases let you reference upstream task outputs by human-readable names instead of raw task IDs.

```yaml
with:
  article: $fetch_page
  summary: $summarize
```

Here, `article` and `summary` are aliases. They are used in templates as `{{with.article}}` and `{{with.summary}}`.

**Related**: Binding, Template, With Block

### Analyzer (Phase 2)

The second phase of the AST pipeline. The Analyzer takes a Raw AST (from the parser) and produces an Analyzed AST with validated references, resolved dependencies, feature gates, and error diagnostics. This is where structural correctness is verified.

Pipeline: Raw AST (Phase 1) --> Analyzed AST (Phase 2) --> Runtime types (Phase 3, Lower).

**Related**: AST, Parser, Lower

### Artifact

A file output produced by a workflow task. Artifacts are written to a specified directory with optional format and manifest support. They provide a structured way to capture workflow results as files.

```yaml
artifacts:
  dir: ./output/reports
  format: text
  manifest: true

tasks:
  - id: report
    infer: "Generate a status report."
    artifact:
      path: output/status-report.md
```

**Related**: CAS, Output

### AST (Abstract Syntax Tree)

The internal representation of a parsed workflow. Nika uses a three-phase AST pipeline:
1. **Raw AST** (Phase 1): Direct YAML parse result with source spans
2. **Analyzed AST** (Phase 2): Validated, resolved, with diagnostics
3. **Runtime types** (Phase 3, Lower): Execution-ready structures

You never interact with the AST directly; it is an implementation detail of the `nika check` and `nika run` pipeline.

**Related**: Analyzer, Parser, Lower, Schema

---

## B

### Binding

The mechanism for passing data between tasks. Bindings are declared in `with:` blocks using the `$task_id` syntax to reference upstream task outputs. The binding system supports direct references, JSONPath access, environment variables, and lazy resolution.

```yaml
with:
  data: $fetch_api
  name: $data.response.user.name
  token: $env.API_TOKEN
```

**Related**: Alias, JSONPath, Template, With Block

### Blocklist (Command)

A security feature that prevents execution of dangerous shell commands. The blocklist includes commands like `rm -rf /`, `mkfs`, and other destructive operations. Applied automatically when using the `exec:` verb.

**Related**: Security, Exec

### Boss Level

A course level that gates progression and requires mastery of all prior concepts. In the current course, Level 12 (SuperNovae) is the only boss level. All 5 exercises must pass before the course is considered complete.

**Related**: Course, Level

### Builtin Tools

Tools that ship embedded in the Nika binary. They require no network, no installation, and no API keys. Accessed via the `invoke:` verb with the `nika:` namespace prefix. There are 12 core tools and 24 media tools across 3 tiers.

Core builtins: `nika:log`, `nika:emit`, `nika:assert`, `nika:sleep`, `nika:complete`, `nika:run`
File tools: `nika:read`, `nika:write`, `nika:edit`, `nika:glob`, `nika:grep`

**Related**: Invoke, Media Tools, Namespace

---

## C

### CAS (Content-Addressable Storage)

A storage system where files are addressed by their content hash rather than their file path. When you import a file with `nika:import`, it is stored in the CAS and referenced by its hash. This guarantees deduplication, integrity, and immutability.

```yaml
tasks:
  - id: import_photo
    invoke:
      tool: "nika:import"
      params:
        path: "./photos/landscape.jpg"
```

The output includes the CAS hash, which can be used in vision content blocks or downstream media tools.

**Related**: Media Tools, Import, Vision

### Check

The `nika check` command validates a workflow file without executing it. It parses the YAML, runs the three-phase AST pipeline, validates the DAG, checks for cycles, verifies bindings, and reports any errors with NIKA-XXX error codes.

```bash
nika check my-workflow.nika.yaml
```

**Related**: AST, Error Code, Validation

### Completion Mode

Controls how an agent signals that it has finished its task. Three modes are available:

| Mode | Behavior |
|------|----------|
| `explicit` | Agent must call `nika:complete` (recommended for complex tasks) |
| `natural` | Completes when the LLM stops making tool calls |
| `pattern` | Completes when output matches a regex pattern |

```yaml
agent:
  completion:
    mode: explicit
    signal:
      tool: nika:complete
      fields:
        required: [result]
```

**Related**: Agent, Guardrails, Limits

### Concurrency

The number of parallel iterations allowed when using `for_each:`. Controls resource usage for parallel fan-out patterns.

```yaml
for_each: [a, b, c, d, e]
concurrency: 3
```

**Related**: DAG, For Each, Parallel Execution

### Context File

An external file loaded into a task's context. Allows injecting large content (prompts, data files, schemas) without inlining everything in the YAML.

**Related**: Imports, Inputs

### Course

The interactive 12-level learning system built into Nika. Generated with `nika init --course`, it creates 44 exercises with TODO markers, MISSION.md briefings for each level, and a progress tracking system.

```bash
nika init --course
nika course status
nika course check 1
nika course hint
nika course next
```

**Related**: Level, Exercise, Hint, Boss Level

---

## D

### DAG (Directed Acyclic Graph)

The execution model used by Nika. Every workflow is compiled into a DAG where tasks are nodes and `depends_on` relationships are edges. Tasks without dependencies run in parallel automatically. The DAG validator checks for cycles, missing dependencies, and unreachable tasks.

```
        [start]
        /      \
   [task_a]  [task_b]
        \      /
        [merge]
```

In this diamond pattern, `task_a` and `task_b` run simultaneously after `start`, and `merge` waits for both.

**Related**: Depends On, Parallel Execution, Cycle Detection

### Depends On

The field that declares explicit ordering between tasks. Without `depends_on`, tasks run in parallel. With it, a task waits for all listed dependencies to complete before starting.

```yaml
tasks:
  - id: fetch_data
    fetch: https://api.example.com/data
  - id: process
    depends_on: [fetch_data]
    exec: echo "Data fetched"
```

**Related**: DAG, Parallel Execution, With Block

### Diamond Pattern

A common DAG shape where a single task fans out to multiple parallel tasks, which then fan back into a single merge task. This maximizes parallelism while ensuring all results are collected before the next stage.

**Related**: DAG, Depends On, Fan-Out

---

## E

### Error Code

Every Nika error has a unique code in the format `NIKA-XXX`. Error codes are grouped by category:

| Range | Category |
|-------|----------|
| 000-009 | Workflow errors |
| 010-019 | Schema/validation |
| 020-029 | DAG errors |
| 030-039 | Provider errors |
| 040-049 | Template/binding |
| 050-059 | Path/task/security |
| 060-069 | Output (JSON/schema) |
| 070-079 | With block validation |
| 080-089 | DAG validation |
| 090-099 | JSONPath/IO/Execution |
| 100-109 | MCP errors |
| 110-119 | Agent + Guardrails |
| 120-129 | Resilience |
| 130-139 | TUI errors |
| 140-151 | AST analysis |
| 160-166 | Parse/Policy/Boot |
| 200-219 | File/Builtin tools |
| 251-259 | Media pipeline |
| 260-269 | Package URI |
| 270-279 | Skill errors |
| 280-285 | Artifact/media |
| 290-297 | Media tools |
| 300-309 | Structured output |
| 310-319 | Course errors |

**Related**: Check, Validation, NikaError

### Exec

One of the 5 verbs. Runs a shell command and captures stdout, stderr, and exit code. Supports shorthand (string) and full form (object with `command:`, `shell:`, `timeout:`, `env:`, `cwd:`).

```yaml
# Shorthand
- id: list
  exec: "ls -la"

# Full form
- id: info
  exec:
    command: "uname -s && whoami"
    shell: true
    timeout: 10
    env:
      LANG: "en_US.UTF-8"
```

**Related**: Verb, Shell, Timeout, Security

### Exercise

A single learning unit within a course level. Each exercise has a template (with TODO markers for the learner to complete) and a solution (complete, valid YAML that passes `nika check`). Exercises are graded automatically by the course check system.

**Related**: Course, Level, Hint, Check

### Extract

A `fetch:` option that specifies how to post-process the HTTP response body. Nine extract modes are available:

| Mode | Description |
|------|-------------|
| `markdown` | Clean Markdown from HTML |
| `article` | Main article content (Readability) |
| `text` | Visible text, optionally filtered by CSS selector |
| `selector` | Raw HTML matching CSS selectors |
| `metadata` | OG, Twitter Cards, JSON-LD, SEO tags |
| `links` | Rich link classification |
| `jsonpath` | JSONPath query on JSON responses |
| `feed` | RSS/Atom/JSON Feed parsing |
| `llm_txt` | AI-era content discovery |

```yaml
- id: scrape
  fetch:
    url: "https://example.com"
    extract: markdown
```

**Related**: Fetch, Selector, Response Mode

---

## F

### Fail Fast

A workflow-level setting that cancels all remaining tasks when any task fails. Without fail_fast, other independent branches continue executing.

**Related**: On Error, DAG, Resilience

### Fetch

One of the 5 verbs. Makes HTTP requests and returns the response. Supports GET (default), POST, PUT, DELETE, PATCH with headers, JSON bodies, extract modes, and response modes.

```yaml
- id: get_data
  fetch:
    url: "https://httpbin.org/json"
    headers:
      Accept: "application/json"
    extract: jsonpath
    selector: "$.slideshow.title"
```

**Related**: Verb, Extract, Response Mode, Headers

### For Each

A task-level field that iterates over a list, executing the task once per item. Supports concurrency limits for controlled parallelism.

```yaml
- id: translate
  for_each: ["en", "fr", "de", "ja"]
  concurrency: 2
  infer:
    prompt: "Translate to {{with.item}}: Hello world"
```

**Related**: Concurrency, DAG, Parallel Execution

---

## G

### Guardrails

Validation rules applied to agent output before accepting it. Four types are available:

| Type | Validates |
|------|-----------|
| `length` | Word count bounds (`min_words`, `max_words`) |
| `regex` | Output matches a pattern |
| `schema` | JSON validates against a JSON Schema |
| `llm` | Secondary LLM evaluates quality |

Each guardrail has an `on_failure` action: `retry` (ask agent to fix), `escalate` (flag for review), or `fail` (stop the agent).

```yaml
guardrails:
  - type: length
    min_words: 100
    max_words: 500
    on_failure: retry
  - type: regex
    pattern: "^## "
    message: "Output must start with a markdown heading"
    on_failure: retry
```

**Related**: Agent, Completion Mode, Limits

---

## H

### Headers

HTTP headers sent with `fetch:` requests. Specified as a key-value map under the `headers:` field.

```yaml
fetch:
  url: "https://api.example.com/data"
  headers:
    Authorization: "Bearer {{with.token}}"
    Accept: "application/json"
```

**Related**: Fetch, Template

### Hint

Progressive help available for course exercises. Three tiers of increasing specificity:
1. **Conceptual** -- high-level concept nudge
2. **Specific** -- specific technique or pattern
3. **Solution** -- near-complete solution

Using fewer hints earns bonus achievements. Access with `nika course hint`.

**Related**: Course, Exercise, Bonus

---

## I

### Infer

One of the 5 verbs. Sends a prompt to an LLM provider and returns the completion. Supports shorthand (string prompt) and full form (object with `prompt:`, `system:`, `temperature:`, `max_tokens:`, `output:`, `content:`).

```yaml
# Shorthand
- id: think
  infer: "Explain open source in one sentence."

# Full form
- id: analyze
  infer:
    prompt: "Summarize: {{with.article}}"
    system: "You are a concise technical writer."
    temperature: 0.3
    max_tokens: 500
    output:
      format: json_schema
      schema:
        type: object
        properties:
          summary: { type: string }
        required: [summary]
```

**Related**: Verb, Provider, Model, Structured Output, Vision, Temperature

### Imports

A workflow-level field for importing definitions from other YAML files. Enables sharing common configurations, task definitions, and schemas across multiple workflows.

**Related**: Context File, Inputs

### Inputs

A workflow-level field for declaring parameters that can be overridden from the CLI. Makes workflows reusable and configurable without editing the YAML.

```yaml
inputs:
  url: "https://example.com"
  depth: 3
  output_format: "markdown"
```

Override at runtime: `nika run workflow.nika.yaml --input url=https://other.com --input depth=5`

**Related**: Imports, Template

### Invoke

One of the 5 verbs. Calls a tool by name with parameters. Used for builtin tools (`nika:*` namespace), MCP server tools, and external tool calls.

```yaml
- id: write_report
  invoke:
    tool: "nika:write"
    params:
      file_path: "report.txt"
      content: "{{with.analysis}}"
```

**Related**: Verb, Builtin Tools, MCP, Media Tools

---

## J

### JSONPath

A query language for extracting values from JSON data. Used in `with:` blocks to reach into nested task outputs, and in `fetch:` with `extract: jsonpath`.

Supported syntax: dot notation for objects (`$.response.user.name`), brackets for arrays (`$.items[0]`), simple paths only (no wildcards or filters).

```yaml
with:
  name: $api_response
# Access as: {{with.name.data.users[0].email}}
```

**Related**: Binding, Template, Extract

---

## L

### Level

A grouping of exercises in the course system. Each level has a theme, a slug, a description, and a fixed number of exercises. There are 12 levels total:

| # | Slug | Name | Exercises |
|---|------|------|-----------|
| 1 | jailbreak | Jailbreak | 5 |
| 2 | hot_wire | Hot Wire | 4 |
| 3 | fork_bomb | Fork Bomb | 4 |
| 4 | root_access | Root Access | 3 |
| 5 | shapeshifter | Shapeshifter | 3 |
| 6 | pay_per_dream | Pay-Per-Dream | 3 |
| 7 | swiss_knife | Swiss Knife | 3 |
| 8 | gone_rogue | Gone Rogue | 3 |
| 9 | data_heist | Data Heist | 4 |
| 10 | open_protocol | Open Protocol | 3 |
| 11 | pixel_pirate | Pixel Pirate | 4 |
| 12 | supernovae | SuperNovae | 5 |

**Related**: Course, Exercise, Boss Level

### Limits

Cost and resource controls for agent loops. Prevent runaway agents from consuming excessive tokens or running indefinitely.

| Limit | Description |
|-------|-------------|
| `max_turns` | Maximum loop iterations |
| `token_budget` | Total token budget across all turns |
| `max_cost_usd` | Dollar cost ceiling |
| `max_duration_secs` | Wall-clock timeout |

```yaml
agent:
  limits:
    max_turns: 20
    max_cost_usd: 0.50
    max_duration_secs: 120
```

**Related**: Agent, Guardrails, Token Budget

### Lower (Phase 3)

The third phase of the AST pipeline. Takes an Analyzed AST and produces runtime-ready types that the execution engine can directly process. This is the final transformation before execution.

**Related**: AST, Analyzer, Parser

---

## M

### MCP (Model Context Protocol)

An open protocol for connecting AI models to external tools and data sources. Nika acts as an MCP client, connecting to MCP servers configured in the workflow. Tools from MCP servers are called via the `invoke:` verb.

```yaml
mcp:
  servers:
    novanet:
      command: "cargo"
      args: ["run", "--", "mcp"]
      cwd: "../novanet"

tasks:
  - id: query
    invoke:
      tool: "novanet:search"
      params:
        query: "workflow patterns"
```

**Related**: Invoke, NovaNet, Protocol, Tool

### Media Tools

24 builtin tools for image and media processing, organized in 3 tiers:

**Tier 1 -- Always-on** (5 tools): `nika:import`, `nika:dimensions`, `nika:thumbhash`, `nika:dominant_color`, `nika:pipeline`

**Tier 2 -- media-core default** (6 tools): `nika:thumbnail`, `nika:convert`, `nika:strip`, `nika:metadata`, `nika:optimize`, `nika:svg_render`

**Tier 3 -- Opt-in** (13 tools): `nika:phash`, `nika:compare`, `nika:pdf_extract`, `nika:chart`, `nika:provenance`, `nika:verify`, `nika:qr_validate`, `nika:quality`, `nika:html_to_md`, `nika:css_select`, `nika:extract_metadata`, `nika:extract_links`, `nika:readability`

**Related**: CAS, Invoke, Pipeline, Vision

### Model

The specific LLM model to use for `infer:` and `agent:` tasks. Specified in `provider/model` format. Can be set at workflow level (default for all tasks) or overridden per task.

```yaml
# Workflow-level default
model: "anthropic/claude-sonnet-4-20250514"

tasks:
  - id: fast_task
    model: "groq/llama-4-maverick"
    infer: "Quick response needed"
```

**Related**: Provider, Infer, Agent

---

## N

### Namespace

The `nika:` prefix for builtin tools. All tools that ship with the Nika binary use this namespace (e.g., `nika:log`, `nika:write`, `nika:thumbnail`). MCP server tools use their server name as namespace (e.g., `novanet:search`).

**Related**: Builtin Tools, MCP, Invoke

### NikaError

The unified error type used throughout the engine. Every error variant includes a NIKA-XXX code, a human-readable message, and optional fix suggestions. Implements both `thiserror::Error` for compatibility and `miette::Diagnostic` for rich terminal display.

**Related**: Error Code, Check

### NovaNet

The knowledge graph and MCP server that acts as Nika's "brain." Nika connects to NovaNet exclusively via MCP protocol (Zero Cypher rule). NovaNet stores NodeClasses, ArcClasses, and provides semantic search capabilities.

**Related**: MCP, Invoke, Zero Cypher Rule

---

## O

### On Error

A task-level field that controls behavior when a task fails. Options: `fail` (default, stops the workflow), `continue` (mark failed but allow other tasks to proceed).

```yaml
- id: optional_check
  exec: "curl -s https://unstable-api.example.com/health"
  on_error: continue
```

**Related**: Fail Fast, Resilience, Retry

### Output

A task-level field that specifies the expected format and schema of a task's output. Used with `infer:` to force structured JSON responses from LLMs.

```yaml
output:
  format: json_schema
  schema:
    type: object
    properties:
      sentiment: { type: string, enum: [positive, negative, neutral] }
      confidence: { type: number }
    required: [sentiment, confidence]
```

Formats: `json` (any valid JSON), `json_schema` (JSON matching a schema), or omitted (raw text).

**Related**: Structured Output, Infer, Schema

---

## P

### Parallel Execution

Tasks without explicit `depends_on` relationships run simultaneously. The DAG scheduler automatically identifies independent tasks and executes them in parallel, maximizing throughput.

**Related**: DAG, Depends On, For Each, Concurrency

### Parser (Phase 1)

The first phase of the AST pipeline. Reads raw YAML text and produces a Raw AST with source spans for error reporting. The parser validates syntax and basic structure but not semantic correctness.

**Related**: AST, Analyzer, Lower

### Pipeline (Media)

The `nika:pipeline` tool chains multiple media operations in memory without writing intermediate files to disk. Significantly faster and more efficient than chaining individual tools.

```yaml
- id: process
  invoke:
    tool: "nika:pipeline"
    params:
      input: "{{with.photo.media[0].hash}}"
      operations:
        - thumbnail: { width: 256 }
        - convert: { format: webp }
        - optimize: {}
```

**Related**: Media Tools, CAS, Invoke

### Pipe Transform

Inline data transformation applied within template expressions using the `|` (pipe) operator. Transforms chain left to right.

```yaml
exec: echo "{{with.name | trim | uppercase}}"
```

Full catalog: `upper`, `lower`, `trim`, `trim_start`, `trim_end`, `length`, `first`, `last`, `keys`, `values`, `flatten`, `reverse`, `sort`, `unique`, `compact`, `to_string`, `to_number`, `to_bool`, `to_json`, `parse_json`, `round`, `abs`, `ceil`, `floor`, `type_of`, `shell`, `join`, `split`, `default`

**Related**: Template, Binding, With Block

### Provider

An LLM service that Nika can connect to for `infer:` and `agent:` tasks. Nika supports 9 providers. The provider is configured via environment variables (API keys) and selected in the workflow.

Supported providers: Anthropic (Claude), OpenAI, Mistral, Groq, DeepSeek, Google (Gemini), xAI (Grok), Cohere, and `native` for local GGUF inference.

```yaml
provider: anthropic
model: claude-sonnet-4-20250514
```

Auto-detection: `RigProvider::auto()` scans environment variables and selects the first available provider.

**Related**: Model, Infer, Agent, Native

---

## R

### Response Mode

A `fetch:` option that controls the shape of the HTTP response output.

| Mode | Output |
|------|--------|
| `full` | JSON with status, headers, body, final URL |
| `binary` | Store in CAS, return hash |
| (default) | Raw body text |

```yaml
fetch:
  url: "https://example.com/image.png"
  response: binary
```

**Related**: Fetch, Extract, CAS

### Retry

A task-level configuration for automatic retry on failure. Supports max attempts and backoff strategies.

```yaml
retry:
  max_attempts: 3
  backoff: exponential
```

Also used in structured output (`enable_retry:`) and guardrails (`on_failure: retry`).

**Related**: On Error, Resilience, Guardrails

### RunContext

The runtime data store that holds task results during workflow execution. The binding system resolves `$task_id` references by looking up completed task outputs in the RunContext.

**Related**: Binding, DAG, Task Result

---

## S

### Schema

The version declaration at the top of every workflow file. Currently `nika/workflow@0.12`. The schema version determines which features are available (e.g., `for_each` requires `@0.3+`).

```yaml
schema: "nika/workflow@0.12"
```

**Related**: AST, Check, Validation

### Selector

A CSS selector or JSONPath expression used with `fetch:` extract modes. For `extract: selector`, it filters HTML elements. For `extract: jsonpath`, it queries JSON data.

```yaml
fetch:
  url: "https://example.com"
  extract: selector
  selector: "article.main h1"
```

**Related**: Extract, Fetch

### Shell

A boolean flag in the `exec:` full form that enables shell features (pipes, chaining, variable expansion). When `shell: false` (default), commands run directly without a shell interpreter, which is more secure.

```yaml
exec:
  command: "echo hello | tr '[:lower:]' '[:upper:]'"
  shell: true
```

**Related**: Exec, Security, Blocklist

### Showcase

A collection of 115 production-ready, runnable workflows bundled with Nika. Organized by category: system, devops, network, API, data, core tools, file tools, media, content, engineering, analysis, automation. Browse with `nika showcase list`, extract with `nika showcase extract <name>`.

**Related**: Course, Template

### Structured Output

The combination of `output:` with `format: json_schema` that forces LLMs to return validated JSON matching a specific schema. Includes automatic retry when validation fails.

**Related**: Output, Infer, Schema, Retry

---

## T

### Task

The fundamental unit of work in a Nika workflow. Each task has a unique `id`, exactly one verb (`exec:`, `fetch:`, `infer:`, `invoke:`, or `agent:`), and optional fields like `depends_on:`, `with:`, `for_each:`, `on_error:`, `retry:`, `timeout:`, and `artifact:`.

```yaml
tasks:
  - id: my_task
    depends_on: [upstream_task]
    with:
      data: $upstream_task
    infer:
      prompt: "Process: {{with.data}}"
```

**Related**: Verb, DAG, Depends On, With Block

### Temperature

A parameter for `infer:` and `agent:` that controls LLM output randomness. Lower values (0.0-0.3) produce more deterministic output; higher values (0.7-1.0) produce more creative output.

```yaml
infer:
  prompt: "Write a poem"
  temperature: 0.9
```

**Related**: Infer, Max Tokens, Model

### Template

A string containing `{{...}}` expressions that are resolved at runtime. Templates can reference `with:` aliases, inputs, and apply pipe transforms. Used in prompts, commands, URLs, headers, and parameters.

```yaml
exec: echo "Hello {{with.name | uppercase}}, you have {{with.count}} items"
```

Syntax: `{{with.alias}}`, `{{with.alias.field}}`, `{{with.alias | transform}}`, `{{inputs.key}}`

**Related**: Binding, Pipe Transform, With Block, Alias

### Timeout

A time limit for task execution. Specified in seconds. For `exec:` tasks, the shell command is killed after the timeout. For `fetch:` tasks, the HTTP request is cancelled.

```yaml
exec:
  command: "long-running-process"
  timeout: 30
```

Note: In the current schema, `timeout: 30` means 30 seconds (the parser converts to milliseconds internally).

**Related**: Exec, Fetch, Limits

### Token Budget

The total number of tokens an agent can consume across all turns. Acts as a cost control that is more fine-grained than `max_turns`.

```yaml
agent:
  token_budget: 10000
```

**Related**: Agent, Limits, Max Tokens

### Tool Choice

Controls how an agent selects tools. Options:
- `auto` (default): LLM decides whether to call a tool or respond with text
- `required`: LLM must call a tool every turn
- `none`: LLM cannot call tools (text-only response)

**Related**: Agent, Builtin Tools, MCP

### Transform

See **Pipe Transform**.

### TUI (Terminal User Interface)

The interactive terminal interface launched with `nika ui`. Provides three views:
- `1/s` Studio -- workflow editing and visualization
- `2/c` Command -- command execution and output
- `3/x` Control -- system control and configuration

**Related**: CLI

---

## V

### Validation

The process of checking a workflow for correctness without executing it. Includes YAML syntax validation, schema version checking, DAG cycle detection, binding reference verification, and feature gate enforcement.

**Related**: Check, Error Code, AST

### Verb

One of the 5 action types that a task can perform. Every task has exactly one verb:

| Verb | Purpose | Requires LLM |
|------|---------|--------------|
| `exec:` | Run shell commands | No |
| `fetch:` | Make HTTP requests | No |
| `infer:` | LLM text generation | Yes |
| `invoke:` | Call a tool (builtin or MCP) | No |
| `agent:` | Multi-turn LLM loop with tools | Yes |

**Related**: Task, Exec, Fetch, Infer, Invoke, Agent

### Vision

Multimodal support for sending images to vision-capable LLMs. Uses the `content:` block with `type: image` entries. CAS hashes are automatically resolved to base64 for API transmission.

```yaml
infer:
  content:
    - type: image
      source: "{{with.photo.media[0].hash}}"
      detail: high
    - type: text
      text: "Describe this image"
```

Supported: Claude, OpenAI, Mistral, Groq, Gemini, xAI. Not supported: DeepSeek.
Native vision: `NativeModelKind::VisionHf` with HuggingFace models (not GGUF).

**Related**: Infer, CAS, Media Tools, Content

---

## W

### With Block

The mechanism for declaring data bindings between tasks. Maps aliases to task outputs using `$task_id` references. The aliases become available in templates as `{{with.alias}}`.

```yaml
- id: process
  depends_on: [fetch_data, get_config]
  with:
    data: $fetch_data
    config: $get_config
  infer:
    prompt: |
      Data: {{with.data}}
      Config: {{with.config}}
```

Rules:
- `$` prefix is required for task references
- `$env.VAR` accesses environment variables
- The referenced task must be declared in `depends_on:`

**Related**: Binding, Alias, Template, Depends On

### Workflow

The top-level unit of execution in Nika. A workflow is a single `.nika.yaml` file containing a schema declaration, an optional workflow name, optional provider/model defaults, optional inputs/artifacts configuration, and a list of tasks.

```yaml
schema: "nika/workflow@0.12"
workflow: my-automation
provider: anthropic
model: claude-sonnet-4-20250514
inputs:
  target_url: "https://example.com"
tasks:
  - id: step1
    fetch:
      url: "{{inputs.target_url}}"
```

**Related**: Schema, Task, DAG, Verb

---

## Z

### Zero Cypher Rule

An architectural constraint: Nika workflows never use raw Cypher queries. All database interactions with the NovaNet knowledge graph go through MCP tools via the `invoke:` verb. This ensures protocol-level separation between the workflow engine and the database.

**Related**: MCP, NovaNet, Invoke

---

*"Every concept you understand is a tool you own. Every tool you own is a lock you can open."*
