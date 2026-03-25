# GitHub README -- Nika

> The definitive GitHub README for the Nika repository.
> Designed for maximum clarity, quick onboarding, and technical credibility.

---

<!-- BEGIN README CONTENT -->

<div align="center">

# Nika

**Semantic YAML workflow engine for AI tasks**

[![Crates.io](https://img.shields.io/crates/v/nika.svg)](https://crates.io/crates/nika)
[![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-8100%2B-brightgreen.svg)]()
[![Rust](https://img.shields.io/badge/rust-1.86%2B-orange.svg)](https://www.rust-lang.org)
[![Schema](https://img.shields.io/badge/schema-nika%2Fworkflow%400.12-purple.svg)]()

5 verbs. 22 providers. 24 media tools. One YAML file.

[Quick Start](#quick-start) | [The 5 Verbs](#the-5-verbs) | [Install](#installation) | [Course](#learn-with-the-built-in-course) | [Docs](#documentation)

</div>

---

## What is Nika?

Nika is a workflow engine that lets you orchestrate AI tasks using declarative YAML. Five semantic verbs -- `infer:`, `exec:`, `fetch:`, `invoke:`, and `agent:` -- compose into DAG-scheduled pipelines with automatic dependency resolution, parallel execution, and multi-provider LLM support. Written in 451K lines of Rust, it compiles to a single binary with zero runtime dependencies.

```yaml
# hello.nika.yaml
schema: nika/workflow@0.12

tasks:
  - id: research
    fetch:
      url: https://api.example.com/trends
      extract: markdown

  - id: analyze
    with: { data: $research }
    infer:
      model: claude-sonnet-4-20250514
      prompt: "Summarize the key trends: {{with.data}}"
      structured:
        schema:
          type: object
          properties:
            trends: { type: array, items: { type: string } }
            summary: { type: string }
          required: [trends, summary]

  - id: report
    with: { analysis: $analyze }
    exec:
      command: 'echo "{{with.analysis.summary}}" > report.md'
      shell: true
```

```bash
nika run hello.nika.yaml
```

---

## Feature Grid

| Category | What You Get |
|----------|-------------|
| **Verbs** | `infer:` `exec:` `fetch:` `invoke:` `agent:` -- 5 verbs for any task |
| **Providers** | Claude, GPT-4o, Gemini, Mistral, Groq, DeepSeek, xAI, Perplexity + local GGUF |
| **Execution** | DAG scheduling, parallel tasks, dependency resolution, fail-fast |
| **Output** | JSON Schema validation, structured extraction, pipe transforms |
| **Agents** | Multi-turn loops, guardrails, extended thinking, tool calling, spawn |
| **Media** | 24 tools: thumbnail, convert, metadata, chart, C2PA, QR validation |
| **MCP** | Native Model Context Protocol client, retry/reconnect, builtin routing |
| **Fetch** | 9 extract modes: markdown, article, text, selector, metadata, links, jsonpath, feed, llm_txt |
| **Security** | 28-pattern command blocklist, env validation, path traversal protection |
| **TUI** | 3 views (Studio, Command, Control), live DAG visualization, cost tracking |
| **LSP** | Language server for VS Code, Neovim -- completions, diagnostics, hover |
| **Course** | 12 levels, 44 exercises, progressive hints, auto-validation |
| **Events** | 39 event types, NDJSON trace writer, full observability |
| **Testing** | 8,100+ tests, zero clippy warnings, insta snapshots |

---

## Quick Start

### Install

```bash
# From crates.io
cargo install nika

# From Homebrew (macOS/Linux)
brew install supernovae-st/tap/nika

# From source
git clone https://github.com/supernovae-st/nika.git
cd nika/tools
cargo install --path nika
```

### Your First Workflow

```yaml
# summarize.nika.yaml
schema: nika/workflow@0.12

tasks:
  - id: fetch_page
    fetch:
      url: https://en.wikipedia.org/wiki/Rust_(programming_language)
      extract: article

  - id: summarize
    with: { content: $fetch_page }
    infer: "Summarize this in 3 bullet points: {{with.content}}"
```

```bash
# Check the workflow is valid
nika check summarize.nika.yaml

# Run it
nika run summarize.nika.yaml
```

### Your First Multi-Model Workflow

```yaml
# multi-model.nika.yaml
schema: nika/workflow@0.12

tasks:
  - id: research
    infer:
      provider: groq
      model: llama-3.3-70b-versatile
      prompt: "List the top 5 trends in AI infrastructure in 2026"

  - id: deep_analysis
    with: { trends: $research }
    infer:
      provider: claude
      model: claude-sonnet-4-20250514
      prompt: |
        Analyze each trend in depth:
        {{with.trends}}

        For each trend, explain:
        1. Why it matters
        2. Who benefits
        3. What to watch for
      structured:
        schema:
          type: object
          properties:
            analyses:
              type: array
              items:
                type: object
                properties:
                  trend: { type: string }
                  impact: { type: string }
                required: [trend, impact]
```

Groq provides fast, cheap initial research. Claude provides deep, nuanced analysis. Same workflow, optimal cost.

---

## The 5 Verbs

Every task in Nika maps to exactly one verb:

### `infer:` -- LLM Generation

```yaml
- id: generate
  infer:
    provider: claude
    model: claude-sonnet-4-20250514
    prompt: "Write a product description for {{with.product}}"
    temperature: 0.7
    structured:
      schema:
        type: object
        properties:
          headline: { type: string }
          body: { type: string }
        required: [headline, body]
```

Supports vision/multimodal content, extended thinking, streaming, and structured output with JSON Schema validation.

### `exec:` -- Shell Commands

```yaml
- id: build
  exec:
    command: "cargo build --release"
    timeout: 120
```

Secure by default: 28-pattern command blocklist prevents dangerous operations. `shell: false` (default) uses shlex parsing for safety.

### `fetch:` -- HTTP Requests

```yaml
- id: scrape
  fetch:
    url: https://news.ycombinator.com
    extract: article
    selector: ".storylink"
```

9 extract modes: `markdown`, `article`, `text`, `selector`, `metadata`, `links`, `jsonpath`, `feed`, `llm_txt`. Binary mode stores responses in content-addressable storage.

### `invoke:` -- MCP Tool Calls

```yaml
- id: resize_image
  invoke:
    tool: nika:thumbnail
    params:
      hash: "{{with.image_hash}}"
      width: 800
      height: 600
      quality: 85
```

Calls any MCP tool -- built-in media tools (`nika:*`), external MCP servers, or NovaNet knowledge graph tools.

### `agent:` -- Multi-Turn Loops

```yaml
- id: coding_agent
  agent:
    model: claude-sonnet-4-20250514
    prompt: "Implement the feature described in {{with.spec}}"
    tools: [nika:read, nika:write, nika:edit, nika:glob, nika:grep]
    max_turns: 15
    guardrails:
      - type: length
        max: 50000
    completion:
      mode: explicit
    limits:
      max_cost_usd: 1.00
```

Agents run multi-turn tool-calling loops with guardrails, cost limits, and configurable completion conditions. Sub-agent spawning with depth limits.

---

## Architecture

```
                    YAML Source
                        |
                   +---------+
                   | Phase 1 |  Raw AST (marked_yaml, spans)
                   +---------+
                        |
                   +---------+
                   | Phase 2 |  Analyzed AST (validated, interned TaskIds)
                   +---------+
                        |
                   +---------+
                   |   DAG   |  Dependency graph, cycle detection
                   +---------+
                        |
                +---------------+
                |    Runtime    |  tokio tasks, JoinSet, CancellationToken
                +-------+-------+
               /    |       |    \
          infer:  exec:  fetch:  invoke:  agent:
              \    |       |    /
                +-------+-------+
                |   Egghead     |  DashMap concurrent store
                +---------------+
                        |
                +---------------+
                |    Events     |  39 EventKind, NDJSON traces
                +---------------+
```

### Workspace Crates (10)

| Crate | Lines | Role |
|-------|------:|------|
| `nika` | 2K | CLI entry point |
| `nika-engine` | 134K | Execution engine (embeddable) |
| `nika-tui` | 92K | Terminal UI (ratatui) |
| `nika-core` | 23K | AST, types, catalogs (zero I/O) |
| `nika-cli` | 8K | CLI subcommands |
| `nika-mcp` | 9K | MCP client (rmcp) |
| `nika-lsp-core` | 9K | LSP intelligence |
| `nika-event` | 4K | Event log, trace writer |
| `nika-media` | 3.5K | CAS store, media processor |
| `nika-lsp` | 2.5K | LSP binary |

### Brain + Body: Nika + NovaNet

```
NovaNet (Brain)              MCP Protocol              Nika (Body)
+-- Knowledge Graph    <=========================>     +-- YAML Workflows
+-- NodeClasses                                        +-- 5 Verbs
+-- ArcClasses                                         +-- DAG Execution
+-- MCP Tools                                          +-- 22 Providers
```

Nika connects to NovaNet (a Neo4j-backed knowledge graph) exclusively via MCP. Zero Cypher in Nika -- all graph operations go through `invoke:` with NovaNet MCP tools.

---

## Installation

### Requirements

- Rust 1.86+ (for building from source)
- One or more LLM API keys (e.g., `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`)

### From crates.io

```bash
cargo install nika
```

### From Homebrew

```bash
brew install supernovae-st/tap/nika
```

### From GitHub Releases

Download pre-built binaries for macOS (arm64, x86_64) and Linux (x86_64) from the [Releases page](https://github.com/supernovae-st/nika/releases).

### From Source

```bash
git clone https://github.com/supernovae-st/nika.git
cd nika/tools
cargo install --path nika
```

### Verify Installation

```bash
nika --version
# nika 0.42.0

nika provider list
# Shows which API keys are configured
```

---

## Learn with the Built-in Course

Nika ships with a 12-level, 44-exercise interactive course:

```bash
mkdir learn-nika && cd learn-nika
nika init --course
```

### The Liberation Journey

| Level | Name | What You Learn |
|------:|------|---------------|
| 1 | Spark | First workflow, basic infer |
| 2 | Kindle | Variables, bindings, templates |
| 3 | Signal | Fetch verb, HTTP requests |
| 4 | Echo | Exec verb, shell commands |
| 5 | Bridge | Multi-task workflows, DAG basics |
| 6 | Prism | Structured output, JSON Schema |
| 7 | Current | MCP and invoke verb |
| 8 | Cascade | Complex DAG patterns, parallelism |
| 9 | Horizon | Agent verb, tool calling |
| 10 | Storm | Multi-model, cost optimization |
| 11 | Aurora | Media pipeline, vision |
| 12 | Nova | Full orchestration, everything combined |

### Course Commands

```bash
nika course status          # Constellation progress map
nika course next            # Open next exercise
nika course check           # Validate all exercises
nika course check 3         # Validate level 3
nika course hint exercise   # Progressive hints (3 tiers)
nika course run exercise    # Run a specific exercise
nika course watch           # Auto-check on file save
```

---

## Showcase Workflows

200+ ready-to-use workflows covering real-world scenarios:

```bash
nika showcase list                    # Browse all workflows
nika showcase extract summarizer      # Extract to current dir
```

---

## Media Pipeline

24 built-in media tools accessible via `invoke: nika:*`:

### Tier 1 -- Always On
| Tool | Description |
|------|-------------|
| `nika:import` | Import files into content-addressable storage |
| `nika:dimensions` | Image dimensions from headers (~0.1ms) |
| `nika:thumbhash` | 25-byte image placeholder |
| `nika:dominant_color` | Color palette extraction |
| `nika:pipeline` | Chain operations in-memory |

### Tier 2 -- Default
| Tool | Description |
|------|-------------|
| `nika:thumbnail` | SIMD-accelerated resize (Lanczos3) |
| `nika:convert` | Format conversion (PNG/JPEG/WebP) |
| `nika:strip` | Remove EXIF metadata |
| `nika:metadata` | Universal metadata extraction |
| `nika:optimize` | Lossless PNG optimization (oxipng) |
| `nika:svg_render` | SVG to PNG rasterization (resvg) |

### Tier 3 -- Opt-in
| Tool | Description |
|------|-------------|
| `nika:phash` | Perceptual image hashing |
| `nika:compare` | Visual comparison |
| `nika:pdf_extract` | PDF text extraction |
| `nika:chart` | Bar/line/pie charts from JSON |
| `nika:provenance` | C2PA content credentials |
| `nika:verify` | C2PA manifest verification |
| `nika:qr_validate` | QR decode + scan score |
| `nika:quality` | Image quality assessment (DSSIM) |

---

## Comparison

| Feature | Nika | LangChain | Dify | n8n | Temporal |
|---------|:----:|:---------:|:----:|:---:|:--------:|
| Language | Rust | Python | Python/TS | TypeScript | Go |
| Paradigm | Declarative YAML | Imperative SDK | Visual Builder | Visual + Code | Code SDK |
| LLM Providers | 22 | 50+ | 15+ | 10+ | N/A |
| Learning Curve | 5 verbs | SDK + Python | Low (visual) | Low (visual) | High |
| Performance | Native binary | Python runtime | Docker stack | Node.js | Go binary |
| AI-Specific | Purpose-built | Purpose-built | Purpose-built | General | General |
| Media Tools | 24 built-in | None | None | Via plugins | None |
| MCP Support | Native | Via adapter | None | None | None |
| Course System | 44 exercises | None | None | None | None |
| Agent Loops | Built-in | Built-in | Built-in | Via code | Via code |
| Structured Output | JSON Schema | Pydantic | JSON Schema | JSON Schema | N/A |
| DAG Scheduling | Automatic | Manual | Visual | Visual | Manual |
| License | AGPL-3.0 | MIT | Apache-2.0 | Sustainable Use | MIT |
| Deployment | Single binary | pip + deps | Docker compose | Docker | Binary + deps |

---

## CLI Reference

```bash
# Core commands
nika run workflow.nika.yaml       # Execute a workflow
nika check workflow.nika.yaml     # Validate without running
nika ui                           # Launch Terminal UI

# Provider management
nika provider list                # Show configured providers

# Course system
nika init --course                # Generate learning course
nika init --minimal               # Minimal scaffold (5 workflows)
nika course status                # Progress map
nika course next                  # Next exercise
nika course check [level]         # Validate exercises
nika course hint [exercise]       # Progressive hints
nika course watch                 # Auto-check on save

# Showcase
nika showcase list                # Browse 200+ workflows
nika showcase extract <name>      # Extract workflow

# Setup
nika setup                        # IDE integration
```

---

## Configuration

### Environment Variables

```bash
# LLM Providers (set one or more)
export ANTHROPIC_API_KEY="sk-ant-..."
export OPENAI_API_KEY="sk-..."
export MISTRAL_API_KEY="..."
export GROQ_API_KEY="gsk_..."
export DEEPSEEK_API_KEY="sk-..."
export GEMINI_API_KEY="..."
export XAI_API_KEY="xai-..."
export PERPLEXITY_API_KEY="pplx-..."
```

### Auto-Detection

Nika automatically detects available providers by checking environment variables. Priority order: Anthropic > OpenAI > Mistral > Groq > DeepSeek > Gemini.

---

## Data Flow and Bindings

Nika's binding system connects tasks through `with:` declarations:

```yaml
tasks:
  - id: fetch_users
    fetch:
      url: https://api.example.com/users
      extract: jsonpath
      selector: "$.data[*]"

  - id: enrich
    with: { users: $fetch_users }
    infer:
      model: claude-sonnet-4-20250514
      prompt: "Categorize these users by engagement level: {{with.users}}"

  - id: report
    with:
      enriched: $enrich
      raw: $fetch_users
    exec:
      command: 'python3 generate_report.py --data "{{with.enriched}}"'
      shell: true
```

**Key concepts:**
- `$task_id` references another task's output
- `{{with.alias}}` templates are resolved at runtime
- Pipe transforms: `{{with.data | uppercase | trim}}` for inline processing
- `context:` block for static file content: `{{context.files.config}}`
- `inputs:` for runtime parameters: `{{inputs.locale}}`
- Dependencies are automatic -- no `depends_on:` needed when using `with:`

### Structured Output

Every `infer:` task can validate its output against a JSON Schema:

```yaml
- id: extract_entities
  infer:
    model: claude-sonnet-4-20250514
    prompt: "Extract people and organizations from: {{with.text}}"
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
                title: { type: string }
              required: [name]
          organizations:
            type: array
            items: { type: string }
        required: [people]
```

If the LLM response doesn't conform to the schema, Nika retries automatically. Downstream tasks receive guaranteed-valid JSON.

### Agent Guardrails

Agents support safety constraints out of the box:

```yaml
- id: writer
  agent:
    model: claude-sonnet-4-20250514
    prompt: "Write a blog post about {{with.topic}}"
    guardrails:
      - type: length
        min: 500
        max: 5000
      - type: regex
        pattern: "^(?!.*TODO).*$"
        message: "Output must not contain TODO markers"
    completion:
      mode: explicit
    limits:
      max_turns: 20
      max_cost_usd: 0.50
```

Three guardrail types: `length` (character count), `regex` (pattern match), and custom validators. The `limits:` block caps total spend and turn count.

---

## Error Handling

Nika uses structured error codes (NIKA-000 through NIKA-319) with source span information:

```
NIKA-020: Cycle detected in task graph
  --> pipeline.nika.yaml:15:5
   |
15 |   - id: analyze
   |     ^^^^^^^^^^^^ this task
   |
  note: cycle: analyze -> summarize -> analyze
```

**Error code ranges:**

| Range | Category |
|-------|----------|
| 000-009 | Workflow structure |
| 010-019 | Schema validation |
| 020-029 | DAG (cycles, dependencies) |
| 030-039 | Provider (connection, auth) |
| 040-049 | Template resolution |
| 050-059 | Security violations |
| 060-069 | Output validation |
| 100-109 | MCP connection |
| 110-119 | Agent + guardrails |
| 200-214 | Built-in tools |
| 251-259 | Media pipeline |
| 300-309 | Structured output |
| 310-319 | Course system |

The `nika check` command validates workflows without executing them, catching errors at parse time rather than runtime.

---

## Security

Nika takes security seriously:

- **Command blocklist:** 28 patterns block dangerous shell operations (rm -rf, chmod 777, etc.)
- **Shell-free default:** `exec:` uses shlex parsing by default -- no shell injection
- **CAS for media:** Files referenced by content hash, not path -- prevents traversal attacks
- **Path validation:** Import operations validate against directory traversal
- **Environment validation:** Sensitive env vars are checked but never logged
- **SVG sanitization:** SVGs are sanitized before parsing to prevent XXE and script injection
- **File size limits:** 50MB default on imports and downloads
- **Agent cost limits:** Hard caps on LLM spending per agent run

---

## Fetch Extract Modes

The `fetch:` verb supports 9 post-processing modes:

```yaml
# Clean Markdown from any HTML page
fetch: { url: "https://example.com", extract: markdown }

# Main article content (Readability algorithm)
fetch: { url: "https://blog.example.com/post", extract: article }

# CSS selector extraction
fetch: { url: "https://example.com", extract: selector, selector: ".title" }

# OpenGraph, Twitter Cards, JSON-LD metadata as JSON
fetch: { url: "https://example.com", extract: metadata }

# Link classification (internal/external, nav/content/footer)
fetch: { url: "https://example.com", extract: links }

# JSONPath on JSON API responses
fetch: { url: "https://api.example.com", extract: jsonpath, selector: "$.items[*]" }

# RSS, Atom, or JSON Feed parsing
fetch: { url: "https://example.com/feed.xml", extract: feed }

# Visible text only, optionally filtered by selector
fetch: { url: "https://example.com", extract: text, selector: "main" }

# AI-era content discovery (llms.txt standard)
fetch: { url: "https://example.com", extract: llm_txt }
```

---

## Vision Support

The `infer:` verb supports multimodal content for vision-capable LLMs:

```yaml
- id: describe_image
  with: { photo: $import_photo }
  infer:
    model: claude-sonnet-4-20250514
    content:
      - type: image
        source: "{{with.photo.media[0].hash}}"
        detail: high
      - type: text
        text: "Describe what you see in this image"
```

CAS image hashes are automatically resolved to base64. Paths never leak to LLM APIs. Vision is supported on Claude, OpenAI, Mistral, Groq, Gemini, and xAI. Local vision via mistral.rs supports HuggingFace models with ISQ quantization.

---

## Contributing

We welcome contributions. See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

```bash
# Run tests (safe -- no keychain popups)
cargo test --workspace --lib

# Run clippy
cargo clippy --workspace -- -D warnings

# Run a specific crate's tests
cargo test -p nika-engine --lib
```

---

## License

Nika is licensed under [AGPL-3.0-or-later](LICENSE).

**What this means for you:**

- **Using Nika as a tool:** Free to use for any purpose -- personal, commercial, anything. Run `nika run` on your workflows with zero restrictions.
- **Modifying Nika's source:** If you modify Nika and distribute it or offer it as a service, you must share your modifications under AGPL-3.0.
- **Why AGPL:** We chose AGPL because we believe open source infrastructure should stay open. The AGPL prevents cloud providers from taking Nika, adding proprietary features, and selling it as a closed service. Your contributions benefit everyone.

---

## Credits

Built by [SuperNovae Studio](https://supernovae.studio) by Thibaut Melen.

Nika is the Greek goddess of victory -- a winged figure who crowns the triumphant. The butterfly symbol represents metamorphosis: a creature that breaks free from confinement and gains the ability to fly. In open source, victory belongs to everyone.

### Acknowledgments

Nika is built on the shoulders of the Rust ecosystem:
- [rig-core](https://github.com/0xPlaygrounds/rig) -- Multi-provider LLM abstraction
- [rmcp](https://github.com/anthropics/rmcp) -- Model Context Protocol client
- [ratatui](https://github.com/ratatui/ratatui) -- Terminal UI framework
- [mistral.rs](https://github.com/EricLBuehler/mistral.rs) -- Local model inference
- [marked_yaml](https://github.com/kinnison/marked-yaml) -- YAML with source spans
- [insta](https://github.com/mitsuhiko/insta) -- Snapshot testing

---

<div align="center">

**[Get Started](https://github.com/supernovae-st/nika)** | **[Documentation](https://github.com/supernovae-st/nika/wiki)** | **[Showcase](https://github.com/supernovae-st/nika/tree/main/showcase)** | **[Discord](https://discord.gg/supernovae)**

</div>

<!-- END README CONTENT -->
