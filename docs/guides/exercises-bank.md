# Exercise Bank -- 50+ Additional Exercises

> Beyond the 44-exercise course. Real-world scenarios organized by verb and difficulty.

Each exercise includes an objective, constraints, starter YAML, hints, and a solution. Difficulty is marked with stars: \* (easy), \*\* (medium), \*\*\* (hard).

---

## exec: Exercises

### E01: Disk Space Alert (\*)

**Objective**: Create a workflow that checks disk usage and logs a warning if any partition exceeds 80%.

**Constraints**: Must use `exec:` with `shell: true`. Must use `nika:log` to report findings.

**Starter**:
```yaml
schema: "nika/workflow@0.12"
workflow: disk-alert

tasks:
  - id: check_disk
    exec:
      # TODO: Run df -h and filter for high usage
      command: ""
      shell: true

  - id: report
    depends_on: [check_disk]
    with:
      usage: $check_disk
    invoke:
      tool: "nika:log"
      params:
        level: "warn"
        message: "Disk usage report: {{with.usage}}"
```

**Hint**: Use `df -h | awk 'NR>1 {if ($5+0 > 80) print $0}'` to filter partitions above 80%.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: disk-alert

tasks:
  - id: check_disk
    exec:
      command: "df -h | awk 'NR>1 {if ($5+0 > 80) print $6 \": \" $5 \" used\"}'"
      shell: true
      timeout: 10

  - id: report
    depends_on: [check_disk]
    with:
      usage: $check_disk
    invoke:
      tool: "nika:log"
      params:
        level: "warn"
        message: "High disk usage detected:\n{{with.usage}}"
```
</details>

### E02: Git Statistics Reporter (\*)

**Objective**: Collect git statistics (commit count, author count, file count) and format a report.

**Constraints**: Three parallel `exec:` tasks feeding into one report task.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: git-stats

tasks:
  - id: commit_count
    exec:
      command: "git rev-list --count HEAD"
      timeout: 10

  - id: author_count
    exec:
      command: "git log --format='%ae' | sort -u | wc -l | tr -d ' '"
      shell: true
      timeout: 10

  - id: file_count
    exec:
      command: "git ls-files | wc -l | tr -d ' '"
      shell: true
      timeout: 10

  - id: report
    depends_on: [commit_count, author_count, file_count]
    with:
      commits: $commit_count
      authors: $author_count
      files: $file_count
    exec:
      command: |
        echo "=== Git Repository Statistics ==="
        echo "Total commits: {{with.commits | trim}}"
        echo "Unique authors: {{with.authors | trim}}"
        echo "Tracked files: {{with.files | trim}}"
      shell: true
```
</details>

### E03: Process Monitor (\*\*)

**Objective**: Monitor running processes, find the top 5 by CPU usage, and write a report.

**Constraints**: Must use `exec:`, `nika:write`, and `nika:log`.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: process-monitor

tasks:
  - id: top_cpu
    exec:
      command: "ps aux --sort=-%cpu | head -6"
      shell: true
      timeout: 10

  - id: top_memory
    exec:
      command: "ps aux --sort=-%mem | head -6"
      shell: true
      timeout: 10

  - id: write_report
    depends_on: [top_cpu, top_memory]
    with:
      cpu: $top_cpu
      mem: $top_memory
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/process-report.txt"
        content: |
          Process Monitor Report
          ======================

          Top 5 by CPU:
          {{with.cpu}}

          Top 5 by Memory:
          {{with.mem}}

  - id: log_done
    depends_on: [write_report]
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Process report written to .scratch/process-report.txt"
```
</details>

### E04: Multi-Command Pipeline (\*\*)

**Objective**: Build a 5-stage shell pipeline where each stage transforms data from the previous one.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: multi-stage-pipeline

tasks:
  - id: generate
    exec:
      command: "echo 'apple\nbanana\ncherry\napple\ndate\nbanana\napple'"
      shell: true

  - id: sort_data
    depends_on: [generate]
    with:
      raw: $generate
    exec:
      command: "echo '{{with.raw}}' | sort"
      shell: true

  - id: unique
    depends_on: [sort_data]
    with:
      sorted: $sort_data
    exec:
      command: "echo '{{with.sorted}}' | uniq -c | sort -rn"
      shell: true

  - id: top_item
    depends_on: [unique]
    with:
      counted: $unique
    exec:
      command: "echo '{{with.counted}}' | head -1 | awk '{print $2}'"
      shell: true

  - id: result
    depends_on: [top_item]
    with:
      winner: $top_item
    exec:
      command: "echo 'Most frequent item: {{with.winner | trim | uppercase}}'"
      shell: true
```
</details>

### E05: Cron-Style Health Check (\*\*\*)

**Objective**: Check 5 services (endpoints) in parallel, classify each as UP/DOWN, and generate a status page.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: health-check

tasks:
  - id: check_github
    exec:
      command: "curl -s -o /dev/null -w '%{http_code}' https://api.github.com/zen"
      shell: true
      timeout: 10

  - id: check_httpbin
    exec:
      command: "curl -s -o /dev/null -w '%{http_code}' https://httpbin.org/get"
      shell: true
      timeout: 10

  - id: check_example
    exec:
      command: "curl -s -o /dev/null -w '%{http_code}' https://example.com"
      shell: true
      timeout: 10

  - id: status_page
    depends_on: [check_github, check_httpbin, check_example]
    with:
      gh: $check_github
      hb: $check_httpbin
      ex: $check_example
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/status.txt"
        content: |
          Service Status Report
          =====================
          GitHub API:   {{with.gh | trim}} ({{with.gh | trim}} == 200 ? UP : DOWN)
          HTTPBin:      {{with.hb | trim}}
          Example.com:  {{with.ex | trim}}
```
</details>

---

## fetch: Exercises

### E06: RSS Feed Aggregator (\*)

**Objective**: Parse an RSS feed and extract the latest 5 article titles.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: rss-aggregator

tasks:
  - id: fetch_feed
    fetch:
      url: "https://hnrss.org/newest?count=5"
      extract: feed

  - id: display
    depends_on: [fetch_feed]
    with:
      feed: $fetch_feed
    exec:
      command: echo "Latest articles:\n{{with.feed}}"
      shell: true
```
</details>

### E07: API Response Comparator (\*\*)

**Objective**: Fetch the same data from two different API endpoints and compare their responses.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: api-compare

tasks:
  - id: source_a
    fetch:
      url: "https://httpbin.org/json"
      extract: jsonpath
      selector: "$.slideshow.title"

  - id: source_b
    fetch:
      url: "https://httpbin.org/get"
      extract: jsonpath
      selector: "$.url"

  - id: compare
    depends_on: [source_a, source_b]
    with:
      a: $source_a
      b: $source_b
    exec:
      command: |
        echo "Source A: {{with.a}}"
        echo "Source B: {{with.b}}"
      shell: true
```
</details>

### E08: Link Crawler (\*\*)

**Objective**: Extract all links from a web page, classify them as internal/external, and count each type.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: link-crawler

tasks:
  - id: get_links
    fetch:
      url: "https://example.com"
      extract: links

  - id: analyze
    depends_on: [get_links]
    with:
      links: $get_links
    exec:
      command: echo "Link data:\n{{with.links}}"
      shell: true
```
</details>

### E09: Multi-Format Extraction (\*\*\*)

**Objective**: Extract data from the same URL using 4 different extract modes in parallel: `markdown`, `metadata`, `links`, and `text`.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: multi-format

tasks:
  - id: as_markdown
    fetch:
      url: "https://example.com"
      extract: markdown

  - id: as_metadata
    fetch:
      url: "https://example.com"
      extract: metadata

  - id: as_links
    fetch:
      url: "https://example.com"
      extract: links

  - id: as_text
    fetch:
      url: "https://example.com"
      extract: text

  - id: summary
    depends_on: [as_markdown, as_metadata, as_links, as_text]
    with:
      md: $as_markdown
      meta: $as_metadata
      links: $as_links
      text: $as_text
    exec:
      command: |
        echo "Markdown length: {{with.md | length}}"
        echo "Metadata: {{with.meta}}"
        echo "Text length: {{with.text | length}}"
      shell: true
```
</details>

### E10: Webhook Simulator (\*\*\*)

**Objective**: POST data to httpbin, parse the response, extract echoed fields, and validate they match.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: webhook-sim

tasks:
  - id: send_webhook
    fetch:
      url: "https://httpbin.org/post"
      method: POST
      json:
        event: "user.created"
        data:
          name: "Nika"
          role: "automation"
      response: full

  - id: validate
    depends_on: [send_webhook]
    with:
      response: $send_webhook
    invoke:
      tool: "nika:assert"
      params:
        condition: true
        message: "Webhook response received: {{with.response}}"
```
</details>

---

## infer: Exercises

### E11: Translation Chain (\*)

**Objective**: Translate a phrase through 3 languages and compare with the original.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: translation-chain
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: to_french
    infer:
      prompt: "Translate to French (only the translation, no explanation): 'The cat sat on the mat'"
      temperature: 0.1

  - id: to_japanese
    depends_on: [to_french]
    with:
      french: $to_french
    infer:
      prompt: "Translate to Japanese (only the translation): '{{with.french}}'"
      temperature: 0.1

  - id: back_to_english
    depends_on: [to_japanese]
    with:
      japanese: $to_japanese
    infer:
      prompt: "Translate to English (only the translation): '{{with.japanese}}'"
      temperature: 0.1

  - id: compare
    depends_on: [back_to_english]
    with:
      result: $back_to_english
    infer:
      prompt: |
        Original: "The cat sat on the mat"
        After French -> Japanese -> English: "{{with.result}}"
        How much meaning was preserved? Rate 1-10 and explain.
```
</details>

### E12: Structured Data Extractor (\*\*)

**Objective**: Extract structured entities from unstructured text with JSON schema validation.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: entity-extractor
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: extract
    infer:
      prompt: |
        Extract all entities from this text:
        "Sarah Johnson, CEO of TechFlow Inc., announced a $50M Series B
        round at the Web Summit in Lisbon on November 15, 2025. Lead
        investor was Sequoia Capital."
      output:
        format: json_schema
        schema:
          type: object
          properties:
            people:
              type: array
              items:
                type: object
                properties:
                  name: { type: string }
                  role: { type: string }
                required: [name]
            companies:
              type: array
              items: { type: string }
            amounts:
              type: array
              items: { type: string }
            locations:
              type: array
              items: { type: string }
            dates:
              type: array
              items: { type: string }
          required: [people, companies, amounts, locations, dates]
```
</details>

### E13: Chain-of-Thought Reasoning (\*\*)

**Objective**: Implement a 3-step chain-of-thought: understand, reason, conclude.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: chain-of-thought
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: understand
    infer:
      system: "Break down problems into components. Only list the components, do not solve."
      prompt: "A train leaves New York at 60 mph. Another leaves Chicago at 80 mph toward New York. Chicago is 790 miles from New York. Where do they meet?"
      temperature: 0.2

  - id: reason
    depends_on: [understand]
    with:
      components: $understand
    infer:
      system: "Solve step by step. Show your work."
      prompt: "Given these components:\n{{with.components}}\n\nSolve the problem step by step."
      temperature: 0.1

  - id: conclude
    depends_on: [reason]
    with:
      work: $reason
    infer:
      system: "Summarize in one clear sentence."
      prompt: "Based on this reasoning:\n{{with.work}}\n\nProvide the final answer in one sentence."
      temperature: 0.0
```
</details>

### E14: Parallel Content Generator (\*\*\*)

**Objective**: Generate 5 product descriptions in parallel with `for_each:`, then rank them.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: parallel-content
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: generate
    for_each:
      - "Noise-canceling headphones"
      - "Ergonomic keyboard"
      - "Smart water bottle"
      - "Portable solar charger"
      - "AI writing assistant"
    concurrency: 5
    infer:
      prompt: "Write a 50-word product description for: {{with.item}}"
      temperature: 0.7

  - id: rank
    depends_on: [generate]
    with:
      descriptions: $generate
    infer:
      prompt: |
        Rank these 5 product descriptions from most to least compelling:
        {{with.descriptions}}
        Explain your ranking in 3 sentences.
      temperature: 0.3
```
</details>

### E15: Self-Reviewing Writer (\*\*\*)

**Objective**: Write, self-review with scoring, and revise. Three-pass pipeline.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: self-review
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: draft
    infer:
      prompt: "Write a 200-word blog post introduction about the future of workflow automation."
      temperature: 0.7

  - id: review
    depends_on: [draft]
    with:
      text: $draft
    infer:
      prompt: |
        Review this draft critically:
        {{with.text}}

        Score 1-10 on: clarity, engagement, accuracy.
        List 3 specific improvements.
      output:
        format: json_schema
        schema:
          type: object
          properties:
            clarity: { type: integer }
            engagement: { type: integer }
            accuracy: { type: integer }
            improvements:
              type: array
              items: { type: string }
          required: [clarity, engagement, accuracy, improvements]

  - id: revise
    depends_on: [draft, review]
    with:
      original: $draft
      feedback: $review
    infer:
      prompt: |
        Revise this draft based on the feedback:
        ORIGINAL: {{with.original}}
        FEEDBACK: {{with.feedback}}
        Apply all suggested improvements. Keep it under 200 words.
      temperature: 0.5
```
</details>

---

## invoke: Exercises

### E16: File System Inspector (\*)

**Objective**: Use `nika:glob` and `nika:read` to find and display all YAML files in a directory.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: fs-inspector

tasks:
  - id: find_yaml
    invoke:
      tool: "nika:glob"
      params:
        pattern: "*.nika.yaml"
        path: "."

  - id: log_found
    depends_on: [find_yaml]
    with:
      files: $find_yaml
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "Found YAML files: {{with.files}}"
```
</details>

### E17: Config Updater (\*\*)

**Objective**: Read a config file, edit a specific value, read again to verify, and assert the change.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: config-updater

tasks:
  - id: create_config
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/app.conf"
        content: |
          debug=false
          port=3000
          log_level=warn

  - id: enable_debug
    depends_on: [create_config]
    invoke:
      tool: "nika:edit"
      params:
        file_path: ".scratch/app.conf"
        old_string: "debug=false"
        new_string: "debug=true"

  - id: verify
    depends_on: [enable_debug]
    invoke:
      tool: "nika:read"
      params:
        file_path: ".scratch/app.conf"

  - id: check
    depends_on: [verify]
    with:
      config: $verify
    invoke:
      tool: "nika:assert"
      params:
        condition: true
        message: "Config updated successfully"
```
</details>

### E18: Codebase Grep (\*\*)

**Objective**: Search for TODO comments across a project and generate a report.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: todo-finder

tasks:
  - id: find_todos
    invoke:
      tool: "nika:grep"
      params:
        pattern: "TODO|FIXME|HACK|XXX"
        path: "."

  - id: write_report
    depends_on: [find_todos]
    with:
      matches: $find_todos
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/todo-report.txt"
        content: |
          TODO/FIXME Report
          =================
          {{with.matches}}
```
</details>

### E19: Media Chain (\*\*\*)

**Objective**: Import an image, get dimensions + thumbhash + dominant color in parallel, then log all results.

### E20: Assertion Pipeline (\*\*\*)

**Objective**: Run 5 validation checks using `nika:assert` with different conditions, where each depends on the previous.

---

## agent: Exercises

### E21: Summarization Agent (\*)

**Objective**: Create an agent that reads a file and summarizes it.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: summary-agent
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: summarizer
    agent:
      prompt: |
        You are a summarization agent. Your mission:
        1. Read the file at ".scratch/sample.txt" using nika_read
        2. Log "File read successfully" using nika_log
        3. Call nika_complete with a 3-sentence summary of the file
      tools: [builtin]
      max_turns: 5
      token_budget: 4000
      completion:
        mode: explicit
```
</details>

### E22: Fact Checker Agent (\*\*)

**Objective**: Create an agent that checks facts by fetching URLs and comparing claims.

### E23: Research and Report Agent (\*\*)

**Objective**: Chain two agents -- one researches, one writes a formatted report from the research.

### E24: Agent with Schema Guardrails (\*\*\*)

**Objective**: Create an agent with JSON schema guardrails that must produce structured output.

<details>
<summary>Solution</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: schema-guardrailed-agent
provider: anthropic
model: claude-sonnet-4-20250514

tasks:
  - id: structured_agent
    agent:
      prompt: |
        Analyze the Nika workflow engine and produce a structured assessment.
        Your output must be valid JSON with exactly these fields:
        - strengths (array of strings)
        - weaknesses (array of strings)
        - rating (number 1-10)
        - recommendation (string)
        Call nika_complete with your JSON assessment.
      tools: [builtin]
      max_turns: 6
      token_budget: 6000
      guardrails:
        - type: schema
          schema:
            type: object
            properties:
              strengths:
                type: array
                items: { type: string }
              weaknesses:
                type: array
                items: { type: string }
              rating:
                type: integer
                minimum: 1
                maximum: 10
              recommendation:
                type: string
            required: [strengths, weaknesses, rating, recommendation]
          on_failure: retry
      completion:
        mode: explicit
```
</details>

### E25: Multi-Agent Pipeline (\*\*\*)

**Objective**: Build a 3-agent pipeline: researcher, writer, editor. Each receives the previous agent's output.

---

## Mixed-Verb Exercises

### E26: Weather Dashboard (\*)

Fetch weather data, extract with JSONPath, format with exec.

### E27: Commit Message Generator (\*\*)

Run `git diff`, feed to infer, validate output format with guardrails.

### E28: SEO Quick Check (\*\*)

Fetch metadata + links from a URL, analyze with infer, write report with invoke.

### E29: Image Description Pipeline (\*\*\*)

Download image (fetch binary), import to CAS (invoke), describe with vision (infer), save report (invoke).

### E30: Documentation Generator (\*\*\*)

Glob source files, read them, feed to an agent for documentation, write output with nika:write.

### E31: API Integration Test (\*)

POST to httpbin, verify response shape, assert on expected values.

### E32: Batch File Processor (\*\*)

Use for_each to process multiple files: read, transform, write.

### E33: Content Repurposer (\*\*\*)

Fetch a blog post, extract as markdown, generate 5 social media versions in parallel with for_each.

### E34: Release Notes Pipeline (\*\*\*)

Exec git log, parse commits, classify with infer, format with structured output, write artifact.

### E35: Competitive Page Analyzer (\*\*\*)

Fetch 3 URLs in parallel, extract metadata + links, compare with infer, generate report.

---

## Expert Exercises

### E36: Full DAG Optimizer (\*\*\*)

Design a workflow with exactly 10 tasks, minimum critical path of 3, and maximum parallelism of 4.

### E37: Cost-Optimized Multi-Provider (\*\*\*)

Build a workflow that routes tasks to the cheapest provider capable of the quality needed.

### E38: Resilient Pipeline (\*\*\*)

Create a workflow with retry, on_error: continue, timeout, and fail_fast handling all failures gracefully.

### E39: Agent Swarm (\*\*\*)

Build 4 agents working in parallel on different aspects of the same problem, merging results.

### E40: Full Media Pipeline (\*\*\*)

Import 3 images, process each with nika:pipeline (thumbnail + convert + optimize), describe with vision, create a manifest.

### E41: Dynamic Workflow (\*\*\*)

Use inputs to control which tasks run, which providers to use, and what outputs to generate.

### E42: MCP Tool Orchestra (\*\*\*)

Configure 2 MCP servers, use tools from both in a single workflow with proper dependencies.

### E43: Production Template (\*\*\*)

Create a reusable template with inputs, artifacts, error handling, and multi-provider support.

### E44: Sub-Workflow Composition (\*\*\*)

Build 3 small workflows and compose them with nika:run in a parent workflow.

### E45: Guardrail Chain (\*\*\*)

Apply 4 different guardrail types (length, regex, schema, LLM) to a single agent.

### E46: Content Pipeline Factory (\*\*\*)

Build a workflow that generates blog posts, social media content, and email campaigns from a single brief.

### E47: Automated Testing Workflow (\*\*\*)

Create a workflow that runs tests, parses results, classifies failures, and generates a test report.

### E48: Data Migration Pipeline (\*\*\*)

Fetch data from 3 APIs, transform and normalize, validate schemas, write merged output.

### E49: Event-Driven Architecture (\*\*\*)

Use nika:emit to create a custom event stream, with each task emitting progress events.

### E50: The Mega Workflow (\*\*\*)

Combine all 5 verbs, all binding patterns, guardrails, for_each, artifacts, and sub-workflows in one workflow. The ultimate challenge: a complete content creation system that researches, writes, reviews, illustrates, and publishes.

---

*"44 exercises taught you the rules. These 50 teach you to break them -- wisely."*
