# Workflow Patterns

This guide covers the essential patterns for building effective Nika workflows: task chaining, parallel execution, data transformation, error handling, iteration, and output management. Each pattern includes a complete working example.

## The DAG Model

Every Nika workflow is a Directed Acyclic Graph (DAG). Tasks are nodes, and dependencies are edges. This means:

- Tasks with no dependencies run in parallel automatically
- Tasks declare dependencies via `depends_on:` (ordering) or `with:` (data flow)
- Cycles are forbidden -- Nika detects and rejects them at validation time
- Failed tasks cascade: downstream tasks are skipped unless retry succeeds

## Pattern 1: Sequential Chain

The simplest pattern: tasks run one after another, each consuming the previous task's output.

```yaml
schema: nika/workflow@0.12
workflow: sequential-chain
provider: anthropic

tasks:
  - id: raw_data
    exec: "echo 'The quick brown fox jumps over the lazy dog'"

  - id: process
    depends_on: [raw_data]
    with:
      text: $raw_data | trim
    exec:
      command: "echo '{{with.text}}' | wc -w | tr -d ' '"
      shell: true

  - id: report
    depends_on: [process]
    with:
      count: $process | trim
    exec: "echo 'Word count: {{with.count}}'"
```

DAG: `raw_data → process → report`

**When to use:** Simple pipelines where each step depends on exactly one predecessor.

## Pattern 2: Diamond

Multiple tasks process the same data in parallel, then a merge task combines their results.

```yaml
schema: nika/workflow@0.12
workflow: diamond-pattern
provider: anthropic

tasks:
  - id: source
    exec: "echo 'Artificial intelligence is transforming industries worldwide.'"

  - id: sentiment
    depends_on: [source]
    with:
      text: $source | trim
    infer:
      prompt: "Rate the sentiment of this text from 1-10: {{with.text}}"
      temperature: 0.0

  - id: keywords
    depends_on: [source]
    with:
      text: $source | trim
    infer:
      prompt: "Extract 3 keywords from: {{with.text}}. Return as comma-separated."
      temperature: 0.0

  - id: language
    depends_on: [source]
    with:
      text: $source | trim
    infer:
      prompt: "What language is this text written in? One word answer."
      temperature: 0.0

  - id: merge
    depends_on: [sentiment, keywords, language]
    with:
      sent: $sentiment | trim
      kw: $keywords | trim
      lang: $language | trim
    exec:
      command: |
        echo "Analysis Results:"
        echo "  Sentiment: {{with.sent}}"
        echo "  Keywords:  {{with.kw}}"
        echo "  Language:  {{with.lang}}"
      shell: true
```

DAG:
```
          ┌── sentiment ──┐
source ───┼── keywords  ──┼── merge
          └── language  ──┘
```

**When to use:** Running multiple independent analyses on the same data, then combining results. This is the most common pattern for AI workflows.

## Pattern 3: Fan-Out with for_each

Process a dynamic list of items in parallel.

```yaml
schema: nika/workflow@0.12
workflow: fan-out
provider: anthropic

tasks:
  - id: get_urls
    exec: |
      echo '["https://example.com", "https://httpbin.org/get", "https://jsonplaceholder.typicode.com/posts/1"]'
    output:
      format: json

  - id: fetch_all
    depends_on: [get_urls]
    for_each: $get_urls
    as: url
    concurrency: 3
    fetch:
      url: "{{with.url}}"
      timeout: 10

  - id: summarize
    depends_on: [fetch_all]
    with:
      results: $fetch_all
    infer:
      prompt: "Summarize what was fetched from these URLs: {{with.results | to_json}}"
```

The `for_each:` configuration:

| Field | Purpose | Default |
|-------|---------|---------|
| `items` | Task reference producing an array (use for_each: $task_id, not items:) | Required |
| `as` | Variable name for current item | `item` |
| `concurrency` | Max parallel iterations | Unlimited |
| `fail_fast` | Stop all on first error | `true` |

Inside the task, reference the current item with `{{with.item}}` or `{{with.item.field}}` for objects.

**When to use:** Processing lists of URLs, files, database records, or any dynamic collection.

## Pattern 4: Data Flow with Bindings

### Basic Bindings

```yaml
with:
  text: $source_task                    # Full output of source_task
  name: $user_task.profile.name         # JSONPath nested access
  key: $env.API_KEY                     # Environment variable
```

### Pipe Transforms

Transform bound values inline:

```yaml
with:
  clean: $raw_data | trim                        # Remove whitespace
  upper: $text | trim | upper                    # Chain: trim then uppercase
  count: $items | length                         # Array/string length
  first3: $list | sort | unique | first(3)       # Sort, dedupe, take 3
  csv: $names | join(", ")                       # Array to string
  parts: $csv_line | split(",")                  # String to array
  safe: $maybe_null | default("N/A")             # Null safety
  typed: $value | type_of                        # Get type name
```

### Complete Transform Catalog

**String transforms:**

| Transform | Input | Output |
|-----------|-------|--------|
| `upper` | `"hello"` | `"HELLO"` |
| `lower` | `"HELLO"` | `"hello"` |
| `trim` | `" hi "` | `"hi"` |
| `trim_start` | `" hi "` | `"hi "` |
| `trim_end` | `" hi "` | `" hi"` |

**Collection transforms:**

| Transform | Input | Output |
|-----------|-------|--------|
| `length` | `[1,2,3]` | `3` |
| `first` | `[1,2,3]` | `1` |
| `last` | `[1,2,3]` | `3` |
| `first(2)` | `[1,2,3]` | `[1,2]` |
| `last(2)` | `[1,2,3]` | `[2,3]` |
| `keys` | `{"a":1}` | `["a"]` |
| `values` | `{"a":1}` | `[1]` |
| `flatten` | `[[1],[2]]` | `[1,2]` |
| `reverse` | `[1,2,3]` | `[3,2,1]` |
| `sort` | `[3,1,2]` | `[1,2,3]` |
| `unique` | `[1,1,2]` | `[1,2]` |
| `compact` | `[1,null,2]` | `[1,2]` |

**Type conversion transforms:**

| Transform | Effect |
|-----------|--------|
| `to_string` | Any value to its string representation |
| `to_number` | Parse string as number |
| `to_bool` | Parse string as boolean |
| `to_json` | Serialize value to JSON string |
| `parse_json` | Deserialize JSON string to value |

**Numeric transforms:**

| Transform | Effect |
|-----------|--------|
| `round(2)` | Round to N decimal places |
| `abs` | Absolute value |
| `ceil` | Ceiling (round up) |
| `floor` | Floor (round down) |

**Utility transforms:**

| Transform | Effect |
|-----------|--------|
| `default("val")` | Fallback value if input is null |
| `type_of` | Returns the JSON type name |
| `join(",")` | Join array elements with separator |
| `split(",")` | Split string into array |
| `shell` | Execute the value as a shell command |

### Fallback Operator

Use `??` to provide a default when a JSONPath is null:

```yaml
with:
  temp: $weather.data.temp ?? 20
  name: $user.profile.display_name ?? "Anonymous"
```

## Pattern 5: Parallel Execution

Tasks without dependencies on each other run in parallel automatically:

```yaml
schema: nika/workflow@0.12
workflow: parallel
provider: anthropic

tasks:
  # These three run simultaneously
  - id: fetch_news
    fetch:
      url: "https://api.example.com/news"
      extract: markdown

  - id: fetch_weather
    fetch:
      url: "https://api.example.com/weather"
      extract: jsonpath
      selector: "$.current"

  - id: get_date
    exec: "date '+%A, %B %d'"

  # This waits for all three
  - id: dashboard
    depends_on: [fetch_news, fetch_weather, get_date]
    with:
      news: $fetch_news | trim
      weather: $fetch_weather | trim
      date: $get_date | trim
    infer:
      prompt: |
        Create a morning briefing for {{with.date}}:
        News: {{with.news}}
        Weather: {{with.weather}}
```

Nika automatically detects which tasks can run in parallel based on the dependency graph. You never need to manually specify parallelism -- just declare your dependencies correctly.

## Pattern 6: Error Handling with Retries

### Retry Configuration

Add resilience to tasks that might fail (API calls, network requests):

```yaml
  - id: api_call
    fetch:
      url: "https://api.example.com/data"
      timeout: 10
    retry:
      max_attempts: 3       # Total attempts (1 initial + 2 retries)
      delay_ms: 1000        # Wait 1 second between retries
      backoff: 2.0          # Exponential: 1s, 2s, 4s
```

### fail_fast with for_each

Control whether one failed iteration stops all others:

```yaml
  - id: process_all
    for_each: $urls
    concurrency: 5
    fail_fast: false  # Continue processing even if some fail
    fetch:
      url: "{{with.item}}"
```

With `fail_fast: true` (default), the first failure cancels all remaining iterations. With `fail_fast: false`, all iterations run and failures are collected.

## Pattern 7: Structured Output

Enforce JSON Schema on LLM output for reliable structured data:

### Inline Schema

```yaml
  - id: extract_entities
    infer:
      prompt: "Extract all people and organizations from: {{with.text}}"
    structured:
      schema:
        type: object
        properties:
          people:
            type: array
            items: { type: string }
          organizations:
            type: array
            items: { type: string }
        required: [people, organizations]
      max_retries: 2
      enable_repair: true
```

### Schema File Reference

```yaml
  - id: classify
    infer:
      prompt: "Classify this document."
    structured: ./schemas/classification.json
```

The structured output engine uses multiple layers to achieve near-perfect compliance:
1. **Tool injection** -- Sends the schema as a tool parameter to the provider
2. **Extraction** -- Parses and validates the response
3. **Retry** -- Re-prompts with validation errors if output is malformed
4. **Repair** -- Uses an LLM to fix complex schema violations

## Pattern 8: Artifacts (File Output)

Save task results to files:

```yaml
schema: nika/workflow@0.12
workflow: artifact-example
provider: anthropic

artifacts:
  dir: ./output

tasks:
  - id: generate_report
    infer:
      prompt: "Write a market analysis report."
    artifact:
      path: report.md
      format: text

  - id: extract_data
    depends_on: [generate_report]
    with:
      report: $generate_report
    infer:
      prompt: "Extract key metrics from: {{with.report}}"
    structured:
      schema:
        type: object
        properties:
          metrics:
            type: array
            items:
              type: object
              properties:
                name: { type: string }
                value: { type: number }
              required: [name, value]
        required: [metrics]
    artifact:
      path: metrics.json
      format: json
```

Artifacts are written to the `artifacts.dir` directory (or `.nika/artifacts/` by default).

## Pattern 9: Context Files

Load external files as context for your workflow:

```yaml
schema: nika/workflow@0.12
workflow: with-context
provider: anthropic

context:
  files:
    readme: ./README.md
    guidelines: ./docs/style-guide.md

tasks:
  - id: review
    infer:
      prompt: |
        Review this README against our style guidelines.

        README:
        {{context.readme}}

        Style Guide:
        {{context.guidelines}}
```

Context files are loaded once at workflow boot and available in templates via `{{context.alias}}`.

## Pattern 10: Workflow Inputs

Accept parameters at runtime:

```yaml
schema: nika/workflow@0.12
workflow: parameterized
provider: anthropic

inputs:
  topic: "artificial intelligence"
  max_words: 200
  language: "English"

tasks:
  - id: write
    infer:
      prompt: |
        Write a {{inputs.max_words}}-word essay about {{inputs.topic}} in {{inputs.language}}.
      temperature: 0.7
```

Override inputs from the CLI:

```bash
nika run parameterized.nika.yaml -i topic="quantum computing" -i max_words=500
```

## Pattern 11: Multi-Provider Comparison

Compare outputs from different LLM providers on the same prompt:

```yaml
schema: nika/workflow@0.12
workflow: provider-comparison

tasks:
  - id: prompt_source
    exec: "echo 'Explain quantum entanglement in one paragraph.'"

  - id: claude
    depends_on: [prompt_source]
    with: { p: $prompt_source | trim }
    provider: anthropic
    model: claude-sonnet-4-20250514
    infer:
      prompt: "{{with.p}}"

  - id: gpt
    depends_on: [prompt_source]
    with: { p: $prompt_source | trim }
    provider: openai
    model: gpt-4o
    infer:
      prompt: "{{with.p}}"

  - id: groq_fast
    depends_on: [prompt_source]
    with: { p: $prompt_source | trim }
    provider: groq
    model: llama-4-maverick
    infer:
      prompt: "{{with.p}}"

  - id: compare
    depends_on: [claude, gpt, groq_fast]
    with:
      c: $claude | trim
      g: $gpt | trim
      q: $groq_fast | trim
    exec:
      command: |
        echo "=== Claude ==="
        echo "{{with.c}}"
        echo ""
        echo "=== GPT-4o ==="
        echo "{{with.g}}"
        echo ""
        echo "=== Llama 4 (Groq) ==="
        echo "{{with.q}}"
      shell: true
```

All three LLM calls run in parallel since they only depend on `prompt_source`.

## Pattern 12: Chained Fetch and Infer

A common pattern: fetch web data, then process with an LLM.

```yaml
schema: nika/workflow@0.12
workflow: fetch-and-analyze
provider: anthropic

tasks:
  - id: fetch_article
    fetch:
      url: "https://blog.example.com/latest"
      extract: article
      timeout: 15

  - id: fetch_meta
    fetch:
      url: "https://blog.example.com/latest"
      extract: metadata

  - id: analyze
    depends_on: [fetch_article, fetch_meta]
    with:
      content: $fetch_article | trim
      meta: $fetch_meta
    infer:
      prompt: |
        Analyze this article:

        Metadata: {{with.meta}}

        Content:
        {{with.content}}

        Provide: summary (3 sentences), tone, target audience, and SEO score (1-10).
      temperature: 0.3
```

## Pattern 13: Agent with Tools

Use the `agent:` verb for complex tasks that require multiple rounds of tool use:

```yaml
schema: nika/workflow@0.12
workflow: agent-research
provider: anthropic

mcp:
  github:
    command: npx
    args: ["-y", "@modelcontextprotocol/server-github"]
    env:
      GITHUB_TOKEN: "{{$env.GITHUB_TOKEN}}"

tasks:
  - id: research
    agent:
      prompt: |
        Research the top 5 trending Rust repositories on GitHub.
        For each, find: name, stars, description, and latest release.
      system: "You are a meticulous research assistant."
      tools: [web_search]
      mcp: [github]
      max_turns: 15
      temperature: 0.2
      guardrails:
        - type: length
          min_words: 100
```

## Pattern 14: Imports and Modular Workflows

Split large workflows into reusable modules:

```yaml
# main.nika.yaml
schema: nika/workflow@0.12
workflow: main-pipeline

imports:
  - path: ./modules/setup.nika.yaml
    prefix: setup_
  - path: ./modules/analysis.nika.yaml
    prefix: analyze_

tasks:
  - id: combine
    depends_on: [setup_data, analyze_results]
    with:
      data: $setup_data
      results: $analyze_results
    exec: "echo 'Combined: {{with.data}} + {{with.results}}'"
```

Imported tasks are prefixed to avoid name collisions. The prefix turns `data` into `setup_data`.

## Anti-Patterns to Avoid

### Missing $ prefix

```yaml
# WRONG: treated as literal string "source_task"
with:
  data: source_task

# CORRECT: references the task output
with:
  data: $source_task
```

### Using depends_on for data flow

```yaml
# WRONG: depends_on is ordering only, no data passes
- id: consumer
  depends_on: [producer]
  exec: "echo 'I cannot access producer output here'"

# CORRECT: use with: for data flow
- id: consumer
  depends_on: [producer]
  with:
    data: $producer
  exec: "echo '{{with.data}}'"
```

### Circular dependencies

```yaml
# WRONG: creates a cycle (NIKA-020)
- id: a
  depends_on: [b]
- id: b
  depends_on: [a]
```

### Shell pipes without shell: true

```yaml
# WRONG: pipe is interpreted as part of the command string
- id: count
  exec: "cat file.txt | wc -l"

# CORRECT: enable shell interpretation
- id: count
  exec:
    command: "cat file.txt | wc -l"
    shell: true
```

## Best Practices

1. **Name tasks descriptively** -- `fetch_user_data` is better than `step1`
2. **Use descriptions** -- The `description:` field documents intent
3. **Validate before running** -- `nika check` catches errors without spending API credits
4. **Set timeouts** -- Prevent runaway tasks with `timeout:` fields
5. **Use transforms wisely** -- `| trim` after shell output is almost always needed
6. **Start simple, iterate** -- Build one task at a time, validating as you go
7. **Use the mock provider** -- Test workflow structure without API calls: `--provider mock`
