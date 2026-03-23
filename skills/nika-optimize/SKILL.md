---
name: nika-optimize
description: >-
  Optimize Nika YAML workflow performance (.nika.yaml). Identify parallelization
  opportunities, reduce LLM token usage, add caching, tune max_tokens and
  temperature, minimize API calls, optimize DAG structure, and improve cost
  efficiency. Use when a Nika workflow is slow, expensive, or needs performance
  tuning (schema nika/workflow@0.12).
---

# Optimize Nika Workflows

Improve speed, cost, and reliability of `.nika.yaml` workflows.

## Optimization Categories

1. **Parallelism**: Run independent tasks simultaneously
2. **Token efficiency**: Reduce LLM input/output tokens
3. **Provider selection**: Choose the right model for each task
4. **DAG structure**: Minimize critical path length
5. **Retry/resilience**: Handle failures gracefully
6. **Caching**: Avoid redundant work

## 1. Maximize Parallelism

### Remove Unnecessary Dependencies

```yaml
# SLOW: Sequential (3x time)
- id: a
  exec: "echo a"
- id: b
  depends_on: [a]        # Does b actually need a's output?
  exec: "echo b"
- id: c
  depends_on: [b]
  exec: "echo c"

# FAST: Parallel (1x time)
- id: a
  exec: "echo a"
- id: b                   # No depends_on = parallel
  exec: "echo b"
- id: c
  exec: "echo c"
```

Rule: Only add `depends_on:` when the task genuinely needs the upstream output.

### Use for_each for Batch Operations

```yaml
# SLOW: 3 sequential tasks
- id: translate_en
  infer: "Translate to English"
- id: translate_fr
  depends_on: [translate_en]
  infer: "Translate to French"
- id: translate_de
  depends_on: [translate_fr]
  infer: "Translate to German"

# FAST: 3 parallel iterations
- id: translate
  for_each: ["English", "French", "German"]
  as: lang
  infer: "Translate to {{with.lang}}: {{with.text}}"
```

### Fan-Out Expensive Operations

```yaml
# FAST: 3 LLM calls in parallel
- id: tone
  infer: "Analyze tone: {{with.text}}"
- id: topics
  infer: "Extract topics: {{with.text}}"
- id: entities
  infer: "Extract entities: {{with.text}}"
# All three run simultaneously, then merge
- id: merge
  depends_on: [tone, topics, entities]
  with:
    t: $tone
    k: $topics
    e: $entities
  infer: "Synthesize: {{with.t}}, {{with.k}}, {{with.e}}"
```

## 2. Reduce Token Usage

### Set max_tokens

Always set `max_tokens:` to prevent runaway generation:

```yaml
- id: summary
  infer: "Summarize in 2 sentences"
  max_tokens: 100                    # Hard limit
```

### Use Concise System Prompts

```yaml
# EXPENSIVE: Verbose system prompt (500 tokens)
system: |
  You are an expert data analyst with years of experience in...
  [long instruction]

# CHEAPER: Concise system prompt (30 tokens)
system: "You are a data analyst. Be concise. Output JSON only."
```

### Minimize Input Data

```yaml
# EXPENSIVE: Pass entire document
with:
  data: $fetch_full_page
infer: "Summarize: {{with.data}}"         # Could be 50K tokens

# CHEAPER: Extract first, then summarize
- id: extract
  fetch:
    url: "https://example.com"
    extract: article                       # Readability extracts main content
- id: summarize
  depends_on: [extract]
  with: { text: $extract }
  infer: "Summarize: {{with.text}}"        # Much less input
```

### Use Structured Output Instead of Parsing

```yaml
# EXPENSIVE: Free-form then parse
- id: extract
  infer: "List the names and ages"
- id: parse
  depends_on: [extract]
  exec: "echo '{{with.data}}' | parse_somehow"

# CHEAPER: One step with structured output
- id: extract
  infer: "Extract names and ages"
  structured:
    schema:
      type: object
      properties:
        people:
          type: array
          items:
            type: object
            properties:
              name: { type: string }
              age: { type: number }
            required: [name, age]
      required: [people]
  temperature: 0.0
```

## 3. Provider Selection

### Choose the Right Model for the Task

| Task Type | Recommended | Why |
|-----------|------------|-----|
| Simple extraction | gpt-4.1-mini | Fast, cheap |
| Classification | gpt-4.1-mini or groq | Simple task |
| Complex reasoning | claude-sonnet-4-20250514 or gpt-4.1 | Quality |
| Creative writing | claude-sonnet-4-20250514 | Best prose |
| Translation | mistral-large | Multilingual strength |
| Speed-critical | groq (llama-3.3-70b) | Fastest inference |
| Cost-sensitive | deepseek-chat | Cheapest quality |
| Long context | gemini-2.5-flash | 1M token window |
| Privacy/offline | native (local GGUF) | No API calls |

### Mix Providers in One Workflow

```yaml
schema: nika/workflow@0.12
workflow: optimized-pipeline

tasks:
  - id: classify
    infer: "Classify this text"
    provider: groq                   # Fast for simple task
    model: llama-3.3-70b-versatile
    max_tokens: 50

  - id: analyze
    depends_on: [classify]
    infer: "Deep analysis of: {{with.data}}"
    provider: claude                 # Quality for complex task
    model: claude-sonnet-4-20250514
    max_tokens: 1000
```

## 4. DAG Structure Optimization

### Minimize Critical Path

The workflow finishes when the longest chain completes. Shorten it:

```yaml
# SLOW: All sequential (critical path = 4 tasks)
A -> B -> C -> D

# FAST: Parallel where possible (critical path = 2 tasks)
A -> C
B -> C
     C -> D
# A and B run in parallel, then C, then D
```

### Avoid Unnecessary Merges

```yaml
# BAD: Merge before fan-out again
A -> merge -> B1, B2, B3

# GOOD: Direct fan-out
A -> B1
A -> B2
A -> B3
```

## 5. Retry and Resilience

```yaml
- id: flaky_api
  fetch:
    url: "https://unreliable-api.com"
  timeout: 30
  retry:
    max_attempts: 3
    delay: 2                         # Seconds between retries
```

### Structured Output Retry

```yaml
- id: extract
  infer: "Extract data"
  structured:
    schema:
      type: object
      properties:
        name: { type: string }
      required: [name]
  retry:
    max_attempts: 3                  # Retry on schema mismatch
  temperature: 0.0                   # Low temp for consistency
```

## 6. Use fetch extract Modes

Instead of fetching raw HTML and processing with LLM:

```yaml
# EXPENSIVE: LLM processes raw HTML
- id: fetch
  fetch: { url: "https://example.com" }
- id: process
  infer: "Extract article from HTML: {{with.html}}"  # 50K tokens of HTML!

# CHEAP: Extract before LLM
- id: fetch
  fetch:
    url: "https://example.com"
    extract: article                  # Readability: ~2K tokens
- id: process
  infer: "Summarize: {{with.text}}"   # Much cheaper
```

## Optimization Checklist

- [ ] All independent tasks run in parallel (no unnecessary `depends_on:`)
- [ ] `max_tokens:` set on every `infer:` task
- [ ] Cheapest viable model selected per task
- [ ] `fetch:` uses `extract:` modes to reduce LLM input
- [ ] `for_each:` used instead of manual fan-out
- [ ] `retry:` on network/API calls
- [ ] `timeout:` set on external operations
- [ ] `temperature: 0.0` for deterministic tasks
- [ ] `structured:` used instead of free-form + parse
- [ ] Critical path is minimized

## Common Mistakes

| Mistake | Fix |
|---------|-----|
| Using gpt-4.1 for simple classification | Use gpt-4.1-mini or groq |
| Sequential tasks that could be parallel | Remove unnecessary `depends_on:` |
| No `max_tokens:` | Always set to control cost |
| Passing raw HTML to LLM | Use `extract: article` or `extract: markdown` |
| Single provider for everything | Mix providers per task complexity |

## Validation

```bash
nika check workflow.nika.yaml    # Validate optimized workflow
nika run workflow.nika.yaml      # Benchmark execution time
```
