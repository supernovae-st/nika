<div align="center">

# Nika

**Semantic YAML workflow engine for AI**

[![Version](https://img.shields.io/badge/v0.37.0-7c3aed?style=flat-square&logo=semver&logoColor=white)](CHANGELOG.md)
[![Schema](https://img.shields.io/badge/schema-nika/workflow@0.12-0ea5e9?style=flat-square)](docs/schema/)
[![Rust](https://img.shields.io/badge/rust_1.86+-f97316?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/AGPL--3.0-22c55e?style=flat-square&logo=gnu&logoColor=white)](LICENSE)
[![Tests](https://img.shields.io/badge/7400+_tests-10b981?style=flat-square)](https://github.com/supernovae-st/nika/actions)

*5 verbs. 8 providers. 43 builtin tools. One YAML file.*

</div>

---

```yaml
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: research
    fetch:
      url: "https://api.semanticscholar.org/graph/v1/paper/search?query=LLM+agents&limit=5"
      extract: jsonpath
      selector: "$.data[*].title"

  - id: analyze
    with: { papers: $research }
    infer:
      prompt: "Summarize these AI papers: {{with.papers}}"
      output:
        format: json
        schema:
          type: object
          required: [summary, key_findings]
```

```bash
nika run research.nika.yaml
```

---

## Quick Start

```bash
# Install from source
cargo install --git https://github.com/supernovae-st/nika.git

# Set a provider key
export ANTHROPIC_API_KEY=sk-ant-...

# Create a workflow
cat > hello.nika.yaml << 'EOF'
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: greet
    infer: "Say hello in French, then in Japanese"
EOF

# Run it
nika run hello.nika.yaml

# Or open the TUI
nika ui hello.nika.yaml
```

---

## The 5 Verbs

Every task uses exactly one verb. That's the entire API.

### `infer:` — LLM Generation

```yaml
# Shorthand
- id: quick
  infer: "Explain quantum computing in one paragraph"

# Full form with structured output
- id: analysis
  infer:
    system: "You are a senior code reviewer."
    prompt: "Review this code: {{with.code}}"
    temperature: 0.3
    max_tokens: 2000
    output:
      format: json
      schema:
        type: object
        required: [score, issues, suggestions]

# Vision — multimodal content
- id: describe
  infer:
    content:
      - type: image
        source: "{{with.photo.media[0].hash}}"
        detail: high
      - type: text
        text: "Describe this image in detail"

# Extended thinking (Claude only)
- id: reason
  infer:
    prompt: "Solve this step by step: {{with.problem}}"
    extended_thinking: true
    thinking_budget: 8192
```

### `exec:` — Shell Commands

```yaml
# Shorthand
- id: status
  exec: "git log --oneline -5"

# Full form with timeout
- id: build
  exec:
    command: "cargo build --release"
    timeout: 120
    cwd: "./project"
```

### `fetch:` — HTTP + Extraction

```yaml
# API call with JSONPath extraction
- id: weather
  fetch:
    url: "https://api.weather.gov/points/40.7,-74.0"
    extract: jsonpath
    selector: "$.properties.forecast"

# Web scraping to clean Markdown
- id: docs
  fetch:
    url: "https://docs.example.com/guide"
    extract: markdown

# RSS feed parsing
- id: news
  fetch:
    url: "https://hnrss.org/frontpage"
    extract: feed

# Binary download to CAS
- id: download
  fetch:
    url: "https://example.com/image.png"
    response: binary
```

**9 extract modes:** `markdown` `article` `text` `selector` `metadata` `links` `jsonpath` `feed` `llm_txt`

### `invoke:` — MCP Tool Calls

```yaml
# Call any MCP server tool
- id: query
  invoke:
    mcp: novanet
    tool: read_neo4j_cypher
    params:
      query: "MATCH (n:Entity) RETURN n.name LIMIT 10"
```

### `agent:` — Multi-Turn Agentic Loops

```yaml
# Autonomous agent with tool access
- id: researcher
  agent:
    prompt: "Find and summarize recent AI safety papers"
    mcp: [web_search, filesystem]
    max_turns: 15
    guardrails:
      max_length: 5000
      schema:
        type: object
        required: [papers, summary]
    completion:
      mode: explicit  # Agent must call nika:complete
```

---

## Data Flow

### Bindings (`with:`)

Tasks pass data to downstream tasks via `with:` blocks:

```yaml
tasks:
  - id: fetch_data
    fetch: { url: "https://api.example.com/users" }

  - id: process
    with:
      users: $fetch_data                           # Reference upstream output
      count: $fetch_data.total ?? 0                # JSONPath + default
      name: $fetch_data.data[0].name ?? "Unknown"  # Nested path + fallback
    infer:
      prompt: "Found {{with.count}} users. First: {{with.name}}"
```

### Pipe Transforms (27 available)

```yaml
with:
  upper: $data | uppercase
  lower: $data | lowercase
  clean: $data | trim | lowercase
  list:  $data | sort | unique | reverse
  safe:  $data | shell_escape
  len:   $data | length
  path:  $data | jq('.results[0].name')
```

### Parallel Execution (`for_each`)

```yaml
- id: translate
  for_each: ["en-US", "fr-FR", "ja-JP", "de-DE", "ko-KR"]
  as: locale
  concurrency: 5
  infer:
    prompt: "Translate to {{each.locale}}: {{with.text}}"
```

### Dependencies

```yaml
- id: step_b
  depends_on: [step_a]         # Explicit dependency
  with: { result: $step_a }    # Implicit dependency (auto-detected)
```

---

## Providers

8 LLM providers via [rig-core](https://github.com/0xPlaygrounds/rig), plus local inference:

| Provider | Env Variable | Models |
|:---------|:-------------|:-------|
| **Claude** | `ANTHROPIC_API_KEY` | opus-4, sonnet-4, haiku-3.5 |
| **OpenAI** | `OPENAI_API_KEY` | gpt-4o, gpt-4-turbo, o1 |
| **Mistral** | `MISTRAL_API_KEY` | mistral-large, codestral |
| **Groq** | `GROQ_API_KEY` | mixtral-8x7b, llama-3 |
| **DeepSeek** | `DEEPSEEK_API_KEY` | deepseek-chat, deepseek-reasoner |
| **Gemini** | `GEMINI_API_KEY` | gemini-2.0, gemini-1.5 |
| **xAI** | `XAI_API_KEY` | grok-3, grok-2 |
| **Native** | *(local)* | Any GGUF model via mistral.rs |

```yaml
# Per-workflow default
provider: claude
model: sonnet-4

# Per-task override
tasks:
  - id: fast
    provider: groq
    model: mixtral-8x7b
    infer: "Quick answer needed"

  - id: local
    provider: native
    model: "Qwen/Qwen2.5-7B-Instruct"
    infer: "This runs entirely on your machine"
```

### Native Inference

Run models locally via [mistral.rs](https://github.com/EricLBuehler/mistral.rs) — no API keys, no network:

```bash
nika model list                                    # Browse available models
nika model download Qwen/Qwen2.5-7B-Instruct      # Download from HuggingFace
nika model vision Qwen/Qwen2.5-VL-7B-Instruct     # Vision-capable models
```

---

## Media Tools

26 builtin media tools organized in 3 tiers, accessible via `invoke: nika:*`:

### Tier 1 — Always On

| Tool | Purpose |
|:-----|:--------|
| `nika:import` | Import any file into CAS (content-addressable storage) |
| `nika:dimensions` | Image dimensions from headers (~0.1ms) |
| `nika:thumbhash` | 25-byte compact image placeholder |
| `nika:dominant_color` | Color palette extraction |
| `nika:pipeline` | Chain operations in-memory (zero intermediate files) |

### Tier 2 — Default (`media-core`)

| Tool | Purpose |
|:-----|:--------|
| `nika:thumbnail` | SIMD-accelerated resize (Lanczos3) |
| `nika:convert` | Format conversion (PNG/JPEG/WebP) |
| `nika:strip` | Remove EXIF/metadata |
| `nika:metadata` | Universal EXIF/audio/video metadata |
| `nika:optimize` | Lossless PNG optimization (oxipng) |
| `nika:svg_render` | SVG to PNG rasterization (resvg) |

### Tier 3 — Opt-In

| Tool | Feature Flag | Purpose |
|:-----|:-------------|:--------|
| `nika:phash` | `media-phash` | Perceptual image hashing |
| `nika:compare` | `media-phash` | Visual similarity comparison |
| `nika:pdf_extract` | `media-pdf` | PDF text extraction |
| `nika:chart` | `media-chart` | Bar/line/pie charts from JSON |
| `nika:provenance` | `media-provenance` | C2PA content credentials (sign) |
| `nika:verify` | `media-provenance` | C2PA verification + EU AI Act compliance |
| `nika:qr_validate` | `media-qr` | QR decode + 0-100 quality score |
| `nika:quality` | `media-iqa` | Image quality assessment (DSSIM/SSIM) |

Plus 5 web extraction tools: `nika:html_to_md`, `nika:css_select`, `nika:extract_metadata`, `nika:extract_links`, `nika:readability`

All media is stored in a **Content-Addressable Store** (CAS) using blake3 hashing with zstd compression.

---

## MCP Integration

Nika is an MCP-native client. Connect to any [Model Context Protocol](https://modelcontextprotocol.io/) server:

```yaml
schema: "nika/workflow@0.12"

mcp:
  novanet:
    command: cargo
    args: [run, --bin, novanet-mcp]
  filesystem:
    command: npx
    args: ["-y", "@anthropic/mcp-filesystem"]

tasks:
  - id: query
    invoke:
      mcp: novanet
      tool: read_neo4j_cypher
      params:
        query: "MATCH (e:Entity) RETURN e.name LIMIT 5"

  - id: agent_task
    agent:
      prompt: "Organize the project files by category"
      mcp: [filesystem]
      max_turns: 10
```

**100 MCP server aliases** built-in — use common names like `neo4j`, `filesystem`, `web_search`, `slack`, `github` and Nika auto-resolves the full server configuration.

---

## Structured Output

5-layer defense for guaranteed JSON schema compliance:

```yaml
- id: extract
  infer:
    prompt: "Extract entities from: {{with.text}}"
    output:
      format: json
      schema:
        type: object
        required: [entities]
        properties:
          entities:
            type: array
            items:
              type: object
              required: [name, type, confidence]
```

| Layer | Strategy | When |
|:------|:---------|:-----|
| **0** | Provider-native schema enforcement (DynamicSubmitTool) | Always tried first |
| **1** | rig Extractor (schemars) | Future |
| **2** | Extract + Validate JSON | Fallback |
| **3** | Retry with error feedback | On validation failure |
| **4** | LLM repair call | Last resort |

---

## Agent Guardrails

Validate and constrain agent outputs:

```yaml
- id: writer
  agent:
    prompt: "Write a product description"
    guardrails:
      max_length: 1000
      schema:
        type: object
        required: [title, body, tags]
      regex: "^[A-Z]"                    # Must start with uppercase
    completion:
      mode: explicit                     # Agent must call nika:complete
      confidence_threshold: 0.8          # Minimum confidence score
    limits:
      max_turns: 20
      timeout: 300
```

---

## TUI

Three views for the complete workflow lifecycle:

```
+-----------------------------------------------------------------------------+
| Nika Studio                                                  v0.37.0        |
|-----------------------------------------------------------------------------|
| +- Files ----------+ +- Editor ------------------------------------------+ |
| | > workflows/     | |  1 | schema: "nika/workflow@0.12"                 | |
| |   deploy.nika    | |  2 | provider: claude                             | |
| |   review.nika    | |  3 |                                              | |
| +- DAG ------------+ |  4 | tasks:                                       | |
| |                  | |  5 |   - id: research                             | |
| | [research]--+    | |  6 |     agent:                                   | |
| |      |      |    | |  7 |       prompt: "Find AI papers"               | |
| | [analyze] [eval] | |  8 |       mcp: [web_search]                      | |
| |      |      |    | +--------------------------------------------------+ |
| | [   report    ]  |                                                        |
| +------------------+                                                        |
|-----------------------------------------------------------------------------|
| [1/s] Studio  [2/c] Command  [3/x] Control                                 |
+-----------------------------------------------------------------------------+
```

| View | Key | Features |
|:-----|:----|:---------|
| **Studio** | `1` / `s` | File browser, YAML editor with tree-sitter highlighting, LSP integration (completion, hover, diagnostics, go-to-def, code actions), DAG preview |
| **Command** | `2` / `c` | Interactive chat with LLM, workflow execution monitor, streaming responses, real-time task progress |
| **Control** | `3` / `x` | Provider configuration, theme selection, editor preferences |

### Editor Features

- Tree-sitter YAML syntax highlighting
- LSP-powered completions, hover docs, go-to-definition
- Diagnostic gutter with underlines
- Code actions and quick fixes
- Undo/redo with edit history
- Git status gutter (git2)
- Fuzzy file search (nucleo)
- Vi/Emacs keybinding modes

---

## LSP

Full Language Server Protocol support for external editors:

```bash
# Standalone LSP server
cargo install --git https://github.com/supernovae-st/nika.git --bin nika-lsp

# Or via VS Code extension
code --install-extension supernovae.nika-vscode
```

| Capability | Details |
|:-----------|:--------|
| **Completion** | 16-variant context detection: verbs, fields, providers, models, task refs, templates, vision content |
| **Hover** | Documentation for all verbs, fields, providers, and models |
| **Go-to-Definition** | Jump from `depends_on:` and `with:` references to task definitions |
| **Diagnostics** | Schema validation, binding errors, syntax errors, model compatibility |
| **Semantic Tokens** | 20+ token types for syntax-aware highlighting |
| **Document Symbols** | Workflow outline with task hierarchy |
| **Code Actions** | Quick fixes for common mistakes |
| **Inlay Hints** | Timeout values, binding sources, dependency counts |
| **CodeLens** | Validate, Run Workflow, task count badges |
| **Document Links** | Clickable references to tasks and files |
| **Folding Ranges** | Collapse tasks, with: blocks, MCP configs |
| **References** | Find all references to a task ID |

---

## Architecture

```
                    Three-Phase AST Pipeline
                    ========================

  .nika.yaml ──> Raw Parser ──> Analyzer ──> Lower ──> Runtime
                  (spans)       (validate)   (types)     |
                                                         |
                 ┌───────────────────────────────────────┘
                 |
          ┌──────┴──────┐
          │  DAG Engine  │
          │  (petgraph)  │
          └──────┬──────┘
                 |
     ┌───────────┼───────────┐
     |           |           |
  ┌──┴──┐  ┌────┴────┐  ┌───┴───┐
  │infer│  │  fetch   │  │invoke │  + exec, agent
  └──┬──┘  └────┬────┘  └───┬───┘
     |           |           |
  ┌──┴──────────┴───────────┴──┐
  │        8 LLM Providers      │
  │     MCP Server Pool         │
  │     43 Builtin Tools        │
  │     CAS Media Store         │
  └─────────────────────────────┘
```

### Key Design Decisions

- **Three-phase AST** — Raw (spans) → Analyzed (validated) → Lowered (runtime). rustc-inspired, pure guarantees at each phase.
- **Immutable DAG** — After construction, the dependency graph is frozen for safe concurrent execution.
- **Content-Addressable Storage** — blake3 hashing, zstd compression, reflink-copy. Media never duplicated.
- **Event Sourcing** — 41 event types, NDJSON traces, full replay capability.
- **Zero Cypher** — Nika never talks to databases directly. All graph access goes through MCP.

---

## CLI Reference

```bash
# Workflow execution
nika run workflow.nika.yaml              # Execute a workflow
nika run workflow.nika.yaml --detail max # Verbose output with all events
nika run workflow.nika.yaml --quiet      # Single-line summary
nika check workflow.nika.yaml            # Validate without executing
nika check workflow.nika.yaml --strict   # + MCP server connectivity

# Interactive
nika ui                                  # Launch TUI
nika ui workflow.nika.yaml               # Open file in Studio view
nika chat                                # Direct chat mode
nika studio workflow.nika.yaml           # Open Studio view

# Initialization
nika init                                # Create project with 30 example workflows
nika init --no-example                   # Minimal project structure

# Providers
nika provider list                       # Show all providers with key status
nika provider test claude                # Validate API key with provider

# Models (native inference)
nika model list                          # Available local models
nika model download MODEL_ID             # Download from HuggingFace
nika model vision MODEL_ID               # Download vision-capable model
nika model remove MODEL_ID               # Remove local model

# MCP servers
nika mcp list                            # List configured MCP servers
nika mcp test workflow.yaml SERVER       # Test server connection
nika mcp tools workflow.yaml SERVER      # List available tools

# Media (CAS)
nika media list                          # List stored media with stats
nika media inspect HASH                  # Show metadata for a CAS entry
nika media clean                         # Remove orphaned media

# Tracing
nika trace list                          # List workflow traces
nika trace show ID                       # Show trace details
nika trace export ID --format json       # Export trace as JSON

# Configuration
nika config list                         # Show all settings
nika config get KEY                      # Get a setting
nika config set KEY VALUE                # Set a setting

# Schema
nika schema list                         # List known schema versions
nika schema validate workflow.nika.yaml  # Validate against schema

# System
nika doctor                              # Full system health check
nika doctor --full                       # + LSP, editor, MSRV checks
nika completion bash|zsh|fish            # Shell completions
```

---

## Production Examples

### SEO Audit Pipeline

```yaml
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: crawl
    fetch:
      url: "https://example.com"
      extract: metadata

  - id: audit
    with: { meta: $crawl }
    infer:
      prompt: |
        Audit this page's SEO metadata and suggest improvements:
        {{with.meta}}
      output:
        format: json
        schema:
          type: object
          required: [score, issues, recommendations]
          properties:
            score: { type: integer, minimum: 0, maximum: 100 }
            issues: { type: array, items: { type: string } }
            recommendations: { type: array, items: { type: string } }
```

### Multi-Language Content Pipeline

```yaml
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: generate
    for_each: ["en-US", "fr-FR", "ja-JP", "de-DE", "ko-KR"]
    as: locale
    concurrency: 5
    infer:
      prompt: |
        Write a product tagline for locale {{each.locale}}.
        Max 120 characters. Adapt tone for the culture.

  - id: review
    with: { taglines: $generate }
    infer:
      prompt: "Review these taglines for cultural sensitivity: {{with.taglines}}"
      output:
        format: json
        schema:
          type: object
          required: [approved, flagged]
```

### Image Processing Pipeline

```yaml
schema: "nika/workflow@0.12"

tasks:
  - id: import
    invoke:
      tool: nika:import
      params: { path: "./photos/hero.jpg" }

  - id: process
    with: { img: $import }
    invoke:
      tool: nika:pipeline
      params:
        hash: "{{with.img.hash}}"
        ops:
          - { op: thumbnail, width: 800 }
          - { op: optimize }
          - { op: convert, format: webp }

  - id: analyze
    with: { img: $import }
    invoke:
      tool: nika:quality
      params: { hash: "{{with.img.hash}}" }
```

### Agentic Research with Guardrails

```yaml
schema: "nika/workflow@0.12"
provider: claude
model: sonnet-4

mcp:
  web_search:
    command: npx
    args: ["-y", "@anthropic/mcp-web-search"]

tasks:
  - id: research
    agent:
      prompt: |
        Research the latest developments in quantum computing.
        Find 5 recent papers, summarize each, and identify trends.
      mcp: [web_search]
      max_turns: 20
      guardrails:
        max_length: 10000
        schema:
          type: object
          required: [papers, trends, summary]
      completion:
        mode: explicit
        confidence_threshold: 0.85
      limits:
        timeout: 600
```

### Health Check Dashboard

```yaml
schema: "nika/workflow@0.12"

tasks:
  - id: check
    for_each:
      - "https://api.example.com/health"
      - "https://cdn.example.com/status"
      - "https://db.example.com/ping"
    as: endpoint
    concurrency: 3
    fetch:
      url: "{{each.endpoint}}"
      timeout: 10
      response: full

  - id: report
    with: { results: $check }
    provider: claude
    infer:
      prompt: "Generate a status report from these health checks: {{with.results}}"
      output:
        format: json
        schema:
          type: object
          required: [status, services, timestamp]
```

---

## Installation

### From Source (recommended)

```bash
cargo install --git https://github.com/supernovae-st/nika.git
```

### Clone and Build

```bash
git clone https://github.com/supernovae-st/nika.git
cd nika && cargo install --path tools/nika
```

### Verify

```bash
nika --version       # nika 0.37.0
nika doctor          # Full system health check
```

### Feature Flags

Nika ships with 22 default features enabled. Customize at build time:

```bash
# Minimal (no TUI, no native inference, no media)
cargo install --path tools/nika --no-default-features

# With specific features
cargo install --path tools/nika --features "tui,native-inference,media-core"
```

| Feature | Default | Description |
|:--------|:--------|:------------|
| `tui` | yes | Terminal UI (ratatui, tree-sitter, git2) |
| `native-inference` | yes | Local GGUF models via mistral.rs |
| `media-core` | yes | Tier 2 media tools (thumbnail, convert, etc.) |
| `media-phash` | yes | Perceptual hashing + comparison |
| `media-pdf` | yes | PDF text extraction |
| `media-chart` | yes | Chart generation from JSON |
| `media-qr` | yes | QR code validation |
| `media-iqa` | yes | Image quality assessment |
| `media-provenance` | no | C2PA signing + verification |
| `media-compression` | yes | zstd CAS compression |
| `fetch-extract` | yes | HTML extraction (text, selector, metadata, links) |
| `fetch-markdown` | yes | HTML to Markdown (htmd) |
| `fetch-article` | yes | Article extraction (dom_smoothie) |
| `fetch-feed` | yes | RSS/Atom/JSON Feed parsing |
| `lsp` | no | Standalone LSP server binary |
| `nika-daemon` | yes | Background daemon for key management |

---

## Project Structure

```
nika/
├── tools/
│   ├── nika/src/               # Main binary (100k+ LOC)
│   │   ├── ast/                # Three-phase AST pipeline (40+ files)
│   │   ├── runtime/            # DAG execution + 5 verb implementations
│   │   │   ├── executor/       # Task dispatch + verb runners
│   │   │   └── builtin/        # 43 builtin tools (file, media, web)
│   │   ├── mcp/                # MCP client pool (rmcp 0.16)
│   │   ├── provider/           # 8 LLM providers (rig-core + mistral.rs)
│   │   ├── tui/                # Terminal UI (3 views, 40+ files)
│   │   ├── binding/            # Data flow: templates, transforms, JSONPath
│   │   ├── dag/                # Graph validation + execution ordering
│   │   ├── event/              # 41 event types + NDJSON tracing
│   │   ├── media/              # CAS store + blake3 + zstd
│   │   └── cli/                # CLI subcommands
│   ├── nika-core/              # Zero-dep AST core (fast compilation)
│   ├── nika-lsp-core/          # Protocol-agnostic LSP intelligence
│   └── nika-lsp/               # Standalone LSP server
├── examples/                   # 32 example workflows
├── editors/nika-vscode/        # VS Code extension
├── docs/                       # Documentation
└── spec/                       # Formal specification
```

---

## Error Codes

Nika uses structured error codes (`NIKA-XXX`) for every failure:

| Range | Category |
|:------|:---------|
| `000-009` | Workflow parsing |
| `010-019` | Schema validation |
| `020-029` | DAG (cycles, missing deps) |
| `030-039` | Provider errors |
| `040-049` | Template/binding resolution |
| `050-059` | Security (path traversal, blocked commands) |
| `060-069` | Output validation (JSON schema) |
| `100-109` | MCP (connection, tool errors) |
| `110-119` | Agent + Guardrails |
| `200-219` | Builtin tools |
| `251-259` | Media pipeline |
| `290-297` | Media tools |
| `300-309` | Structured output |

---

## Contributing

```bash
git clone https://github.com/supernovae-st/nika.git
cd nika

cargo build                       # Build
cargo test --lib                  # Run 7400+ tests (safe, no keychain popups)
cargo clippy -- -D warnings       # Zero warnings policy
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Conventions

- **Tests first** — TDD preferred, edge cases always
- **Error codes** — `NikaError` with `NIKA-XXX`, never `anyhow`
- **AST phases** — Always Raw → Analyzed → Lower, never skip
- **Extensions** — `.nika.yaml` for workflows
- **Zero Cypher** — Use MCP `invoke:`, never direct database access

---

## Ecosystem

Nika is the workflow engine of the **SuperNovae** ecosystem:

```
NovaNet (Brain)              Nika (Body)
├── Knowledge Graph    <──>  ├── YAML Workflows
├── Node/Arc Schema    MCP   ├── 5 Verbs + DAG Engine
├── MCP Server         ───>  ├── 8 LLM Providers
└── Neo4j                    └── 43 Builtin Tools
```

---

<div align="center">

**Nika v0.37.0** | Schema `nika/workflow@0.12` | Rust 1.86+ | AGPL-3.0

7400+ tests | 100k+ LOC | 0 clippy warnings | 0.x.x forever

[SuperNovae Studio](https://supernovae.studio) — [QR Code AI](https://qrcode-ai.com) — [GitHub](https://github.com/supernovae-st/nika)

</div>
