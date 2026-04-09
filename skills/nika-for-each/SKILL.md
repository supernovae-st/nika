---
name: nika-for-each
description: >-
  Expert at Nika for_each parallel loops in .nika.yaml workflows. Covers the
  critical array-output semantics (output is ALWAYS an array), BUG-003 items
  binding syntax ($binding_ref only), BUG-005 rate limit danger with parallel
  infer, for_each_index, fail_fast, and complete fan-out/fan-in patterns. Use
  when processing arrays, building parallel pipelines, or debugging for_each
  output access errors (schema nika/workflow@0.12).
globs:
  - "**/*.nika.yaml"
---

# Nika for_each: Parallel Loops

Process arrays of items with optional parallelism.

## Syntax

```yaml
- id: process_all
  for_each:
    items: "$outline.sections"   # ← ONLY $binding_ref works (see below)
    as: section                  # loop variable name (access via {{with.section}})
    concurrency: 3               # parallel iterations (default: 1 = sequential!)
    fail_fast: false             # false = continue all; true (default) = stop on first failure
  infer: "Expand this section: {{with.section.title}}"
```

## ⚠️ Output Is ALWAYS a JSON Array

**The most common mistake with for_each.** The task output is `[result_0, result_1, ...]` — always an array, regardless of how many items.

```yaml
# ✅ CORRECT downstream access
- id: consume
  depends_on: [process_all]
  with:
    results: $process_all           # Value::Array([...])
    first: "$process_all | first"   # Get first element
    count: "$process_all | length"  # Count items
  infer: |
    Processed {{with.count}} items.
    First result: {{with.first}}
    All: {{with.results | to_json}}

# ❌ WRONG — results is an array, not a scalar
with:
  title: $process_all.title          # FAILS — access array first
  # Use: $process_all[0].title OR $process_all | first | jq('.title')
```

## ⚠️ BUG-003: Only `$binding_ref` Works in `items:`

```yaml
# ❌ WRONG — template syntax rejected in items:
for_each:
  items: "{{with.data.sections}}"    # NIKA-041 — template not allowed here

# ✅ CORRECT — $binding_ref only
for_each:
  items: "$task_id.sections"         # path access
for_each:
  items: "$inputs.locales"           # inputs reference
for_each:
  items: ["en", "fr", "de"]          # inline array — also works
```

## ⚠️ BUG-005: Rate Limit Danger with Parallel infer

High concurrency with LLM providers causes 429 rate limit errors that cascade into failures.

```yaml
# ❌ DANGEROUS — hits 429 at scale
for_each:
  items: "$outline.sections"
  concurrency: 5              # 5 simultaneous LLM calls
infer: "Expand: {{with.section}}"

# ✅ SAFE — sequential with partial failure tolerance
for_each:
  items: "$outline.sections"
  concurrency: 1              # sequential
  fail_fast: false            # continue even if one fails
infer: "Expand: {{with.section}}"
```

## Default: Sequential (concurrency: 1)

**Parallelism is opt-in.** Without `concurrency: N`, iterations run one by one.

```yaml
# Sequential (default) — safe but slower
for_each:
  items: "$files"
  as: file
exec: "process {{with.file.path | shell}}"

# Parallel — faster but watch rate limits
for_each:
  items: "$files"
  as: file
  concurrency: 4
exec: "process {{with.file.path | shell}}"
```

## fail_fast Semantics

```yaml
# fail_fast: true (DEFAULT) — stop all iterations on first failure
# fail_fast: false — continue all, collect partial results
for_each:
  items: "$urls"
  as: url
  concurrency: 3
  fail_fast: false      # continue even if some URLs fail
fetch:
  url: "{{with.url}}"
  extract: markdown
```

## for_each_index — Current Iteration Index

```yaml
for_each:
  items: "$script.lines"
  as: line
fetch:
  url: "https://api.tts.example.com/generate"
  json:
    text: "{{with.line.text}}"
artifact:
  path: "audio/{{with.for_each_index}}-{{with.line.speaker}}.mp3"
  format: binary
```

## Binary Artifacts in for_each

```yaml
- id: generate_audio
  for_each:
    items: "$script.lines"
    as: line
  fetch:
    url: "https://api.elevenlabs.io/v1/text-to-speech"
    method: POST
    headers:
      xi-api-key: "{{with.key}}"
    json:
      text: "{{with.line.text}}"
    response: binary
  artifact:
    path: "audio/{{with.for_each_index}}-{{with.line.speaker}}.mp3"
    format: binary
```

## Complete Fan-Out / Fan-In Example

```yaml
tasks:
  # 1. Generate list of items to process
  - id: get_urls
    infer: "List 5 research URLs about {{inputs.topic}}"
    structured:
      schema:
        type: object
        properties:
          urls: { type: array, items: { type: string } }

  # 2. Fan-out: fetch all URLs in parallel
  - id: fetch_all
    depends_on: [get_urls]
    with:
      urls: $get_urls.urls
    for_each:
      items: "$get_urls.urls"
      as: url
      concurrency: 3
      fail_fast: false
    fetch:
      url: "{{with.url}}"
      extract: article

  # 3. Fan-in: synthesize all results
  - id: synthesize
    depends_on: [fetch_all]
    with:
      articles: $fetch_all          # Array of article objects
      count: "$fetch_all | length"
    infer: |
      Synthesize {{with.count}} articles into a report:
      {{with.articles | to_json}}
```

## Parallel Multi-Provider Fan-Out

```yaml
# Run same prompt on 3 providers simultaneously
- id: research_web
  provider: xai
  model: grok-3
  infer: "Research: {{inputs.topic}}"

- id: research_academic
  infer: "Academic perspective on: {{inputs.topic}}"

- id: research_creative
  provider: gemini
  model: gemini-2.5-flash
  infer: "Creative angles: {{inputs.topic}}"

- id: merge
  depends_on: [research_web, research_academic, research_creative]
  with:
    web: $research_web
    academic: $research_academic
    creative: $research_creative
  infer: "Merge these 3 perspectives: {{with.web}} | {{with.academic}} | {{with.creative}}"
```

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| `items: "{{with.data}}"` (template) | `items: "$task_id.data"` ($binding_ref only) |
| `$for_each_task.field` directly | `$for_each_task[0].field` — output is array |
| No `concurrency:` = parallel | Default is sequential. Set `concurrency: N` for parallel |
| `concurrency: 5` with LLM | Use `concurrency: 1` + `fail_fast: false` for safety |
| `fail_fast: false` when all must succeed | Use `fail_fast: true` (default) to stop on first error |

## Related Skills

- `/nika-dag` — depends_on, DAG patterns, fan-out/fan-in design
- `/nika-transforms` — array transforms (first, last, length, pluck, where) for consuming for_each output
- `/nika-fetch` — fetch in for_each loops for batch scraping
- `/nika-security` — L-SEC-004 (untrusted data in for_each amplification)
