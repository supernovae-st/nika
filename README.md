<div align="center">

# Nika

**Open-source agentic YAML workflow engine for AI**

[![Version](https://img.shields.io/badge/v0.35.4-7c3aed?style=flat-square&logo=semver&logoColor=white)](tools/nika/CHANGELOG.md)
[![Rust](https://img.shields.io/badge/rust_1.86+-f97316?style=flat-square&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/AGPL--3.0-22c55e?style=flat-square&logo=gnu&logoColor=white)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-6846+_passing-10b981?style=flat-square)](https://github.com/supernovae-st/nika/actions)

</div>

```yaml
schema: "nika/workflow@0.12"
description: "Research and summarize AI papers"
provider: openai

tasks:
  - id: search
    fetch:
      url: "https://api.semanticscholar.org/graph/v1/paper/search?query=LLM+agents&limit=5"
      extract: jsonpath
      selector: "$.data[*].title"

  - id: analyze
    depends_on: [search]
    with: { papers: $search }
    infer:
      system: "You are a research analyst."
      prompt: "Summarize these papers: {{with.papers}}"
```

---

## Quick Start

```bash
# Install
cargo install --git https://github.com/supernovae-st/nika.git

# Set any provider key
export ANTHROPIC_API_KEY=sk-ant-...

# Run a workflow
nika run workflow.nika.yaml
```

Create `hello.nika.yaml`:

```yaml
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: greet
    infer: "Say hello in French, then in Japanese"
```

```bash
nika run hello.nika.yaml
```

---

## The 5 Verbs

Every task uses exactly one verb. That's the entire API.

| Verb | Purpose | Example |
|:-----|:--------|:--------|
| `infer:` | LLM generation | `infer: "Summarize this"` |
| `exec:` | Shell command | `exec: "git diff HEAD~1"` |
| `fetch:` | HTTP + extraction | `fetch: { url: "...", extract: markdown }` |
| `invoke:` | MCP tool call | `invoke: { mcp: search, tool: query }` |
| `agent:` | Multi-turn loop | `agent: { prompt: "...", mcp: [tools] }` |

---

## Features

```
8 LLM providers    26 media tools     9 extract modes    3 TUI views
DAG execution      for_each loops     pipe transforms    structured output
Vision support     CAS storage        NDJSON traces      MCP native
```

**Providers:** Claude, OpenAI, Mistral, Groq, DeepSeek, Gemini, xAI, Native (local GGUF)

**Extract modes:** markdown, article, text, selector, metadata, links, jsonpath, feed, llm_txt

**Media tools:** import, thumbnail, convert, optimize, chart, provenance, QR validation, and more

---

## Production Examples

### SEO Audit

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
```

### Content Pipeline

```yaml
schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: generate
    for_each: ["en-US", "fr-FR", "ja-JP", "de-DE"]
    as: locale
    concurrency: 4
    infer:
      prompt: |
        Write a product tagline for locale {{each.locale}}.
        Max 120 characters. Adapt tone for the culture.
```

### Parallel Health Check

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

  - id: report
    with: { results: $check }
    provider: claude
    infer: "Summarize these health check results: {{with.results}}"
```

---

## Architecture

```
YAML --> Parser --> Analyzer --> DAG --> Runtime --> Output
                                          |
                                   8 LLM Providers
                                   MCP Servers
                                   26 Builtin Tools
```

Workflows are parsed into a three-phase AST (Raw, Analyzed, Lowered), validated as a directed acyclic graph via petgraph, then executed in topological order with full NDJSON tracing.

---

## TUI

```
+-----------------------------------------------------------------------------+
| Nika Studio                                                  v0.35.4        |
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
| | [   report    ]  | +- Chat DAG ----------------------------------------+ |
| +------------------+ | @1 research -> @2 analyze -> @4 report            | |
|                      |      '-------> @3 evaluate --'                    | |
|                      +---------------------------------------------------+ |
|-----------------------------------------------------------------------------|
| [1/s] Studio  [2/c] Command  [3/x] Control                                 |
+-----------------------------------------------------------------------------+
```

```bash
nika ui                          # Launch TUI
nika ui workflow.nika.yaml       # Open file in Studio
nika check workflow.nika.yaml    # Validate without running
nika provider list               # Show configured providers
```

---

## Installation

**From source (recommended):**

```bash
cargo install --git https://github.com/supernovae-st/nika.git
```

**Clone and build:**

```bash
git clone https://github.com/supernovae-st/nika.git
cd nika && cargo install --path tools/nika
```

**Verify:**

```bash
nika --version
# nika 0.35.4
```

### Provider Setup

Set the environment variable for your preferred provider:

```bash
export ANTHROPIC_API_KEY=sk-ant-...    # Claude (default)
export OPENAI_API_KEY=sk-...           # OpenAI
export MISTRAL_API_KEY=...             # Mistral
export GROQ_API_KEY=gsk_...            # Groq
export DEEPSEEK_API_KEY=sk-...         # DeepSeek
export GEMINI_API_KEY=...              # Gemini
export XAI_API_KEY=xai-...             # xAI (Grok)
```

Or specify per-workflow with `provider:` / per-task with `infer: { provider: ... }`.

---

## MCP Integration

Nika is an MCP-native client. Connect to any [Model Context Protocol](https://modelcontextprotocol.io/) server:

```yaml
schema: "nika/workflow@0.12"

mcp:
  filesystem:
    command: npx
    args: ["-y", "@anthropic/mcp-filesystem"]
  web_search:
    command: npx
    args: ["-y", "@anthropic/mcp-web-search"]

tasks:
  - id: research
    agent:
      prompt: "Find recent AI safety papers and save a summary"
      mcp: [web_search, filesystem]
      max_turns: 15

  - id: notify
    invoke:
      mcp: filesystem
      tool: write_file
      params:
        path: "/output/report.md"
        content: "{{with.research}}"
```

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

```bash
git clone https://github.com/supernovae-st/nika.git
cd nika
cargo build                       # Build
cargo test --lib                  # Run 6846+ tests (safe, no keychain)
cargo clippy -- -D warnings       # Zero warnings policy
```

---

<div align="center">

```
Nika v0.35.4 | Schema @0.12 | Rust 1.86+ | AGPL-3.0
6846+ tests | 110k LOC | 0 clippy warnings | 0.x.x forever
```

[SuperNovae Studio](https://supernovae.studio) -- [nika.sh](https://nika.sh) -- [GitHub](https://github.com/supernovae-st/nika)

</div>
