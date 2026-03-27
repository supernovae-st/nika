# Dev.to / Medium Article -- Nika

> Long-form technical article for Dev.to, Medium, and Hashnode.
> SEO-optimized, code-heavy, developer audience.

---

<!-- BEGIN ARTICLE -->

# Why We Built a 451K-Line Rust Engine for AI Workflows (And Why You Should Care)

**Tags:** `#rust` `#ai` `#opensource` `#workflow`
**Canonical URL:** https://supernovae.studio/blog/why-we-built-nika
**Reading time:** ~15 minutes
**SEO description:** Nika is a semantic YAML workflow engine for AI tasks (v0.49.0), written in 451K lines of Rust. Learn why 5 declarative verbs replace SDK boilerplate, how multi-model routing cuts costs by 60%, and what makes this approach different from LangChain, Dify, and Temporal.

---

## The Hook

Here is an entire AI pipeline that fetches data from an API, analyzes it with Claude, validates the output against a JSON Schema, and saves the result:

```yaml
schema: nika/workflow@0.12

tasks:
  - id: data
    fetch:
      url: https://api.example.com/metrics
      extract: jsonpath
      selector: "$.results[*]"

  - id: analyze
    with: { metrics: $data }
    infer:
      model: claude-sonnet-4-20250514
      prompt: "Analyze these metrics and identify anomalies: {{with.metrics}}"
      structured:
        schema:
          type: object
          properties:
            anomalies:
              type: array
              items:
                type: object
                properties:
                  metric: { type: string }
                  deviation: { type: number }
                  severity: { type: string, enum: [low, medium, high] }
                required: [metric, deviation, severity]
          required: [anomalies]

  - id: save
    with: { report: $analyze }
    exec:
      command: 'echo "{{with.report}}" > anomaly-report.json'
      shell: true
```

Three tasks. Three verbs (`fetch:`, `infer:`, `exec:`). Automatic dependency resolution. Structured output validation. No SDK. No boilerplate. No Python runtime.

Save this as `monitor.nika.yaml` and run it:

```bash
nika run monitor.nika.yaml
```

This is **Nika** (v0.49.0) -- a semantic YAML workflow engine for AI tasks, written in 451K lines of Rust across 12 crates. Today, we're open-sourcing it under AGPL-3.0.

---

## The Problem: AI Workflow Tools Are Stuck

If you're building AI pipelines in 2026, you're probably using one of these approaches:

### Approach 1: Visual Builders (Dify, n8n, Langflow)

Drag boxes, connect arrows, configure forms. Great for the demo. Terrible for the pull request.

Visual workflows can't be diffed, reviewed in PRs, or version-controlled meaningfully. When something breaks in production, you're debugging a JSON blob that represents a visual graph. Composability is limited to "copy the whole workflow and modify it."

### Approach 2: Python SDKs (LangChain, LangGraph, CrewAI)

Write Python code that calls LLMs. Maximum flexibility. Maximum boilerplate.

Here's what a simple two-step pipeline looks like in LangChain:

```python
from langchain_anthropic import ChatAnthropic
from langchain_core.output_parsers import JsonOutputParser
from langchain_core.prompts import ChatPromptTemplate

model = ChatAnthropic(model="claude-sonnet-4-20250514")
parser = JsonOutputParser()

prompt = ChatPromptTemplate.from_messages([
    ("system", "Analyze the data and return JSON."),
    ("human", "{data}")
])

chain = prompt | model | parser
result = chain.invoke({"data": raw_data})

# Now what? Save it? Send it? Chain to another model?
# More code. More abstractions. More imports.
```

This is fine for prototyping. But when you have a team of 5 people maintaining 20 workflows, Python SDK code becomes opaque fast. What does this chain do? What model is it using? What happens on failure? You need to read the code to find out.

### Approach 3: General Workflow Platforms (Temporal, Prefect, Airflow)

These are built for data engineering, not AI. They handle durability and scheduling beautifully, but they have no concept of LLMs, structured output, MCP, vision models, or agent loops. Every AI-specific feature is a custom integration you build yourself.

### What's Missing

None of these approaches give you:

1. **Declarative AI-specific workflows** that are readable, diffable, and reviewable
2. **Multi-model routing** in a single workflow (use Claude for analysis, Groq for simple tasks)
3. **Built-in media processing** without external services
4. **MCP integration** for tool calling and knowledge graph access
5. **A single binary** with zero runtime dependencies

That's what we built.

---

## The Solution: 5 Verbs and a DAG

Nika's design is built on one observation: **every AI workflow task is one of 5 operations.**

| Verb | What It Does | Example |
|------|-------------|---------|
| `infer:` | Call an LLM | Generate text, analyze data, extract structure |
| `exec:` | Run a shell command | Build code, run tests, convert files |
| `fetch:` | Make an HTTP request | Scrape web pages, call APIs, download files |
| `invoke:` | Call an MCP tool | Process images, query databases, call services |
| `agent:` | Run a multi-turn loop | Code with tools, research with browsing |

That's it. Five verbs. Anything you can't express with these verbs, you break into sub-tasks until you can.

### Automatic DAG Scheduling

Tasks declare their dependencies through `with:` bindings:

```yaml
tasks:
  - id: fetch_news
    fetch: { url: "https://news.ycombinator.com", extract: article }

  - id: fetch_reddit
    fetch: { url: "https://reddit.com/r/programming.json", extract: jsonpath, selector: "$.data.children[*]" }

  - id: analyze
    with:
      hn: $fetch_news
      reddit: $fetch_reddit
    infer: "Compare trends between HN and Reddit: HN={{with.hn}} Reddit={{with.reddit}}"
```

Nika sees that `analyze` depends on both `fetch_news` and `fetch_reddit`, but those two don't depend on each other. So it runs them in parallel and feeds both results to `analyze` when they complete.

No explicit `parallel:` blocks. No DAG builder API. Just `with:` bindings, and the engine figures out the execution order.

### Multi-Model Routing

Different tasks need different models. A research task needs speed (Groq, Llama 3.3 70B at 300 tok/s). A quality writing task needs Claude. A simple formatting task needs DeepSeek ($0.14/1M tokens).

```yaml
tasks:
  - id: research
    infer:
      provider: groq
      model: llama-3.3-70b-versatile
      prompt: "List the top 5 trends in {{with.topic}}"

  - id: deep_analysis
    with: { trends: $research }
    infer:
      provider: claude
      model: claude-sonnet-4-20250514
      prompt: "Analyze each trend in depth: {{with.trends}}"

  - id: format
    with: { analysis: $deep_analysis }
    infer:
      provider: deepseek
      model: deepseek-chat
      prompt: "Format as HTML: {{with.analysis}}"
```

Same workflow. Three providers. Each task uses the optimal model for its job. Cost savings vs. single-model: roughly 60%.

### Structured Output

LLM responses are validated against JSON Schema:

```yaml
- id: extract
  infer:
    model: claude-sonnet-4-20250514
    prompt: "Extract entities from: {{with.text}}"
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
                role: { type: string }
              required: [name]
          organizations:
            type: array
            items: { type: string }
        required: [people]
```

If the LLM response doesn't match the schema, Nika retries automatically. No more "hope the LLM returns valid JSON."

---

## Deep Dive: What Makes Nika Different

### 1. Two-Phase AST with Source Spans

Nika doesn't just parse YAML and execute it. It builds a proper Abstract Syntax Tree in two phases:

**Phase 1: Raw AST** -- Parses YAML with `marked_yaml` to preserve source spans (file, line, column). Every element in the AST knows where it came from in the source file.

**Phase 2: Analyzed AST** -- Semantic validation: TaskId interning (string deduplication), dependency resolution, cycle detection, provider validation, template variable checking. If anything is wrong, you get an error with the exact line number.

This means error messages like:

```
NIKA-020: Cycle detected in task graph
  --> pipeline.nika.yaml:15:5
   |
15 |   - id: analyze
   |     ^^^^^^^^^^^^ this task
   |
  note: cycle: analyze -> summarize -> analyze
```

Not "RuntimeError: maximum recursion depth exceeded."

### 2. Content-Addressable Storage for Media

When you import a file into Nika's media pipeline, it goes into a CAS (Content-Addressable Storage). Files are referenced by their content hash, never by path.

```yaml
- id: import_photo
  invoke:
    tool: nika:import
    params:
      path: "./photo.jpg"
  # Returns: { hash: "sha256:a1b2c3..." }

- id: thumbnail
  with: { photo: $import_photo }
  invoke:
    tool: nika:thumbnail
    params:
      hash: "{{with.photo.hash}}"
      width: 800
```

This design:
- **Prevents path traversal attacks** -- tools only see content hashes, not file paths
- **Enables deduplication** -- same content = same hash = stored once
- **Makes pipelines reproducible** -- same input hash = same output

### 3. 24 Built-in Media Tools

Nika ships with 24 media tools accessible via `invoke: nika:*`:

```yaml
# Image operations
invoke: nika:thumbnail   # SIMD-accelerated resize (Lanczos3)
invoke: nika:convert     # PNG/JPEG/WebP conversion
invoke: nika:optimize    # Lossless PNG optimization (oxipng)
invoke: nika:svg_render  # SVG to PNG (resvg)
invoke: nika:strip       # Remove EXIF metadata
invoke: nika:metadata    # Extract metadata
invoke: nika:dimensions  # Image size (~0.1ms)
invoke: nika:thumbhash   # 25-byte placeholder
invoke: nika:dominant_color  # Color palette

# Analysis
invoke: nika:phash       # Perceptual hash
invoke: nika:compare     # Visual comparison
invoke: nika:quality     # DSSIM quality assessment
invoke: nika:qr_validate # QR code scanning

# Documents
invoke: nika:pdf_extract # PDF text extraction
invoke: nika:chart       # Charts from JSON data

# Provenance
invoke: nika:provenance  # C2PA signing
invoke: nika:verify      # C2PA verification

# Pipeline
invoke: nika:pipeline    # Chain operations in-memory
```

Zero external services. Zero API keys. Zero Docker containers. Just built-in Rust crates.

### 4. MCP-Native Architecture

The Model Context Protocol (MCP) is becoming the standard for AI tool calling. Nika speaks MCP natively via `rmcp`:

```yaml
mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "-p", "novanet-mcp"]

tasks:
  - id: context
    invoke:
      tool: novanet_context
      server: novanet
      params:
        focus_key: "qr-code-ai"
        locale: "fr-FR"
```

Any MCP server works. Databases, APIs, knowledge graphs, custom tools -- if it speaks MCP, Nika can call it.

### 5. Agent Loops with Guardrails

The `agent:` verb runs multi-turn tool-calling loops with safety built in:

```yaml
- id: coding_agent
  agent:
    model: claude-sonnet-4-20250514
    prompt: "Fix the failing tests in src/parser.rs"
    tools:
      - nika:read
      - nika:write
      - nika:edit
      - nika:glob
      - nika:grep
    max_turns: 15
    guardrails:
      - type: length
        max: 50000
      - type: regex
        pattern: "^(?!.*rm -rf).*$"
        message: "Cannot use rm -rf"
    completion:
      mode: explicit
    limits:
      max_cost_usd: 1.00
```

The agent can read files, search code, and edit files -- but it can't exceed 50K characters, can't run destructive commands, and can't spend more than $1.00. When it decides it's done, it explicitly signals completion.

### 6. Nine Fetch Extract Modes

The `fetch:` verb isn't just "make an HTTP request." It has 9 extract modes for post-processing HTML:

```yaml
# Clean Markdown from any webpage
fetch: { url: "https://example.com", extract: markdown }

# Main article content (Readability algorithm)
fetch: { url: "https://blog.example.com/post", extract: article }

# CSS selector extraction
fetch: { url: "https://example.com", extract: selector, selector: ".product-title" }

# OpenGraph, Twitter Cards, JSON-LD metadata
fetch: { url: "https://example.com", extract: metadata }

# RSS/Atom/JSON Feed parsing
fetch: { url: "https://example.com/feed.xml", extract: feed }

# JSONPath on API responses
fetch: { url: "https://api.example.com/data", extract: jsonpath, selector: "$.results[*].name" }

# AI-era content discovery
fetch: { url: "https://example.com", extract: llm_txt }
```

Web scraping without a headless browser. Data extraction without Beautiful Soup.

---

## The Numbers

| Metric | Value |
|--------|-------|
| Lines of Rust | 451K |
| Workspace crates | 12 |
| Tests passing | 8,300+ |
| Clippy warnings | 0 |
| Unsafe blocks | 0 |
| LLM providers | 9 |
| Built-in media tools | 24 |
| Error codes | 300+ |
| Event types | 39 |
| Fetch extract modes | 9 |
| Course exercises | 44 |
| Showcase workflows | 115 |
| TUI views | 3 |
| MCP aliases | 100+ |

---

## The Terminal UI

We invested 92K lines of Rust (using `ratatui`) into a full terminal UI. This might seem excessive for a workflow engine, but watching your AI pipeline execute in real-time changes how you debug and optimize.

The TUI has three views (accessible via `1/s`, `2/c`, `3/x`):

**Studio View (1/s):** A live DAG visualization showing all tasks, their states (pending, running, completed, failed), data flow connections, and parallel execution. You can see which tasks are running simultaneously and which are waiting for dependencies.

**Command View (2/c):** Streaming LLM output, token by token, for all active tasks. Real-time cost tracking per task and per provider. When you're debugging why a prompt isn't producing good output, watching the tokens flow is invaluable.

**Control View (3/x):** System overview -- provider status, event log, overall progress, total cost, and timing information. This is your dashboard for understanding the health of a workflow run.

The TUI isn't just nice to have. It's a core part of the development workflow: write YAML, run with `nika ui`, watch the DAG execute, identify bottlenecks, adjust, repeat.

---

## Security: Not an Afterthought

AI workflow engines have a unique security surface: they call LLMs (which can hallucinate file paths), execute shell commands (which can be dangerous), and process arbitrary media (which can contain exploits).

Nika addresses each attack vector:

**Shell injection prevention.** The `exec:` verb uses shlex parsing by default (`shell: false`), meaning commands are tokenized without a shell interpreter. No glob expansion, no pipe injection, no command chaining. A 28-pattern blocklist catches dangerous operations even when `shell: true` is enabled.

**Path traversal in media.** All media operations go through content-addressable storage. Tools receive content hashes, not file paths. Even if an LLM hallucinates a path like `../../../../etc/passwd`, it never reaches the file system -- the CAS layer only resolves content by hash.

**SVG sanitization.** SVG files can contain `<script>` tags, external entity references, and other attack vectors. Nika sanitizes all SVG input before parsing with resvg.

**Agent cost limits.** Without limits, an agent can rack up significant LLM costs. The `limits: { max_cost_usd: 1.00 }` field provides a hard cap. Combined with `max_turns`, this prevents runaway agents.

**Environment variable handling.** API keys are validated (present/absent) but never logged, never included in event traces, and never passed to LLMs.

---

## The Learning Path

One thing we're particularly proud of: Nika ships with a built-in interactive course.

```bash
nika init --course
```

This generates 12 levels of progressive exercises -- 44 in total. Each exercise is a partially-complete `.nika.yaml` file with `# TODO` markers. You fill in the blanks, and Nika validates your solution.

The levels follow a "liberation journey" -- each level frees a new capability:

| Level | Name | What You Learn |
|------:|------|---------------|
| 1 | Spark | Your first infer: task |
| 2 | Kindle | Variables and templates |
| 3 | Signal | The fetch: verb |
| 4 | Echo | The exec: verb |
| 5 | Bridge | Multi-task DAGs |
| 6 | Prism | Structured output |
| 7 | Current | MCP and invoke: |
| 8 | Cascade | Parallel execution |
| 9 | Horizon | Agent loops |
| 10 | Storm | Multi-model routing |
| 11 | Aurora | Media pipeline |
| 12 | Nova | Everything combined |

Each level has:
- **Progressive hints** (3 tiers: nudge, guide, solution)
- **Auto-validation** on file save (`nika course watch`)
- **A constellation progress map** showing your journey

We believe the best documentation teaches you to fish. The course IS the documentation.

---

## Why Rust (and What We Learned)

We could have written Nika in Python. It would have been faster to develop. It would have had access to the largest AI ecosystem. It would have been wrong.

### Why Rust was right

**Single binary deployment.** `cargo install nika` gives you one binary with everything -- engine, TUI, LSP, media tools. No Python version management, no `requirements.txt`, no Docker.

**Performance where it matters.** The media pipeline uses SIMD-accelerated Lanczos3 resampling, oxipng lossless optimization, and resvg SVG rasterization. These operations process megabytes of image data. Python would delegate to C libraries anyway -- Rust does it natively.

**Type-safe AST pipeline.** Our two-phase AST (Raw -> Analyzed -> Runtime) is enforced by the type system. You literally cannot pass unvalidated YAML to the executor. The compiler catches it. This is worth more than any test suite.

**Fearless concurrency.** DAG scheduling means parallel task execution. We use tokio's `JoinSet` with `CancellationToken` for fail-fast semantics. The `DashMap` concurrent store holds task results. In Python, this would be `asyncio` + the GIL. In Rust, the compiler proves your concurrent code is data-race-free.

### What surprised us

The TUI is 92K lines of Rust using `ratatui`. Writing a terminal UI in Rust sounds painful, but `ratatui`'s Elm-style architecture (state -> view -> update) is genuinely pleasant. The result is a 60fps terminal application with live DAG visualization, streaming LLM output, and real-time cost tracking.

### What was hard

Async Rust in a workflow engine is challenging. Lifetime annotations in closures that capture mutable state. The borrow checker fighting our dynamic DAG patterns. Complex trait bounds when threading providers through the execution pipeline.

But every fight with the compiler was a bug we didn't ship.

---

## Get Started in 60 Seconds

### Install

```bash
cargo install nika
```

### Set up a provider

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

### Create your first workflow

```yaml
# hello.nika.yaml
schema: nika/workflow@0.12

tasks:
  - id: greet
    infer: "Write a haiku about open source software"
```

### Run it

```bash
nika run hello.nika.yaml
```

### Explore further

```bash
nika init --course        # 44-exercise interactive course
nika showcase list        # 115 ready-made workflows
nika ui                   # Terminal UI (3 views)
```

---

## The Philosophy

Nika is named after the Greek goddess of victory -- a winged figure who crowns the triumphant. The butterfly symbol represents metamorphosis: breaking free from confinement.

We built Nika because we believe:

1. **AI workflows should be readable.** If a non-engineer can't understand what your pipeline does by reading the YAML, it's too complex.
2. **Open source should stay open.** The AGPL ensures Nika can't be strip-mined by cloud providers.
3. **Developer tools should be fast.** A Rust binary starts in milliseconds. Tests run in seconds. The TUI renders at 60fps.
4. **Constraints breed creativity.** Five verbs, not fifty. YAML, not a Turing-complete language. The limitations are features.

---

## What's Next

Nika v0.49.0 is the current release. The roadmap includes:

- **Model routing presets** -- Named model slots (default, lite, think, search) for per-task provider selection
- **Record compression** -- Compressed task results for bounded context growth
- **Orchestration mode** -- Dynamic workflow generation from a goal description
- **Context budgeting** -- Per-task token budget management
- **Persistent memory** -- Cross-session records via NovaNet knowledge graph

All of this builds on the existing 5-verb foundation. The verbs don't change. The engine gets smarter.

---

## Links

- **GitHub:** [https://github.com/supernovae-st/nika](https://github.com/supernovae-st/nika)
- **Install:** `cargo install nika`
- **License:** AGPL-3.0-or-later
- **Author:** Thibaut Melen / [SuperNovae Studio](https://supernovae.studio)

---

*What would you build with 5 verbs? Let us know in the comments.*

*If you found this useful, consider starring the [GitHub repo](https://github.com/supernovae-st/nika) -- it helps more developers find the project. And if you build something with Nika, we'd love to feature it in the showcase.*

<!-- END ARTICLE -->
