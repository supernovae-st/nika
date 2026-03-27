# Advanced Workflow Patterns

Deep-dive into Nika's most powerful features: DAG patterns, conditional execution, resilience strategies, template composition, fan-out/fan-in, diamond DAGs, and error recovery.

---

## Pattern 1: Diamond DAG

**Problem:** You have a setup task that fans out to multiple parallel branches, which then reconverge at a synthesis step. This is the "diamond" pattern.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: diamond-dag
description: "Diamond DAG: setup -> parallel branches -> synthesis"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/diamond-dag

tasks:
  # Top of the diamond: single setup task
  - id: setup
    exec: "echo '{\"project\": \"demo\", \"timestamp\": \"2026-03-23\"}'"

  # Left branch: web scraping
  - id: branch_web
    depends_on: [setup]
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: markdown
      timeout: 20

  # Center branch: API data
  - id: branch_api
    depends_on: [setup]
    fetch:
      url: "https://jsonplaceholder.typicode.com/posts?_limit=5"
      timeout: 15

  # Right branch: metadata extraction
  - id: branch_meta
    depends_on: [setup]
    fetch:
      url: "https://github.com"
      extract: metadata
      timeout: 20

  # Bottom of the diamond: synthesis (waits for ALL branches)
  - id: synthesis
    depends_on: [branch_web, branch_api, branch_meta]
    with:
      web: $branch_web
      api: $branch_api
      meta: $branch_meta
    infer:
      prompt: |
        Synthesize data from three parallel sources:
        Web content: {{with.web | first(1500)}}
        API data: {{with.api | first(1000)}}
        Metadata: {{with.meta | first(500)}}

        Create a unified summary.
      temperature: 0.3
      max_tokens: 1500
    artifact:
      path: synthesis.md
```

**Explanation:**

The DAG shape:

```
         setup
        /  |  \
  branch  branch  branch
   web     api     meta
        \  |  /
       synthesis
```

Nika's DAG executor automatically parallelizes the three branches since they only depend on `setup`. The `synthesis` task waits for all three to complete via `depends_on: [branch_web, branch_api, branch_meta]`. Each branch's output is available through `with:` bindings.

---

## Pattern 2: Fan-Out / Fan-In

**Problem:** You need to process a dynamic list of items in parallel, then aggregate all results into a single output.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: fan-out-fan-in
description: "Process items in parallel, aggregate results"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/fan-pattern

tasks:
  # Generate the list to process
  - id: discover_items
    exec:
      command: |
        echo '[
          {"id": 1, "url": "https://httpbin.org/get", "name": "API 1"},
          {"id": 2, "url": "https://httpbin.org/ip", "name": "API 2"},
          {"id": 3, "url": "https://httpbin.org/uuid", "name": "API 3"},
          {"id": 4, "url": "https://httpbin.org/headers", "name": "API 4"},
          {"id": 5, "url": "https://httpbin.org/user-agent", "name": "API 5"}
        ]'
      shell: true

  # Fan-out: process each item in parallel
  - id: process_items
    depends_on: [discover_items]
    for_each: "$discover_items"
    as: item
    concurrency: 5
    fail_fast: false
    fetch:
      url: "{{with.item.url}}"
      response: full
      timeout: 15
    retry:
      max_attempts: 2
      delay_ms: 1000

  # Fan-in: aggregate all results
  - id: aggregate
    depends_on: [process_items]
    with:
      results: $process_items
    infer:
      prompt: |
        Aggregate these parallel processing results:
        {{with.results | first(4000)}}

        Provide:
        1. Success rate (passed/total)
        2. Average response time
        3. Failures and reasons
        4. Overall assessment
      response_format: json
      temperature: 0.1
      max_tokens: 1000
    structured:
      schema:
        type: object
        properties:
          total_processed:
            type: integer
          success_count:
            type: integer
          failure_count:
            type: integer
          summary:
            type: string
        required: [total_processed, success_count, summary]
    artifact:
      path: aggregation-report.json
      format: json
```

**Explanation:**

The `for_each: "$discover_items"` syntax uses a `$` reference to dynamically iterate over the output of a previous task. This is the key to dynamic fan-out -- the list is not hardcoded but comes from a preceding task's output. The `$` prefix tells Nika to look up the task result by ID and parse it as a JSON array for iteration.

The `fail_fast: false` is critical for fan-out patterns: it ensures all items are processed even if some fail, and the aggregate task receives partial results.

---

## Pattern 3: Retry with Exponential Backoff

**Problem:** You need to call unreliable external APIs with automatic retry and increasing delays between attempts.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: resilient-api-calls
description: "Retry patterns with exponential backoff"

tasks:
  # Basic retry
  - id: basic_retry
    fetch:
      url: "https://httpbin.org/status/200"
      timeout: 10
    retry:
      max_attempts: 3
      delay_ms: 1000

  # Exponential backoff: 1s, 2s, 4s between retries
  - id: exponential_backoff
    depends_on: [basic_retry]
    fetch:
      url: "https://httpbin.org/get"
      timeout: 15
    retry:
      max_attempts: 5
      delay_ms: 1000
      backoff: 2.0

  # Aggressive retry for critical endpoints
  - id: critical_endpoint
    depends_on: [basic_retry]
    fetch:
      url: "https://httpbin.org/headers"
      timeout: 30
    retry:
      max_attempts: 10
      delay_ms: 500
      backoff: 1.5

  # Batch processing with per-item retry
  - id: batch_with_retry
    for_each:
      - "https://httpbin.org/status/200"
      - "https://httpbin.org/status/201"
      - "https://httpbin.org/get"
    as: url
    concurrency: 3
    fail_fast: false
    fetch:
      url: "{{with.url}}"
      response: full
      timeout: 10
    retry:
      max_attempts: 2
      delay_ms: 2000
      backoff: 2.0

  - id: report
    depends_on: [exponential_backoff, critical_endpoint, batch_with_retry]
    exec: "echo 'All resilient calls completed successfully'"
```

**Explanation:**

The `retry:` block has three parameters:
- `max_attempts`: Total number of tries (including the first attempt)
- `delay_ms`: Initial delay before the first retry (in milliseconds)
- `backoff`: Multiplier applied to the delay after each retry

With `delay_ms: 1000` and `backoff: 2.0`, the delays are: 1s, 2s, 4s, 8s, 16s between retries.

When `retry:` is combined with `for_each:`, each item in the iteration gets its own retry budget independently.

---

## Pattern 4: Timeout Control

**Problem:** You need to prevent slow tasks from blocking the entire workflow, with different timeout windows for different task types.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: timeout-control
description: "Timeout strategies for different task types"

tasks:
  # Fast command: tight timeout
  - id: quick_check
    exec:
      command: "echo 'System check: OK'"
      timeout: 5

  # Network call: moderate timeout
  - id: api_call
    depends_on: [quick_check]
    fetch:
      url: "https://httpbin.org/delay/1"
      timeout: 10

  # Slow processing: generous timeout
  - id: heavy_processing
    depends_on: [quick_check]
    exec:
      command: "echo 'Processing complete after heavy computation'"
      timeout: 120

  # External API with retry + timeout combo
  - id: external_api
    depends_on: [quick_check]
    fetch:
      url: "https://httpbin.org/get"
      timeout: 15
    retry:
      max_attempts: 3
      delay_ms: 2000
      backoff: 2.0

  # Agent with resource limits (includes timeout)
  - id: agent_with_limits
    depends_on: [api_call]
    with:
      data: $api_call
    agent:
      system: "Analyze the data briefly."
      prompt: "Data: {{with.data | first(500)}}. Summarize."
      tools: [builtin]
      max_turns: 4
      max_tokens: 500
      token_budget: 5000
      completion:
        mode: explicit
      limits:
        max_duration_secs: 60

  - id: done
    depends_on: [heavy_processing, external_api, agent_with_limits]
    exec: "echo 'All tasks completed within their timeout windows'"
```

**Explanation:**

Timeout is specified in **seconds** (the parser converts to milliseconds internally). Different task types need different timeout strategies:

| Task Type | Recommended Timeout |
|-----------|-------------------|
| `exec:` (local commands) | 5-30s |
| `fetch:` (HTTP requests) | 10-30s |
| `fetch:` (slow APIs) | 30-120s |
| `infer:` (LLM generation) | Provider-dependent |
| `agent:` (multi-turn) | Use `limits.max_duration_secs` |

For agents, use `limits.max_duration_secs` instead of task-level timeout. This gives the agent a wall-clock budget for its entire multi-turn loop.

---

## Pattern 5: Parallel Execution with Concurrency Control

**Problem:** You need to process many items but limit the number of simultaneous operations to avoid overwhelming external services.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: concurrency-control
description: "Process items with controlled parallelism"
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  # Low concurrency: don't overwhelm the API
  - id: careful_fetch
    for_each:
      - "https://httpbin.org/get?q=1"
      - "https://httpbin.org/get?q=2"
      - "https://httpbin.org/get?q=3"
      - "https://httpbin.org/get?q=4"
      - "https://httpbin.org/get?q=5"
      - "https://httpbin.org/get?q=6"
      - "https://httpbin.org/get?q=7"
      - "https://httpbin.org/get?q=8"
      - "https://httpbin.org/get?q=9"
      - "https://httpbin.org/get?q=10"
    as: url
    concurrency: 2
    fetch:
      url: "{{with.url}}"
      timeout: 15

  # High concurrency: internal service can handle it
  - id: fast_parallel
    for_each:
      - "item-1"
      - "item-2"
      - "item-3"
      - "item-4"
      - "item-5"
    as: item
    concurrency: 5
    exec: "echo 'Processing {{with.item}}'"

  # LLM calls: moderate concurrency to respect rate limits
  - id: llm_batch
    depends_on: [careful_fetch]
    with:
      data: $careful_fetch
    for_each:
      - { topic: "performance", angle: "metrics" }
      - { topic: "security", angle: "vulnerabilities" }
      - { topic: "reliability", angle: "uptime" }
    as: brief
    concurrency: 3
    infer:
      prompt: |
        Analyze {{with.brief.topic}} from the {{with.brief.angle}} perspective.
        Data context: {{with.data | first(1000)}}
      temperature: 0.3
      max_tokens: 500
```

**Explanation:**

The `concurrency:` parameter controls how many items from a `for_each:` block are processed simultaneously:

| `concurrency` | Effect |
|---------------|--------|
| 1 | Sequential (one at a time) |
| 2-3 | Conservative (rate-limited APIs) |
| 5 | Moderate (most use cases) |
| 10+ | Aggressive (internal services) |

Without `concurrency:`, all items run in parallel (limited only by system resources). Always set `concurrency:` when calling external APIs to avoid rate limiting.

---

## Pattern 6: for_each with Structured Objects

**Problem:** You need to iterate over complex objects, not just simple strings, passing multiple properties to each iteration.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: structured-foreach
description: "Iterate over complex objects with for_each"
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  # Inline structured objects
  - id: analyze_competitors
    for_each:
      - name: "Zapier"
        url: "https://zapier.com"
        category: "no-code"
        pricing: "freemium"
      - name: "n8n"
        url: "https://n8n.io"
        category: "open-source"
        pricing: "self-hosted free, cloud paid"
      - name: "Make"
        url: "https://www.make.com/en"
        category: "visual builder"
        pricing: "freemium"
    as: competitor
    concurrency: 3
    infer:
      prompt: |
        Quick analysis of {{with.competitor.name}}:
        - Category: {{with.competitor.category}}
        - Pricing: {{with.competitor.pricing}}
        - URL: {{with.competitor.url}}

        Summarize their positioning in 100 words.
      temperature: 0.5
      max_tokens: 300

  # Reference array from previous task
  - id: data_source
    exec: |
      echo '[{"city": "Paris", "country": "FR"}, {"city": "Berlin", "country": "DE"}, {"city": "Tokyo", "country": "JP"}]'

  - id: process_dynamic
    depends_on: [data_source]
    for_each: "$data_source"
    as: location
    concurrency: 3
    exec: "echo 'Processing {{with.location.city}}, {{with.location.country}}'"
```

**Explanation:**

`for_each:` accepts three forms:
1. **Array of strings**: `["a", "b", "c"]`
2. **Array of objects**: `[{ name: "x", url: "y" }, ...]`
3. **Dynamic reference**: `"$task_id"` to use a previous task's JSON array output

Object properties are accessed with dot notation: `{{with.competitor.name}}`, `{{with.competitor.category}}`, etc. The `as:` field names the iteration variable used in `{{with.<as>}}` templates.

---

## Pattern 7: Conditional Branching with Depends

**Problem:** You need different processing paths based on the output of an earlier task, without native if/else syntax.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: conditional-branching
description: "Simulate conditional logic with parallel branches and structured output"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/conditional

tasks:
  # Determine the path
  - id: classify
    exec: |
      echo '{"category": "technical", "complexity": "high", "language": "en"}'

  # Technical analysis path
  - id: technical_path
    depends_on: [classify]
    with:
      classification: $classify
    infer:
      prompt: |
        Classification: {{with.classification}}

        If the category is "technical", write a deep technical analysis.
        If not, write "N/A - not a technical item".
      temperature: 0.3
      max_tokens: 1000

  - id: editorial_path
    depends_on: [classify]
    with:
      classification: $classify
    infer:
      prompt: |
        Classification: {{with.classification}}

        If the category is "editorial", write an editorial review.
        If not, write "N/A - not an editorial item".
      temperature: 0.3
      max_tokens: 1000

  # Merge: select the relevant output
  - id: merge
    depends_on: [technical_path, editorial_path]
    with:
      technical: $technical_path
      editorial: $editorial_path
      classification: $classify
    infer:
      prompt: |
        Select the relevant output based on classification: {{with.classification}}

        Technical output: {{with.technical | first(500)}}
        Editorial output: {{with.editorial | first(500)}}

        Return only the relevant output, formatted as the final result.
      temperature: 0.1
      max_tokens: 1500
    artifact:
      path: result.md
```

**Explanation:**

Nika does not have native if/else branching. Instead, you can:
1. Run all branches in parallel (they are cheap if the LLM returns "N/A" quickly)
2. Use a merge task to select the relevant output
3. The LLM acts as the conditional router

This pattern works because Nika's DAG executor runs independent branches in parallel, and the merge task has access to all outputs through `with:` bindings.

---

## Pattern 8: Template Composition with Context

**Problem:** You want to share common configuration and prompts across multiple tasks without repetition.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: template-composition
description: "Share configuration via context files and inputs"
provider: anthropic
model: claude-sonnet-4-20250514

context:
  files:
    brand: ./context/brand-voice.md
    glossary: ./context/technical-terms.md
    persona: ./context/target-persona.json

inputs:
  output_format: "markdown"
  target_audience: "senior developers"
  max_length: 1000

tasks:
  # All tasks share the same context and inputs
  - id: intro
    infer:
      system: |
        Brand voice: {{context.files.brand | first(500)}}
        Glossary: {{context.files.glossary | first(500)}}
        Target: {{context.files.persona | first(300)}}
      prompt: |
        Write an introduction for {{inputs.target_audience}} in {{inputs.output_format}}.
        Max length: {{inputs.max_length}} words.
      temperature: 0.5
      max_tokens: 1500

  - id: body
    depends_on: [intro]
    with:
      introduction: $intro
    infer:
      system: |
        Brand voice: {{context.files.brand | first(500)}}
        Glossary: {{context.files.glossary | first(500)}}
      prompt: |
        Continue from this introduction:
        {{with.introduction | first(500)}}

        Write the main body for {{inputs.target_audience}}.
        Format: {{inputs.output_format}}.
        Max length: {{inputs.max_length}} words.
      temperature: 0.5
      max_tokens: 2000

  - id: conclusion
    depends_on: [body]
    with:
      main_content: $body
    infer:
      system: "Brand voice: {{context.files.brand | first(500)}}"
      prompt: |
        Write a conclusion for:
        {{with.main_content | first(1000)}}

        Target: {{inputs.target_audience}}.
        Format: {{inputs.output_format}}.
      temperature: 0.4
      max_tokens: 500
```

**Explanation:**

The three template namespaces enable clean composition:
- `context.files.*` loads files once at workflow start, accessible in every task
- `inputs.*` provides per-run configuration that can be overridden: `nika run --set target_audience=juniors`
- `with.*` passes data between tasks

This pattern avoids duplicating long system prompts or configuration values across tasks.

---

## Pattern 9: Multi-Format Output

**Problem:** You need to produce the same content in multiple formats (JSON, Markdown, plain text) and save each as a separate artifact.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: multi-format-output
description: "Produce content in JSON, Markdown, and plain text formats"
provider: anthropic
model: claude-sonnet-4-20250514

artifacts:
  dir: ./output/multi-format

tasks:
  # Generate the core content
  - id: generate
    infer:
      prompt: "Explain the benefits of workflow automation in 200 words."
      temperature: 0.5
      max_tokens: 500

  # Output as JSON
  - id: json_output
    depends_on: [generate]
    with:
      content: $generate
    infer:
      prompt: |
        Convert this to JSON: {{with.content}}
        Schema: { title, summary, benefits: string[], word_count }
      response_format: json
      temperature: 0.1
      max_tokens: 500
    structured:
      schema:
        type: object
        properties:
          title:
            type: string
          summary:
            type: string
          benefits:
            type: array
            items:
              type: string
          word_count:
            type: integer
        required: [title, summary, benefits]
    artifact:
      path: output.json
      format: json

  # Output as Markdown (already generated)
  - id: markdown_output
    depends_on: [generate]
    with:
      content: $generate
    exec: "echo '{{with.content}}'"
    artifact:
      path: output.md
      format: text

  # Output as plain text
  - id: plaintext_output
    depends_on: [generate]
    with:
      content: $generate
    infer:
      prompt: |
        Convert to plain text (no markdown, no formatting):
        {{with.content}}
      temperature: 0.1
      max_tokens: 500
    artifact:
      path: output.txt
      format: text
```

**Explanation:**

The three output tasks all depend on the same source task and run in parallel. Each produces a different format:
- `format: json` saves structured JSON
- `format: text` saves plain text or markdown
- `format: binary` saves binary data (images, PDFs)

---

## Pattern 10: Full Orchestration (All 5 Verbs)

**Problem:** You want a single workflow that demonstrates all five Nika verbs working together in a realistic scenario.

**Solution:**

```yaml
schema: "nika/workflow@0.12"
workflow: full-orchestration
description: "All 5 verbs: exec, fetch, infer, invoke, agent — in one workflow"
provider: anthropic
model: claude-sonnet-4-20250514

mcp:
  filesystem:
    command: "npx"
    args: ["-y", "@anthropic/mcp-filesystem"]

inputs:
  project_name: "Full Orchestration Demo"

artifacts:
  dir: ./output/orchestration

tasks:
  # VERB 1: exec — gather system data
  - id: system_data
    exec:
      command: "echo '{\"platform\": \"'$(uname -s)'\", \"arch\": \"'$(uname -m)'\"}'"
      shell: true

  # VERB 2: fetch — scrape web content
  - id: web_content
    depends_on: [system_data]
    fetch:
      url: "https://blog.rust-lang.org/"
      extract: markdown
      timeout: 20

  # VERB 2b: fetch — download binary
  - id: download_image
    depends_on: [system_data]
    fetch:
      url: "https://httpbin.org/image/png"
      response: binary
      timeout: 15

  # VERB 4: invoke — process media
  - id: process_media
    depends_on: [download_image]
    with:
      img: $download_image
    invoke:
      tool: "nika:pipeline"
      params:
        hash: "{{with.img.media[0].hash}}"
        steps:
          - op: thumbnail
            width: 400
          - op: convert
            format: webp
    artifact:
      path: processed.webp
      format: binary

  # VERB 4b: invoke — generate chart
  - id: generate_chart
    depends_on: [system_data]
    invoke:
      tool: "nika:chart"
      params:
        type: "bar"
        title: "{{inputs.project_name}} — Metrics"
        width: 800
        height: 500
        series:
          - name: "Score"
            data: [85, 72, 91, 68]
        labels: ["Quality", "Speed", "Coverage", "Docs"]
    artifact:
      path: chart.png
      format: binary

  # VERB 3: infer — analyze with vision
  - id: analyze
    depends_on: [web_content, process_media, generate_chart]
    with:
      content: $web_content
      image: $process_media
      chart: $generate_chart
    infer:
      content:
        - type: image
          source: "{{with.chart.media[0].hash}}"
          detail: high
        - type: text
          text: |
            Analyze this chart alongside the web content:
            {{with.content | first(2000)}}

            Provide a 300-word analysis.
      temperature: 0.3
      max_tokens: 1000
    artifact:
      path: analysis.md

  # VERB 5: agent — deep investigation
  - id: deep_dive
    depends_on: [analyze]
    with:
      analysis: $analyze
    agent:
      system: |
        You are a senior analyst. Use file tools and MCP filesystem
        to investigate further and produce a final report.
      prompt: |
        Initial analysis: {{with.analysis | first(1500)}}

        Investigate deeper. Check for related files.
        Call nika_complete with the final report.
      mcp: [filesystem]
      tools: [builtin]
      max_turns: 6
      max_tokens: 2000
      token_budget: 15000
      completion:
        mode: explicit
      guardrails:
        - type: length
          min_words: 200
          on_failure: retry
      limits:
        max_turns: 6
        max_tokens: 30000
        max_cost_usd: 1.00
        max_duration_secs: 120
    artifact:
      path: final-report.md

  # Completion log
  - id: done
    depends_on: [deep_dive]
    exec: "echo '{{inputs.project_name}} — all 5 verbs completed successfully'"
```

**Explanation:**

All five verbs in action:

| Verb | Task | Purpose |
|------|------|---------|
| `exec:` | system_data | Run shell commands |
| `fetch:` | web_content, download_image | HTTP requests + extraction |
| `infer:` | analyze | LLM generation with vision |
| `invoke:` | process_media, generate_chart | Call builtin tools |
| `agent:` | deep_dive | Multi-turn autonomous loop |

The DAG has multiple levels of parallelism: `web_content`, `download_image`, and `generate_chart` all run concurrently after `system_data`. The diamond converges at `analyze`, then flows to the agent for final investigation.

---

## Summary: Resilience Checklist

| Feature | Syntax | Purpose |
|---------|--------|---------|
| Retry | `retry: { max_attempts: 3, delay_ms: 1000, backoff: 2.0 }` | Handle transient failures |
| Timeout | `timeout: 30` (seconds) | Prevent hanging tasks |
| Fail-fast control | `fail_fast: false` | Continue batch on errors |
| Concurrency | `concurrency: 5` | Limit parallel execution |
| Agent limits | `limits: { max_cost_usd: 1.0 }` | Cap agent resource usage |
| Guardrails | `guardrails: [{ type: length }]` | Enforce output quality |
| Artifacts | `artifact: { mode: append }` | Persistent output storage |
| Context | `context: { files: { ... } }` | Shared configuration |
| Inputs | `inputs: { key: default }` | Runtime parameterization |
