# Nika Workflow Examples

This directory contains example workflows demonstrating Nika's capabilities.

## Quick Start

```bash
# Run any workflow
cargo run -- run examples/<workflow>.yaml

# Validate without executing
cargo run -- validate examples/<workflow>.yaml

# Run with TUI (interactive)
cargo run -- tui examples/<workflow>.yaml
```

## Prerequisites

For workflows using NovaNet MCP:
1. Neo4j running at `bolt://localhost:7687`
2. NovaNet MCP server available (auto-started by workflow)
3. API keys set: `ANTHROPIC_API_KEY` or `OPENAI_API_KEY`

## Example Categories

### v0.3 Feature Showcases

These examples demonstrate v0.3 features: `for_each` parallelism, MCP integration, and agent loops.

| File | Features | Description |
|------|----------|-------------|
| `v03-parallel-locales.yaml` | `for_each`, `concurrency` | Generate content for 5 locales in parallel |
| `v03-denomination-forms.yaml` | `invoke`, ADR-033 | Use NovaNet's denomination_forms for prescriptive naming |
| `v03-entity-pipeline.yaml` | `invoke`, `for_each`, `infer` | Multi-step pipeline: fetch → process → aggregate |
| `v03-agent-with-tools.yaml` | `agent:`, MCP tools | Multi-turn agent with NovaNet tool access |

### Core Examples

| File | Verb | Description |
|------|------|-------------|
| `invoke-novanet.yaml` | `invoke:` | Call NovaNet MCP tools |
| `agent-novanet.yaml` | `agent:` | Agentic loop with NovaNet |
| `agent-simple.yaml` | `agent:` | Basic agent without MCP |

### Use Case Workflows (UC1-UC10)

Production-ready workflow patterns for real-world scenarios.

| File | Use Case |
|------|----------|
| `uc1-entity-generation.yaml` | Single entity content generation |
| `uc2-multi-locale-generation.yaml` | Multi-locale pipeline |
| `uc3-agent-refinement.yaml` | Agent-based content refinement |
| `uc4-seo-pipeline.yaml` | SEO content pipeline |
| `uc5-graph-traversal.yaml` | Knowledge graph traversal |
| `uc6-page-generation.yaml` | Full page generation |
| `uc7-error-recovery.yaml` | Error handling patterns |
| `uc8-batch-entities.yaml` | Batch entity processing |
| `uc9-content-validation.yaml` | Content quality validation |
| `uc10-comprehensive-landing-page.yaml` | Complete landing page pipeline |

## Workflow Schema

All workflows use schema `nika/workflow@0.3`:

```yaml
schema: "nika/workflow@0.3"
provider: claude  # or openai

mcp:
  novanet:
    command: cargo
    args: [run, --manifest-path, ../novanet-mcp/Cargo.toml]
    env:
      NOVANET_MCP_NEO4J_URI: bolt://localhost:7687

tasks:
  - id: task_name
    <verb>: <params>
    output:
      format: json | text | yaml
```

## The 5 Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM text generation | `infer: "Summarize this text"` |
| `exec:` | Shell command | `exec: "npm run build"` |
| `fetch:` | HTTP request | `fetch: { url: "...", method: "GET" }` |
| `invoke:` | MCP tool call | `invoke: { mcp: novanet, tool: novanet_generate }` |
| `agent:` | Multi-turn agentic loop | `agent: { goal: "...", max_turns: 5 }` |

## for_each Parallelism (v0.3)

Execute tasks in parallel over an array:

```yaml
- id: generate_pages
  for_each:
    items: ["fr-FR", "en-US", "es-ES"]
    as: locale
    concurrency: 3    # Max parallel tasks
    fail_fast: true   # Stop on first error
  invoke:
    mcp: novanet
    tool: novanet_generate
    params:
      entity: "qr-code"
      locale: "{{use.locale}}"
```

## Data Flow (use: bindings)

Pass data between tasks:

```yaml
tasks:
  - id: fetch_data
    invoke: ...
    output:
      format: json

  - id: process_data
    use:
      data: fetch_data           # Reference previous task
    infer:
      prompt: |
        Process this data:
        {{use.data}}             # Access in prompt
```

## denomination_forms (ADR-033)

NovaNet returns prescriptive naming forms for entities:

```yaml
# After novanet_generate, response includes:
# denomination_forms:
#   qr-code:
#     text: "code QR"         # Use in prose
#     title: "Code QR"        # Use in headings
#     abbrev: "QR"            # After first mention
#     url: "code-qr"          # In URLs (optional)
```

LLMs MUST use ONLY these forms. No invention, no paraphrase.

## Running Examples

```bash
# v0.3 showcase: parallel locales
cargo run -- run examples/v03-parallel-locales.yaml

# v0.3 showcase: denomination forms
cargo run -- run examples/v03-denomination-forms.yaml

# Use case: entity generation
cargo run -- run examples/uc1-entity-generation.yaml

# With TUI for real-time observation
cargo run -- tui examples/v03-entity-pipeline.yaml
```

## Traces

Workflow executions are traced to `.nika/traces/`:

```bash
# List recent traces
cargo run -- trace list

# Show trace details
cargo run -- trace show <trace-id>
```
