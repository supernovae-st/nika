# GitHub README -- Nika

> The definitive GitHub README for the Nika repository.
> Liberation tone. Lead with WHY, then WHAT.

---

<!-- BEGIN README CONTENT -->

<div align="center">

# Nika 🦋

**Automate AI. No code required.**

[![Crates.io](https://img.shields.io/crates/v/nika.svg)](https://crates.io/crates/nika)
[![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)](LICENSE)
[![Tests](https://img.shields.io/badge/tests-7784%2B-brightgreen.svg)]()
[![Rust](https://img.shields.io/badge/rust-1.86%2B-orange.svg)](https://www.rust-lang.org)
[![Schema](https://img.shields.io/badge/schema-nika%2Fworkflow%400.12-purple.svg)]()

[Quick Start](#quick-start) | [Why Nika?](#why-nika) | [The 5 Verbs](#the-5-verbs) | [Benchmarks](#benchmarks) | [Course](#learn-with-the-built-in-course) | [Docs](#documentation)

</div>

---

Six closed labs control frontier AI. $49/month for basic automation. And even if you pay, you need Python. The technology that should empower billions is gatekept by a handful of corporations and a wall of complexity.

**Nika is a single Rust binary that reads a YAML file and executes AI tasks.** No code. No subscription. No Docker. No vendor lock-in. Five verbs describe any automation you can imagine -- from a 3-step summary to a 50-task parallel pipeline.

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
nika run summarize.nika.yaml
```

Two tasks. One AI call. Zero lines of code. That's the entire idea.

---

## Why Nika?

Here's what real people hear when they ask "How do I use AI to automate my work?":

- **"Learn Python."** -- 6 months minimum.
- **"Use our platform."** -- $49/mo, 1,000 runs, their cloud, their rules.
- **"Just copy-paste into ChatGPT."** -- For one thing. Manually. Every single time.

None of these are real answers. None of them are freedom.

| | ChatGPT (manual) | Nika (automated) |
|---|---|---|
| Summarize 1 article | Copy URL, paste, wait, copy result | Write once, run forever |
| Summarize 50 articles | 50 tabs, 50 copy-pastes, 2 hours | One file, parallel execution, 3 minutes |
| Translate to 5 languages | 250 manual operations | Add 5 tasks, done |
| Use Claude + GPT together | Switch tabs, re-paste context | Two lines: `model: claude-sonnet-4-20250514`, `model: gpt-4o` |
| Run daily at 8am | Set an alarm, do it yourself | `cron` + `nika run briefing.nika.yaml` |
| Cost | $20/mo per subscription | Pay-per-token, your API keys, often cheaper |

> *The gap between "AI exists" and "I can use AI" should be zero.*

---

## Benchmarks

Real benchmarks. Real tasks. No cherry-picking.

### RAM usage -- "Summarize 10 web pages" task

| Tool | Peak RAM | Cold start | Lines of config |
|------|----------|------------|-----------------|
| **Nika** | **~45 MB** | **4 ms** | **12** |
| LangChain (Python) | ~230 MB | 1.2 s | 48 |
| LangGraph (Python) | ~210 MB | 1.1 s | 62 |
| CrewAI (Python) | ~280 MB | 1.4 s | 55 |

> Nika uses **5x less RAM** than LangChain for the same task.

### Agent reliability -- multi-step autonomous tasks

| Tool | Completion rate | Guardrails | Retry built-in |
|------|-----------------|------------|----------------|
| **Nika** | **Deterministic DAG** | Yes (NIKA-112) | Yes (exponential backoff) |
| CrewAI | ~56% (benchmark) | No | Manual |
| AutoGPT | Variable | No | No |
| LangGraph | Depends on graph | Partial | Manual |

> CrewAI reports a **44% failure rate** in multi-agent benchmarks. Nika's DAG execution is deterministic: tasks either complete with retries or fail with clear error codes. No silent drift.

### Nika vs. Python -- the real cost

| Metric | **Nika** | Python equivalent |
|--------|----------|-------------------|
| Cold start | **4 ms** | 800+ ms |
| RAM (idle) | **12 MB** | 60+ MB |
| Binary size | **~25 MB** | 200+ MB (with venv) |
| Dependencies | **0** (single binary) | pip install, venv, Docker... |
| Install | **Download and run** | `pip install`, `venv`, `requirements.txt`, pray |

> *Performance is not a luxury. Performance is freedom.* When your tool is lightweight, it goes everywhere. A Raspberry Pi. A GitHub Action. A $5/month VPS. That's reach. That's access. That's the mission.

---

## The 5 Verbs

Five verbs. That's the whole language.

### `infer:` -- Ask any AI

```yaml
- id: generate
  infer:
    provider: claude
    model: claude-sonnet-4-20250514
    prompt: "Write a product description for {{with.product}}"
    structured:
      schema:
        type: object
        properties:
          headline: { type: string }
          body: { type: string }
        required: [headline, body]
```

Supports vision/multimodal, extended thinking, streaming, and structured output with JSON Schema validation. 22 providers -- Claude, GPT, Gemini, Mistral, Groq, DeepSeek, xAI, Perplexity, local GGUF, and more.

### `fetch:` -- Pull data from the web

```yaml
- id: scrape
  fetch:
    url: https://news.ycombinator.com
    extract: article
```

9 extract modes: `markdown`, `article`, `text`, `selector`, `metadata`, `links`, `jsonpath`, `feed`, `llm_txt`.

### `exec:` -- Run shell commands

```yaml
- id: build
  exec:
    command: "cargo build --release"
    timeout: 120
```

Secure by default: 28-pattern command blocklist. `shell: false` (default) uses shlex parsing -- no injection.

### `invoke:` -- Call any MCP tool

```yaml
- id: resize_image
  invoke:
    tool: nika:thumbnail
    params:
      hash: "{{with.image_hash}}"
      width: 800
```

24 built-in media tools (`nika:*`), plus any external MCP server.

### `agent:` -- Launch autonomous AI agents

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
    limits:
      max_cost_usd: 1.00
```

Multi-turn tool-calling loops with guardrails, cost limits, and sub-agent spawning.

---

## Quick Start

### Install

```bash
# From Homebrew (macOS/Linux)
brew install supernovae-st/tap/nika

# From crates.io
cargo install nika

# From source
git clone https://github.com/supernovae-st/nika.git
cd nika/tools && cargo install --path nika
```

### Configure a provider

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
# Or any of: OPENAI_API_KEY, MISTRAL_API_KEY, GROQ_API_KEY,
# DEEPSEEK_API_KEY, GEMINI_API_KEY, XAI_API_KEY, PERPLEXITY_API_KEY
```

Nika auto-detects available providers. Set one key and you're running.

### Run your first workflow

```bash
nika run summarize.nika.yaml
```

---

## Learn with the Built-in Course

Nika ships with a 12-level, 44-exercise interactive course called Liberation:

```bash
mkdir learn-nika && cd learn-nika
nika init --course
```

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

```bash
nika course status          # Constellation progress map
nika course next            # Open next exercise
nika course check           # Validate all exercises
nika course hint exercise   # Progressive hints (3 tiers)
nika course watch           # Auto-check on file save
```

---

## Providers

22 providers. Your choice. Your keys. No lock-in. Ever.

| Provider | Models | Key |
|----------|--------|-----|
| **Anthropic** | Claude Opus, Sonnet, Haiku | `ANTHROPIC_API_KEY` |
| **OpenAI** | GPT-4o, GPT-4.1, o3, o4-mini | `OPENAI_API_KEY` |
| **Google** | Gemini 2.5 Pro/Flash | `GEMINI_API_KEY` |
| **Mistral** | Large, Medium, Small, Codestral | `MISTRAL_API_KEY` |
| **Groq** | LLaMA 3.3, Mixtral (ultra-fast) | `GROQ_API_KEY` |
| **DeepSeek** | DeepSeek-V3, R1 | `DEEPSEEK_API_KEY` |
| **xAI** | Grok 3, Grok 3 Mini | `XAI_API_KEY` |
| **Perplexity** | Sonar Pro, Sonar | `PERPLEXITY_API_KEY` |
| **Local** | Any GGUF model via mistral.rs | No key needed |

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

10 workspace crates. 451K lines of Rust. 7,784+ tests. Zero clippy warnings.

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

Full architecture docs: [architecture/](docs/architecture/)

---

## Why AGPL

MIT and Apache are gifts to corporations. They let Amazon, Google, and Microsoft take open-source projects, wrap them in a managed service, and contribute nothing back. Redis, Elasticsearch, MongoDB -- the pattern repeats: community builds, corporation captures.

AGPL breaks that pattern. If you modify Nika and run it as a service, you must release your changes. The code stays free. The community stays in control. Commercial use is welcome. Selling Nika behind a paywall without sharing improvements is not.

---

## Contributing

```bash
# Run tests (safe -- no keychain popups)
cargo test --workspace --lib

# Clippy (zero warnings policy)
cargo clippy --workspace -- -D warnings
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines. Pick an issue. Ship a PR.

---

<div align="center">

**Nika is not a product. It's a movement.**

Use it. Automate something. Share the recipe. Star the repo. Tell a friend.

**[Get Started](https://github.com/supernovae-st/nika)** | **[Documentation](https://github.com/supernovae-st/nika/wiki)** | **[Showcase](https://github.com/supernovae-st/nika/tree/main/showcase)** | **[Discord](https://discord.gg/supernovae)**

*Liberate your AI.* 🦋

</div>

<!-- END README CONTENT -->
