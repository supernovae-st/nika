# Press Release -- Nika Launch

> Official press release for the Nika open source launch.
> Format: standard press release structure for tech publications.

---

<!-- BEGIN PRESS RELEASE -->

## FOR IMMEDIATE RELEASE

# SuperNovae Studio Launches Nika: A 451K-Line Rust Engine That Replaces AI Workflow SDKs with 5 YAML Verbs

**Open source workflow engine supports 22 LLM providers, 24 built-in media tools, and ships with a 44-exercise interactive course -- all in a single binary**

**Location:** Paris, France
**Date:** [Launch Date], 2026
**Contact:** Thibaut Melen, SuperNovae Studio -- thibaut@supernovae.studio

---

### Lead

SuperNovae Studio today announced the public release of **Nika**, a semantic YAML workflow engine for AI tasks written in 451,000 lines of Rust. Nika enables developers to orchestrate AI workflows using five declarative verbs -- `infer:`, `exec:`, `fetch:`, `invoke:`, and `agent:` -- replacing hundreds of lines of SDK boilerplate with readable YAML files that serve as both configuration and documentation.

Available immediately on GitHub and crates.io under the AGPL-3.0 license, Nika supports 22 LLM providers (including Claude, GPT-4o, Gemini, Mistral, Groq, and DeepSeek), includes 24 built-in media processing tools, and ships as a single binary with zero runtime dependencies.

---

### The Problem

AI workflow tooling in 2026 forces developers into an uncomfortable choice: visual builders that can't be version-controlled, or Python SDKs that require learning proprietary abstractions. Neither approach produces workflows that are readable by non-specialists, reviewable in pull requests, or portable across environments without runtime dependencies.

"Every AI team I've worked with has the same problem," said Thibaut Melen, founder of SuperNovae Studio and creator of Nika. "The person who wrote the LangChain pipeline left the company, and nobody else can figure out what it does. With Nika, the workflow IS the documentation. Open a YAML file, read 5 verbs, understand the entire pipeline in 30 seconds."

---

### The Solution: Five Verbs, One Engine

Nika's design principle is radical simplicity: every AI workflow task maps to exactly one of five semantic verbs.

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM generation | Text generation, data analysis, entity extraction |
| `exec:` | Shell commands | Build code, run tests, convert files |
| `fetch:` | HTTP requests | Scrape web pages, call APIs, parse RSS feeds |
| `invoke:` | MCP tool calls | Process images, query databases, call services |
| `agent:` | Multi-turn loops | Coding agents, research agents, analysis agents |

Tasks declare dependencies through data bindings, and Nika automatically constructs a directed acyclic graph (DAG) for optimal parallel execution. No explicit ordering is needed -- the engine infers the execution schedule from the data flow.

---

### Key Features

**Multi-Provider LLM Support:** Nika supports 22 LLM providers with a unified syntax. Developers can mix providers within a single workflow, routing cheap tasks to fast models (Groq, DeepSeek) and complex tasks to powerful models (Claude, GPT-4o), reducing costs by up to 60% compared to single-model approaches.

**Built-in Media Pipeline:** 24 media processing tools ship with the engine, including SIMD-accelerated image resizing, PDF text extraction, chart generation from JSON data, C2PA content credential signing for EU AI Act compliance, and QR code validation. All tools operate through a content-addressable storage layer that prevents path traversal attacks.

**Interactive Learning Course:** `nika init --course` generates a 12-level, 44-exercise interactive course that teaches developers to use every feature through hands-on practice. Progressive hints (3 tiers), auto-validation on file save, and a constellation progress map provide a guided learning experience.

**MCP-Native Architecture:** Nika integrates with the Model Context Protocol (MCP), enabling connectivity to any MCP-compatible server -- databases, APIs, knowledge graphs, and custom tools. The companion project NovaNet provides a Neo4j-backed knowledge graph accessible via MCP.

**Terminal UI:** A full-featured terminal UI built with 92K lines of ratatui provides live DAG visualization, streaming LLM output, real-time cost tracking, and three operational views (Studio, Command, Control).

**Language Server Protocol:** An LSP server provides completions, diagnostics, and hover documentation for VS Code, Neovim, and other editors, bringing IDE-quality developer experience to YAML workflow authoring.

---

### Technical Specifications

| Specification | Detail |
|--------------|--------|
| Language | Rust 1.86+ |
| Codebase | 451K lines across 10 workspace crates |
| Tests | 8,100+ passing, zero clippy warnings |
| Schema | nika/workflow@0.12 |
| Deployment | Single binary, zero runtime dependencies |
| License | AGPL-3.0-or-later |
| Platforms | macOS (arm64, x86_64), Linux (x86_64) |
| Install | `cargo install nika` |

---

### Architecture: Brain + Body

Nika is designed as the "body" of a two-part system. The companion project **NovaNet** serves as the "brain" -- a knowledge graph that stores entities, relationships, and semantic context. The two communicate exclusively via the Model Context Protocol, maintaining clean architectural separation.

"We call it the brain-and-body architecture," Melen explained. "NovaNet knows things -- entities, locales, knowledge atoms. Nika does things -- workflows, LLM calls, media processing. MCP is the nervous system connecting them. Zero Cypher in Nika. Zero workflow logic in NovaNet."

---

### Why AGPL-3.0

SuperNovae Studio chose the AGPL-3.0 license to ensure Nika remains truly open source. The AGPL requires that any modifications to Nika offered as a hosted service must be shared with the community, preventing the "strip-mining" pattern where cloud providers fork open source projects, add proprietary features, and compete against the original creators.

"We've watched this pattern destroy open source projects," said Melen. "Elasticsearch, Redis, Terraform -- the story is always the same. A cloud provider takes the community's work and closes the door. The AGPL is our way of saying: this door stays open."

Developers using Nika as a CLI tool face no license restrictions -- personal, commercial, or enterprise use is unrestricted. The AGPL only applies to modifications distributed or offered as a service.

---

### What Developers Are Building

Early adopters have used Nika for:

- **Content pipelines:** Multi-model workflows that research, analyze, write, and publish content with 60% cost reduction versus single-model approaches
- **Image processing automation:** Pipelines that import, resize, optimize, and validate images using built-in media tools without external services
- **Code analysis agents:** Multi-turn LLM agents with file system access, guardrails, and cost limits for automated code review and refactoring
- **Data extraction:** Web scraping workflows with 9 built-in extract modes (markdown, article, RSS, JSONPath, metadata, and more)
- **QR code quality assurance:** Automated scan score validation combined with LLM-powered visual analysis

---

### Availability

Nika v0.42.0 is available immediately:

- **GitHub:** https://github.com/supernovae-st/nika
- **crates.io:** `cargo install nika`
- **Homebrew:** `brew install supernovae-st/tap/nika`
- **Documentation:** https://github.com/supernovae-st/nika/wiki
- **Showcase:** 200+ ready-to-use workflows included

---

### About SuperNovae Studio

SuperNovae Studio is an independent software studio founded by Thibaut Melen, building open source AI infrastructure. The studio's products include Nika (workflow engine), NovaNet (knowledge graph), and QR Code AI (https://qrcode-ai.com). Based in Paris, France, SuperNovae Studio is committed to building AI tools that remain accessible, open, and community-owned.

**Website:** https://supernovae.studio
**GitHub:** https://github.com/supernovae-st
**Contact:** thibaut@supernovae.studio

---

---

### Technical Backgrounder

#### The 5-Verb Design Philosophy

Nika's architecture is built on a single observation: every AI workflow task is one of five operations. The `infer:` verb calls any LLM from 22 supported providers. The `exec:` verb runs shell commands with a 28-pattern security blocklist. The `fetch:` verb makes HTTP requests with 9 built-in extraction modes (markdown, article, RSS, JSONPath, metadata, links, and more). The `invoke:` verb calls MCP tools, including 24 built-in media tools. The `agent:` verb runs multi-turn agentic loops with guardrails and cost limits.

These five verbs compose into directed acyclic graphs (DAGs) through data bindings. When a task declares `with: { data: $other_task }`, Nika automatically resolves the dependency and schedules execution. Tasks without dependencies run in parallel. No explicit ordering is needed.

#### Multi-Model Cost Optimization

A key innovation in Nika is the ability to mix LLM providers within a single workflow. Developers can route computationally simple tasks (data formatting, basic extraction) to cost-effective providers like Groq ($0.06/1M input tokens) or DeepSeek ($0.14/1M tokens), while reserving premium providers like Claude for tasks requiring deep analysis or creative output.

In internal benchmarks, multi-model workflows reduced LLM costs by approximately 60% compared to single-model approaches, while maintaining output quality on the complex tasks that matter most.

#### The Media Pipeline

Unlike competing workflow tools, Nika includes a complete media processing pipeline built in Rust. The 24 tools cover the most common media operations:

- **Image processing:** SIMD-accelerated Lanczos3 resizing, format conversion (PNG/JPEG/WebP), lossless PNG optimization via oxipng, EXIF metadata stripping
- **Analysis:** Perceptual hashing for duplicate detection, image quality assessment (DSSIM/SSIM), dominant color extraction, QR code scanning with readability scoring
- **Documents:** PDF text extraction, SVG to PNG rasterization via resvg, chart generation from JSON data
- **Content authenticity:** C2PA content credential signing and verification for EU AI Act compliance
- **Infrastructure:** Content-addressable storage (CAS) with deduplication, in-memory pipeline chaining for zero-intermediate-file workflows

All tools operate through the CAS layer, where files are referenced by content hash rather than path. This prevents path traversal attacks and enables automatic deduplication.

#### The Interactive Course System

`nika init --course` generates a complete learning journey: 12 levels with 44 exercises that progress from basic LLM calls to multi-agent orchestration. Each exercise is a partially-complete `.nika.yaml` file with `# TODO` markers. The learner fills in the blanks, and Nika validates the solution.

The course includes:
- **Progressive hints:** 3 tiers (nudge, guide, solution) accessible via `nika course hint`
- **Auto-validation:** `nika course watch` monitors files for changes and validates automatically
- **Progress tracking:** A constellation progress map shows completed and upcoming levels
- **Offline operation:** The entire course runs locally, no internet required (except for LLM calls)

The course follows a "liberation journey" theme: each level name evokes emergence and freedom, from "Spark" (first workflow) through "Aurora" (media pipeline) to "Nova" (full mastery). The progression mirrors the butterfly lifecycle -- cocoon to flight.

#### Integration with NovaNet

Nika is designed as the "body" in a brain-body architecture. NovaNet, a companion project built on Neo4j, serves as the "brain" -- a knowledge graph that stores entities, relationships, locales, and semantic context. Communication between Nika and NovaNet occurs exclusively through the Model Context Protocol (MCP), maintaining clean architectural separation.

This architecture enables entity-aware AI workflows: a content generation pipeline can query NovaNet for brand guidelines, locale-specific expressions, and cultural taboos, then use that context to produce culturally appropriate output in any language.

---

### Industry Context

The AI workflow market is experiencing rapid growth as organizations move beyond proof-of-concept LLM deployments toward production-grade pipelines. Current tools fall into three categories: visual builders (Dify, n8n) optimized for simplicity, Python SDKs (LangChain, LangGraph) optimized for flexibility, and general-purpose orchestrators (Temporal, Prefect) optimized for durability.

Nika targets a fourth category: declarative, AI-specific workflow engines that prioritize readability, reviewability, and operational simplicity. The YAML-first approach addresses a growing pain point among development teams: AI pipelines that become opaque and unmaintainable as they grow in complexity.

The choice of AGPL-3.0 licensing reflects a broader trend in open source toward copyleft licenses that prevent cloud-provider exploitation -- a pattern seen in the re-licensing of Elasticsearch (to SSPL), Redis (to dual license), and Terraform (to BSL).

---

### Media Assets

High-resolution logos, screenshots, and architecture diagrams are available at:
https://github.com/supernovae-st/nika/tree/main/docs/assets

### Press Contact

**Thibaut Melen**
Founder, SuperNovae Studio
thibaut@supernovae.studio
https://github.com/ThibautMelen

###

<!-- END PRESS RELEASE -->
