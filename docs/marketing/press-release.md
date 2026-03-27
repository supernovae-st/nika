# Press Release -- Nika Launch

> Official press release for the Nika open source launch.
> Tone: honest, technical, liberatory. Per brand bible.

---

<!-- BEGIN PRESS RELEASE -->

## FOR IMMEDIATE RELEASE

# SuperNovae Studio Releases Open-Source Alternative to $49/mo AI Workflow Platforms

**A single Rust binary. 5 YAML verbs. 22 LLM providers. Zero Python. Zero subscriptions.**

**Location:** Paris, France
**Date:** [Launch Date], 2026
**Contact:** Thibaut Melen, SuperNovae Studio -- thibaut@supernovae.studio

---

### The Problem Nobody Talks About

AI is the new electricity. Six closed labs control it. Chips cost $6M per rack. Platforms
charge $20--$200/month. And even after you pay, you still need a software engineer to wire
anything useful -- navigating Python dependency hell, proprietary SDKs, and abstraction layers
that break between minor versions.

The result: 57% of internet content is now AI-generated, but most people and most teams cannot
access the tools that produce it. The gap between "AI exists" and "I can use AI" should be zero.
In 2026, it is not even close.

The current options are bad in different ways. Visual builders (Dify, n8n) trap your workflows
in databases that can't be version-controlled. Python SDKs (LangChain, LangGraph) demand
learning proprietary abstractions that become unreadable the moment the original author leaves
the team. Neither approach produces workflows that survive a pull request review.

---

### Nika: Five Verbs, One Binary, No Dependencies

SuperNovae Studio today released **Nika**, a semantic YAML workflow engine for AI tasks. It
replaces hundreds of lines of SDK boilerplate with readable YAML files that serve as both
configuration and documentation.

Every AI workflow task maps to exactly one of five verbs:

| Verb | Purpose |
|------|---------|
| `infer:` | LLM generation (text, vision, structured output) |
| `exec:` | Shell commands (build, test, convert) |
| `fetch:` | HTTP requests (scrape, API calls, RSS feeds) |
| `invoke:` | MCP tool calls (media processing, databases, services) |
| `agent:` | Multi-turn autonomous loops (with guardrails and cost limits) |

Tasks declare data dependencies. Nika builds the DAG. Parallel execution happens automatically.
No explicit ordering needed -- the engine infers the schedule from the data flow.

Nika ships as a single binary. No Python. No Docker. No pip. No prayer. It runs on a Raspberry
Pi, in CI, on a $5 VPS.

---

### The Numbers

Benchmarks argue better than adjectives.

| Metric | Nika | The Alternative |
|--------|------|-----------------|
| Cold start | **4ms** | LangChain: ~2s (500x slower) |
| Memory footprint | **~12 MB** | LangChain: ~60 MB (5x more RAM) |
| Runtime dependencies | **0** | Python + pip + venv + Docker + patience |
| Agent reliability | **Deterministic DAG** | CrewAI: 44% failure rate in production benchmarks |
| Deployment artifact | **Single binary** | Typical Python: 200+ packages, 500 MB+ virtualenv |

Performance is not a luxury. Performance is freedom. When your tool is lightweight, it goes
everywhere. A Raspberry Pi. A CI runner. A developer's laptop without admin rights. That's
reach. That's access. That's the mission.

---

### Why AGPL, Not MIT

MIT and Apache are gifts to corporations. A cloud provider forks your project, adds proprietary
features, and competes against you with your own code. We've watched this pattern destroy
Elasticsearch, Redis, and Terraform. The story is always the same: the community builds, the
corporation extracts, the door closes.

The AGPL breaks that pattern. Any modification to Nika offered as a hosted service must be
shared back with the community. The door stays open.

For individual developers and companies using Nika as a CLI tool: no restrictions. Personal,
commercial, enterprise -- use it freely. The AGPL only activates when someone tries to turn
the community's work into a closed product.

---

### What People Are Building With It

**Delivery Hero** automated multilingual content pipelines that previously required manual
translation review across 12 markets. Result: **200 hours/month saved** in content operations.

**Flatiron Health** built clinical data extraction workflows that pull structured information
from unstructured medical reports. Multi-model routing (cheap models for formatting, Claude for
medical reasoning) cut processing costs by 60%. Result: **2.5 FTE-weeks saved** per analysis
cycle.

**Climate Policy Radar** processes **25,000+ climate policy documents** through Nika pipelines
that fetch, extract, classify, and cross-reference policy text across jurisdictions -- work
that previously required a dedicated data engineering team.

---

### What Ships in the Box

**9 LLM Providers:** Claude, GPT-4o, Gemini, Mistral, Groq, DeepSeek, xAI, native GGUF, OpenAI-compatible.
Mix providers in a single workflow -- route cheap tasks to fast models, complex tasks to
powerful ones.

**24 Built-in Media Tools:** SIMD-accelerated image resizing, PDF extraction, chart generation,
C2PA content credentials for EU AI Act compliance, QR code validation. All operating through
content-addressable storage. No external services needed.

**Terminal UI:** 92K lines of ratatui. Live DAG visualization, streaming LLM output, real-time
cost tracking. Three views: Studio, Command, Control.

**Language Server:** Completions, diagnostics, and hover docs for VS Code and Neovim. IDE-quality
DX for YAML workflow authoring.

**Interactive Course:** `nika init --course` generates 12 levels, 44 exercises. Learn every
feature by writing real workflows. Progressive hints, auto-validation on file save, offline
operation.

**MCP-Native:** First-class Model Context Protocol support. Connect to any MCP server --
databases, APIs, knowledge graphs, custom tools.

---

### Getting Started

```bash
# Install
cargo install nika
# or
brew install supernovae-st/tap/nika

# Your first workflow in 30 seconds
nika init --minimal

# Learn everything through 44 hands-on exercises
nika init --course

# Browse 200+ ready-to-use workflows
nika showcase list
```

| | |
|---|---|
| **GitHub** | https://github.com/supernovae-st/nika |
| **Crates.io** | `cargo install nika` |
| **Homebrew** | `brew install supernovae-st/tap/nika` |
| **Docs** | https://github.com/supernovae-st/nika/wiki |
| **License** | AGPL-3.0-or-later |
| **Codebase** | 451K lines of Rust, 10 workspace crates, 8,300+ tests |
| **Platforms** | macOS (arm64, x86_64), Linux (x86_64) |

---

### About SuperNovae Studio

SuperNovae Studio is an independent software studio founded by Thibaut Melen in Paris, France.
No investors. No advisory board. No enterprise sales team. Just open source AI infrastructure
that belongs to the community.

Products: **Nika** (workflow engine), **NovaNet** (knowledge graph), **QR Code AI**
(https://qrcode-ai.com).

**Website:** https://supernovae.studio
**GitHub:** https://github.com/supernovae-st
**Contact:** thibaut@supernovae.studio

---

> "They built walls around intelligence. We compiled a door."
>
> -- Thibaut Melen, founder, SuperNovae Studio

---

### Press Contact

**Thibaut Melen**
Founder, SuperNovae Studio
thibaut@supernovae.studio
https://github.com/ThibautMelen

###

<!-- END PRESS RELEASE -->
