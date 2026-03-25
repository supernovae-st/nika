# Nika — Complete Pack for NotebookLM

> This document compiles the essential Nika documentation for AI-powered content generation.
> Source: 10 documents covering manifesto, technical deep-dive, competitive positioning,
> real-world use cases, and the engineering story.

---

# PART 1 — THE MANIFESTO

> **Automate AI. No code required.**
> *AI is the new electricity. It should be accessible to everyone.*

## The Problem

Six closed labs control frontier AI. Chips cost $6 million per rack. LLM subscriptions run $20 to $200 a month. And even if you pay, you still need a software engineer to wire anything useful together.

**The result?** AI is powerful, but locked. Locked behind code, subscriptions, and vendor walls. The technology that should empower billions is gatekept by a handful of corporations.

Meanwhile, the tools that promise to "democratize AI" charge you $49/month to run automations on *their* servers, with *their* limits, under *their* terms. They call it accessible. We call it a new middleman. Here's what real people hear when they ask "How do I use AI to automate my work?":

- **"Learn Python."** — 6 months minimum.
- **"Use our platform."** — $49/mo, 1,000 runs, their cloud, their rules.
- **"Just copy-paste into ChatGPT."** — For one thing. Manually. Every single time.

**None of these are real answers. None of them are freedom.**

## The Vision

Electricity doesn't ask you to learn electrical engineering before you flip a switch. Water doesn't require a plumbing license before it flows from your tap. **AI should work the same way.**

Write what you want in a plain text file. Describe the steps. Pick any AI. Press run. No code. No subscription. No vendor lock-in. No PhD required.

A file that says *"fetch this page, summarize it, translate it to French, save it"* should just work. On your machine. With your choice of AI. For free. This is not a feature request. This is a fundamental belief:

> **The gap between "AI exists" and "I can use AI" should be zero.**

## The Solution

**Nika** is a single binary that reads a YAML text file and executes it.

```yaml
# my-automation.nika.yaml
schema: "nika/workflow@0.12"
name: morning-briefing
tasks:
  - id: headlines
    fetch: { url: "https://news.ycombinator.com", extract: article }

  - id: summarize
    infer:
      model: claude-sonnet-4-20250514
      prompt: "Summarize these headlines in 5 bullets: {{with.news}}"
    with:
      news: $headlines
    depends_on: [headlines]

  - id: translate
    infer:
      model: gpt-4o
      prompt: "Translate to French: {{with.summary}}"
    with:
      summary: $summarize
    depends_on: [summarize]
```

Three steps. Two AI providers. Zero lines of code.

**That's the entire idea.** Describe steps in a text file. Nika handles execution — parallel tasks, retries, error handling, streaming, cost tracking — so you don't have to.

### Five verbs. That's the whole language.

| Verb | What it does |
|------|-------------|
| `infer:` | Ask any AI to generate text, analyze images, think |
| `fetch:` | Pull data from the web — pages, APIs, feeds |
| `exec:` | Run shell commands on your machine |
| `invoke:` | Call external tools via MCP protocol |
| `agent:` | Launch an autonomous AI agent with guardrails |

Five verbs to describe any automation. From a 3-step summary to a 50-task parallel pipeline processing hundreds of articles, images, and datasets.

### Manual vs. Automated: the real comparison

| | ChatGPT (manual) | Nika (automated) |
|---|---|---|
| Summarize 1 article | Copy URL, paste, wait, copy result | Write once, run forever |
| Summarize 50 articles | 50 tabs, 50 copy-pastes, 2 hours | One file, parallel execution, 3 minutes |
| Translate to 5 languages | 250 manual operations | Add 5 tasks, done |
| Use Claude + GPT together | Switch tabs, re-paste context | Two lines: `model: claude-sonnet-4-20250514`, `model: gpt-4o` |
| Run daily at 8am | Set an alarm, do it yourself | `cron` + `nika run briefing.nika.yaml` |
| Cost | $20/mo per subscription | Pay-per-token, your API keys, often cheaper |

## Why Open Source

Nika is licensed under **AGPL-3.0-or-later**. Not MIT. Not Apache. AGPL.

MIT and Apache are gifts to corporations. They let Amazon, Google, and Microsoft take open-source projects, wrap them in a managed service, and contribute nothing back. Redis, Elasticsearch, MongoDB — the pattern: community builds, corporation captures.

**AGPL breaks that pattern.** If you modify Nika and run it as a service, you must release your changes. The code stays free. The community stays in control.

### The principles

- **Multi-provider by design.** Claude, GPT, Mistral, Gemini, Groq, xAI, DeepSeek, local GGUF — all of them. You choose. You switch. No lock-in. Ever.
- **Your machine, your data.** Nika runs locally. Your files never touch our servers (we don't have servers). Your API keys stay in your OS keychain.
- **Community-owned.** No VC exit strategy. No "open core" bait-and-switch. The full engine is open source. Period.

## Why Rust

> **Performance is not a luxury. Performance is freedom.**

If your tool needs 2 GB of RAM, it won't run on a $200 laptop. If it takes 8 seconds to start, it won't run in a CI pipeline. If it requires Python, it won't run on a bare server without setup. Nika is a single Rust binary. No runtime. No dependencies. No Docker.

| Metric | **Nika** | Python equivalent |
|--------|------|-------------------|
| Cold start | **4 ms** | 800+ ms |
| RAM (idle) | **12 MB** | 60+ MB |
| Binary size | **~25 MB** | 200+ MB (with venv) |
| Dependencies | **0** (single binary) | pip install, venv, Docker... |
| Install | **Download and run** | `pip install`, `venv`, `requirements.txt`, pray |

A Raspberry Pi can run Nika. A GitHub Action can run Nika. A $5/month VPS can run Nika.

> **When your tool is lightweight, it goes everywhere.** That's not optimization for optimization's sake. That's reach. That's access. That's the mission.

## The Numbers

### RAM usage — "Summarize 10 web pages" task

| Tool | **Peak RAM** | **Cold start** | **Lines of config** |
|------|----------|------------|-----------------|
| **Nika** | **~45 MB** | **4 ms** | **12** |
| LangChain (Python) | ~230 MB | 1.2 s | 48 |
| LangGraph (Python) | ~210 MB | 1.1 s | 62 |
| CrewAI (Python) | ~280 MB | 1.4 s | 55 |

> Nika uses **5x less RAM** than LangChain for the same task.

### Agent reliability — multi-step autonomous tasks

| Tool | **Completion rate** | **Guardrails** | **Retry built-in** |
|------|----------------|------------|----------------|
| **Nika** | **Deterministic DAG** | Yes (NIKA-112) | Yes (exponential backoff) |
| CrewAI | ~56% (benchmark) | No | Manual |
| AutoGPT | Variable | No | No |
| LangGraph | Depends on graph | Partial | Manual |

> CrewAI reports a **44% failure rate** in multi-agent benchmarks. Nika's DAG execution is deterministic: tasks either complete with retries or fail with clear error codes. No silent drift.

### Security

| Tool | **Known critical CVEs (2024-2025)** | **Sandboxing** | **Dependency count** |
|------|------|------|------|
| **Nika** | **0** | Command blocklist + env validation | ~180 (compiled) |
| LangChain | CVSS 9.3 (CVE-2023-46229) + others | None by default | 400+ (runtime) |
| CrewAI | Inherits LangChain CVEs | None | 300+ (runtime) |

## The Name

In an old legend, there is a warrior who goes from place to place — not conquering, not ruling, but **liberating**. Not with weapons. Not with force. With joy.

> **The people called this warrior Nika.**

We chose this name because that's what this tool is for. Not to conquer a market. Not to build an empire. To **liberate** — AI from the labs, automation from the coders, power from the platforms. The butterfly is the symbol.

A butterfly is fragile, beautiful, and free. It transforms completely — from something earthbound to something that flies. And a single butterfly can start a storm on the other side of the world.

Nika is a butterfly. Small. Light. Free. And when enough people use it, when enough people realize that a 10-line text file can do what a $49/month platform does —

> **That's a storm.**

---

# PART 2 — THE COMPLETE STORY

## How a Solo Developer Built a 317,000-Line Rust Workflow Engine to Democratize AI

There is a moment in the manga One Piece when the protagonist Luffy awakens the power of Nika, the Sun God — a mythical figure whose entire essence is freedom, joy, and the refusal to accept limitation. The fruit of Nika grants its user a body "limited only by imagination." It is not a coincidence that a French developer named Thibaut Melen chose this name for a software project that aspires to do something remarkably similar for AI workflows. Nika, the workflow engine, is designed to be a body for AI — limited only by the YAML you write.

## The Problem: AI Orchestration is Broken

To understand why Nika exists, you need to understand the state of AI workflow tooling in 2025 and 2026. The AI revolution brought extraordinary capabilities — language models that can write code, analyze images, reason about complex problems, and generate creative content. But chaining these capabilities together into reliable, reproducible pipelines remained surprisingly difficult.

If you wanted to build a workflow that scrapes a website, sends the content to an LLM for analysis, generates a report, and posts the results to an API, you had roughly five options. You could write a Python script using LangChain, which meant hundreds of lines of imperative code that were difficult to version-control, impossible to audit, and fragile to maintain. You could use a visual builder like Dify or n8n, which required a server, a database, and Docker. You could reach for a data pipeline tool like Prefect or Airflow, designed for ETL jobs and awkwardly retrofitted for AI tasks. You could write raw API calls with requests and openai, producing throwaway scripts with no structure. Or you could use a multi-agent framework like CrewAI or AutoGen, which meant Python, more Python, and a lot of boilerplate Python.

Every single major AI orchestration tool in 2025 required either Python, a server, Docker, or Kubernetes. Often several of these at once. The barrier to entry was enormous.

Thibaut looked at this landscape and asked a heretical question: what if AI workflows could be declared in YAML and run from a single binary, the same way you run `git` or `cargo`? What if the workflow definition was the documentation, the specification, and the executable, all in one file? What if the entire thing was written in Rust so it compiled to a zero-dependency binary that ran on any machine without Python, Node.js, Docker, or the cloud?

That question became Nika.

## What Nika Actually Is

At its core, Nika is a semantic YAML workflow engine for AI tasks. You write a `.nika.yaml` file that describes a series of tasks, and Nika parses it, validates it, constructs a directed acyclic graph (DAG) of dependencies, and executes the tasks in the correct order — automatically parallelizing tasks that have no dependencies on each other.

Here is the simplest possible Nika workflow:

```yaml
schema: "nika/workflow@0.12"
workflow: hello-world
tasks:
  - id: greet
    infer: "Say hello to the world"
```

That is a complete, valid, executable workflow. Run `nika run hello-world.nika.yaml` and Nika will detect which LLM provider you have configured, call the model, and display the result. But Nika scales from that trivial example to workflows with dozens of tasks, multiple LLM providers, MCP tool integrations, structured JSON output validation, image processing pipelines, and autonomous agent loops — all declared in the same YAML format.

## The Five Verbs: Nika's Grammar

The design philosophy crystallizes in "the five verbs." Every task does exactly one of five things:

**infer:** calls a language model. It supports 22 LLM providers, vision (multimodal content with images), extended thinking, streaming, and structured output with JSON schema validation.

**exec:** runs a shell command. It includes a 28-pattern security blocklist and NFKC Unicode normalization to prevent homoglyph attacks.

**fetch:** makes an HTTP request with nine extraction modes: markdown conversion, article extraction, text, CSS selector matching, metadata, link classification, JSONPath, RSS/Atom feed parsing, and llms.txt.

**invoke:** calls an MCP tool. This is how Nika connects to NovaNet (the knowledge graph), external databases, and any tool that speaks MCP. Nika also ships 43 builtin tools under the `nika:*` namespace.

**agent:** runs an autonomous multi-turn loop with tools, guardrails, completion conditions, and cost limits.

The decision to have exactly five verbs was deliberate. Any AI task can be decomposed into these five primitives. The constraint forces clarity: every task has a single, unambiguous purpose.

## The Three-Phase AST: Why Most YAML Tools Are Wrong

Most YAML-based tools parse YAML and immediately try to execute it. Nika takes a fundamentally different approach. It treats `.nika.yaml` files the way a compiler treats source code, processing them through a three-phase pipeline inspired by rustc:

Phase 1 (Raw Parse): uses marked-yaml for span-preserving YAML parsing. Every value carries a source span — exact file, line, and column.

Phase 2 (Analysis): validates schema, interns task IDs (string to u32 for O(1) comparison), parses bindings, detects cycles, resolves MCP references, and collects ALL errors in a single pass.

Phase 3 (Lowering): converts to runtime-optimized types. Spans stripped, FxHashMap for faster hashing, Arc for zero-copy sharing across Tokio task spawns.

This is why Nika has an LSP with real-time completion, hover information, go-to-definition, and diagnostics.

## The Connection to NovaNet: Brain and Body

Nika is half of a two-part system. NovaNet is the brain — a knowledge graph engine with 59 node classes and 200+ locale definitions. Nika is the body — it executes workflows.

The two communicate exclusively through MCP. Nika never touches Neo4j directly — zero Cypher in the entire codebase. This architecture was inspired by Dr. Vegapunk from One Piece — the scientist who externalized his brain into satellite workers.

## Why AGPL

The creator of Nika is an open source activist who views the current AI landscape through the lens of the Great Pirate Era from One Piece. Open source AI projects are the pirates fighting for freedom against the "World Government" of closed-source big tech.

## The Solo Developer Story: 317K Lines of Rust

As of version 0.42, the project contains over 451,000 lines of Rust source code across 10 workspace crates. The largest crate, nika-engine, accounts for 162,547 lines. The TUI alone is 92,959 lines. The core AST library is 23,114 lines. This is the work of a solo developer, Thibaut Melen, working with AI assistance (Claude).

## The Course: Liberation Through Learning

Nika includes a built-in 12-level interactive course called "Liberation," themed after the One Piece narrative of freedom and discovery. 44 exercises organized in progressive difficulty. Levels: Jailbreak, Hot Wire, Fork Bomb, Root Access, Shapeshifter, Pay-Per-Dream, Swiss Knife, Gone Rogue, Data Heist, Open Protocol, Pixel Pirate, and SuperNovae (the final boss).

## The QR Code AI Connection

Nika is built to serve a real product: QR Code AI (qrcode-ai.com). This means every feature is motivated by a real production use case. The media pipeline exists because QR Code AI processes thousands of images. The structured output exists because marketing copy needs to conform to brand guidelines.

---

# PART 3 — THE FIVE VERBS PHILOSOPHY

## Why Exactly Five, What They Mean, and How They Change Everything

There is a principle in language design that says the expressiveness of a language is not determined by how many constructs it has, but by how well its constructs compose. C has fewer than forty keywords. SQL has five fundamental operations. UNIX has one principle: every program should do one thing well.

Nika applies this principle to AI workflows. The five verbs correspond to five fundamentally different types of computation:

- **Generation** (infer) — creating new content through AI inference
- **System interaction** (exec) — running commands on the local machine
- **Network communication** (fetch) — retrieving data from the internet
- **Tool calling** (invoke) — delegating to external capabilities via protocol
- **Autonomous reasoning** (agent) — multi-step decision-making with tool use

The constraint serves: **Readability** (you instantly know what kind of operation), **Analyzability** (the analyzer can reason statically), **Security** (only exec can run commands, only fetch can make network requests), **Composability** (all verbs consume bindings and produce results the same way).

### How the Five Verbs Compare to Other Approaches

**LangChain** uses chains — arbitrary sequences of Python function calls. No constraint = no way to reason without reading code.

**LangGraph** uses state graphs with conditional edges. More structured but still requires reading Python.

**CrewAI** uses role-based agents. Higher-level but non-deterministic.

**Dify** uses a visual canvas. Accessible but doesn't version-control well.

**n8n** uses visual trigger-based workflows. Not designed for AI-first.

Nika sits in a unique position: more constrained than imperative frameworks (only five operations in YAML), but this enables static analysis, LSP support, security auditing, and deterministic execution.

### Building with Five Verbs: Composition Patterns

- **ETL Pipeline**: fetch → infer → exec
- **Content Generation**: infer (plan) → infer (generate sections via for_each) → infer (review)
- **Research Agent**: agent (search the web) → infer (synthesize report)
- **Media Processing**: fetch (binary) → invoke (nika:import) → invoke (nika:thumbnail) → invoke (nika:optimize)
- **MCP Integration**: invoke (get from NovaNet) → infer (generate) → invoke (store back)
- **Multi-Provider Cost Optimization**: infer (cheap: Groq) → infer (expensive: Claude) → infer (cheap: DeepSeek)

### The "Workflow as Data" Argument

When workflows are code, they can do anything — but they're opaque. When workflows are data (YAML), they're analyzable, auditable, and toolable. You can build an LSP, a TUI, a security scanner, diff them in git, review them in PRs.

### Historical Context

**Makefiles** (1976): Declarative build specifications. WHAT not HOW. Nika's DAG applies the same principle.

**SQL** (1986): Small set of declarative verbs expressed the full range of data operations. Initially controversial ("too slow"). Won because constraints enabled optimization, access control, and transactions.

**Kubernetes manifests** (2014): Complex infrastructure declared in YAML. Nika applies this to AI workflows.

Each precedent proved that declarative, verb-constrained systems outperform imperative systems — not because they are more flexible, but because their constraints enable capabilities that flexibility prevents.

> The five verbs of Nika are not a limitation. They are a language. And like any good language, their power comes from what they can express when combined, not from how many words they contain.

---

# PART 4 — WHY RUST (The Engineering Story)

## Why Rust for a Workflow Engine, and What 451,000 Lines of It Buys You

Nika is not just a workflow engine. It is a compiler, a concurrent runtime, an image processor, an MCP protocol client, a TUI application, a language server, and an interactive course platform.

### The Performance Argument

The media pipeline uses fast_image_resize with SIMD instructions (Neon on ARM, AVX2 on x86) — thumbnails in under 20 milliseconds vs 200 milliseconds in Python's Pillow. BLAKE3 hashing is SIMD-accelerated with memory-mapped files.

But the real story is concurrency. Nika uses Tokio's multi-threaded runtime — every independent task runs concurrently on all CPU cores. Python's asyncio is limited by the GIL for CPU-bound operations. Rust has no such limitation.

### The Safety Argument

Nika handles untrusted input constantly. Buffer overflows, use-after-free, data races, null pointer dereferences — these simply do not compile in safe Rust. The security model for exec uses NFKC Unicode normalization to prevent homoglyph attacks. SVG sanitization is enforced by the type system.

### The 10-Crate Architecture

- **nika-core** (23,114 lines) — Zero-I/O foundation. AST types, parser, analyzer, catalogs. No tokio, no reqwest.
- **nika-engine** (162,547 lines) — Execution engine. Runtime, DAG, bindings, media pipeline, 43 builtin tools.
- **nika-tui** (92,959 lines) — Terminal interface. 3 views, 40+ widgets, real-time DAG visualization.
- **nika-event** (4,303 lines) — EventLog, TraceWriter. 41 event types, NDJSON traces.
- **nika-mcp** (8,996 lines) — MCP client. rmcp v0.16, connection pooling, retry logic.
- **nika-media** (3,516 lines) — Content-addressable storage. BLAKE3, zstd compression.
- **nika-cli** (8,576 lines) — CLI subcommands.
- **nika-lsp-core** (8,874 lines) — Protocol-agnostic LSP intelligence.
- **nika-lsp** (2,514 lines) — Standalone LSP binary.
- **nika** (2,217 lines) — CLI entry point.

### Key Dependencies

- **rig-core** (0.32) — Unified LLM interface for 22 providers
- **rmcp** (0.16) — Rust MCP client
- **marked-yaml** (0.8) — Span-preserving YAML parsing (enables LSP)
- **petgraph** (0.6) — Graph algorithms
- **dashmap** (6.1) — Concurrent HashMap (sharded RwLocks)
- **blake3** (1.8) — SIMD-accelerated hashing for CAS
- **fast_image_resize** — SIMD Lanczos3 image resampling
- **miette** (7.6) — Fancy error reporting (like rustc)

### AutoAgents Benchmark (2026)

| Framework | Lang | Avg Latency | Throughput | Peak Memory | Score |
|-----------|------|-------------|------------|-------------|-------|
| **AutoAgents** | **Rust** | **5,714 ms** | **4.97 rps** | **1,046 MB** | **98.03** |
| **Rig** (Nika's engine) | **Rust** | **6,065 ms** | **4.44 rps** | **1,019 MB** | **90.06** |
| LangChain | Python | 6,046 ms | 4.26 rps | 5,706 MB | 48.55 |
| LangGraph | Python | 10,155 ms | 2.70 rps | 5,570 MB | 0.85 |

**CrewAI excluded: 44% failure rate under test conditions.**

Memory: 5x advantage. Throughput: 36-84% advantage. Cold start: 15x advantage (4ms vs 62ms).

**Key Quote:** "The memory advantage is 5x, and it's structural — not something you tune away with configuration."

---

# PART 5 — COMPETITIVE POSITIONING

## Nika occupies a unique and defensible position

No other tool combines:
1. **Rust-native performance** (5x memory advantage over Python)
2. **Declarative YAML syntax** (closest to "Ansible for AI" — nobody else does this)
3. **5 verbs** semantic model (infer, exec, fetch, invoke, agent)
4. **MCP-native integration** (protocol-first, not bolted on)
5. **AGPL open-source** license (protects against cloud exploitation)

### Competitor Analysis

**LangChain/LangGraph:** Dominant mindshare, declining satisfaction. Debugging hell, performance overhead (2GB RAM for basic retrieval), breaking changes, CVE-2023-46229 (CVSS 9.3). LangGraph: 0.85/100 in benchmarks.

**CrewAI:** 44% failure rate in benchmarks. "Loop of Doom" — agents retry indefinitely, costs reaching $7/run. Not production-ready.

**AutoGen/AG2:** Split into two projects. Ecosystem confusion. Enterprise-focused, heavy Microsoft coupling.

**Flowise:** Visual drag-and-drop. Acquired by Workday (direction uncertain). Node.js runtime.

**n8n:** Mature automation. AI bolted on, not native. General workflow, not AI-first.

### Competitive Matrix

| Dimension | Nika | LangChain | CrewAI | Dify | n8n |
|-----------|------|-----------|--------|------|-----|
| Language | Rust | Python | Python | Python | JS/TS |
| Paradigm | YAML | Code | Role agents | GUI | Visual |
| Memory | ~1 GB | ~5.7 GB | N/A (excluded) | Unknown | Unknown |
| Reliability | DAG execution | Debug hell | 44% failure | Growing | Mature |
| MCP | Native invoke: | Plugin | No | Supported | No |
| License | AGPL-3.0 | MIT | MIT | Apache | Sustainable Use |
| Cold start | 4ms | 62ms | Unknown | Seconds | Seconds |

### Three Moats

1. **Performance moat:** 5x memory, 4ms cold start — structural advantages Python cannot match
2. **Declarative moat:** YAML-first (not YAML-serialized) — "Ansible for AI" unclaimed
3. **Protocol moat:** MCP-native from day one

### What competitors need to match Nika

| To match... | They would need to... |
|---|---|
| Performance | Rewrite in Rust (years) |
| YAML-native | Redesign core abstraction (breaking change) |
| MCP-native | Add protocol (months, bolted-on) |
| Single binary | Abandon interpreter runtimes |

---

# PART 6 — 28 REAL-WORLD USE CASES

## A — Sales & Lead Operations (Highest volume of paid users)

**1. B2B Lead Enrichment from LinkedIn + Company Websites**
Manually researching 500+ leads/week. Nika: fetch → infer (summarize + score) → exec (push to CRM). n8n's #1 workflow category.

**2. Personalized Cold Outreach at Scale**
Research prospect's blog/tweets, generate personalized 3-paragraph email. fetch → infer → exec chain with rate limits.

**3. CRM Hiring Spike Detection & Competitive Intel Alerts**
Monitor 100+ accounts for job surges. Scheduled DAG fanning out. PredictLeads + Crunchbase → infer → Slack.

## B — Content & Media Production (Fastest growing)

**4. AI Video Pipeline: Script → Avatar → TikTok/Instagram Upload**
Daily short-form video. LLM script → ElevenLabs voice → VEO3 video → auto-post. 6+ top trending n8n templates.

**5. Multi-Platform Content Repurposing**
One blog post → Twitter thread, LinkedIn carousel, IG reel, podcast clip, email excerpt. DAG fan-out, 5 parallel infer tasks.

**6. SEO Content Factory**
Keyword rankings → gap analysis → article generation → internal linking → CMS publish. Structured output validation at each step.

## C — Document Processing (Highest enterprise value)

**7. Cancer Research Data Pipeline** (Flatiron Health)
Millions of clinical records. Structured data extraction, de-identification. **Saved 2.5 FTE weeks per project.**

**8. Climate Policy Processing** (Climate Policy Radar)
25,000 PDFs, 70+ classifiers, 350,000 annual users. **"Months saved."** 173,000 synthetic Q&A pairs, ~1M workflow runs.

**9. Contract Risk Scanning**
Parse PDFs into clauses, compare against legal playbook, flag non-compliance. Langflow featured template.

**10. Receipt/Invoice Processing**
Vision LLM extracts vendor/amount/date from photos. Push to QuickBooks/Xero.

## D — IT Operations & DevOps (Proven enterprise ROI)

**11. Employee Account Recovery** (Delivery Hero)
53,000 employees, 70+ countries. **Saved 200 hours/month from ONE workflow.** 5 hours to deploy.

**12. Security Alert Triage (SOAR)**
SIEM → threat intel → AI classify → containment → ticket. Enterprise customers: Meta, Microsoft, Vodafone.

**13. Incident Response** (WHOOP)
PagerDuty → AI root cause → Slack → runbook. **Incidents cut 75%, MTTR improved 40%+.**

## E — Customer Support

**14. Support Ticket Classification + AI Draft Response**
Auto-classify, RAG draft, route. Dify serves 19,000+ employees across 20+ departments.

**15. RAG Chatbot over Internal Docs**
Dify: 1M+ apps deployed. Ricoh reduced **18,000 hours/year**.

## F — Data Engineering & Analytics

**16. Daily KPI Dashboard**
Parallel fetch from Stripe/GA/HubSpot/GitHub → AI narrative → Slack.

**17. VC Deal Scouting**
Weekly scan → AI company profiles → PDF digest.

## G — E-Commerce

**18. Product Descriptions from Supplier CSV + Images** (50k SKUs)
for_each CSV rows → vision → descriptions → Shopify API.

**19. Customer Review Analysis**
Aggregate from 5 platforms → sentiment → response generation → post.

## H — Compliance & Legal

**20. GDPR/Privacy Compliance Scan**
Crawl sites → detect trackers → compare privacy policy → report.

**21. Regulatory Document Monitoring**
Monitor 50+ government sites → change detection → impact analysis → alert.

## I — Developer Tools

**22. PR Review Agent**
GitHub webhook → diff → parallel: AI review + SAST → post comments.

**23. Documentation Generation from Code Changes**
git diff → identify API changes → generate docs → create PR.

## J — Healthcare

**24. Medical Literature Monitoring**
Daily PubMed scan → AI summarize → weekly digest. Flatiron + Snorkel AI (20x throughput).

**25. Patient Intake Form Processing**
Scanned forms → vision extract → EHR. HIPAA-compliant with local model routing.

## K — Media & Creative

**26. AI Podcast Generation**
Research → script → multi-voice TTS → audio merge → show notes → upload.

**27. QR Code Art Pipeline** (Nika's core domain)
AI design → nika:qr_validate → conditional retry → variants → C2PA provenance.

## L — Finance & Real Estate

**28. Real Estate Listing Enrichment**
MLS → vision describe photos → AI listing text → optimize images → publish to platforms.

---

## Why YAML/Code-First > No-Code

| No-Code Pain | Nika Solution |
|---|---|
| No version control | .nika.yaml lives in git |
| Can't test workflows | nika check validates. CI/CD. |
| Can't handle binary data | CAS, response: binary, 24 media tools |
| No structured output validation | Schema validation per step, retry |
| Security audit impossible | YAML is plain text, auditable |
| Data leaves your network | Self-hosted. PHI/PII stays. |

> Climate Policy Radar rejected AWS Step Functions: "JSON-style ASL pipeline definitions are much harder for developers."
> Flatiron Health: "I don't have to think about the inner workings... I just have to understand code."

## Top Industries by Adoption

| Rank | Industry | Why |
|---|---|---|
| 1 | **Sales/Growth Ops** | Gateway drug. Highest volume. |
| 2 | **Content/Media** | AI video exploding. Media pipeline = moat. |
| 3 | **Healthcare/Pharma** | Highest $/deal. Compliance = self-hosted. |
| 4 | **IT/SecOps/SRE** | Proven ROI: 200h/month saved. |
| 5 | **Legal/Compliance** | Contract analysis. High $/workflow. |

---

# PART 7 — KEY NUMBERS

| Metric | Value |
|--------|-------|
| Total Rust code | ~451K lines |
| Workspace crates | 10 |
| Tests | 7,800+ (lib only) |
| LLM providers | 22 (7 cloud + native + aliases) |
| MCP aliases | 100 |
| Builtin tools | 12 core + 24 media + 5 file |
| Showcase workflows | 200+ |
| Course exercises | 44 across 12 levels |
| Error codes | NIKA-000 through NIKA-314 |
| Extract modes (fetch) | 9 |
| Pipe transforms | 27 |
| Cold start | 4 ms |
| Peak RAM (10 web pages) | ~45 MB |

---

# PART 8 — THE STACK

```
You write:          A .nika.yaml file (plain text, human-readable)
Nika reads:         5 verbs, DAG of tasks, any AI provider
Nika runs:          Parallel execution, streaming, retries, cost tracking
You get:            Results. On your machine. Under your control.
```

### Install

```bash
# macOS
brew install supernovae-st/tap/nika

# From source
cargo install nika

# Then
nika run my-automation.nika.yaml
```

---

**Liberate your AI.**
