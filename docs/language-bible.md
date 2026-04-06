# The Nika Language Bible

> How to write Nika like a native speaker.
> This is not a reference manual — it's a guide to **taste**.
> The reference tells you what's valid. The Bible tells you what's **good**.

---

## Philosophy

Seven principles. Every decision in this guide traces back to one of them.

1. **The workflow IS the documentation.** If you can't read it in 30 seconds, it's too complex.
2. **`with:` is the DAG.** Every `$binding` creates a dependency edge. That's the only concept to learn.
3. **Zero tokens for data.** Transforms and `jq` for reshaping. `infer:` for thinking. Never the reverse.
4. **Natural prompts always.** The 5-layer defense handles JSON. Never mention format in your prompt.
5. **Cheap for bulk, smart for synthesis.** Haiku for 50 items. Sonnet for the final report.
6. **Fail explicit, never silent.** Prefer a clear error to surprising behavior.
7. **Five verbs, three layers.** Basics in 30 minutes. Power in an afternoon. Expert over weeks.

---

## The Three Layers

You don't need to learn everything at once.

### Layer 1 — Basics (80% of workflows)

`infer:`, `fetch:`, `with:`, `for_each:`, `structured:`, `inputs:`.
This covers: scrape + summarize, translate, extract data, parallel processing.

### Layer 2 — Power (15% of workflows)

`exec:`, `invoke:`, `agent:`, transforms (top 15), `retry:`, `when:`, `artifact:`.
This covers: shell commands, builtin tools, autonomous agents, error handling.

### Layer 3 — Expert (5% of workflows)

All 61 tools, `jq()`, presets, guardrails, `routing:`, `on_error:`, `decompose:`,
`context_budget:`, `from_example:`, `repair_model:`, completion confidence routing.
This covers: production hardening, cost optimization, complex orchestration.

---

## Part 1: Data Flow

### `with:` is the heart of the language

`with:` does three things at once:
1. **Binds data** from upstream tasks
2. **Transforms data** via pipe chains
3. **Creates DAG edges** (implicit dependencies)

```yaml
- id: process
  with:
    raw: $fetch_page                         # 1. bind the output of fetch_page
    clean: $fetch_page.text_content | trim   # 2. extract field + transform
    safe: $fetch_page.title ?? "Untitled"    # 3. fallback if null
  infer: "Process: {{with.clean}}"           # reference via {{with.alias}}
```

**Rule: `with:` creates the dependency automatically.** You almost never need `depends_on:`.

```yaml
# ❌ Redundant — depends_on is implicit from with:
- id: step2
  depends_on: [step1]
  with: { data: $step1 }

# ✅ Clean — with: already creates the edge
- id: step2
  with: { data: $step1 }
```

`depends_on:` is only for ordering without data flow (e.g., "wait for the file to be written
before reading it with a different task").

### Binding paths

```yaml
with:
  full: $task                        # entire task output
  field: $task.name                  # nested field access
  deep: $task.users[0].profile.role  # array index + nested
  safe: $task.score ?? 0             # fallback on null (JSON literal only)
  env: $env.API_KEY                  # environment variable
  input: $inputs.locale              # workflow input parameter
  ctx: $context.files.readme         # loaded context file
  vault: $vault.stripe.secret_key    # encrypted vault access
```

**Array indexing** uses brackets: `$task.items[0].name`. Indices are zero-based.
Out-of-bounds returns null (use `??` to guard).

**The `??` operator:**
- Fires when the path resolves to null (missing field, missing task output)
- Accepts any JSON literal: `?? "text"`, `?? 0`, `?? true`, `?? null`, `?? []`, `?? {}`
- Single quotes do NOT work: `?? 'text'` is invalid (must be `?? "text"`)
- Only one `??` per binding (the rightmost one wins)
- Fires AFTER transforms: `$task | upper ?? "fallback"` — if task is null, upper never runs

### Templates

Templates use `{{...}}` in any string value. Three namespaces:

```yaml
infer: "Hello {{with.name}}, topic is {{inputs.topic}}, context: {{context.readme}}"
```

| Namespace | Source | Example |
|-----------|--------|---------|
| `with.*` | Bound from `with:` block | `{{with.data}}` |
| `inputs.*` | Workflow parameters | `{{inputs.locale}}` |
| `context.*` | Loaded files | `{{context.files.readme}}` |

**Object injection:** If your binding is an object or array, you MUST use `| to_json`:

```yaml
# ❌ Objects render as "[object Object]" or error
infer: "Dimensions: {{with.dims}}"

# ✅ Serialize for safe injection
infer: "Dimensions: {{with.dims | to_json}}"
```

**Content truncation:** Always truncate large fetched content before prompt injection:

```yaml
with:
  page: $scrape.text_content | first(8000)    # max 8K chars
infer: "Summarize: {{with.page}}"
```

Or use automatic budget management:

```yaml
- id: summarize
  context_budget: 4000       # max tokens across ALL with: bindings
  with: { page: $scrape.text_content }
  infer: "Summarize: {{with.page}}"
```

`context_budget:` truncates the largest bindings proportionally. CJK-aware (2 chars/token
for CJK vs 4 chars/token for Latin). Minimum 50 tokens per binding preserved.

---

## Part 2: Transforms

63 transforms available via pipe chains: `$task.field | trim | upper | first(100)`.

### When to use what

```
Simple string → | trim, | upper, | lower, | replace("a","b"), | truncate(N)
Simple array  → | first, | last, | length, | join(","), | sort, | unique, | flatten
Field extract → | pluck(field)
Array filter  → | where(field, val) or | where(field, ">", 100)
Sort objects  → | sort_by(field)
Group objects → | group_by(field)
Keep/remove   → | pick(f1, f2) / | omit(f1, f2)
Regex extract → | regex("pattern")       — returns first match or null
Any complex   → | jq([.[] | select(.score > 80) | .name])
```

### The top 15 you'll use constantly

| Transform | What it does | Gotcha |
|-----------|-------------|--------|
| `trim` | Strip whitespace | Fails on null — guard with `default("")` |
| `first` | First element or char | On array: first item. On string: first char |
| `first(N)` | First N elements/chars | **THE context budget pattern** |
| `length` | Count items/chars | Works on null (returns null, not error) |
| `join(",")` | Array to string | Separator is required |
| `to_json` | Serialize to JSON string | **Required for objects in prompts** |
| `pluck(field)` | Extract field from array of objects | Supports dot-paths: `pluck("addr.city")` |
| `where(f, v)` | Filter by field equality | Default op is `eq`. Also: `!=`, `>`, `<`, `>=`, `<=` |
| `default("x")` | Fallback value | Fires on null AND empty string |
| `jq(expr)` | Full jq stdlib inline | LRU-cached (1000x speedup in for_each) |
| `sort_by(field)` | Sort array of objects | Supports dot-paths |
| `unique` | Deduplicate array | Compares by JSON serialization |
| `flatten` | Flatten one level | Single level only, not recursive |
| `compact` | Remove nulls | Also removes empty strings |
| `parse_json` | String to JSON value | Idempotent. Strips markdown fences + BOM |

### Null safety rules

19 transforms fail on null input with NIKA-153. Always guard:

```yaml
# ❌ Crashes if $task returns null
with:
  name: $task.user.name | upper

# ✅ Guard with default()
with:
  name: $task.user.name | default("Unknown") | upper
```

**Propagating transforms** (return null on null, no error): `length`, `keys`, `values`,
`to_string`, `to_json`, `type_of`, `has`, `not`, `add`.

All others are **strict** (NIKA-153 on null).

### `| jq(expr)` — the power tool

Full jq stdlib via jaq-core 3.0. 100+ functions. LRU-cached compilation.

```yaml
# Inline — zero overhead, replaces simple invoke: nika:jq
with:
  names: $users | jq([.[] | .name])
  adults: $users | jq([.[] | select(.age >= 18)])
  stats: $data | jq({total: length, avg: (map(.score) | add / length)})
```

When to use `| jq(expr)` vs `invoke: nika:jq`:
- Simple expressions → `| jq(expr)` inline (zero task overhead)
- Complex multi-line → `invoke: nika:jq` with `expression:` param
- Inside `for_each` → always `| jq(expr)` or `invoke: nika:jq` (compilation is cached)

---

## Part 3: The 5 Verbs

### `infer:` — think

Ask any LLM to generate, analyze, summarize, translate, extract.

```yaml
# Short form — simple prompt
- id: haiku
  infer: "Write a haiku about butterflies"

# Full form — with options
- id: analyze
  model: claude-sonnet-4-20250514
  infer:
    prompt: "Analyze this data: {{with.data | to_json}}"
    system: "You are a data analyst. Be concise."
    temperature: 0.3
    max_tokens: 2000
```

**Where fields go:**
- `model:`, `provider:`, `preset:` → **task level** (slash syntax: `model: groq/llama-3.3-70b`)
- `prompt:`, `system:`, `temperature:`, `max_tokens:` → **inside `infer:` block** (full form)
- Short form only: `temperature:`, `max_tokens:`, `system:` can be at task level

### `fetch:` — get data from the web

```yaml
# Short form — simple GET
- id: page
  fetch: "https://example.com"

# With extraction — 11 modes
- id: article
  fetch:
    url: "https://blog.example.com/post"
    extract: article          # Returns { title, content, text_content, excerpt, byline }

# POST with JSON body
- id: api_call
  fetch:
    url: "https://api.example.com/data"
    method: POST
    json:                     # Auto-serializes + sets Content-Type
      query: "{{with.search}}"
      limit: 10
```

**Extract modes (11):**

| Mode | Returns | Use for |
|------|---------|---------|
| `markdown` | Clean Markdown | Blog posts, docs |
| `article` | `{ title, text_content, excerpt, byline }` | News articles |
| `text` | Visible text (+ optional CSS selector) | Any HTML |
| `selector` | Raw HTML of matched elements | Scraping specific elements |
| `metadata` | OG, Twitter Cards, JSON-LD | SEO analysis |
| `links` | Classified link list | Crawling |
| `jsonpath` | JSONPath query result | JSON APIs |
| `feed` | Parsed RSS/Atom/JSON Feed | RSS readers |
| `llm_txt` | AI content discovery | /llms.txt |
| `sitemap` | Parsed XML sitemap | Site mapping |
| `metadata_links` | Combined metadata + links | Full page analysis |

**Important:** `extract: article` returns an OBJECT, not text. Use `$task.text_content`
to get the plain text string.

**`json:` vs `body:`:** Use `json:` for JSON payloads (auto-serializes). Use `body:` only
for raw text, XML, or form-encoded data. Never `body: "{{with.obj | to_json}}"` — use `json:`.

**Hidden features:** `cache: true` (HTTP caching), `session: true` (cookie persistence),
`response: slim` (metadata only, no body download).

### `exec:` — run shell commands

```yaml
# Short form — no shell
- id: list
  exec: "ls -la"

# With shell features (pipes, redirects)
- id: count
  exec:
    command: "cat data.txt | wc -l"
    shell: true
```

**CRITICAL: `| shell` is mandatory** for all template bindings in `shell: true` commands:

```yaml
# ❌ NIKA-053 — blocked since v0.66
exec:
  command: "echo {{with.input}}"
  shell: true

# ✅ Correct
exec:
  command: "echo {{with.input | shell}}"
  shell: true

# ✅ Single quotes are exempt
exec:
  command: "jq --arg x '{{with.val}}' '.data'"
  shell: true
```

`| shell` wraps the value in POSIX single quotes with proper escaping.

### `invoke:` — call tools

```yaml
# invoke: always requires block form (no short form)
- id: dims
  invoke:
    tool: nika:dimensions
    params:
      input: "photo.jpg"

# With params
- id: resize
  invoke:
    tool: nika:thumbnail
    params:
      hash: "{{with.img.hash}}"
      width: 800

# MCP server tool
- id: search
  invoke:
    tool: "novanet::novanet_search"
    params:
      query: "{{with.topic}}"
```

62 builtin tools (`nika:*`) available without any server. Key ones:

| Tool | Purpose | Key param |
|------|---------|-----------|
| `nika:jq` | Full jq on JSON data | `expr:` |
| `nika:import` | File → CAS hash | `path:` |
| `nika:write` | Write file (always use `overwrite: true`) | `file_path:`, `content:`, `overwrite:` |
| `nika:read` | Read file (`raw: true` saves 15% tokens) | `file_path:`, `raw:`, `optional:` |
| `nika:pipeline` | Chain media ops in-memory | `hash:`, `steps:` |
| `nika:chunk` | Split text for RAG | `text:`, `chunk_size:`, `mode:` |
| `nika:filter` | Filter array with operators | `array:`, `field:`, `op:`, `value:` |
| `nika:aggregate` | Sum/avg/min/max on array | `array:`, `ops:`, `field:` |
| `nika:inject` | Replace markers in template file | `template:`, `output:`, `content:` |

### `agent:` — autonomous multi-turn

```yaml
- id: research
  agent:
    prompt: "Research the competitive landscape for {{inputs.product}}"
    system: "Break complex questions into sub-questions, reason through each, synthesize."
    tools: [nika:read, nika:write, nika:glob, nika:grep]
    max_turns: 15
    completion:
      mode: natural       # Stops when no more tool calls
    limits:
      max_cost_usd: 1.00
```

**8 built-in presets** — use `preset:` or `from:`:

| Preset | Model | temp | turns | Use for |
|--------|-------|------|-------|---------|
| `think` | sonnet | 0.3 | 5 | Deep reasoning |
| `lite` | haiku | 0.5 | 3 | Fast, cheap |
| `search` | sonnet | 0.3 | 10 | Research |
| `vision` | sonnet | 0.3 | 3 | Image analysis |
| `judge` | sonnet | 0.1 | 3 | Evaluation, PASS/FAIL |
| `coder` | sonnet | 0.2 | 8 | Code writing |
| `summary` | haiku | 0.3 | 3 | Summarization (cheap) |
| `creative` | sonnet | 0.9 | 5 | Creative writing |

```yaml
# Use a preset — inherits all config, override what you need
- id: classify
  preset: judge
  infer: "Is this email spam? Answer PASS or FAIL."
```

**Presets work on BOTH `infer:` and `agent:`** — not just agents.

**Completion modes:**
- `explicit` — agent must call `nika:complete` to stop (default for tool-heavy agents)
- `natural` — stops when the agent makes no more tool calls (good for reasoning)
- `pattern` — stops when output matches a regex

**Guardrails** (work on both `agent:` AND `infer:` tasks):

```yaml
guardrails:
  - type: length
    max_words: 2000
    on_failure: retry
  - type: schema
    json_schema: { type: object, required: [findings] }
  - type: regex
    pattern: "^## (Findings|Summary)"
  - type: llm
    judge_prompt: "Is this factually accurate? PASS or FAIL."
    pass_pattern: "^PASS"
```

---

## Part 4: Structured Output

The killer feature. Guaranteed schema-valid JSON from ANY provider.

### The golden rule

**Never mention JSON in the prompt.** The 5-layer defense injects schema instructions automatically.
Your prompt should describe WHAT you want in natural language.

```yaml
# ❌ WRONG — defeats the 5-layer defense
- id: extract
  infer: "Return a JSON object with name, age, and skills for Alice, 30, developer"
  structured: { schema: { ... } }

# ✅ CORRECT — natural prompt
- id: extract
  infer: "Tell me about Alice, 30 years old, Rust and Python developer"
  structured:
    schema:
      type: object
      required: [name, age, skills]
      properties:
        name: { type: string }
        age: { type: number, minimum: 0 }
        skills: { type: array, items: { type: string }, minItems: 1 }
```

### All 8 fields

```yaml
structured:
  schema: { ... }                    # JSON Schema (inline or file path)
  from_example:                      # OR give an example (mutually exclusive)
    name: "Alice"
    score: 42
    tags: ["rust", "python"]
  strict: true                       # from_example only: no extra keys allowed
  enable_tool_injection: true        # Layer 0 toggle (default: true)
  enable_retry: true                 # Layer 3 toggle (default: true)
  enable_repair: true                # Layer 4 toggle (default: true)
  max_retries: 2                     # Layer 3 retry count (default: 2)
  repair_model: claude-haiku-4-5     # Layer 4 uses cheaper model (default: task model)
```

### Shorthand

```yaml
# File reference
structured: ./schemas/user.json

# From example — simpler than writing a schema
structured:
  from_example:
    name: "Example"
    price: 29.99
    in_stock: true
  strict: true
```

`from_example` output keys are **reordered to match your example** — guaranteed.

### Cost optimization

```yaml
structured:
  schema: { ... }
  repair_model: claude-haiku-4-5    # 10x cheaper for JSON repair
  max_retries: 3                    # More attempts with cheap model
```

### Fail-fast mode

```yaml
structured:
  schema: { ... }
  enable_retry: false
  enable_repair: false
  # Layer 2 only — 1 attempt, fail immediately if invalid
```

---

## Part 5: Parallel Execution

### for_each

```yaml
- id: translate
  for_each: ["en", "fr", "ja", "de", "ko"]
  as: lang
  concurrency: 5
  fail_fast: false
  with: { text: $source }
  infer: "Translate to {{with.lang}}: {{with.text}}"
```

**CRITICAL: `concurrency: 1` is the default (SEQUENTIAL).** Always set it explicitly.

**CRITICAL: Output is ALWAYS an array.** Even with concurrency: 1.

```yaml
# ❌ WRONG — $translate is an Array, not a scalar
with: { text: $translate }
infer: "{{with.text.title}}"          # CRASH

# ✅ CORRECT
with:
  all: $translate                      # the array
  first: $translate | first            # first element
  count: $translate | length           # count
  titles: $translate | pluck(title) | join(", ")
```

**Items sources:**

```yaml
for_each: ["inline", "array"]                          # literal
for_each: $upstream_task                               # task output
for_each: "$task.data.items"                           # nested path
for_each:
  items: "$data | pluck('url') | unique"               # with transforms
  as: url
  concurrency: 10
```

**Auto-parse:** If a task outputs a JSON string (`'["a","b"]'`), `for_each` parses it
automatically. No `| parse_json` needed. Even handles markdown-fenced JSON.

**Null items → zero iterations** (not error). Empty array → zero iterations.

**Max 50,000 items** (NIKA-026 above that).

**`fail_fast: false`** — always use for network tasks. Failed items produce null in the
output array. Downstream tasks must handle nulls.

### Automatic parallelism

Tasks with no shared dependencies run simultaneously. The DAG engine figures it out:

```yaml
tasks:
  # These 3 run IN PARALLEL (no dependencies between them)
  - id: research_tech
    infer: "Technical analysis of {{inputs.topic}}"
  - id: research_social
    infer: "Social analysis of {{inputs.topic}}"
  - id: research_economic
    infer: "Economic analysis of {{inputs.topic}}"

  # This runs AFTER all 3 complete (with: creates edges)
  - id: synthesize
    with:
      tech: $research_tech
      social: $research_social
      economic: $research_economic
    infer: "Synthesize: {{with.tech}} + {{with.social}} + {{with.economic}}"
```

Never add `depends_on: []` to tasks that should be parallel — that's the default.

**Hard cap: 64 concurrent tasks** per workflow run. Not configurable. Includes
both regular tasks and `for_each` iterations sharing a global semaphore.

---

## Part 6: Cost Optimization

Nine patterns that save real money.

### 1. Cost routing — cheap models for bulk

```yaml
tasks:
  - id: summarize_each
    model: claude-haiku-4-5              # $0.25/MTok — 12x cheaper
    for_each: $articles
    concurrency: 10
    infer: "Summarize: {{with.item | first(4000)}}"

  - id: synthesis
    model: claude-sonnet-4-20250514      # $3/MTok — only for the final step
    with: { summaries: $summarize_each }
    infer: "Executive report: {{with.summaries | to_json}}"
```

### 2. repair_model on structured output

```yaml
structured:
  schema: { ... }
  repair_model: claude-haiku-4-5         # JSON repair doesn't need a smart model
```

### 3. `| first(N)` for context budget

```yaml
with:
  page: $scrape.text_content | first(8000)
```

### 4. Zero-token data transforms

```yaml
# ❌ Costs tokens
- id: get_names
  infer: "Extract names from: {{with.users | to_json}}"

# ✅ Free — zero LLM calls
- id: next
  with:
    names: $users | pluck(name) | join(", ")
```

### 5. Presets for automatic cost control

```yaml
- id: classify
  preset: lite              # Uses haiku automatically
  infer: "Is this spam?"
```

### 6. `provider: mock` for development

```yaml
provider: mock              # Workflow default
# Override at CLI: nika run flow.nika.yaml --provider anthropic
```

### 7. `when:` to skip unnecessary tasks

```yaml
- id: translate
  when: "{{inputs.locale != 'en'}}"
```

### 8. `response: slim` for URL checks

```yaml
fetch: { url: "...", response: slim }    # No body download
```

### 9. `context_budget:` for automatic truncation

```yaml
- id: analyze
  context_budget: 4000
  with: { doc1: $fetch1, doc2: $fetch2, doc3: $fetch3 }
```

---

## Part 7: Error Handling and Resilience

### Task-level retry

```yaml
- id: api_call
  retry:
    max_attempts: 3
    delay_ms: 1000
    backoff: 2.0              # Exponential: 1s, 2s, 4s
  fetch: { url: "https://api.example.com" }
```

**`retry:` goes at TASK level, never inside verb blocks.** `retry:` inside `fetch:` is
silently ignored — this is the #1 gotcha.

Retried errors: 429, 500, 502, 503, timeout, connection refused.
NOT retried: 401, 403 (permanent failures).

### Provider fallback

```yaml
- id: generate
  routing:
    fallback: [anthropic, openai, gemini]
  infer: "..."
```

If anthropic fails (missing key, 500), automatically tries openai, then gemini.
Emits `ProviderFallback` events for observability.

### Task-level error handling

```yaml
# Ignore failure — output is null, workflow continues
- id: optional_step
  on_error:
    ignore: true
  fetch: { url: "https://maybe-down.com" }

# Retry with different provider
- id: important_step
  on_error:
    retry_with_provider: openai
  infer: "..."

# Run alternative task on failure
- id: primary
  on_error:
    fallback: backup_task_id
  infer: "..."
```

### `when:` — conditional execution

```yaml
- id: translate
  when: "{{inputs.locale != 'en'}}"
  infer: "Translate to {{inputs.locale}}"
```

**Warning:** Skipped tasks return null. Downstream tasks STILL RUN and receive null.
Always propagate `when:` or guard with `??`:

```yaml
# ✅ Option A: propagate when:
- id: save
  when: "{{inputs.locale != 'en'}}"
  with: { text: $translate }

# ✅ Option B: null guard
- id: save
  with: { text: $translate ?? "original text" }
```

---

## Part 8: Anti-Patterns

### AP-01: Mentioning JSON in structured output prompts

```yaml
# ❌
infer: "Return a JSON object with name, age, skills"
structured: { schema: { ... } }

# ✅
infer: "Tell me about Alice, 30, Rust and Python developer"
structured: { schema: { ... } }
```

### AP-02: Using LLM for deterministic data transforms

```yaml
# ❌ Costs tokens, non-deterministic
infer: "Extract all names from: {{with.users | to_json}}"

# ✅ Free, deterministic
with:
  names: $users | pluck(name)
```

### AP-03: `for_each` without explicit `concurrency:`

```yaml
# ❌ SEQUENTIAL (concurrency: 1 default) — 50x slower
for_each: $items
infer: "..."

# ✅ Parallel
for_each: $items
concurrency: 10
fail_fast: false
infer: "..."
```

### AP-04: `retry:` inside verb blocks

```yaml
# ❌ Silently ignored — retry is not a fetch: field
fetch:
  url: "..."
  retry: { max_attempts: 3 }

# ✅ Correct — retry at task level
retry: { max_attempts: 3, delay_ms: 1000 }
fetch: { url: "..." }
```

### AP-05: `structured:` inside `infer:` block

```yaml
# ❌ Silently ignored — structured is not an infer: field
infer:
  prompt: "..."
  structured: { schema: { ... } }

# ✅ Correct — structured at task level
infer:
  prompt: "..."
structured:
  schema: { ... }
```

### AP-06: Treating `for_each` output as scalar

```yaml
# ❌ CRASH — $results is an Array
with: { data: $my_foreach }
infer: "{{with.data.title}}"

# ✅ Access array elements
with:
  first: $my_foreach | first
  all: $my_foreach | to_json
```

### AP-07: Using `body:` for JSON payloads

```yaml
# ❌ Manual serialization
fetch: { url: "...", method: POST, body: "{{with.data | to_json}}" }

# ✅ Auto-serializes + sets Content-Type
fetch: { url: "...", method: POST, json: { query: "{{with.term}}" } }
```

### AP-08: `extract: article` treated as string

```yaml
# ❌ CRASH — article returns an object
with: { text: $scrape }
infer: "{{with.text | trim}}"

# ✅ Extract the text field
with: { text: $scrape.text_content }
infer: "{{with.text | trim}}"
```

### AP-09: No `| shell` in shell: true commands

```yaml
# ❌ NIKA-053
exec: { command: "echo {{with.val}}", shell: true }

# ✅
exec: { command: "echo {{with.val | shell}}", shell: true }
```

### AP-10: Missing `overwrite: true` on nika:write

```yaml
# ❌ NIKA-215 on second run
invoke: { tool: nika:write, params: { file_path: "out.md", content: "..." } }

# ✅ Idempotent
invoke: { tool: nika:write, params: { file_path: "out.md", content: "...", overwrite: true } }
```

### AP-11: Expensive model for everything

```yaml
# ❌ Sonnet for 50 simple summaries
model: claude-sonnet-4-20250514
for_each: $articles
infer: "Summarize"

# ✅ Haiku for bulk, Sonnet for synthesis
- id: bulk
  model: claude-haiku-4-5
  for_each: $articles
  infer: "Summarize"
- id: synthesis
  model: claude-sonnet-4-20250514
  infer: "Executive report: {{with.bulk | to_json}}"
```

### AP-12: Large content without truncation

```yaml
# ❌ Could be 200K chars — blows context
with: { page: $scrape.text_content }

# ✅ Truncate
with: { page: $scrape.text_content | first(8000) }
```

### AP-13: Missing `when:` propagation

```yaml
# ❌ translate skipped → save gets null → crash
- id: translate
  when: "{{inputs.locale != 'en'}}"
  infer: "Translate"
- id: save
  with: { text: $translate }    # null!

# ✅ Propagate or guard
- id: save
  when: "{{inputs.locale != 'en'}}"
  with: { text: $translate }
```

### AP-14: Objects in prompts without `| to_json`

```yaml
# ❌ "[object Object]"
infer: "Analyze: {{with.metadata}}"

# ✅
infer: "Analyze: {{with.metadata | to_json}}"
```

---

## Part 9: Provider and Model

### 9 primary providers

| Provider | Default Model | Env Var |
|----------|--------------|---------|
| `anthropic` (alias: `claude`) | claude-sonnet-4-6 | `ANTHROPIC_API_KEY` |
| `openai` (alias: `gpt`) | gpt-4o | `OPENAI_API_KEY` |
| `gemini` (alias: `google`) | gemini-2.0-flash | `GEMINI_API_KEY` |
| `mistral` | mistral-large-latest | `MISTRAL_API_KEY` |
| `groq` | llama-3.3-70b-versatile | `GROQ_API_KEY` |
| `deepseek` (alias: `deep-seek`) | deepseek-chat | `DEEPSEEK_API_KEY` |
| `xai` (alias: `grok`) | grok-3-fast | `XAI_API_KEY` |
| `native` (alias: `local`) | local GGUF model | `NIKA_NATIVE_MODEL_PATH` |
| `mock` | deterministic responses | — |

7 additional OpenAI-compatible providers: `openrouter`, `together`, `fireworks`,
`cerebras`, `sambanova`, `cohere`, `ai21`.

### Custom endpoints (vLLM, Ollama, etc.)

```yaml
# Slash syntax — provider inferred from prefix
- id: local
  model: ollama/llama3.2
  infer: "Hello from Ollama"

# Named endpoint from nika.toml
- id: gpu
  model: h100/Qwen/Qwen3-8B
  infer: "Hello from vLLM"
```

```toml
# nika.toml or ~/.config/nika/config.toml
[endpoints.h100]
base_url = "http://10.0.1.42:8000/v1"
model = "Qwen/Qwen3-8B"

[endpoints.ollama]
base_url = "http://localhost:11434/v1"
```

### Model-per-task routing

```yaml
tasks:
  - id: classify
    model: claude-haiku-4-5           # Cheap for classification
    infer: "Is this positive or negative?"

  - id: analyze
    model: claude-sonnet-4-20250514   # Smart for analysis
    with: { sentiment: $classify }
    infer: "Deep analysis given sentiment {{with.sentiment}}"
```

---

## Part 10: Workflow Patterns

### Pattern 1: Scrape + Summarize

```yaml
schema: "nika/workflow@0.12"
provider: claude
tasks:
  - id: scrape
    fetch: { url: "{{inputs.url}}", extract: article }
  - id: summarize
    with: { content: $scrape.text_content | first(8000) }
    infer: "Summarize in 3 bullet points: {{with.content}}"
```

### Pattern 2: Fan-out / Fan-in

```yaml
tasks:
  - id: research_tech
    infer: "Technical angle on {{inputs.topic}}"
  - id: research_social
    infer: "Social angle on {{inputs.topic}}"
  - id: research_economic
    infer: "Economic angle on {{inputs.topic}}"
  - id: synthesize
    with:
      tech: $research_tech
      social: $research_social
      economic: $research_economic
    infer: "Synthesize these perspectives: {{with.tech}} / {{with.social}} / {{with.economic}}"
```

### Pattern 3: Translate N languages

```yaml
tasks:
  - id: translate
    for_each: ["fr", "ja", "de", "ko", "es"]
    as: locale
    concurrency: 5
    fail_fast: false
    with: { text: $source }
    infer: "Translate to {{with.locale}}: {{with.text}}"
```

### Pattern 4: Extract structured data

```yaml
tasks:
  - id: extract
    infer: "Analyze this product listing: {{with.page | first(6000)}}"
    structured:
      from_example:
        name: "Example Product"
        price: 29.99
        in_stock: true
        features: ["feature 1", "feature 2"]
      strict: true
      repair_model: claude-haiku-4-5
```

### Pattern 5: Image pipeline

```yaml
tasks:
  - id: import
    invoke: { tool: nika:import, params: { path: "./photo.jpg" } }
  - id: process
    with: { img: $import }
    invoke:
      tool: nika:pipeline
      params:
        hash: "{{with.img.hash}}"
        steps:
          - { op: thumbnail, width: 800 }
          - { op: optimize }
          - { op: convert, format: webp }
    artifact: { path: output.webp, format: binary }
```

### Pattern 6: Cost-optimized batch processing

```yaml
tasks:
  - id: scrape_all
    for_each: $urls
    concurrency: 5
    fail_fast: false
    fetch: { url: "{{with.item}}", extract: article }

  - id: summarize_all
    model: claude-haiku-4-5
    for_each: $scrape_all
    concurrency: 10
    fail_fast: false
    with: { page: "{{with.item.text_content | first(4000)}}" }
    infer: "Summarize in 2 sentences: {{with.page}}"

  - id: report
    model: claude-sonnet-4-20250514
    with: { summaries: $summarize_all }
    infer: "Executive report from these summaries: {{with.summaries | to_json}}"
    artifact: { path: report.md }
```

### Pattern 7: Agent with format step

```yaml
tasks:
  - id: research
    agent:
      prompt: "Research {{inputs.topic}} thoroughly"
      tools: [nika:read, nika:glob, nika:grep]
      max_turns: 10
      completion: { mode: natural }
  - id: format
    with: { raw: $research }
    infer:
      prompt: "Format this research into a clean report: {{with.raw}}"
      max_tokens: 2000
    artifact: { path: report.md }
```

---

## Part 11: Validation

### Before running: always check

```bash
nika check workflow.nika.yaml          # Syntax + DAG + bindings
nika check workflow.nika.yaml --strict # + test MCP connections
nika lint workflow.nika.yaml           # Best-practice linting (10 rules)
nika run workflow.nika.yaml --dry-run  # Validate without executing
nika test workflow.nika.yaml           # Test with mock provider
```

### What `nika check` validates

- YAML syntax and schema
- DAG cycle detection
- Binding references (task exists, is upstream)
- Template alias declarations
- Provider name recognition with "did you mean?" suggestions
- Model name recognition (warns on unknown prefixes without `/`)
- Model/provider compatibility (e.g. `gpt-4o` on `anthropic` → warning)
- Builtin tool name validation (`nika:typo` → error with "did you mean?")
- Inline template transform validation (`{{with.x | bogus}}` → error)
- Nested template detection (`{{with.a.{{with.b}}}}` → NIKA-074 error)
- Misplaced LLM field detection (task-level `temperature:` with full form `infer:`)
- for_each concurrency hint (>3 literal items without explicit `concurrency:`)
- Skill and context file existence
- Structured schema file validity
- Security hints (missing `| shell` in shell commands)
- Provider API key availability

### What `nika check` does NOT validate (known gaps)

- `extract: selector` / `extract: jsonpath` without `selector:` field
- `temperature:` range (5.0 passes — should warn outside 0-2)
- `thinking_budget:` without `extended_thinking: true`

### 11 lint rules

| Rule | Catches |
|------|---------|
| L001 | Missing workflow description |
| L010 | Missing task description |
| L020 | fetch:/invoke: without retry: |
| L030 | for_each concurrency > 10 (rate limit risk) |
| L031 | for_each without explicit concurrency: (sequential by default) |
| L050 | Agent task (nudge to check max_turns) |
| L060 | Orphan task (no upstream, no downstream) |
| L070 | Single-task workflow |
| L080 | Expensive task after when: without own when: |
| L090 | Duplicate task names |

---

## Appendix: Field Placement Reference

Where each field goes — the definitive answer.

| Field | Task level | Inside verb block | Notes |
|-------|-----------|-------------------|-------|
| `model:` | Yes | No | NIKA-163 if inside infer: |
| `provider:` | Yes | No | |
| `base_url:` | **Removed** | No | Use `[endpoints.*]` in nika.toml |
| `preset:` | Yes | No | Works on infer: and agent: |
| `retry:` | **Yes** | **No (NIKA-163 error)** | Misplaced field detected |
| `structured:` | **Yes** | **No (NIKA-163 error)** | Misplaced field detected |
| `when:` | Yes | No | |
| `artifact:` | Yes | No | Can be array for multi-output |
| `on_error:` | Yes | No | |
| `context_budget:` | Yes | No | |
| `for_each:` | Yes | No | |
| `prompt:` | No | Inside infer:/agent: | |
| `system:` | Task level (shorthand) | Inside infer:/agent: | |
| `temperature:` | Task level (shorthand) | Inside infer:/agent: | |
| `max_tokens:` | Task level (shorthand) | Inside infer:/agent: | |
| `tools:` | No | Inside agent: | |
| `max_turns:` | No | Inside agent: | |
| `guardrails:` | No | Inside agent: (or task level) | |
| `url:` | No | Inside fetch: | |
| `extract:` | No | Inside fetch: | |
| `command:` | No | Inside exec: | |
| `shell:` | No | Inside exec: | |
| `tool:` | No | Inside invoke: | |
| `params:` | No | Inside invoke: | |

**Shorthand rule:** `temperature:`, `max_tokens:`, `system:`, `extended_thinking:`,
`thinking_budget:`, `response_format:` at task level **only work with shorthand** `infer: "prompt"`.
With full form `infer: { prompt: "..." }`, these must go inside the `infer:` block.
**`nika check` now warns** when these fields are at task level with full form infer.

---

<div align="center">

*The workflow IS the documentation.*
*`with:` is the DAG.*
*Five verbs. Three layers. Zero ambiguity.*

</div>
