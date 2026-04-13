# Nika — Technical Deep Dive

**v0.27.0 | 370 source files | 216K lines of Rust | 6,526 tests | Schema @0.12**

Nika is a semantic YAML workflow engine that compiles AI task graphs into executable DAGs. It replaces imperative LLM orchestration code (Python scripts, LangChain chains) with declarative YAML that goes through a 3-phase compiler, executes on an async Tokio runtime, and emits full NDJSON observability traces.

```
                         NIKA SYSTEM OVERVIEW

  .nika.yaml ──► COMPILER (3-phase) ──► DAG RUNTIME ──► NDJSON TRACES
                                             │
               ┌─────────────────────────────┤
               │             │               │               │
           ┌───▼───┐   ┌────▼────┐   ┌──────▼──────┐   ┌───▼───┐
           │ infer │   │  exec   │   │   invoke    │   │ agent │
           │  LLM  │   │ Shell   │   │  MCP Tool   │   │ Multi │
           │ Call  │   │Command  │   │    Call      │   │ Turn  │
           └───┬───┘   └────┬────┘   └──────┬──────┘   └───┬───┘
               │             │               │               │
           ┌───▼───┐   ┌────▼────┐   ┌──────▼──────┐   ┌───▼───┐
           │7 LLM  │   │ tokio   │   │ rmcp v0.16  │   │  rig  │
           │provid.│   │ process │   │   stdio     │   │ core  │
           │rig 32 │   │ spawn   │   │  transport  │   │ v0.32 │
           └───────┘   └─────────┘   └─────────────┘   └───────┘
```

---

## 1. Compiler Architecture — 3-Phase Pipeline

Nika compiles YAML into executable workflow structures through a pipeline inspired by rustc.
Each phase has a distinct responsibility, and errors are collected (not fail-fast) so that
IDEs and CLI can report all problems in a single pass.

```
  ┌──────────────────────────────────────────────────────────────────────┐
  │                     COMPILATION PIPELINE                            │
  │                                                                     │
  │   YAML string                                                       │
  │       │                                                             │
  │       ▼                                                             │
  │   ┌────────────────────────────────────┐                            │
  │   │  PHASE 1 — RAW PARSE              │  marked_yaml crate         │
  │   │  • YAML → RawWorkflow             │  Every value = Spanned<T>  │
  │   │  • Preserves line:col positions    │  (file_id, byte_start,    │
  │   │  • No validation, just structure   │   byte_end)               │
  │   │  • Errors: NIKA-001..005          │                            │
  │   └──────────────┬─────────────────────┘                            │
  │                  │                                                  │
  │                  ▼                                                  │
  │   ┌────────────────────────────────────┐                            │
  │   │  PHASE 2 — ANALYZE                │  Collects ALL errors       │
  │   │  • Schema version gate (@0.12)    │  in one pass (Vec<Error>)  │
  │   │  • TaskId interning (u32, not str)│                            │
  │   │  • with: binding parsing          │  Jaro-Winkler fuzzy match  │
  │   │  • Cycle detection (DFS)          │  for "Did you mean?"       │
  │   │  • MCP server resolution          │  suggestions               │
  │   │  • Implicit dep extraction        │                            │
  │   │  • Errors: NIKA-140..150          │                            │
  │   └──────────────┬─────────────────────┘                            │
  │                  │                                                  │
  │                  ▼                                                  │
  │   ┌────────────────────────────────────┐                            │
  │   │  PHASE 3 — LOWER                  │  AnalyzedWorkflow          │
  │   │  • Analyzed → Runtime types       │  → Workflow (Arc<Task>)    │
  │   │  • TaskId → String names          │                            │
  │   │  • SSE MCP servers dropped        │  FxHashMap for perf        │
  │   │  • Provider/model defaults merged │  (no SipHash overhead)     │
  │   │  • Output: Workflow struct        │                            │
  │   └────────────────────────────────────┘                            │
  └──────────────────────────────────────────────────────────────────────┘
```

### Why 3 Phases Matter

**Phase 1** preserves exact source positions. Every single value in the YAML carries a `Span { file_id, start, end }`. This enables the LSP to provide hover, go-to-definition, and diagnostics at the exact character position.

**Phase 2** uses TaskId interning — converting string task names to `TaskId(u32)` for O(1) comparison. The analyzer collects ALL errors in a single pass (not fail-fast), so the IDE shows every problem at once. Binding expressions like `step1.data.temp ?? 20` are fully parsed here into `WithEntry { source: BindingPath, transforms: Vec<TransformExpr>, default: Option<Value> }`.

**Phase 3** strips spans and produces runtime-optimized types. `FxHashMap` replaces `HashMap` (Fx uses a faster non-cryptographic hash). `Arc<Task>` wraps tasks for zero-copy sharing across Tokio task spawns.

### Schema Version Gating

```yaml
schema: "nika/workflow@0.12"    # Required first line
```

Features are gated by schema version:

| Version | Features Unlocked |
|---------|-------------------|
| `@0.10` | Extended thinking (`thinking: true`, `thinking_budget:`) |
| `@0.11` | Structured output, agent skills |
| `@0.12` | Decompose, guardrails, limits, artifacts |

Phase 2 rejects features used with older schema versions (error NIKA-149).

### Error Code System

Nika uses structured error codes, not generic messages:

| Range | Category | Example |
|-------|----------|---------|
| NIKA-001..005 | YAML parsing | `NIKA-001: Invalid YAML syntax at line 12` |
| NIKA-010..019 | Workflow/schema | `NIKA-012: Unknown schema version "nika/workflow@0.99"` |
| NIKA-020..029 | DAG structure | `NIKA-021: Cycle detected: a → b → c → a` |
| NIKA-030..039 | Provider | `NIKA-031: No API key for provider "anthropic"` |
| NIKA-040..049 | Templates | `NIKA-041: Unresolved template {{with.missing}}` |
| NIKA-050..059 | Security | `NIKA-051: Control character in exec command` |
| NIKA-070..082 | Binding/with: | `NIKA-080: with references unknown task "step99"` |
| NIKA-090..099 | JSONPath/IO | `NIKA-091: Invalid JSONPath expression` |
| NIKA-100..109 | MCP | `NIKA-101: MCP server "novanet" connection timeout` |
| NIKA-110..119 | Agent | `NIKA-111: Agent max turns exceeded` |
| NIKA-140..150 | AST analysis | `NIKA-143: Cyclic dependency via with: bindings` |
| NIKA-200..219 | File/Builtin tools | `NIKA-201: File not found for nika:read` |
| NIKA-280..289 | Artifacts | `NIKA-281: Artifact write failed` |
| NIKA-300..309 | Structured output | `NIKA-301: JSON schema validation failed` |
| NIKA-400..429 | Daemon/IO/Sync | `NIKA-401: Daemon socket not available` |

---

## 2. The 5 Semantic Verbs

Every task in a Nika workflow uses exactly one verb. The verb determines which executor path runs.

```
  TaskExecutor::execute(task)
       │
       ├── TaskAction::Infer  ──► run_infer()   ──► RigProvider ──► LLM API
       ├── TaskAction::Exec   ──► run_exec()    ──► tokio::process::Command
       ├── TaskAction::Fetch  ──► run_fetch()   ──► reqwest::Client
       ├── TaskAction::Invoke ──► run_invoke()  ──► McpClient (rmcp)
       └── TaskAction::Agent  ──► run_agent()   ──► RigAgentLoop (multi-turn)
```

### 2.1 `infer:` — LLM Inference

Calls a language model and returns its text response.

```yaml
# Shorthand (string = prompt)
- id: headline
  infer: "Generate a headline about quantum computing"

# Full form
- id: article
  provider: anthropic          # Override workflow default
  model: claude-sonnet-4-6  # Override default model
  infer:
    prompt: "Write a technical article about {{with.topic}}"
    system: "You are a senior technical writer. Be precise and concise."
    temperature: 0.7           # 0.0 - 2.0
    max_tokens: 4000
    stop: ["---", "END"]       # Stop sequences
    thinking: true             # Claude extended thinking
    thinking_budget: 8192      # Tokens for chain-of-thought (1024-65536)
```

**Under the hood — execution path:**

```
  run_infer(task, bindings)
       │
       ├─ 1. Template resolution
       │     "Write about {{with.topic}}" → "Write about quantum computing"
       │
       ├─ 2. Policy check
       │     Token budget estimation from prompt length
       │
       ├─ 3. Provider dispatch
       │     Task provider > Workflow provider > Auto-detect
       │     → RigProvider::infer_with_options(prompt, opts)
       │
       └─ 4. Response handling
             Raw text OR Structured output (4-layer defense)
```

#### Structured Output — 4-Layer Defense

When a task defines `structured:` or `output: { format: json, schema: {...} }`, Nika uses a defense-in-depth approach to extract valid JSON:

```
  ┌──────────────────────────────────────────────────────────────┐
  │              STRUCTURED OUTPUT DEFENSE                       │
  │                                                              │
  │  LAYER 0: Tool Injection                                    │
  │  ├─ Inject DynamicSubmitTool into LLM call                  │
  │  ├─ Set tool_choice: Required                               │
  │  └─ Forces LLM to call submit_result({...}) with JSON       │
  │                                                              │
  │  LAYER 1: Direct JSON Extraction                            │
  │  ├─ Regex scan for JSON blocks in response                  │
  │  └─ Parse first valid JSON object/array found               │
  │                                                              │
  │  LAYER 2: Schema Validation                                 │
  │  ├─ Validate extracted JSON against JSON Schema              │
  │  └─ jsonschema v0.26 (Draft 2020-12)                        │
  │                                                              │
  │  LAYER 3: LLM Repair                                        │
  │  ├─ If validation failed, call LLM again                    │
  │  ├─ Prompt includes: original response + validation errors  │
  │  └─ Retry up to max_retries times                           │
  │                                                              │
  │  Each layer emits StructuredOutputAttempt events             │
  └──────────────────────────────────────────────────────────────┘
```

Example with structured output:

```yaml
- id: extract
  infer: "Extract product info from: {{with.text}}"
  structured:
    schema:
      type: object
      properties:
        name: { type: string }
        price: { type: number }
        currency: { type: string, enum: [USD, EUR, GBP] }
      required: [name, price]
```

### 2.2 `exec:` — Shell Command Execution

Runs a system command via `tokio::process::Command`.

```yaml
# Shorthand
- id: build
  exec: "npm run build"

# Full form
- id: deploy
  exec:
    command: "docker build -t myapp:{{with.version}} ."
    shell: true               # Use sh -c (default: false = safe shlex split)
    working_dir: /app
    env:
      NODE_ENV: production
      VERSION: "{{with.version}}"
    timeout_ms: 120000        # 2 min (default: 120s)
    capture_stdout: true
    capture_stderr: true
```

**Security model:**

```
  ┌──────────────────────────────────────────────────┐
  │  EXEC SECURITY                                   │
  │                                                   │
  │  shell: false (DEFAULT)                          │
  │  ├─ shlex::split() tokenizes command             │
  │  ├─ No shell metacharacters interpreted          │
  │  ├─ No pipe, redirect, glob expansion            │
  │  └─ Safe against injection                       │
  │                                                   │
  │  shell: true (OPT-IN)                            │
  │  ├─ Wraps in sh -c "command"                     │
  │  ├─ Full shell features available                │
  │  └─ User accepts injection risk                  │
  │                                                   │
  │  ALWAYS:                                          │
  │  ├─ validate_exec_command() blocks               │
  │  │   control characters (\\x00-\\x1f)             │
  │  └─ tokio::time::timeout enforces deadline       │
  └──────────────────────────────────────────────────┘
```

**Output**: `stdout` as trimmed string. Non-zero exit → `TaskFailed` with stderr.

### 2.3 `fetch:` — HTTP Requests

Makes HTTP calls via a shared `reqwest::Client`.

```yaml
# Shorthand (GET)
- id: data
  fetch: "https://api.example.com/data"

# Full form
- id: create
  fetch:
    url: "https://api.example.com/items"
    method: POST
    headers:
      Authorization: "Bearer {{with.token}}"
      Content-Type: application/json
    json:                       # Serialized as JSON body
      name: "{{with.name}}"
      tags: ["ai", "workflow"]
    timeout_ms: 10000
    follow_redirects: true      # Default: true
```

**Retry with exponential backoff:**

```yaml
- id: resilient_call
  fetch:
    url: "https://flaky-api.example.com/data"
  retry:
    max_attempts: 3
    delay_ms: 1000              # Initial delay
    backoff: 2.0                # Multiplier
```

```
  Attempt 1: call → 503 → wait 1000ms
  Attempt 2: call → 503 → wait 2000ms (1000 × 2.0)
  Attempt 3: call → 200 → success

  Retryable: 5xx server errors + network errors
  Not retryable: 4xx client errors (immediate return)
```

**HTTP client config**: 10s default timeout, 5s connect timeout, max 10 redirects, connection pooling.

### 2.4 `invoke:` — MCP Tool Calls

Calls tools on MCP (Model Context Protocol) servers via the rmcp v0.16 SDK.

```yaml
# Shorthand
- id: search
  invoke: novanet::novanet_search

# Full form
- id: context
  invoke:
    mcp: novanet
    tool: novanet_context
    params:
      focus_key: "qr-code"
      locale: "fr-FR"
      mode: "page"
    timeout_ms: 30000

# Resource read (alternative to tool call)
- id: read_entity
  invoke:
    resource: "entity://qr-code/fr-FR"
```

**Execution path:**

```
  run_invoke(task)
       │
       ├─ 1. Is it a builtin tool? (nika:sleep, nika:read, etc.)
       │     YES → BuiltinToolRouter::dispatch() (no MCP)
       │     NO  → continue to MCP
       │
       ├─ 2. Get/create MCP client
       │     McpClientPool → OnceCell per server (lazy init)
       │     Transport: stdio (TokioChildProcess)
       │
       ├─ 3. Call tool with timeout race
       │     tokio::select! {
       │       result = client.call_tool(name, params)
       │       _ = tokio::time::sleep(INVOKE_TASK_DEADLINE)  // 5 min
       │       _ = cancel_token.cancelled()                   // abort
       │     }
       │
       └─ 4. Parse response
             ToolCallResult → JSON value or string fallback
```

### 2.5 `agent:` — Multi-Turn Agentic Loop

Runs an LLM agent that can call tools across multiple turns until it completes its goal.

```yaml
- id: researcher
  agent:
    goal: "Research the latest developments in quantum computing and write a summary"
    tools:
      - nika:search          # Builtin web search
      - nika:read            # Builtin file read
      - nika:write           # Builtin file write
    max_iterations: 10       # Turn limit
    max_tokens: 8192         # Per-turn token budget

# Agent with MCP tools
- id: graph_agent
  agent:
    goal: "Find all entities related to QR codes and create a report"
    tools:
      - novanet::novanet_search
      - nika:write
    max_iterations: 15
```

**Architecture:**

```
  ┌─────────────────────────────────────────────────────┐
  │  AGENT EXECUTION (RigAgentLoop)                     │
  │                                                      │
  │  ┌──────────┐    ┌─────────────┐    ┌───────────┐  │
  │  │  Prompt   │───►│  LLM Call    │───►│ Tool Call │  │
  │  │  + History│    │  (rig-core)  │    │ Dispatch  │  │
  │  └──────────┘    └──────┬──────┘    └─────┬─────┘  │
  │       ▲                 │                  │        │
  │       │                 ▼                  ▼        │
  │       │          ┌─────────────┐    ┌───────────┐  │
  │       │          │  Response   │    │   MCP or  │  │
  │       └──────────│  + Tool     │◄───│  Builtin  │  │
  │     (next turn)  │   Results   │    │  Execute  │  │
  │                  └─────────────┘    └───────────┘  │
  │                                                      │
  │  Stop conditions:                                    │
  │  ├─ LLM returns stop_reason = "end_turn"            │
  │  ├─ max_iterations reached                           │
  │  ├─ nika:complete tool called                        │
  │  ├─ Token budget exhausted                           │
  │  └─ Cancellation token triggered                     │
  │                                                      │
  │  Per-turn events: AgentTurn (thinking, tokens, cost) │
  └─────────────────────────────────────────────────────┘
```

**Provider routing** in agent (auto-detection priority):

```
  agent.provider field (explicit)
       │ not set?
       ▼
  ANTHROPIC_API_KEY → run_claude()
  OPENAI_API_KEY    → run_openai()
  MISTRAL_API_KEY   → run_mistral()
  GROQ_API_KEY      → run_groq()
  DEEPSEEK_API_KEY  → run_deepseek()
  GEMINI_API_KEY    → run_gemini()
```

**Nested agents** (spawn_agent tool):
- Agents can spawn child agents
- `depth_limit` prevents infinite recursion (default: 3)
- Task-local depth tracking (thread-safe, no global state)

---

## 3. Data Flow System

### 3.1 The `with:` Binding Block

`with:` is the data plumbing between tasks. It resolves upstream task outputs into named aliases
available as `{{with.alias}}` templates.

```yaml
tasks:
  - id: weather
    fetch: "https://api.weather.com/current"

  - id: report
    with:
      # Simple task reference (entire output)
      raw: weather

      # JSONPath navigation
      temp: weather.data.temperature
      city: weather.location.name

      # Default values (null-coalescing)
      humidity: weather.data.humidity ?? 50
      unit: weather.data.unit ?? "celsius"

      # Object/array defaults
      config: weather.meta ?? {"debug": false}

      # Environment variables
      api_key: $env.WEATHER_API_KEY

      # Workflow inputs
      locale: $inputs.locale

      # Context files (loaded at workflow start)
      template: $context.files.report_template

      # Loop variable (inside for_each)
      item: $item

    infer: |
      Generate a weather report for {{with.city}}.
      Temperature: {{with.temp}}°{{with.unit}}
      Humidity: {{with.humidity}}%
      Use this template: {{with.template}}
    depends_on: [weather]
```

### 3.2 Binding Resolution Pipeline

```
  YAML with: block
       │
       ▼
  ┌─────────────────────────────┐
  │  PARSE (Phase 2)            │
  │  "weather.data.temp ?? 20"  │
  │       │                     │
  │       ▼                     │
  │  WithEntry {                │
  │    source: BindingPath {    │
  │      source: Task("weather")│
  │      segments: [            │
  │        Field("data"),       │
  │        Field("temp")        │
  │      ]                      │
  │    },                       │
  │    transforms: [],          │
  │    default: Some(20),       │
  │  }                          │
  └──────────────┬──────────────┘
                 │
                 ▼  (at runtime)
  ┌─────────────────────────────┐
  │  RESOLVE                    │
  │  1. Lookup task "weather"   │
  │     in RunContext datastore  │
  │  2. Navigate .data.temp     │
  │     via serde_json path     │
  │  3. If null → use default   │
  │  4. Apply transforms        │
  │  5. Store as Resolved(val)  │
  └──────────────┬──────────────┘
                 │
                 ▼
  ┌─────────────────────────────┐
  │  TEMPLATE SUBSTITUTION      │
  │  "Temperature: {{with.temp}}│
  │  → "Temperature: 22"        │
  └─────────────────────────────┘
```

### 3.3 Binding Sources

| Prefix | Source | Example |
|--------|--------|---------|
| `task_id` | Task output | `step1.result.items[0].name` |
| `$env.` | Environment variable | `$env.API_KEY` |
| `$inputs.` | Workflow inputs | `$inputs.locale` |
| `$context.files.` | Preloaded context file | `$context.files.brand` |
| `$item` | Loop variable (for_each) | `$item.id` |

### 3.4 Transform Pipe Chains

27 built-in transforms applied via `|` pipe syntax:

```yaml
with:
  title: step1 | upper                    # "hello" → "HELLO"
  slug: step1 | lower | kebab_case        # "Hello World" → "hello-world"
  preview: step1 | trim | take(100)        # First 100 chars
  hash: step1 | sha256                     # SHA-256 hex digest
  encoded: step1 | base64                  # Base64 encode
  parsed: step1 | json_decode             # String → JSON object
```

**Complete transform list:**

| Category | Transforms |
|----------|-----------|
| **Case** | `upper`, `lower`, `capitalize`, `title`, `snake_case`, `kebab_case`, `camel_case`, `pascal_case` |
| **Whitespace** | `trim`, `ltrim`, `rtrim` |
| **Encoding** | `base64`, `base64_decode`, `url_encode`, `url_decode`, `json_encode`, `json_decode` |
| **Hash** | `sha256`, `md5` |
| **String** | `reverse`, `slug`, `length` |
| **Grammar** | `pluralize`, `singularize` |
| **Collection** | `split`, `join`, `take`, `drop` |
| **Utility** | `default`, `parse_json`, `get_field` |

---

## 4. DAG Execution Engine

### 4.1 Parallel Task Scheduling

```
  ┌──────────────────────────────────────────────────────────────┐
  │  DAG EXECUTION LOOP (Runner)                                 │
  │                                                               │
  │  loop {                                                       │
  │    ├─ Check cancellation token                                │
  │    ├─ Check pause state (resumable)                           │
  │    │                                                          │
  │    ├─ Get ready tasks:                                        │
  │    │   for each task:                                         │
  │    │     if all dependencies completed → ready                │
  │    │     if any dependency failed → DependencyFailed          │
  │    │                                                          │
  │    ├─ Spawn ready tasks into tokio::JoinSet:                  │
  │    │   ┌──────────┐ ┌──────────┐ ┌──────────┐               │
  │    │   │ Task A   │ │ Task B   │ │ Task C   │  (parallel)   │
  │    │   └────┬─────┘ └────┬─────┘ └────┬─────┘               │
  │    │        │             │             │                     │
  │    │        ▼             ▼             ▼                     │
  │    │   JoinSet::join_next() — waits for ANY to complete      │
  │    │                                                          │
  │    ├─ Store results in RunContext (Arc<DashMap>)              │
  │    │                                                          │
  │    └─ if all_done() → break                                   │
  │  }                                                            │
  └──────────────────────────────────────────────────────────────┘
```

Example DAG:

```yaml
tasks:
  - id: fetch_data                  #  fetch_data ─┐
    fetch: "https://api.com/data"   #              │
                                    #              ▼
  - id: fetch_config                #  fetch_config─┐
    fetch: "https://api.com/cfg"    #              │ │
                                    #              ▼ ▼
  - id: process                     #  process (waits for both)
    with:                           #       │
      data: fetch_data              #       ▼
      config: fetch_config          #  format
    infer: "Process {{with.data}} with {{with.config}}"
    depends_on: [fetch_data, fetch_config]

  - id: format
    with:
      result: process
    infer: "Format: {{with.result}}"
    depends_on: [process]
```

```
  DAG visualization:

  fetch_data ──┐
               ├──► process ──► format
  fetch_config─┘

  Execution timeline:
  ════════════════════════════════════════
  t0  │ fetch_data ▓▓▓▓▓▓▓▓░░░░░░░░░░░░░
  t0  │ fetch_config ▓▓▓▓▓░░░░░░░░░░░░░░
  t1  │ process ░░░░░░░░░▓▓▓▓▓▓▓▓▓░░░░░
  t2  │ format ░░░░░░░░░░░░░░░░░░░▓▓▓▓▓
  ════════════════════════════════════════
       ▓ = executing   ░ = waiting
```

### 4.2 `for_each` — Parallel Iteration

```yaml
- id: translate
  for_each: ["en", "fr", "de", "es", "ja"]   # Or: $upstream_task
  as: lang                                    # Loop variable name
  concurrency: 3                              # Concurrency limit
  fail_fast: true                             # Stop all on first error
  with:
    lang: $item
  infer: "Translate the document to {{with.lang}}"
```

**Execution model:**

```
  ┌─────────────────────────────────────────────┐
  │  FOR_EACH ENGINE                            │
  │                                              │
  │  Items: [en, fr, de, es, ja]                │
  │  Semaphore: 3 permits                       │
  │                                              │
  │  t0: ┌────┐ ┌────┐ ┌────┐                  │
  │      │ en │ │ fr │ │ de │  (3 concurrent)  │
  │      └──┬─┘ └──┬─┘ └──┬─┘                  │
  │         │      │      │                     │
  │  t1:    ▼      │      ▼                     │
  │      ┌────┐    │   ┌────┐                   │
  │      │ es │    │   │ ja │  (semaphore free) │
  │      └────┘    ▼   └────┘                   │
  │                ✓                             │
  │                                              │
  │  Aggregation:                                │
  │  ├─ Sort by original index                  │
  │  ├─ Collect into Value::Array               │
  │  └─ Store under parent task ID              │
  │                                              │
  │  Output: ["English text", "French text",    │
  │           "German text", "Spanish text",    │
  │           "Japanese text"]                   │
  └─────────────────────────────────────────────┘
```

**Item sources** (3 types):

| Source | Syntax | Example |
|--------|--------|---------|
| Array literal | JSON array string | `items: '["a", "b", "c"]'` |
| Binding reference | `$task` or `{{with.alias}}` | `items: $step1` |
| Decompose | MCP-based graph traversal | `decompose: { strategy: semantic }` |

### 4.3 Decompose — Dynamic DAG Expansion

Decompose uses graph traversal (via MCP) to generate iteration items at runtime:

```yaml
- id: generate_all
  decompose:
    strategy: semantic           # Uses novanet_search(mode=walk) MCP call
    arc_family: ownership        # Follow ownership arcs
    source: $parent_entity       # Starting node
    max_depth: 2
    max_items: 50
  for_each: $decompose
  concurrency: 4
  invoke:
    mcp: novanet
    tool: novanet_context
    params:
      focus_key: "{{with.item}}"
      locale: "fr-FR"
      mode: "block"
```

**3 strategies:**

| Strategy | How | Use Case |
|----------|-----|----------|
| `semantic` | Calls `novanet_search(mode=walk)` MCP tool | Knowledge graph children |
| `static` | Resolves binding to array directly | Pre-computed lists |
| `nested` | Recursive BFS with depth limiting | Multi-level tree processing |

### 4.4 Cancellation and Pause

```
  Cancellation token (CancellationToken from tokio_util):
  ├─ Propagated to all running tasks via clone
  ├─ invoke: uses tokio::select! to race cancel vs execution
  ├─ for_each: checks cancel before each iteration
  └─ Runner main loop: checks cancel at top of each cycle

  Pause/Resume:
  ├─ Runner checks pause flag in main loop
  ├─ If paused: emits WorkflowPaused, blocks on condvar
  └─ On resume: emits WorkflowResumed, continues
```

---

## 5. LLM Backend System — 7 Providers

All cloud providers use **rig-core v0.32**, a Rust LLM framework that provides a unified API.

```
  ┌──────────────────────────────────────────────────────────────┐
  │                    PROVIDER ARCHITECTURE                      │
  │                                                               │
  │  ┌─────────┐                                                  │
  │  │  YAML   │  provider: anthropic                            │
  │  │ workflow│  model: claude-sonnet-4-6                      │
  │  └────┬────┘                                                  │
  │       │                                                       │
  │       ▼                                                       │
  │  ┌─────────────────────────────┐                              │
  │  │  RigProvider (enum)         │                              │
  │  │  ├─ Claude(Client)          │  rig-core anthropic module  │
  │  │  ├─ OpenAI(Client)          │  rig-core openai module     │
  │  │  ├─ Mistral(Client)         │  rig-core mistral module    │
  │  │  ├─ Groq(Client)            │  rig-core groq module       │
  │  │  ├─ DeepSeek(Client)        │  rig-core deepseek module   │
  │  │  ├─ Gemini(Client)          │  rig-core gemini module     │
  │  │  └─ Native(NativeRuntime)   │  mistral.rs (GGUF local)   │
  │  └─────────────┬───────────────┘                              │
  │                │                                              │
  │                ▼                                              │
  │  Methods:                                                     │
  │  ├─ infer(prompt, model)                                     │
  │  ├─ infer_with_options(prompt, opts)                          │
  │  │   opts: temperature, max_tokens, system, stop             │
  │  ├─ infer_with_tools(prompt, tools, model, max_tokens)       │
  │  │   Used for structured output (tool_choice: Required)       │
  │  └─ default_model() → per-provider default                   │
  └──────────────────────────────────────────────────────────────┘
```

### Provider Details

| Provider | ID | Default Model | Env Var | Key Prefix |
|----------|-----|--------------|---------|------------|
| Anthropic Claude | `anthropic` / `claude` | `claude-sonnet-4-6` | `ANTHROPIC_API_KEY` | `sk-ant-` |
| OpenAI | `openai` / `gpt` | `gpt-4o` | `OPENAI_API_KEY` | `sk-` |
| Mistral AI | `mistral` | `mistral-large-latest` | `MISTRAL_API_KEY` | — |
| Groq | `groq` | `llama-4-maverick` | `GROQ_API_KEY` | `gsk_` |
| DeepSeek | `deepseek` / `deep-seek` | `deepseek-chat` | `DEEPSEEK_API_KEY` | `sk-` |
| Google Gemini | `gemini` / `google` | `gemini-2.5-flash` | `GEMINI_API_KEY` | — |
| Native (local) | `native` | GGUF model loaded | `NIKA_NATIVE_MODEL_PATH` | — |

### Native Inference (Local GGUF Models)

Feature-gated (`native-inference`). Uses mistral.rs for local model inference.

**15 curated models:**

| Model | Size | RAM Required | Use Case |
|-------|------|-------------|----------|
| `qwen3:8b` | 8B | 8GB | General purpose |
| `qwen3:1.7b` | 1.7B | 4GB | Lightweight |
| `qwen3:32b` | 32B | 24GB | Complex reasoning |
| `llama3.2:3b` | 3B | 6GB | Efficient small |
| `llama3.1:8b` | 8B | 8GB | Versatile (128K ctx) |
| `phi4:14b` | 14B | 12GB | Reasoning-focused |
| `mistral:7b` | 7B | 8GB | Classic efficient |
| `gemma2:9b` | 9B | 10GB | Instruction-tuned |
| `deepseek-coder:6.7b` | 6.7B | 8GB | Code generation |
| `starcoder2:7b` | 7B | 8GB | Code completion |
| `llava:7b` | 7B | 10GB | Vision-language |
| `nomic-embed:1.5` | — | 2GB | Text embeddings |
| `bge:large` | — | 2GB | Retrieval embeddings |

**Auto-quantization**: `auto_select_quantization(model, available_ram)` picks the best quantization level for available RAM with 2GB headroom. 15 quantization levels supported (F16, Q8_0, Q6_K, Q5_K_M, Q4_K_M, Q3_K_M, Q2_K, IQ2_XS, etc.).

### Cost Tracking

Built-in per-provider pricing tables:

```
  ProviderResponded event includes:
  ├─ input_tokens: 1523
  ├─ output_tokens: 847
  ├─ cache_read_tokens: 0
  ├─ ttft_ms: 432          # Time to first token
  ├─ finish_reason: "stop"
  └─ cost_usd: 0.0089      # Computed from pricing table
```

---

## 6. MCP Integration

### 6.1 Client Architecture

```
  ┌───────────────────────────────────────────────────────────────┐
  │                   MCP CLIENT STACK                            │
  │                                                                │
  │  Workflow YAML                                                 │
  │  ┌─────────────────────────────────┐                          │
  │  │ mcp:                            │                          │
  │  │   novanet:                      │                          │
  │  │     command: cargo              │                          │
  │  │     args: [run, ...]            │                          │
  │  │     env: { NEO4J_URI: ... }     │                          │
  │  └──────────────┬──────────────────┘                          │
  │                 │                                              │
  │                 ▼                                              │
  │  ┌────────────────────────────────┐                           │
  │  │  McpClientPool                 │ Lazy init per server      │
  │  │  ├─ DashMap<name, McpClient>   │ OnceCell for thread-safe  │
  │  │  └─ Connection reuse           │ single initialization     │
  │  └──────────────┬─────────────────┘                           │
  │                 │                                              │
  │                 ▼                                              │
  │  ┌────────────────────────────────┐                           │
  │  │  McpClient                     │                           │
  │  │  ├─ connect() (30s timeout)    │                           │
  │  │  ├─ call_tool(name, params)    │  JSON-RPC 2.0            │
  │  │  ├─ list_tools()               │                           │
  │  │  ├─ read_resource(uri)         │                           │
  │  │  ├─ ping() → latency + status  │                           │
  │  │  └─ Response cache (5m TTL)    │  FxHash for speed        │
  │  └──────────────┬─────────────────┘                           │
  │                 │                                              │
  │                 ▼                                              │
  │  ┌────────────────────────────────┐                           │
  │  │  RmcpClientAdapter             │ rmcp v0.16               │
  │  │  ├─ TokioChildProcess          │ stdio transport          │
  │  │  ├─ JSON-RPC ↔ rmcp::Service   │                           │
  │  │  ├─ Tool def cache (with TTL)  │                           │
  │  │  └─ Error code extraction      │                           │
  │  └────────────────────────────────┘                           │
  │                                                                │
  │  Timeouts:                                                     │
  │  ├─ Connect: 30s (CONNECT_TIMEOUT)                            │
  │  ├─ Tool call: 120s (MCP_CALL_TIMEOUT)                        │
  │  └─ Reconnect: 5s (RECONNECT_TIMEOUT)                        │
  └───────────────────────────────────────────────────────────────┘
```

### 6.2 The 100 MCP Aliases

Pre-configured shortcuts for popular MCP servers. `nika mcp add perplexity` expands to the full npm package.

**Anthropic Official (8)**

| Alias | Package |
|-------|---------|
| `filesystem` | `@modelcontextprotocol/server-filesystem` |
| `memory` | `@modelcontextprotocol/server-memory` |
| `puppeteer` | `@modelcontextprotocol/server-puppeteer` |
| `brave-search` | `@modelcontextprotocol/server-brave-search` |
| `google-maps` | `@modelcontextprotocol/server-google-maps` |
| `fetch` | `@modelcontextprotocol/server-fetch` |
| `github` | `@modelcontextprotocol/server-github` |
| `gitlab` | `@modelcontextprotocol/server-gitlab` |

**Databases (8)**

| Alias | Package |
|-------|---------|
| `neo4j` | `@neo4j/mcp-neo4j` |
| `postgres` | `@modelcontextprotocol/server-postgres` |
| `mysql` | `mcp-server-mysql` |
| `sqlite` | `@anthropic/mcp-server-sqlite` |
| `mongodb` | `mcp-mongodb` |
| `redis` | `mcp-redis` |
| `supabase` | `mcp-supabase` |
| `neon` | `@neondatabase/mcp-server-neon` |

**Search and Web (8)**

| Alias | Package |
|-------|---------|
| `perplexity` | `perplexity-mcp` |
| `firecrawl` | `firecrawl-mcp` |
| `brave` | `@anthropic/mcp-server-brave-search` |
| `exa` | `exa-mcp-server` |
| `tavily` | `tavily-mcp` |
| `serper` | `serper-mcp` |
| `searchapi` | `searchapi-mcp` |
| `bing` | `bing-mcp` |

**Developer Tools (8)**

| Alias | Package |
|-------|---------|
| `linear` | `mcp-linear` |
| `sentry` | `@modelcontextprotocol/server-sentry` |
| `raygun` | `raygun-mcp` |
| `buildkite` | `buildkite-mcp` |
| `circleci` | `circleci-mcp` |
| `vercel` | `vercel-mcp` |
| `cloudflare` | `cloudflare-mcp` |
| `aws` | `aws-mcp` |

**Productivity (8)**

| Alias | Package |
|-------|---------|
| `slack` | `@anthropic/mcp-server-slack` |
| `google-drive` | `@anthropic/mcp-server-google-drive` |
| `notion` | `notion-mcp` |
| `airtable` | `airtable-mcp` |
| `todoist` | `todoist-mcp` |
| `asana` | `asana-mcp` |
| `trello` | `trello-mcp` |
| `monday` | `monday-mcp` |

**AI and Specialized (8)**

| Alias | Package |
|-------|---------|
| `langchain` | `langchain-mcp` |
| `e2b` | `@e2b/mcp-server` |
| `sequential-thinking` | `@modelcontextprotocol/server-sequential-thinking` |
| `context7` | `context7-mcp` |
| `21st` | `21st-mcp` |
| `supadata` | `supadata-mcp` |
| `dataforseo` | `dataforseo-mcp` |
| `ahrefs` | `ahrefs-mcp` |

### 6.3 Strict Validation (`nika check --strict`)

```
  Normal check:
  ├─ YAML parse
  ├─ AST analyze (tasks, bindings, cycles)
  └─ Lower to runtime

  Strict check (adds):
  ├─ Connect to each MCP server defined in mcp: block
  ├─ Fetch tool definitions (JSON Schema for each tool)
  ├─ For each invoke: task:
  │   ├─ Match tool name against server's tool list
  │   ├─ Validate params against tool's inputSchema
  │   └─ Report missing/extra/wrong-type parameters
  └─ Fail if any validation errors
```

---

## 7. The 12 Builtin Tools

Builtin tools run in-process (no MCP server needed). They are available in `invoke:` and `agent:` tasks.

```
  ┌──────────────────────────────────────────────────────────────┐
  │                    BUILTIN TOOLS                              │
  │                                                               │
  │  CORE (7):                                                    │
  │  ┌──────────────┬────────────────────────────────────────┐   │
  │  │ nika:sleep   │ Pause execution (humantime, max 5min)  │   │
  │  │ nika:log     │ Emit log (debug/info/warn/error)       │   │
  │  │ nika:emit    │ Custom event with arbitrary payload    │   │
  │  │ nika:assert  │ Fail task if condition is false        │   │
  │  │ nika:prompt  │ Human-in-the-loop: blocks for input   │   │
  │  │ nika:run     │ Execute nested workflow (depth max 10) │   │
  │  │ nika:complete│ Signal agent loop to stop              │   │
  │  └──────────────┴────────────────────────────────────────┘   │
  │                                                               │
  │  FILE (5):                                                    │
  │  ┌──────────────┬────────────────────────────────────────┐   │
  │  │ nika:read    │ Read file with line numbers            │   │
  │  │ nika:write   │ Create or overwrite file               │   │
  │  │ nika:edit    │ String replacement (old → new)         │   │
  │  │ nika:glob    │ Find files by glob pattern             │   │
  │  │ nika:grep    │ Regex search across files              │   │
  │  └──────────────┴────────────────────────────────────────┘   │
  └──────────────────────────────────────────────────────────────┘
```

### Nested Workflows (`nika:run`)

```yaml
- id: sub_workflow
  invoke:
    tool: nika:run
    params:
      workflow: ./sub-tasks/generate-page.nika.yaml
      inputs:
        locale: "fr-FR"
        entity: "qr-code"
      timeout: 300               # Seconds (max 3600)
```

**Depth tracking**: Task-local variable incremented on each `nika:run`. Max depth = 10. Prevents infinite recursion when workflows call themselves.

---

## 8. Observability — 34 NDJSON Events

Every workflow execution produces a trace file: `~/.nika/traces/gen-{uuid}.ndjson`

Each line is a JSON event with timestamp, event kind, and structured data.

```
  ┌──────────────────────────────────────────────────────────────┐
  │                  EVENT TIMELINE                               │
  │                                                               │
  │  [    0ms] WorkflowStarted                                   │
  │  [    1ms]   TaskScheduled   task=fetch_data                 │
  │  [    1ms]   TaskScheduled   task=fetch_config               │
  │  [    2ms]   TaskStarted     task=fetch_data verb=fetch      │
  │  [    2ms]   TaskStarted     task=fetch_config verb=fetch    │
  │  [  342ms]   TaskCompleted   task=fetch_config dur=340ms     │
  │  [  510ms]   TaskCompleted   task=fetch_data dur=508ms       │
  │  [  511ms]   TaskScheduled   task=process                    │
  │  [  511ms]   TaskStarted     task=process verb=infer         │
  │  [  512ms]   TemplateResolved task=process                   │
  │  [  513ms]   ProviderCalled  task=process provider=claude    │
  │  [  945ms]   ProviderResponded in=1523 out=847 cost=$0.009  │
  │  [ 1201ms]   TaskCompleted   task=process dur=690ms          │
  │  [ 1201ms]   TaskScheduled   task=format                     │
  │  [ 1832ms]   TaskCompleted   task=format dur=631ms           │
  │  [ 1833ms] WorkflowCompleted total=1833ms                    │
  └──────────────────────────────────────────────────────────────┘
```

### All 34 Event Kinds

| # | Category | Event | Key Fields |
|---|----------|-------|------------|
| 1 | Workflow | `WorkflowStarted` | generation_id, workflow_hash, nika_version |
| 2 | | `WorkflowCompleted` | final_output, total_duration_ms |
| 3 | | `WorkflowFailed` | error, failed_task |
| 4 | | `WorkflowAborted` | reason, duration_ms, running_tasks |
| 5 | | `WorkflowPaused` | — |
| 6 | | `WorkflowResumed` | — |
| 7 | Task | `TaskScheduled` | task_id, dependencies |
| 8 | | `TaskStarted` | task_id, verb, resolved_inputs |
| 9 | | `TaskCompleted` | task_id, output, duration_ms |
| 10 | | `TaskFailed` | task_id, error, duration_ms |
| 11 | Provider | `TemplateResolved` | task_id, template, result |
| 12 | | `ProviderCalled` | task_id, provider, model, prompt_len |
| 13 | | `ProviderResponded` | input/output/cache tokens, ttft_ms, cost_usd |
| 14 | Context | `ContextAssembled` | sources, total_tokens, budget_used_pct |
| 15 | MCP | `McpInvoke` | call_id, server, tool, params |
| 16 | | `McpResponse` | call_id, output_len, duration_ms, cached |
| 17 | | `McpConnected` | server_name |
| 18 | | `McpError` | server_name, error |
| 19 | | `McpRetry` | attempt, max_attempts, error |
| 20 | Agent | `AgentStart` | task_id, max_turns, mcp_servers |
| 21 | | `AgentTurn` | turn_index, thinking, tokens, stop_reason |
| 22 | | `AgentComplete` | turns, stop_reason |
| 23 | | `AgentSpawned` | parent_task_id, child_task_id, depth |
| 24 | Guardrail | `GuardrailPassed` | guardrail_type, description |
| 25 | | `GuardrailFailed` | guardrail_type, message |
| 26 | | `GuardrailEscalation` | severity, suggested_action |
| 27 | Builtin | `Log` | level, message |
| 28 | | `Custom` | name, payload |
| 29 | Artifact | `ArtifactWritten` | path, size, format |
| 30 | | `ArtifactFailed` | path, reason |
| 31 | Structured | `StructuredOutputAttempt` | layer (0-4), success, error |
| 32 | | `StructuredOutputSuccess` | layer, total_attempts |
| 33 | Limits | `LimitReached` | limit_type, value, threshold |
| 34 | | `PartialCompletion` | progress (0.0-1.0), result_preview |

### Trace Management

```bash
nika trace list                     # List all traces (ID, size, timestamp)
nika trace show abc123              # Show events (partial ID match)
nika trace export abc123 --format yaml -o report.yaml
nika trace clean --keep 10          # Keep only 10 most recent
```

---

## 9. Terminal UI — 4 Views

```
  ┌──────────────────────────────────────────────────────────────┐
  │  NIKA TUI (ratatui + crossterm)                              │
  │                                                               │
  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐       │
  │  │ 1 Studio │ │ 2 Runner │ │ 3 Chat   │ │ 4 Settngs│       │
  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘       │
  │                                                               │
  │  Navigation: Tab/Shift+Tab cycle, 1-4 jump, s/r/c/, letters │
  └──────────────────────────────────────────────────────────────┘
```

### View 1: Studio (`1` or `s`)

```
  ┌────────────────────────────────────────────────────────────┐
  │  ┌──────────┬──────────────────────────┬─────────────────┐ │
  │  │ BROWSER  │  YAML EDITOR             │  DAG PREVIEW    │ │
  │  │          │                          │                 │ │
  │  │ examples/│  schema: nika/workflow    │  fetch_data ─┐  │ │
  │  │  ├─ min..│  provider: anthropic     │              ├► │ │
  │  │  ├─ blog.│                          │  fetch_cfg ──┘  │ │
  │  │  └─ qr.. │  tasks:                 │       │         │ │
  │  │          │    - id: step1           │       ▼         │ │
  │  │ project/ │      infer: "..."        │   process       │ │
  │  │  └─ ...  │                          │       │         │ │
  │  │          │    - id: step2           │       ▼         │ │
  │  │          │      depends_on: [step1] │    format       │ │
  │  └──────────┴──────────────────────────┴─────────────────┘ │
  │  F5: Run  /: Search  Ctrl+S: Save                         │
  └────────────────────────────────────────────────────────────┘
```

- File browser with tree navigation
- YAML editor with syntax highlighting (tree-sitter)
- Live DAG visualization (updates as you type)
- Fuzzy file search (`/` or `Ctrl+P`)

### View 2: Runner (`2` or `r`)

```
  ┌────────────────────────────────────────────────────────────┐
  │  ┌──────────────────────────────┬─────────────────────────┐│
  │  │ PROGRESS                     │ DAG (live)              ││
  │  │                              │                         ││
  │  │ ▓▓▓▓▓▓▓▓▓▓░░░░░ 3/5 tasks  │ fetch ✓──┐              ││
  │  │                              │          ├► process ▓   ││
  │  │ ✓ fetch_data      340ms     │ config ✓─┘      │       ││
  │  │ ✓ fetch_config    210ms     │                  ▼       ││
  │  │ ▓ process         running   │           format ░       ││
  │  │ ░ format          waiting   │                          ││
  │  │ ░ notify          waiting   │                          ││
  │  │                              │                         ││
  │  ├──────────────────────────────┤                         ││
  │  │ IO (task detail)             │                         ││
  │  │ Provider: claude             │                         ││
  │  │ Tokens: in=1523 out=847     │                         ││
  │  │ Cost: $0.009                 │                         ││
  │  └──────────────────────────────┴─────────────────────────┘│
  │  Space: Pause  r: Retry  e: Export trace                   │
  └────────────────────────────────────────────────────────────┘
```

- Real-time task execution progress
- Live DAG with status colors (green=done, yellow=running, gray=waiting, red=failed)
- Per-task IO details (tokens, cost, duration)
- Event stream from the 34 NDJSON events

### View 3: Chat (`3` or `c`)

```
  ┌────────────────────────────────────────────────────────────┐
  │  ┌────────────────────────────────────────────────────────┐│
  │  │                                                        ││
  │  │  You: Research the latest developments in quantum      ││
  │  │       computing and write a 500-word summary           ││
  │  │                                                        ││
  │  │  Agent: I'll research this topic. Let me start by     ││
  │  │         searching for recent papers...                 ││
  │  │                                                        ││
  │  │  [Tool Call] novanet::novanet_search                   ││
  │  │  params: { query: "quantum computing 2024" }           ││
  │  │  result: [{ key: "quantum-supremacy", ... }]           ││
  │  │                                                        ││
  │  │  Agent: Based on the search results, here are the     ││
  │  │         key developments...                            ││
  │  │                                                        ││
  │  └────────────────────────────────────────────────────────┘│
  │  ┌────────────────────────────────────────────────────────┐│
  │  │ > Type a message...                                    ││
  │  └────────────────────────────────────────────────────────┘│
  │  Ctrl+K: Commands  Ctrl+P: Provider  Ctrl+T: Thinking     │
  └────────────────────────────────────────────────────────────┘
```

- Multi-turn agent conversation
- Tool call visibility (MCP calls shown inline)
- Slash commands at `/`
- Toggle deep thinking (`Ctrl+T`)
- Toggle Infer vs Agent mode (`Ctrl+M`)
- Provider switching at runtime (`Ctrl+P`)

### View 4: Settings (`4` or `,`)

- Provider API key management (keychain integration)
- Theme selector
- Default provider/model configuration
- Native model management (GGUF)

### Keyboard Shortcuts

| Context | Key | Action |
|---------|-----|--------|
| Global | `1-4` | Jump to view |
| Global | `Tab/Shift+Tab` | Cycle views |
| Global | `?` | Help overlay |
| Normal | `j/k` | Scroll down/up (vim) |
| Normal | `g/G` | Top/bottom |
| Chat | `i` | Insert mode |
| Chat | `Esc` | Normal mode |
| Chat | `Enter` | Send message |
| Chat | `Ctrl+K` | Command palette |
| Chat | `Ctrl+T` | Toggle thinking |
| Chat | `Ctrl+M` | Toggle infer/agent |
| Studio | `F5` | Run workflow |
| Studio | `/` | Fuzzy search |
| Studio | `Ctrl+S` | Save file |
| Runner | `Space` | Pause/resume |
| Runner | `r` | Retry |
| Runner | `e` | Export trace |

---

## 10. Language Server Protocol (LSP)

Feature-gated (`--features lsp`). Provides IDE integration for `.nika.yaml` files.

```
  ┌──────────────────────────────────────────────────────────────┐
  │  LSP CAPABILITIES                                            │
  │                                                               │
  │  ┌────────────────┐                                          │
  │  │ Hover          │  Task definitions, binding info,         │
  │  │                │  provider docs on hover                  │
  │  ├────────────────┤                                          │
  │  │ Completion     │  Schema-aware completions                │
  │  │                │  Triggers: : . $ {                       │
  │  │                │  Completes: verbs, fields, task refs,    │
  │  │                │  providers, MCP tools, bindings          │
  │  ├────────────────┤                                          │
  │  │ Go-to-Def      │  Jump to task from with: or depends_on  │
  │  ├────────────────┤                                          │
  │  │ Code Actions   │  Quick fixes (QUICKFIX) + refactoring   │
  │  ├────────────────┤                                          │
  │  │ Symbols        │  Document outline (task tree)            │
  │  ├────────────────┤                                          │
  │  │ Semantic Tokens│  34 token types, 8 modifier levels      │
  │  │                │  Rich syntax highlighting beyond regex   │
  │  ├────────────────┤                                          │
  │  │ Diagnostics    │  All Phase 2 errors with spans          │
  │  │                │  + "Did you mean?" suggestions           │
  │  └────────────────┘                                          │
  │                                                               │
  │  Transport: stdio (default) or TCP (:9257)                   │
  │  Protocol: LSP via tower-lsp v0.20                           │
  │  VS Code extension: /editors/vscode/                          │
  └──────────────────────────────────────────────────────────────┘
```

The LSP feeds directly from the Phase 2 analyzer. Because the analyzer collects ALL errors
non-fail-fast and every AST node carries span information, the LSP can report every diagnostic
with precise line:column positions in a single pass.

---

## 11. Security Model

```
  ┌──────────────────────────────────────────────────────────────┐
  │                    SECURITY LAYERS                            │
  │                                                               │
  │  SECRETS                                                      │
  │  ┌──────────────────────────────────────────────┐            │
  │  │                                              │            │
  │  │  With nika-daemon (recommended):             │            │
  │  │  Nika → Unix socket → daemon → OS Keychain   │            │
  │  │         (IPC)         (sole accessor)         │            │
  │  │                                              │            │
  │  │  Without daemon (fallback):                  │            │
  │  │  Nika → keyring crate → OS Keychain          │            │
  │  │         (may trigger macOS popup)            │            │
  │  │                                              │            │
  │  │  All secrets: Zeroize on drop                │            │
  │  └──────────────────────────────────────────────┘            │
  │                                                               │
  │  EXECUTION SAFETY                                             │
  │  ├─ exec: default shell=false (shlex tokenization)           │
  │  ├─ Control character validation on all exec commands         │
  │  ├─ Nested workflow depth limit: 10                           │
  │  ├─ Agent depth limit: configurable (default 3)              │
  │  ├─ for_each fail_fast: stops all iterations on error        │
  │  ├─ Cancellation token propagated to all running tasks       │
  │  └─ Per-task timeouts enforced via tokio::time::timeout      │
  │                                                               │
  │  KEYCHAIN SUPPORT                                             │
  │  ├─ macOS: Keychain Access                                   │
  │  ├─ Windows: Credential Manager                              │
  │  ├─ Linux: Secret Service (GNOME Keyring / KWallet)          │
  │  └─ CI/Docker: NIKA_SKIP_KEYCHAIN=1 (env vars only)         │
  └──────────────────────────────────────────────────────────────┘
```

---

## 12. CLI Commands Reference

```bash
# WORKFLOW EXECUTION
nika <workflow.nika.yaml>                  # Run (positional shorthand)
nika run <file> [--provider X] [--model Y] # Run with overrides
nika check <file> [--strict]               # Validate (--strict = MCP param check)

# INTERACTIVE
nika ui [--view studio|runner|chat|settings] [WORKFLOW]
nika chat [--provider X] [--model Y]       # Chat shortcut
nika studio [WORKFLOW]                      # Editor shortcut

# PROJECT
nika init [--permission deny|plan|accept-edits|accept-all]
nika new [NAME] [--wizard] [--template T] [--verb V] [--with-mcp]

# TRACES
nika trace list [--limit N]
nika trace show <ID>                       # Partial ID match
nika trace export <ID> [--format json|yaml] [--output FILE]
nika trace clean [--keep N]

# PROVIDERS
nika provider list                         # Status: keychain/env/missing
nika keys set <NAME> [KEY]             # Store in OS keychain
nika provider get <NAME>                   # Show masked key
nika keys remove <NAME>
nika provider test <NAME>                  # Test API connectivity
nika provider migrate                      # Env vars → keychain

# MCP SERVERS
nika mcp list [-w WORKFLOW] [--global] [--project]
nika mcp add <NAME|ALIAS> [--command X] [--args ...]
nika mcp remove <NAME>
nika mcp aliases [-c CATEGORY]             # Show 100 aliases
nika mcp test <WORKFLOW> <SERVER>          # Test connectivity
nika mcp tools <WORKFLOW> <SERVER>         # List server tools

# CONFIG
nika config list | get <KEY> | set <KEY> <VAL> | edit | path

# DIAGNOSTICS
nika doctor [--full] [--format text|json]

# LSP
nika lsp [--mode stdio|tcp] [--port 9257]

# OTHER
nika completion <bash|zsh|fish>
nika pkg [SUBCOMMAND]                      # Package management
nika model [SUBCOMMAND]                    # GGUF model management
nika schema [SUBCOMMAND]                   # Schema versions
```

---

## 13. Comparison with Existing Tools

| Feature | Nika | LangChain (Python) | CrewAI | Dify | Prefect |
|---------|------|-------------------|--------|------|---------|
| **Language** | Rust (binary) | Python | Python | Python/TS | Python |
| **Workflow definition** | Declarative YAML | Imperative code | Imperative code | Visual + YAML | Imperative code |
| **Compilation** | 3-phase compiler | None (interpreted) | None | None | None |
| **Type safety** | Compile-time validation | Runtime errors | Runtime errors | Runtime | Runtime |
| **DAG execution** | Native parallel Tokio | Sequential default | Sequential | Sequential | Native parallel |
| **LLM providers** | 7 (rig-core unified) | 100+ | OpenAI-centric | 10+ | N/A (not LLM-specific) |
| **Local models** | GGUF via mistral.rs | Ollama integration | Ollama | Ollama | N/A |
| **MCP support** | Native (rmcp v0.16) | Via adapter | None | None | None |
| **Observability** | 34-event NDJSON traces | LangSmith (paid) | CrewAI logs | Built-in | Built-in |
| **IDE support** | LSP + VS Code extension | None | None | Visual editor | None |
| **TUI** | 3-view ratatui terminal | None | None | Web UI | Web UI |
| **Binary size** | Single binary (~30MB) | pip install (100+ deps) | pip install | Docker | pip install |
| **Startup time** | Instant | 2-5s (Python import) | 2-5s | Container boot | 1-3s |
| **Structured output** | 4-layer defense | Manual parsing | Basic | Template-based | N/A |
| **Error codes** | 60+ typed codes (NIKA-XXX) | Python exceptions | Python exceptions | Generic | Generic |
| **Security** | OS Keychain + daemon | .env files | .env files | Platform secrets | Platform secrets |
| **Tests** | 6,526 | Varies | Limited | Limited | Extensive |

### Key Differentiators

**1. YAML-first, not code-first**: Workflows are data, not programs. This enables static analysis, IDE support, and validation before execution. LangChain/CrewAI workflows are Python code — you can only find errors by running them.

**2. Compiler architecture**: The 3-phase pipeline (parse → analyze → lower) catches errors like cycle dependencies, missing task references, and invalid bindings at compile time. Other tools discover these at runtime.

**3. Native MCP**: Nika is the only workflow engine with first-class MCP (Model Context Protocol) support. 48 pre-configured aliases. Others require custom adapter code.

**4. Single Rust binary**: No Python environment, no Docker, no dependency conflicts. `nika run workflow.nika.yaml` just works. Startup is instant vs seconds for Python-based tools.

**5. Full trace observability**: 34 structured event types in NDJSON format. Not just logs — structured data with token counts, costs, durations, and full provenance chain. No paid service required (unlike LangSmith).

**6. Parallel-first DAG**: Tasks without dependencies run in parallel automatically via Tokio. No explicit parallelism configuration needed. `for_each` adds controlled concurrent iteration with semaphores.

---

## 14. Complete Workflow Examples

### Example 1: Content Pipeline

```yaml
schema: nika/workflow@0.12
workflow: content-pipeline
provider: anthropic

mcp:
  novanet:
    command: cargo
    args: [run, --manifest-path, ../novanet/tools/novanet-mcp/Cargo.toml]

tasks:
  - id: get_context
    invoke:
      mcp: novanet
      tool: novanet_context
      params:
        focus_key: "qr-code"
        locale: "fr-FR"
        mode: "page"

  - id: research
    with:
      context: get_context
    infer:
      prompt: |
        Based on this knowledge graph context:
        {{with.context}}

        Identify the 3 most important topics to cover.
      temperature: 0.3
    depends_on: [get_context]

  - id: write_sections
    with:
      topics: research
      context: get_context
    for_each: $research
    as: topic
    concurrency: 3
    infer:
      prompt: |
        Write a detailed section about: {{with.topic}}
        Use this context: {{with.context}}
      max_tokens: 2000
    depends_on: [research, get_context]

  - id: assemble
    with:
      sections: write_sections
    infer: |
      Combine these sections into a cohesive article:
      {{with.sections}}
    depends_on: [write_sections]
```

### Example 2: Multi-Provider Comparison

```yaml
schema: nika/workflow@0.12
workflow: model-comparison

tasks:
  - id: claude_response
    provider: anthropic
    model: claude-sonnet-4-6
    infer: "Explain quantum entanglement in 100 words"

  - id: gpt_response
    provider: openai
    model: gpt-4o
    infer: "Explain quantum entanglement in 100 words"

  - id: mistral_response
    provider: mistral
    model: mistral-large-latest
    infer: "Explain quantum entanglement in 100 words"

  - id: compare
    with:
      claude: claude_response
      gpt: gpt_response
      mistral: mistral_response
    infer: |
      Compare these 3 explanations of quantum entanglement.
      Rate each on accuracy, clarity, and conciseness.

      Claude: {{with.claude}}
      GPT-4o: {{with.gpt}}
      Mistral: {{with.mistral}}
    depends_on: [claude_response, gpt_response, mistral_response]
```

### Example 3: Agent with MCP Tools

```yaml
schema: nika/workflow@0.12
workflow: research-agent
provider: anthropic

mcp:
  novanet:
    command: cargo
    args: [run, --manifest-path, ../novanet/tools/novanet-mcp/Cargo.toml]

tasks:
  - id: researcher
    agent:
      goal: |
        Research all entities related to QR codes in the knowledge graph.
        For each entity, get its French localization.
        Write a comprehensive report summarizing findings.
        Save the report to ./output/qr-research.md
      tools:
        - novanet::novanet_search
        - novanet::novanet_context
        - nika:write
        - nika:read
      max_iterations: 20
      max_tokens: 8192
```

---

## 15. Project Statistics

```
  ┌────────────────────────────────────────────────┐
  │  NIKA v0.27.0 — BY THE NUMBERS                │
  │                                                 │
  │  Source Files:        370 .rs files             │
  │  Source Lines:        216,000 lines of Rust     │
  │  Test Files:          102 integration tests     │
  │  Total Tests:         6,526                    │
  │  Test Strategy:       Mock (wiremock, McpMock)  │
  │                       + Live (ignored, opt-in)  │
  │                                                 │
  │  Schema Version:      @0.12                     │
  │  Error Codes:         60+ (NIKA-000..429)       │
  │  Event Types:         34 NDJSON variants        │
  │  Builtin Tools:       12 (7 core + 5 file)      │
  │  MCP Aliases:         48 pre-configured         │
  │  LLM Providers:       7 (6 cloud + 1 local)     │
  │  Known Models:        15 (curated GGUF)         │
  │  Transforms:          27 pipe functions         │
  │  CLI Commands:        18 subcommands            │
  │  TUI Views:           4                         │
  │  LSP Features:        7 capabilities            │
  │  Keyboard Shortcuts:  40+                       │
  │                                                 │
  │  Key Dependencies:                              │
  │  ├─ rig-core v0.32 (LLM framework)             │
  │  ├─ rmcp v0.16 (MCP protocol)                  │
  │  ├─ tokio v1.49 (async runtime)                │
  │  ├─ ratatui (TUI framework)                    │
  │  ├─ tower-lsp v0.20 (LSP server)              │
  │  ├─ serde-saphyr v0.20 (YAML parsing)         │
  │  ├─ marked-yaml v0.8 (span tracking)          │
  │  ├─ jsonschema v0.26 (JSON Schema)             │
  │  └─ mistralrs v0.7 (native inference)          │
  │                                                 │
  │  Feature Flags:                                 │
  │  ├─ tui (default)                               │
  │  ├─ lsp (opt-in)                                │
  │  ├─ nika-daemon (default)                       │
  │  ├─ native-keychain (default)                   │
  │  ├─ native-inference (default)                  │
  │  └─ integration (test-only)                     │
  └────────────────────────────────────────────────┘
```
