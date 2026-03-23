---
name: nika-dag
description: >-
  DAG design expert for Nika YAML workflows (.nika.yaml). Optimize task
  dependencies with depends_on, parallelize with for_each, design fan-out/fan-in
  patterns, diamond DAGs, binding chains, and data flow optimization. Use when
  designing complex multi-task workflow DAGs in Nika (schema nika/workflow@0.12).
---

# Nika DAG Design Expert

Design optimal task dependency graphs in `.nika.yaml` workflows.

## Core Rules

1. Tasks without `depends_on:` run in parallel
2. `depends_on:` creates explicit ordering
3. `with:` bindings require `depends_on:` on the source task
4. `for_each:` expands a single task into parallel iterations
5. DAG must be acyclic (no circular dependencies)

## Patterns

### 1. Sequential Chain

Tasks execute one after another:

```yaml
tasks:
  - id: step1
    exec: "echo start"
  - id: step2
    depends_on: [step1]
    with: { data: $step1 }
    exec: "echo {{with.data}}"
  - id: step3
    depends_on: [step2]
    with: { data: $step2 }
    exec: "echo {{with.data}}"
```

### 2. Fan-Out (1 to N)

One task feeds N parallel workers:

```yaml
tasks:
  - id: source
    exec: "echo 'shared data'"

  - id: worker_a
    depends_on: [source]
    with: { d: $source }
    exec: "echo 'A: {{with.d}}'"

  - id: worker_b
    depends_on: [source]
    with: { d: $source }
    exec: "echo 'B: {{with.d}}'"

  - id: worker_c
    depends_on: [source]
    with: { d: $source }
    exec: "echo 'C: {{with.d}}'"
```

### 3. Fan-In (N to 1)

Multiple tasks merge into a single collector:

```yaml
  - id: merge
    depends_on: [worker_a, worker_b, worker_c]
    with:
      a: $worker_a
      b: $worker_b
      c: $worker_c
    exec: "echo '{{with.a}} + {{with.b}} + {{with.c}}'"
```

### 4. Diamond (Fan-Out + Fan-In)

```yaml
tasks:
  - id: source
    exec: "echo 'DATA'"

  - id: left
    depends_on: [source]
    with: { data: $source }
    exec: "echo 'LEFT({{with.data}})'"

  - id: right
    depends_on: [source]
    with: { data: $source }
    exec: "echo 'RIGHT({{with.data}})'"

  - id: merge
    depends_on: [left, right]
    with:
      l: $left
      r: $right
    exec: "echo '{{with.l}} + {{with.r}}'"
```

DAG shape: `source -> left + right -> merge`

### 5. for_each Fan-Out

Dynamic parallelism from data:

```yaml
tasks:
  - id: items
    exec: 'echo ''["a","b","c","d"]'''

  - id: process
    depends_on: [items]
    for_each: "$items"
    as: item
    exec: "echo 'Processing {{with.item}}'"
```

### 6. for_each + Fan-In

Process in parallel, then merge results:

```yaml
tasks:
  - id: generate
    for_each: ["en", "fr", "de"]
    as: lang
    infer: "Translate 'hello' to {{with.lang}}"
    model: gpt-4.1-mini

  - id: compile
    depends_on: [generate]
    with:
      translations: $generate     # Gets array of all for_each results
    exec: "echo '{{with.translations | to_json}}'"
```

### 7. Multi-Stage Pipeline

```yaml
tasks:
  # Stage 1: Parallel data gathering
  - id: fetch_api
    fetch: { url: "https://api.example.com/data" }
  - id: fetch_db
    exec: "echo 'db query result'"

  # Stage 2: Process (waits for both)
  - id: combine
    depends_on: [fetch_api, fetch_db]
    with:
      api: $fetch_api
      db: $fetch_db
    infer: "Combine: {{with.api}} and {{with.db}}"

  # Stage 3: Fan-out distribution
  - id: distribute
    depends_on: [combine]
    for_each: ["slack", "email", "dashboard"]
    as: channel
    with:
      report: $combine
    exec: "echo 'Send to {{with.channel}}: {{with.report}}'"
```

### 8. Conditional-Like Pattern

Nika has no `if:` verb, but you can simulate branching:

```yaml
tasks:
  - id: classify
    infer: "Is this text positive or negative? Text: {{inputs.text}}"
    structured:
      schema:
        type: object
        properties:
          sentiment: { type: string }
        required: [sentiment]

  - id: respond
    depends_on: [classify]
    with:
      result: $classify
    infer: |
      Based on sentiment "{{with.result.sentiment}}", write an appropriate response.
      If positive, be enthusiastic. If negative, be empathetic.
```

## Optimization Strategies

### Maximize Parallelism

Tasks without mutual dependencies run in parallel automatically. Do NOT add unnecessary `depends_on:`:

```yaml
# BAD: Sequential when it could be parallel
- id: a
  exec: "echo a"
- id: b
  depends_on: [a]     # Unnecessary! b doesn't use a's output
  exec: "echo b"

# GOOD: Parallel execution
- id: a
  exec: "echo a"
- id: b
  exec: "echo b"       # Runs in parallel with a
```

### Minimize Critical Path

Put expensive tasks (LLM calls) in parallel branches:

```yaml
# GOOD: Three LLM calls in parallel
- id: analyze_tone
  infer: "Analyze tone: {{with.text}}"
- id: analyze_topics
  infer: "Extract topics: {{with.text}}"
- id: analyze_entities
  infer: "Extract entities: {{with.text}}"
- id: merge
  depends_on: [analyze_tone, analyze_topics, analyze_entities]
  with:
    tone: $analyze_tone
    topics: $analyze_topics
    entities: $analyze_entities
  infer: "Synthesize: {{with.tone}}, {{with.topics}}, {{with.entities}}"
```

### Use for_each Instead of Manual Fan-Out

```yaml
# BAD: Manual parallel tasks
- id: translate_en
  infer: "Translate to English: {{with.text}}"
- id: translate_fr
  infer: "Translate to French: {{with.text}}"
- id: translate_de
  infer: "Translate to German: {{with.text}}"

# GOOD: Dynamic with for_each
- id: translate
  for_each: ["English", "French", "German"]
  as: lang
  infer: "Translate to {{with.lang}}: {{with.text}}"
```

## Error Codes

| Code | Error | Fix |
|------|-------|-----|
| NIKA-020 | Cycle detected in DAG | Break the circular dependency |
| NIKA-021 | Missing dependency | Fix task id in `depends_on:` — unknown task |
| NIKA-022 | Duplicate task ID | Rename one of the duplicate ids |
| NIKA-071 | Unknown alias in with: block | Use `$` prefix in `with:` values |
| NIKA-080 | Unknown task in with: reference | Referenced task does not exist |

## Common Mistakes

| Mistake | Correct |
|---------|---------|
| `depends_on:` without `with:` when data is needed | Add `with:` to access upstream output |
| `with:` without `depends_on:` | Always pair them |
| Adding unnecessary `depends_on:` | Only add when data or ordering is needed |
| Circular deps (A->B->A) | Restructure as A->B or add intermediate task |
| `for_each:` on non-array data | Source must be JSON array |

## Validation

```bash
nika check workflow.nika.yaml    # Detects cycles, missing refs, bad bindings
```
