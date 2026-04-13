# Nika Press Kit

> Official press materials for Nika, the semantic YAML workflow engine for AI tasks.
> Last updated: March 2026

---

## Company Boilerplate

### Short (50 words)

SuperNovae Studio builds open source AI infrastructure. Its flagship product, Nika, is a semantic YAML workflow engine written in Rust that lets anyone orchestrate AI tasks --- from LLM inference to media processing --- with five intuitive verbs, zero dependencies, and a single binary. Licensed AGPL-3.0.

### Medium (100 words)

SuperNovae Studio is an independent software studio building the next generation of open source AI tooling. Its flagship project, Nika, is the first and only Rust-based AI workflow engine --- a single binary that transforms declarative YAML files into fully orchestrated AI pipelines. With five semantic verbs (infer, exec, fetch, invoke, agent), built-in support for 9 LLM providers, 24 media processing tools, native MCP protocol integration, and a terminal UI built on ratatui, Nika occupies a category of one. The project ships under the AGPL-3.0-or-later license, reflecting a deliberate philosophical commitment to keeping AI infrastructure free and unenclosable.

### Long (250 words)

SuperNovae Studio is an independent software studio founded by Thibaut Melen, dedicated to building open source AI infrastructure that democratizes access to intelligent workflow automation. The studio's architecture pairs two complementary systems: NovaNet, a knowledge graph that serves as the "brain," and Nika, a semantic YAML workflow engine that serves as the "body." They communicate exclusively via the Model Context Protocol (MCP), Anthropic's open standard for agent-tool interaction.

Nika is the studio's flagship project: approximately 482,000 lines of code across 10 Rust workspace crates, compiled into a single zero-dependency binary. It introduces a paradigm where AI workflows are defined entirely in YAML using five semantic verbs --- infer (LLM generation), exec (shell commands), fetch (HTTP requests), invoke (MCP tool calls), and agent (multi-turn autonomous loops). Tasks compose into directed acyclic graphs (DAGs) with typed bindings, structured output validation, and full event-sourced observability.

The engine supports 9 LLM providers: 8 cloud providers (OpenAI, Anthropic, Google Gemini, Mistral, Groq, xAI, DeepSeek) plus mistral.rs for local GGUF inference --- all coexisting in the same binary. Its media pipeline provides 24 built-in tools for image processing, PDF extraction, chart generation, and C2PA content provenance. A ratatui-based terminal UI, a Language Server Protocol implementation, and a 12-level interactive learning course round out the developer experience.

Nika ships under the AGPL-3.0-or-later license --- a deliberate choice to ensure that cloud providers and SaaS platforms cannot enclose the technology behind proprietary walls. The name Nika references the Sun God from One Piece, a symbol of liberation, joy, and freedom. The project's butterfly symbol represents transformation: the idea that open source AI tools can metamorphose entire industries.

---

## Founder Bio

### Thibaut Melen --- Founder, SuperNovae Studio

Thibaut Melen is the founder and sole developer of SuperNovae Studio and the creator of Nika, the first Rust-based AI workflow engine. A software engineer and open source advocate, Melen built the entire 482,000-line codebase himself, driven by the conviction that AI orchestration tools should be free, fast, declarative, and accessible to everyone --- not locked behind Python runtimes, Docker containers, or cloud subscriptions.

Melen's technical philosophy centers on what he calls "infrastructure-as-code for AI" --- the idea that AI workflows deserve the same rigor and reproducibility that Terraform brought to cloud infrastructure and Docker Compose brought to containerization. He chose Rust for its memory safety, performance characteristics, and zero-cost abstractions; YAML for its human readability and version-control friendliness; and the AGPL license for its ironclad protection against proprietary enclosure.

The name "Nika" and the project's broader cultural identity draw from Eiichiro Oda's One Piece manga --- specifically the Sun God Nika, whose power is "limited only by imagination" and who embodies liberation through joy. Melen sees a direct parallel between the manga's themes of freedom and the open source movement's fight against corporate enclosure of AI technology.

**Contact:** thibaut@supernovae.studio
**GitHub:** [@ThibautMelen](https://github.com/ThibautMelen)
**Organization:** [@supernovae-st](https://github.com/supernovae-st)

---

## Product Description

### What Nika Is

Nika is a semantic YAML workflow engine for AI tasks. It compiles declarative YAML files into directed acyclic graphs (DAGs) and executes them with full type safety, structured output validation, and event-sourced observability. It ships as a single Rust binary with zero runtime dependencies.

### What Makes It Different

Every major AI orchestration tool in 2025--2026 requires either Python, a server, Docker, or Kubernetes. Nika requires none of them. It is:

- **The only YAML-native AI workflow engine that ships as a single binary.** Haystack comes closest but requires Python. CrewAI requires Python. Julep requires a server.
- **The only Rust-based AI workflow engine.** Windmill is Rust but requires a server and PostgreSQL. Codex CLI is Rust but is a coding agent, not a workflow engine.
- **The first non-IDE, non-assistant CLI tool to implement the MCP client protocol.** All other MCP clients are AI assistants (Claude, ChatGPT), IDEs (Cursor, VS Code), or operating systems (Windows 11).
- **The only AI tool with a built-in terminal UI.** No competitor offers a ratatui/bubbletea-style TUI for AI workflow management.
- **The only tool combining cloud LLM providers with local GGUF inference in one binary.** Switch from GPT-4o to a local Mistral model by changing one line of YAML.
- **The first AI tool to use content-addressable storage (CAS) for media assets in workflows.** Images are hashed, stored once, and referenced by hash --- making workflows reproducible and portable.

### The Five Verbs

| Verb | Purpose | Example |
|------|---------|---------|
| `infer:` | LLM text/vision generation | Generate summaries, analyze images, extract structured data |
| `exec:` | Shell command execution | Run scripts, call system tools, process files |
| `fetch:` | HTTP requests with extraction | Scrape web pages, call APIs, parse RSS feeds |
| `invoke:` | MCP tool calls | Connect to NovaNet, external MCP servers, 100+ aliases |
| `agent:` | Multi-turn autonomous loops | Autonomous research, code generation, complex reasoning |

### Technical Specifications

| Metric | Value |
|--------|-------|
| Language | Rust (edition 2021, MSRV 1.86) |
| Total codebase | ~482,000 lines across 10 workspace crates |
| Rust source code | ~337,000 lines in 659 files |
| YAML workflows | 570 files (showcase + course + examples) |
| Test suite | 8,300+ unit tests, zero clippy warnings |
| Binary size | Single static binary, no runtime dependencies |
| LLM providers | 9 (8 cloud + local GGUF + vision) |
| Media tools | 24 built-in (3 tiers: always-on, default, opt-in) |
| MCP aliases | 100+ pre-configured tool aliases |
| Showcase workflows | 115 ready-to-use workflow templates |
| Learning course | 12 levels, 44 exercises, interactive progression |
| Schema version | nika/workflow@0.12 |
| Current version | v0.49.0 |
| License | AGPL-3.0-or-later |

---

## Key Milestones

| Date | Milestone |
|------|-----------|
| 2025 | Project inception; core engine development begins |
| 2026-Q1 | v0.27.0 --- Stable AST pipeline, DAG execution, multi-provider support |
| 2026-Q1 | v0.34.0 --- Vision support (multimodal content), native GGUF inference, media pipeline with CAS |
| 2026-Q1 | v0.35.0 --- Fetch v2 with 9 extraction modes (markdown, article, metadata, links, feeds, etc.) |
| 2026-Q1 | v0.37.0 --- Cargo workspace unification across 10 crates |
| 2026-Q1 | v0.38.0 --- Crate split: 10 independent workspace crates for embeddable runtime |
| 2026-Q1 | v0.42.0 --- `nika init` + 12-level interactive course, 115 showcase workflows |
| 2026-Q1 | v0.42.0 --- Full security audit, LSP improvements, AI coding tool integration suite |
| Ongoing | Approaching public launch; Homebrew tap, GitHub releases, crates.io, VS Code marketplace |

---

## Logo and Brand Guidelines

### Brand Identity

- **Name:** Nika (pronounced NEE-kah)
- **Symbol:** The butterfly --- representing transformation, liberation, and new beginnings
- **Colors:** Electric blue (#0000FF, SuperNovae brand blue), warm gold/coral for community elements
- **Tagline:** "Semantic YAML workflows for AI tasks"
- **Extended tagline:** "Five verbs. Zero dependencies. Infinite workflows."

### The Butterfly

The Nika butterfly is the project's signature symbol. In the One Piece manga that inspires the project, butterflies represent the spread of freedom --- they appear whenever the Sun God Nika's power activates, spreading liberation in every direction. In the Nika project, the butterfly symbolizes the transformation that open source AI tools can bring: small, beautiful, impossible to contain.

### Name Origin

Nika is named after the Sun God Nika from Eiichiro Oda's One Piece manga. The Sun God Nika is a legendary figure whose power is "limited only by imagination" and who embodies liberation through joy. The Hito Hito no Mi, Model: Nika is called "the most ridiculous power in the world" --- because freedom, at its core, is joyful and absurd. The parallel to software is deliberate: Nika the engine is limited only by the YAML you write.

### Usage Guidelines

- Always capitalize "Nika" when referring to the product
- "SuperNovae Studio" is the company name (note the "ae" ending)
- The butterfly emoji can be used in informal contexts
- Do not use "Nika AI" --- the product is "Nika" or "the Nika workflow engine"
- When referencing the architecture: "NovaNet (brain) + Nika (body)"

---

## Media Contact

**Press inquiries:** thibaut@supernovae.studio

**GitHub:** https://github.com/supernovae-st/nika

**Website:** https://supernovae.studio

**Organization:** SuperNovae Studio

For interview requests, speaking engagements, or review copies, contact Thibaut Melen directly at the email above.

---

## Quick Facts for Journalists

1. **Solo developer project.** The entire 482,000-line codebase was written by one person.
2. **100% Rust.** No Python, no JavaScript, no C bindings. Pure Rust, compiled to a single binary.
3. **AGPL-3.0 licensed.** A deliberate, philosophical choice to prevent cloud exploitation.
4. **Named after a manga character.** The Sun God Nika from One Piece, symbolizing liberation.
5. **Creates a new category.** "Declarative CLI AI Workflow Engine" --- no other tool occupies this position.
6. **Zero dependencies for users.** Download one binary, set an API key, run workflows.
7. **9 LLM providers in one binary.** Cloud and local inference coexist without external processes.
8. **MCP protocol pioneer.** First CLI workflow tool to implement Anthropic's Model Context Protocol.
9. **Built-in terminal UI.** The only AI workflow tool with a ratatui-based TUI.
10. **Interactive learning course.** 12 levels, 44 exercises, inspired by a constellation/liberation theme.

---

## Competitive Positioning

```
                    Code-only          YAML-native
                    (Python/TS)        (declarative)
                    |                  |
Server-based  ---- | Prefect          | Argo
                    | Airflow          | (K8s only)
                    | Windmill         |
                    | Temporal         |
                    |                  |
Cloud/SaaS    ---- | LangChain        | Julep
                    | LangGraph        | Google Cloud
                    | CrewAI           |   Workflows
                    | AutoGen          |
                    |                  |
Visual        ---- | Dify             |
                    | n8n              |
                    | Flowise          |
                    |                  |
Single binary ---- | Codex CLI        | Nika  <-- UNIQUE
                    | (coding only)    |
                    |                  |
```

Nika occupies the only position at the intersection of "YAML-native" and "single binary."

---

## Verified Marketing Claims

These claims have been verified against 80+ sources across 11 research queries (see `/docs/research/2026-03-20-competitive-landscape.md`):

1. "The only YAML-native AI workflow engine that ships as a single binary" --- **Verified**
2. "The first CLI workflow tool with native MCP client support" --- **Verified**
3. "The only AI tool with a built-in terminal UI for workflow management" --- **Verified**
4. "The only tool combining 8+ cloud LLM providers with local GGUF inference in one binary" --- **Verified**
5. "Content-addressable storage for AI media workflows --- a first" --- **Verified**
6. "The only Rust-based AI workflow engine" --- **Verified**
7. "Zero dependencies: no Python, no Docker, no server, no database" --- **Verified**

---

## Sample Workflow

```yaml
# content-pipeline.nika.yaml
schema: nika/workflow@0.12

tasks:
  - id: research
    fetch:
      url: https://news.ycombinator.com
      extract: article

  - id: analyze
    infer:
      model: claude-sonnet-4-20250514
      prompt: "Analyze these tech trends and identify the top 3 themes"
      context: "{{with.research.body}}"
    with: { research: $research }
    depends_on: [research]

  - id: report
    infer:
      model: claude-sonnet-4-20250514
      prompt: "Write a professional briefing based on this analysis"
      context: "{{with.analysis.text}}"
      output:
        schema:
          type: object
          properties:
            title: { type: string }
            themes: { type: array }
            summary: { type: string }
    with: { analysis: $analyze }
    depends_on: [analyze]
```

Three tasks. Two verbs. One file. Zero dependencies. That is Nika.

---

*Press kit prepared by SuperNovae Studio. All statistics verified against source code as of March 2026. For corrections or updates, contact thibaut@supernovae.studio.*
