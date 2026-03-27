# Your First Workflow

This guide walks you through building a real, multi-task Nika workflow from scratch. You will learn each verb progressively, understand how tasks connect through bindings, and see how the DAG execution model works in practice.

By the end, you will have a workflow that fetches data from the web, processes it with shell commands, summarizes it with an LLM, and produces structured output -- all in a single `.nika.yaml` file.

## Prerequisites

- Nika installed and working (`nika --version`)
- At least one LLM API key configured (see [Provider Setup Guide](03-provider-setup-guide.md))
- A terminal and a text editor

## Step 1: The Skeleton

Every Nika workflow starts with the same structure. Create a file called `research.nika.yaml`:

```yaml
schema: nika/workflow@0.12
workflow: web-research
description: "Fetch a webpage, extract its content, and produce an AI summary."
provider: anthropic

tasks: []
```

Let's break down the top-level fields:

| Field | Purpose |
|-------|---------|
| `schema` | Always `nika/workflow@0.12`. This tells Nika which parser to use. |
| `workflow` | A human-readable name for this workflow. Defaults to the filename if omitted. |
| `description` | Optional. Documents what the workflow does. |
| `provider` | The default LLM provider for all `infer:` and `agent:` tasks. |
| `tasks` | The ordered list of tasks to execute. This is the only required field besides `schema`. |

Validate it:

```bash
nika check research.nika.yaml
```

```
  ✓ All checks passed (0 tasks)
```

## Step 2: exec -- Your First Task

The `exec:` verb runs shell commands. It is the simplest verb and requires no API keys. Add your first task:

```yaml
schema: nika/workflow@0.12
workflow: web-research
provider: anthropic

tasks:
  - id: timestamp
    exec: "date '+%Y-%m-%d %H:%M:%S'"
    description: "Capture the current timestamp"
```

Key points about `exec:`:
- The **id** must be unique within the workflow
- The **simplest form** is just a string: `exec: "echo hello"`
- The output is captured as the task result (stdout)
- The **description** is optional but helps readability

Run it:

```bash
nika research.nika.yaml
```

```
  ✓ timestamp ─── 0.01s
    2026-03-23 14:30:00
```

### exec: Extended Form

For more control, use the object form:

```yaml
  - id: build
    exec:
      command: "npm run build && echo 'Done'"
      shell: true       # Required for pipes, &&, redirects
      cwd: ./frontend   # Working directory
      timeout: 60       # Timeout in seconds
      env:
        NODE_ENV: production
```

The `shell: true` flag passes the command through `sh -c`, enabling shell features like pipes (`|`), chaining (`&&`), and redirects (`>`). Without it, the command is executed directly.

## Step 3: fetch -- Getting Data

The `fetch:` verb makes HTTP requests. Let's add a task that fetches a webpage and extracts its content as Markdown:

```yaml
tasks:
  - id: timestamp
    exec: "date '+%Y-%m-%d %H:%M:%S'"

  - id: fetch_page
    fetch:
      url: "https://en.wikipedia.org/wiki/Rust_(programming_language)"
      extract: markdown
    description: "Fetch the Rust Wikipedia page as clean Markdown"
```

The `extract: markdown` option converts the raw HTML into clean Markdown, stripping navigation, scripts, and other non-content elements. This is much more useful than raw HTML for feeding into an LLM.

Run it:

```bash
nika research.nika.yaml
```

Both tasks run. Since `fetch_page` does not depend on `timestamp`, Nika can run them in parallel:

```
  ✓ timestamp ─── 0.01s
    2026-03-23 14:30:00

  ✓ fetch_page ─── 0.87s
    # Rust (programming language)
    Rust is a general-purpose programming language...
    [truncated]
```

### fetch: Extract Modes

Nika supports 9 extraction modes for post-processing HTTP responses:

```yaml
# Clean Markdown
fetch: { url: "...", extract: markdown }

# Main article content (Readability algorithm)
fetch: { url: "...", extract: article }

# Visible text only
fetch: { url: "...", extract: text }

# CSS selector extraction
fetch: { url: "...", extract: selector, selector: "h1, h2, h3" }

# Metadata (Open Graph, Twitter Cards, JSON-LD)
fetch: { url: "...", extract: metadata }

# Link classification
fetch: { url: "...", extract: links }

# JSON API with JSONPath
fetch: { url: "...", extract: jsonpath, selector: "$.data[*].name" }

# RSS/Atom feed parsing
fetch: { url: "...", extract: feed }

# AI-era content discovery
fetch: { url: "...", extract: llm_txt }
```

## Step 4: Connecting Tasks with Bindings

Now comes the powerful part: passing data between tasks. The `with:` block creates bindings, and `{{with.alias}}` templates inject the data into strings.

```yaml
tasks:
  - id: timestamp
    exec: "date '+%Y-%m-%d %H:%M:%S'"

  - id: fetch_page
    fetch:
      url: "https://en.wikipedia.org/wiki/Rust_(programming_language)"
      extract: markdown

  - id: word_count
    depends_on: [fetch_page]
    with:
      page: $fetch_page
    exec: "echo '{{with.page}}' | wc -w | tr -d ' '"
    description: "Count words in the fetched page"
```

Breaking down the data flow:

1. `depends_on: [fetch_page]` -- ensures `word_count` waits for `fetch_page` to complete
2. `with: { page: $fetch_page }` -- binds the output of `fetch_page` to the alias `page`
3. `{{with.page}}` -- injects the bound value into the exec command

The `$` prefix is required for task references. This is how Nika knows to look up another task's output rather than treating it as a literal string.

### Pipe Transforms

You can transform bound values using pipe operators:

```yaml
  - id: clean_data
    with:
      text: $fetch_page | trim | lower
      count: $word_count | trim
    exec: "echo 'Words: {{with.count}}'"
```

Available transforms include `trim`, `upper`, `lower`, `length`, `first`, `last`, `sort`, `unique`, `flatten`, `reverse`, `compact`, `to_json`, `parse_json`, `join(",")`, `split(",")`, `default("fallback")`, `type_of`, and more. See [Workflow Patterns](workflow-patterns.md) for the complete catalog.

## Step 5: infer -- Adding AI

Now let's use an LLM to summarize the fetched content. Add an `infer:` task:

```yaml
tasks:
  - id: timestamp
    exec: "date '+%Y-%m-%d %H:%M:%S'"

  - id: fetch_page
    fetch:
      url: "https://en.wikipedia.org/wiki/Rust_(programming_language)"
      extract: markdown

  - id: word_count
    depends_on: [fetch_page]
    with:
      page: $fetch_page
    exec: "echo '{{with.page}}' | wc -w | tr -d ' '"

  - id: summarize
    depends_on: [fetch_page]
    with:
      content: $fetch_page
    infer:
      prompt: |
        Summarize the following article in 3 bullet points.
        Focus on: what Rust is, its key features, and its adoption.

        Article:
        {{with.content}}
      temperature: 0.3
      max_tokens: 500
    description: "AI-powered summary of the article"
```

Key `infer:` options:

| Field | Purpose | Default |
|-------|---------|---------|
| `prompt` | The text prompt sent to the LLM | Required |
| `system` | System prompt (persona, instructions) | None |
| `temperature` | Creativity (0.0 = deterministic, 2.0 = creative) | Provider default |
| `max_tokens` | Maximum output length | Provider default |
| `provider` | Override workflow-level provider | Workflow default |
| `model` | Override model | Provider default |

## Step 6: Building the DAG

Let's add a final task that combines everything into a report:

```yaml
schema: nika/workflow@0.12
workflow: web-research
description: "Fetch a webpage, extract its content, and produce an AI summary."
provider: anthropic

tasks:
  - id: timestamp
    exec: "date '+%Y-%m-%d %H:%M:%S'"

  - id: fetch_page
    fetch:
      url: "https://en.wikipedia.org/wiki/Rust_(programming_language)"
      extract: markdown

  - id: word_count
    depends_on: [fetch_page]
    with:
      page: $fetch_page
    exec:
      command: "echo '{{with.page}}' | wc -w | tr -d ' '"
      shell: true

  - id: summarize
    depends_on: [fetch_page]
    with:
      content: $fetch_page
    infer:
      prompt: |
        Summarize the following article in 3 bullet points.
        Focus on: what Rust is, its key features, and its adoption.

        Article:
        {{with.content}}
      temperature: 0.3
      max_tokens: 500

  - id: report
    depends_on: [timestamp, word_count, summarize]
    with:
      time: $timestamp | trim
      words: $word_count | trim
      summary: $summarize | trim
    exec:
      command: |
        echo "=== Research Report ==="
        echo "Generated: {{with.time}}"
        echo "Source: Wikipedia - Rust"
        echo "Word count: {{with.words}}"
        echo ""
        echo "=== Summary ==="
        echo "{{with.summary}}"
      shell: true
    description: "Final combined report"
```

The DAG for this workflow looks like:

```
  timestamp ──────────────────────┐
                                  │
  fetch_page ──┬── word_count ────┤
               │                  │
               └── summarize ─────┤
                                  │
                              report
```

`timestamp` and `fetch_page` run in parallel (no dependencies). `word_count` and `summarize` both depend on `fetch_page` and run in parallel with each other. `report` waits for all three upstream tasks.

## Step 7: invoke -- Using Builtin Tools

Nika includes 24 builtin tools accessible through the `invoke:` verb with the `nika:` prefix. Let's add a task that uses the builtin logging tool:

```yaml
  - id: log_start
    invoke:
      tool: "nika:log"
      params:
        message: "Research workflow started"
        level: info
```

Or in shorthand form (when no params are needed):

```yaml
  - id: check_dims
    invoke: "nika:dimensions"
```

Common builtin tools:

| Tool | Purpose |
|------|---------|
| `nika:log` | Log a message at a specific level |
| `nika:emit` | Emit a custom event |
| `nika:assert` | Assert a condition (fails task if false) |
| `nika:import` | Import a file into the content-addressable store |
| `nika:thumbnail` | Generate image thumbnails |
| `nika:chart` | Create charts from JSON data |

## Step 8: Adding Structured Output

For tasks that need to produce JSON output conforming to a specific schema, use the `structured:` field:

```yaml
  - id: extract_facts
    depends_on: [fetch_page]
    with:
      content: $fetch_page
    infer:
      prompt: "Extract 3 key facts from this article: {{with.content}}"
    structured:
      schema:
        type: object
        properties:
          facts:
            type: array
            items:
              type: object
              properties:
                fact:
                  type: string
                category:
                  type: string
                  enum: [history, technical, community]
              required: [fact, category]
        required: [facts]
      max_retries: 2
      enable_repair: true
```

Nika's structured output engine validates the LLM response against the JSON Schema and automatically retries or repairs malformed output, achieving near-perfect compliance.

## Step 9: Adding Artifacts

To save task output to files, add the `artifact:` field:

```yaml
  - id: report
    depends_on: [timestamp, word_count, summarize]
    with:
      time: $timestamp | trim
      words: $word_count | trim
      summary: $summarize | trim
    exec:
      command: |
        echo '{"generated":"{{with.time}}","word_count":{{with.words}},"summary":"{{with.summary}}"}'
      shell: true
    artifact:
      path: report.json
      format: json
```

The output is written to `report.json` in the artifacts directory.

## Step 10: Using for_each for Iteration

When you need to process a list of items, use `for_each:`:

```yaml
schema: nika/workflow@0.12
workflow: multi-page-research
provider: anthropic

tasks:
  - id: urls
    exec: |
      echo '["https://example.com/page1", "https://example.com/page2", "https://example.com/page3"]'
    output:
      format: json

  - id: fetch_all
    depends_on: [urls]
    for_each: $urls
    as: url
    concurrency: 3
    fetch:
      url: "{{with.url}}"
      extract: markdown
    description: "Fetch all pages in parallel"

  - id: summarize_all
    depends_on: [fetch_all]
    with:
      pages: $fetch_all
    infer:
      prompt: |
        Summarize each of these pages in one sentence each:
        {{with.pages | to_json}}
```

The `for_each:` block:
- `items` -- The array to iterate over (from a task reference)
- `as` -- The variable name for the current item (default: `item`)
- `concurrency` -- Maximum parallel iterations
- `fail_fast` -- Stop all iterations if one fails (default: `true`)

Inside the task body, use `{{with.item}}` (or `{{with.item.field}}` for objects) to reference the current iteration value.

## Complete Working Example

Here is the final version of our research workflow, incorporating everything learned:

```yaml
schema: nika/workflow@0.12
workflow: web-research-complete
description: "Full research pipeline: fetch, analyze, summarize, report."
provider: anthropic

tasks:
  - id: timestamp
    exec: "date '+%Y-%m-%d %H:%M:%S'"

  - id: fetch_page
    fetch:
      url: "https://en.wikipedia.org/wiki/Rust_(programming_language)"
      extract: markdown
      timeout: 30

  - id: word_count
    depends_on: [fetch_page]
    with:
      page: $fetch_page
    exec:
      command: "echo '{{with.page}}' | wc -w | tr -d ' '"
      shell: true

  - id: summarize
    depends_on: [fetch_page]
    with:
      content: $fetch_page
    infer:
      prompt: |
        Summarize this article in exactly 3 bullet points:
        {{with.content}}
      temperature: 0.3
      max_tokens: 300
    retry:
      max_attempts: 2
      delay_ms: 1000

  - id: report
    depends_on: [timestamp, word_count, summarize]
    with:
      time: $timestamp | trim
      words: $word_count | trim
      summary: $summarize | trim
    exec:
      command: |
        echo "=== Research Report ==="
        echo "Generated: {{with.time}}"
        echo "Word count: {{with.words}}"
        echo ""
        echo "{{with.summary}}"
      shell: true
    description: "Combine all results into final report"
```

## Validating Before Running

Always validate your workflow before running it, especially as it grows:

```bash
# Basic validation (syntax, DAG, bindings)
nika check research.nika.yaml

# Strict validation (also checks MCP connections)
nika check research.nika.yaml --strict
```

The `check` command validates:
- YAML syntax correctness
- Schema version compatibility
- DAG structure (no cycles, no missing dependencies)
- Binding validity (all `$task_id` references exist)
- Template syntax (balanced `{{` and `}}`)

## What You Have Learned

In this guide you:

1. Created a workflow from scratch with the required `schema:` and `tasks:` fields
2. Used `exec:` for shell commands (simple and extended forms)
3. Used `fetch:` with `extract: markdown` to get web content
4. Connected tasks with `depends_on:`, `with:`, and `{{with.alias}}` templates
5. Applied pipe transforms (`| trim`, `| lower`, etc.)
6. Used `infer:` for LLM text generation with temperature and token control
7. Used `invoke:` for builtin Nika tools
8. Added `structured:` output with JSON Schema validation
9. Saved output with `artifact:`
10. Iterated over arrays with `for_each:`

## Next Steps

- **[Workflow Patterns](workflow-patterns.md)** -- Diamond patterns, fan-out, error handling, retries
- **[infer: Deep Dive](infer-verb-guide.md)** -- Vision, system prompts, structured output
- **[fetch: Deep Dive](fetch-verb-guide.md)** -- All 9 extract modes, response modes
- **[exec/invoke/agent Guide](../../07-exec-invoke-agent-guide.md)** -- Shell tricks, 24 builtin tools, agent loops
