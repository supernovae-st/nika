# Project Ideas -- 20 Complete Projects

> From single-file utilities to multi-agent production systems. Build real things.

Each project includes a description, skills practiced, estimated time, and a solution outline showing the workflow structure.

---

## Beginner Projects (1-2 tasks each)

### Project 1: Morning Briefing

**Description**: A daily automation that collects your public IP, checks a weather API, and formats a "Good morning" message. One workflow, three parallel fetches, one summary.

**Skills practiced**: `fetch:` verb, `depends_on:`, `with:` bindings, parallel execution

**Estimated time**: 30 minutes

**Solution outline**:
```yaml
schema: "nika/workflow@0.12"
workflow: morning-briefing

tasks:
  - id: get_ip
    fetch: "https://httpbin.org/ip"

  - id: get_weather
    fetch:
      url: "https://wttr.in/?format=3"

  - id: get_date
    exec: "date '+%A, %B %d, %Y'"

  - id: briefing
    depends_on: [get_ip, get_weather, get_date]
    with:
      ip: $get_ip
      weather: $get_weather
      date: $get_date
    exec:
      command: |
        echo "=== Morning Briefing ==="
        echo "Date: {{with.date | trim}}"
        echo "IP: {{with.ip}}"
        echo "Weather: {{with.weather | trim}}"
      shell: true
```

---

### Project 2: Dependency Checker

**Description**: Check whether common development tools are installed on the current machine. Run `which` for each tool and report what is available and what is missing.

**Skills practiced**: `exec:` verb, `on_error: continue`, parallel execution, `nika:write`

**Estimated time**: 30 minutes

**Solution outline**:
```yaml
tasks:
  - id: check_git
    exec: "which git"
    on_error: continue

  - id: check_node
    exec: "which node"
    on_error: continue

  - id: check_cargo
    exec: "which cargo"
    on_error: continue

  - id: check_python
    exec: "which python3"
    on_error: continue

  - id: report
    depends_on: [check_git, check_node, check_cargo, check_python]
    with:
      git: $check_git
      node: $check_node
      cargo: $check_cargo
      python: $check_python
    invoke:
      tool: "nika:write"
      params:
        file_path: ".scratch/deps-report.txt"
        content: |
          Dependency Check Report
          =======================
          git: {{with.git | trim}}
          node: {{with.node | trim}}
          cargo: {{with.cargo | trim}}
          python3: {{with.python | trim}}
```

---

### Project 3: JSON Pretty Printer

**Description**: Fetch a JSON API response, extract key fields using JSONPath, and display them in a human-readable format with pipe transforms.

**Skills practiced**: `fetch:` with `extract: jsonpath`, pipe transforms, `with:` bindings

**Estimated time**: 20 minutes

**Solution outline**:
```yaml
schema: "nika/workflow@0.12"
workflow: json-pretty

tasks:
  - id: fetch_data
    fetch:
      url: "https://httpbin.org/json"

  - id: extract_title
    fetch:
      url: "https://httpbin.org/json"
      extract: jsonpath
      selector: "$.slideshow.title"

  - id: display
    depends_on: [fetch_data, extract_title]
    with:
      raw: $fetch_data
      title: $extract_title
    exec:
      command: |
        echo "=== Pretty JSON Report ==="
        echo "Title: {{with.title | trim | upper}}"
        echo "Raw length: {{with.raw | length}} chars"
      shell: true
```

---

### Project 4: Uptime Monitor

**Description**: Check the HTTP status of 5 URLs in parallel and write a simple UP/DOWN status page to a file. Each URL check uses `response: full` to capture the status code.

**Skills practiced**: `fetch:` with `response: full`, parallel execution, `nika:write`, `with:` bindings

**Estimated time**: 30 minutes

**Solution outline**:
```yaml
schema: "nika/workflow@0.12"
workflow: uptime-monitor

inputs:
  urls:
    - "https://example.com"
    - "https://httpbin.org"
    - "https://api.github.com"

tasks:
  - id: check
    for_each: "{{inputs.urls}}"
    concurrency: 5
    fetch:
      url: "{{with.item}}"
      response: full
      timeout: 10

  - id: report
    depends_on: [check]
    with:
      results: $check
    invoke:
      tool: "nika:write"
      params:
        file_path: "output/uptime-status.txt"
        content: |
          Uptime Status Report
          ====================
          {{with.results}}
```

---

### Project 5: Quote of the Day

**Description**: Fetch a random quote from a public API, format it with pipe transforms, add the current date, and save it to a daily file. A simple but complete end-to-end workflow.

**Skills practiced**: `fetch:`, pipe transforms, `nika:write`, `exec:` for date formatting

**Estimated time**: 20 minutes

**Solution outline**:
```yaml
schema: "nika/workflow@0.12"
workflow: quote-of-the-day

tasks:
  - id: get_date
    exec: "date '+%Y-%m-%d'"

  - id: get_quote
    fetch:
      url: "https://httpbin.org/json"
      extract: jsonpath
      selector: "$.slideshow.title"

  - id: format_save
    depends_on: [get_date, get_quote]
    with:
      date: $get_date
      quote: $get_quote
    invoke:
      tool: "nika:write"
      params:
        file_path: "output/quote-{{with.date | trim}}.txt"
        content: |
          Quote of the Day — {{with.date | trim}}
          ==========================================
          "{{with.quote | trim}}"
```

---

## Intermediate Projects (3-5 tasks)

### Project 6: Git PR Summary Generator

**Description**: Capture the git diff, list changed files, feed both to an LLM that generates a pull request title and description. Save the output as an artifact.

**Skills practiced**: `exec:`, `infer:`, structured output, artifacts, `with:` bindings, DAG

**Estimated time**: 1 hour

**Solution outline**:
```yaml
tasks:
  - id: git_diff
    exec:
      command: "git diff HEAD~1 --stat"
      timeout: 10

  - id: git_log
    exec:
      command: "git log --oneline -5"
      timeout: 10

  - id: changed_files
    exec:
      command: "git diff HEAD~1 --name-only"
      timeout: 10

  - id: generate_pr
    depends_on: [git_diff, git_log, changed_files]
    with:
      diff: $git_diff
      log: $git_log
      files: $changed_files
    infer:
      prompt: |
        Generate a pull request summary:
        DIFF STATS: {{with.diff}}
        RECENT COMMITS: {{with.log}}
        CHANGED FILES: {{with.files}}
      output:
        format: json_schema
        schema:
          type: object
          properties:
            title: { type: string }
            description: { type: string }
            type: { type: string, enum: [feat, fix, refactor, docs, chore] }
          required: [title, description, type]
    artifact:
      path: output/pr-summary.json
```

---

### Project 7: Multi-Language Translator

**Description**: Accept a text input and translate it to 5 languages in parallel using `for_each:`. Then compile all translations into a single document.

**Skills practiced**: `infer:`, `for_each:`, `inputs:`, DAG merge patterns, `nika:write`

**Estimated time**: 1 hour

**Solution outline**:
```yaml
inputs:
  text: "Hello, world! Welcome to Nika."

tasks:
  - id: translate
    for_each: ["French", "Spanish", "Japanese", "Arabic", "Russian"]
    concurrency: 5
    infer:
      prompt: "Translate to {{with.item}}: {{inputs.text}}"
      temperature: 0.1

  - id: compile
    depends_on: [translate]
    with:
      translations: $translate
    invoke:
      tool: "nika:write"
      params:
        file_path: "output/translations.md"
        content: |
          # Translations
          Original: {{inputs.text}}

          {{with.translations}}
```

---

### Project 8: Meeting Notes Processor

**Description**: Read a meeting transcript file, extract action items with structured output, identify owners, and generate a follow-up email draft.

**Skills practiced**: `nika:read`, `infer:` with structured output, multi-task DAG, artifacts

**Estimated time**: 1.5 hours

---

### Project 9: Website SEO Snapshot

**Description**: Fetch a URL's metadata, links, and content. Analyze each aspect with an LLM. Combine into a single SEO report with scores.

**Skills practiced**: `fetch:` extraction (3 modes), parallel `infer:`, structured output, DAG merge

**Estimated time**: 1.5 hours

---

### Project 10: Changelog Generator

**Description**: Parse git commits since the last tag, classify them by type (feat, fix, chore), and generate a formatted Markdown changelog.

**Skills practiced**: `exec:`, `infer:` with structured output, pipe transforms, artifacts

**Estimated time**: 1 hour

---

## Advanced Projects (5-10 tasks)

### Project 11: Content Pipeline

**Description**: A complete content creation pipeline:
1. Research a topic by scraping 3 URLs
2. Generate an outline from the research
3. Write 4 sections in parallel
4. Review and score each section
5. Assemble the final article
6. Generate social media posts from the article

**Skills practiced**: `fetch: extract: markdown`, `infer:`, `for_each:`, DAG fan-out/fan-in, structured output, artifacts, multi-provider

**Estimated time**: 2-3 hours

**Solution outline**:
```yaml
inputs:
  topic: "The future of declarative AI workflows"
  urls:
    - "https://example.com/article1"
    - "https://example.com/article2"
    - "https://example.com/article3"

tasks:
  # Fan-out: scrape 3 URLs in parallel
  - id: scrape
    for_each: "{{inputs.urls}}"
    concurrency: 3
    fetch:
      url: "{{with.item}}"
      extract: markdown

  # Generate outline from research
  - id: outline
    depends_on: [scrape]
    with:
      research: $scrape
    infer:
      prompt: "Create a 4-section outline for: {{inputs.topic}}\nResearch:\n{{with.research}}"
      output:
        format: json_schema
        schema:
          type: object
          properties:
            sections:
              type: array
              items:
                type: object
                properties:
                  title: { type: string }
                  key_points: { type: array, items: { type: string } }

  # Fan-out: write 4 sections in parallel
  - id: write_sections
    depends_on: [outline]
    for_each: "{{with.outline.sections}}"
    concurrency: 4
    infer:
      prompt: "Write section '{{with.item.title}}' covering: {{with.item.key_points}}"

  # Fan-in: assemble final article
  - id: assemble
    depends_on: [write_sections]
    with:
      sections: $write_sections
    infer:
      prompt: "Assemble these sections into a cohesive article:\n{{with.sections}}"
    artifact:
      path: output/article.md

  # Generate social posts from the article
  - id: social
    depends_on: [assemble]
    with:
      article: $assemble
    infer:
      prompt: "Generate 3 social media posts (Twitter, LinkedIn, Reddit) from:\n{{with.article}}"
    artifact:
      path: output/social-posts.md
```

---

### Project 12: Image Asset Pipeline

**Description**: Download 5 product images, generate thumbnails, extract dominant colors, create a visual manifest with metadata, and describe each with vision.

**Skills practiced**: `fetch: response: binary`, `nika:import`, `nika:thumbnail`, `nika:dominant_color`, vision (`infer: content:`), `for_each:`, artifacts

**Estimated time**: 2 hours

---

### Project 13: API Documentation Generator

**Description**: Fetch an OpenAPI spec, parse endpoints, generate reference documentation for each endpoint in parallel, and compile into a complete API reference.

**Skills practiced**: `fetch:`, `infer:` with structured output, `for_each:`, DAG, artifacts, `nika:write`

**Estimated time**: 2 hours

---

### Project 14: Competitive Intelligence Report

**Description**: Scrape 3 competitor websites, extract metadata and links, analyze positioning with LLMs, generate comparison scores, and produce an executive summary.

**Skills practiced**: `fetch:` extraction (metadata, links, markdown), parallel processing, `infer:` with structured output, multi-provider, artifacts

**Estimated time**: 2-3 hours

---

### Project 15: Automated Code Review

**Description**: Capture a git diff, analyze for bugs, security issues, and style violations. Generate a structured report with severity levels. Flag critical issues.

**Skills practiced**: `exec:`, `infer:` with structured output and guardrails, agent for deep analysis, artifacts

**Estimated time**: 2 hours

---

## Expert Projects (10+ tasks, multi-agent)

### Project 16: Full-Stack Blog Platform

**Description**: A complete blog automation system:
1. Accept a topic via `inputs:`
2. Research with web scraping (3 sources)
3. Generate SEO-optimized outline
4. Write article sections in parallel
5. Self-review with scoring agent
6. Generate featured image description
7. Extract keywords for SEO
8. Create social media calendar (5 platforms)
9. Generate email newsletter version
10. Produce final asset package with manifest

**Skills practiced**: Every verb, every binding pattern, agents with guardrails, for_each, artifacts, multi-provider, structured output, media tools

**Estimated time**: 4-6 hours

---

### Project 17: Multi-Agent Research System

**Description**: Build a research system with 4 specialized agents:
1. **Scout Agent**: Discovers and scrapes relevant URLs
2. **Analyst Agent**: Extracts key data and identifies patterns
3. **Synthesizer Agent**: Combines findings into coherent narratives
4. **Editor Agent**: Reviews, fact-checks, and polishes the final report

Each agent has specific guardrails and passes output to the next via `with:` bindings.

**Skills practiced**: Agent chaining, guardrails (length, regex, schema, LLM), completion modes, token budgets, multi-provider

**Estimated time**: 4-5 hours

---

### Project 18: Media Processing Hub

**Description**: A comprehensive media processing system:
1. Accept a directory of images via `inputs:`
2. Import all images into CAS
3. For each image: thumbnail, optimize, extract metadata, compute thumbhash, get dominant colors
4. Describe each image with vision
5. Generate a searchable HTML gallery
6. Create a JSON manifest with all metadata
7. Produce a PDF contact sheet with `nika:chart`

**Skills practiced**: All 24 media tools, `for_each:`, vision, `nika:pipeline`, CAS, artifacts

**Estimated time**: 3-4 hours

---

### Project 19: DevOps Dashboard Generator

**Description**: A comprehensive DevOps health check:
1. Git statistics (commits, branches, contributors)
2. Dependency audit (cargo audit, npm audit)
3. Docker status (containers, images, volumes)
4. SSL certificate checks for 5 domains
5. DNS lookup validation
6. API endpoint health checks
7. Resource usage analysis
8. Generate structured JSON report
9. Produce HTML dashboard
10. Send summary notification (mock)

**Skills practiced**: `exec:`, `fetch:`, parallel DAG (10+ parallel streams), `for_each:`, structured output, `nika:write`, complex merge patterns

**Estimated time**: 3-4 hours

---

### Project 20: AI Content Factory

**Description**: The ultimate workflow -- a complete content factory that takes a single brief and produces an entire marketing campaign:
1. Market research (3 competitor analyses)
2. Target audience profiling
3. Brand voice calibration
4. Blog post (research, outline, write, review)
5. Email sequence (3 emails, A/B variants)
6. Social media calendar (7 days, 4 platforms)
7. Landing page copy
8. Ad copy variants (5 versions)
9. FAQ generation
10. SEO keyword mapping
11. Content calendar with scheduling
12. Quality review agent with comprehensive guardrails
13. Final asset manifest

This project uses all 5 verbs, all binding patterns, 3+ providers, agents with guardrails, `for_each:` for parallel generation, media tools for visual assets, and comprehensive artifact management.

**Skills practiced**: Everything Nika offers.

**Estimated time**: 6-8 hours

---

## Project Selection Guide

| Skill Level | Start With | Then Try | Challenge Yourself |
|-------------|-----------|---------|-------------------|
| Just finished Level 4 | Projects 1-5 | Projects 6-7 | Project 10 |
| Just finished Level 8 | Projects 6-10 | Projects 11-12 | Project 15 |
| Just finished Level 12 | Projects 11-15 | Projects 16-18 | Project 20 |
| Expert | Projects 16-20 | Combine and extend | Invent your own |

---

*"Theory without practice is empty. Practice without theory is blind. Build things."*
