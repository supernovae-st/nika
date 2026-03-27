# Newsletter Features --- Nika

> Three newsletter-style features in different formats.
> Written in third person, journalistic tone. Ready for submission.

---

## Feature 1: "Tool of the Week" Style (500 words)

### Nika: The AI Workflow Engine That Fits in One Binary

**What it is:** Nika is a semantic YAML workflow engine for AI tasks. Users define workflows in declarative YAML files using five verbs --- infer (LLM generation), exec (shell commands), fetch (HTTP requests), invoke (MCP tool calls), and agent (multi-turn autonomous loops) --- and run them from a single Rust binary with zero runtime dependencies.

**Why it matters:** Every AI orchestration tool on the market requires Python, Docker, a server, or Kubernetes. Nika requires none of them. Download one binary, set an API key, and run workflows. This creates a new category: "Declarative CLI AI Workflow Engine."

**The pitch in 30 seconds:** Write a YAML file that fetches a webpage, analyzes it with Claude, and outputs a structured report. Run `nika run pipeline.nika.yaml`. That is the entire workflow.

```yaml
tasks:
  - id: scrape
    fetch:
      url: https://news.ycombinator.com
      extract: markdown

  - id: analyze
    infer:
      model: claude-sonnet-4-20250514
      prompt: "Top 3 themes: {{with.page.body}}"
    with: { page: $scrape }
    depends_on: [scrape]
```

**Key stats:**
- ~482,000 lines of Rust across 10 workspace crates
- 9 LLM providers (cloud + local GGUF inference in one binary)
- 24 built-in media tools (image resize, PDF extract, C2PA provenance)
- Built-in terminal UI, LSP, and 12-level learning course
- 115 showcase workflow templates
- First CLI tool to implement Anthropic's MCP protocol
- AGPL-3.0-or-later license

**Who is it for:** DevOps engineers who think in YAML. Data teams that want reproducible AI pipelines without Python dependency management. Solo developers and small teams that need AI automation without infrastructure overhead. Anyone who has ever thought "I just want to run an LLM pipeline without Docker."

**Who built it:** Thibaut Melen, founder of SuperNovae Studio. Solo developer. The entire codebase is his work.

**The name:** Named after the Sun God Nika from One Piece, symbolizing liberation and freedom. The project's butterfly symbol represents transformation.

**The license:** AGPL-3.0-or-later --- a deliberate philosophical choice. AGPL prevents cloud providers from taking the code, wrapping it in a managed service, and monetizing it without contributing back. For a CLI tool that users run locally, this has minimal impact on most users but maximum protection against corporate enclosure.

**Getting started:** Install the binary, run `nika init --course` for a guided learning experience, or `nika showcase list` to browse 115 ready-to-use workflow templates.

**Try it:** `github.com/supernovae-st/nika`

---

## Feature 2: "Deep Dive" Style (1,500 words)

### Inside Nika: The Rust Engine That Wants to Be Terraform for AI

The AI orchestration market in 2026 is worth billions and growing at 20%+ annually. It is also, by almost any architectural standard, a mess.

Want to chain a few LLM calls together? LangChain has you covered --- if you are comfortable with Python, pip, and a library that has changed its API more times than most developers can count. Want a visual workflow builder? Dify works well --- if you can set up Docker, a server, and a database. Want enterprise-grade orchestration? Prefect and Airflow are battle-tested --- for data pipelines that existed before the LLM revolution, retrofitted with AI capabilities.

Thibaut Melen, a software engineer and founder of SuperNovae Studio, looked at this landscape and made an observation that seems obvious in retrospect: the orchestration layer for AI does not need to be written in Python. It does not need a server. It does not need Docker. It needs a declarative format and a fast compiler.

The result is Nika, a semantic YAML workflow engine for AI tasks that compiles to a single Rust binary. At approximately 482,000 lines of code across 10 workspace crates, it is one of the largest solo-developed Rust projects in the AI space.

#### The Architecture That Makes It Work

Nika's internal architecture revolves around a three-phase AST (Abstract Syntax Tree) pipeline:

**Phase 1 (Raw):** YAML files are parsed into a Raw AST where every field is optional. This faithfully represents user-authored YAML --- typos, missing fields, incorrect types are all preserved with their source spans.

**Phase 2 (Analyzed):** The analyzer validates, type-checks, and interns task IDs. If data reaches this phase, it is structurally guaranteed to be valid. Required fields become non-optional types.

**Phase 3 (Lower):** Analyzed types are transformed into runtime-optimized representations --- timeout conversions (in seconds), template pre-compilation, DAG finalization.

This pipeline is enforced by Rust's type system. Passing a Raw AST node to a function expecting an Analyzed node is a compile error. Skipping the analysis phase is impossible. This structural guarantee --- something that cannot be replicated in dynamically typed languages --- is the foundation on which the rest of the system builds.

#### Five Verbs, Not Fifty

Nika's design thesis is that all AI workflows decompose into five primitive operations: infer (LLM generation), exec (shell commands), fetch (HTTP requests with extraction), invoke (MCP tool calls), and agent (multi-turn autonomous loops).

These verbs are not arbitrary. Each maps to a fundamental capability: generating intelligence (infer), interacting with the system (exec), communicating over the network (fetch), calling external tools (invoke), and delegating complex reasoning (agent). Together, they cover the space of what an AI workflow can do.

Tasks combine into directed acyclic graphs via `depends_on:` declarations and exchange data via `with:` bindings. The engine validates the graph for cycles using Kahn's algorithm, type-checks the bindings, and executes independent tasks in parallel via Tokio's work-stealing scheduler.

#### The Provider Puzzle

One of Nika's most distinctive features is its multi-provider architecture. Built on rig-core, the engine supports 9 LLM providers: 8 cloud providers (OpenAI, Anthropic, Google Gemini, Mistral, Groq, xAI, DeepSeek) plus mistral.rs for local GGUF model inference.

The remarkable part is that cloud and local inference coexist in the same binary. There is no separate Ollama server. No additional process. The same binary that calls GPT-4o over HTTPS can load a Mistral 7B GGUF file from disk and run inference on CPU. Switching between providers is a single-field change in the YAML file.

Vision support extends this to multimodal workflows. The `infer:` verb accepts `content:` blocks with image references to the content-addressable storage, enabling image analysis pipelines entirely in YAML.

#### Media Pipeline: The Unexpected Feature

Perhaps the most surprising aspect of Nika is its 24 built-in media tools. For a "workflow engine," the media processing capabilities are substantial: SIMD-accelerated image resizing, format conversion, metadata extraction, PNG optimization, SVG rasterization, perceptual hashing, PDF text extraction, chart generation, and C2PA content provenance (both signing and EU AI Act compliance verification).

All of these run natively in the binary, backed by content-addressable storage where media assets are identified by SHA-256 hash. This pattern --- borrowed from Git and Docker --- provides deduplication, reproducibility, and security (file paths never leak to LLM APIs).

#### The Fetch Verb: More Than HTTP

The `fetch:` verb deserves special attention. On the surface, it makes HTTP requests. But with nine built-in extraction modes, it functions as a complete web content processing system:

- **markdown:** Converts HTML pages to clean Markdown via htmd
- **article:** Extracts main article content using Readability-style algorithms
- **metadata:** Parses OpenGraph tags, Twitter Cards, JSON-LD structured data, and SEO metadata
- **links:** Classifies links as internal/external, navigation/content/footer
- **feed:** Parses RSS, Atom, and JSON Feed formats
- **jsonpath:** Queries JSON API responses with JSONPath expressions
- **llm_txt:** Discovers AI-era content via the /llms.txt standard

Each mode runs natively in the binary, using streaming HTML processing (lol_html) that operates in O(1) memory regardless of page size. This makes Nika's fetch verb competitive with dedicated web scraping tools --- but integrated directly into the workflow engine.

#### MCP: The Protocol Bridge

Nika is the first non-IDE, non-assistant CLI tool to implement Anthropic's Model Context Protocol (MCP). All other MCP clients in March 2026 are AI assistants (Claude, ChatGPT, Gemini), IDEs (Cursor, VS Code, Windsurf), or operating systems (Windows 11).

Through the `invoke:` verb, Nika connects to any MCP server, making external tool ecosystems available as workflow steps. The project ships with 100+ pre-configured MCP aliases and uses the protocol to connect to NovaNet, its companion knowledge graph.

This is significant because MCP is becoming the standard for agent-tool interaction. By implementing it in a workflow engine rather than an AI assistant, Nika transforms MCP from a chatbot-tool protocol into a workflow orchestration protocol. Any MCP server --- databases, APIs, custom services --- becomes a workflow step.

#### The License Question

Nika ships under AGPL-3.0-or-later --- a choice that is as philosophical as it is practical. Melen's position is explicit: AGPL prevents cloud providers from taking the code, wrapping it in a managed service, and capturing value without contributing back. For a CLI tool that users run locally, the AGPL's network copyleft provision rarely triggers, making it functionally equivalent to MIT for most use cases while maintaining protection against service-level enclosure.

#### Security by Design

Worth noting for enterprise-minded readers: Nika's security model is not bolted on. The engine enforces a command blocklist on the `exec:` verb, validates file paths against directory traversal attacks, applies pre-read size limits on file imports (50 MB default), sanitizes SVG files before parsing, and uses memory-bounded image decoding to prevent decompression bombs. A PolicyEnforcer component validates workflows against configurable security policies before execution. These protections are part of the AST analysis phase --- insecure workflows are rejected before they reach the runtime.

#### Learning and Adoption

Recognizing that a new paradigm requires onboarding, Nika includes a 12-level interactive learning course (44 exercises) accessible via `nika init --course`, a showcase library of 115 ready-to-use workflows, and an LSP implementation for real-time editor integration.

#### What Is Missing

Nika is approaching public launch but is not there yet. Distribution via Homebrew, GitHub releases, crates.io, and VS Code marketplace is planned but not complete. Community adoption is zero --- the project has been developed privately. Benchmark comparisons with competitors do not exist. And the solo-developer model, while productive, raises questions about long-term sustainability.

#### The Bet

Nika's fundamental bet is that declarative YAML files executed by a single binary can replace the Python scripts, Docker containers, and server infrastructure that dominate AI orchestration. Whether this bet pays off depends on whether the developer community is willing to adopt a new paradigm for a tangible improvement in deployment simplicity and workflow reproducibility.

The technical foundation --- 482,000 lines, 7,700+ tests, 10 crates, zero clippy warnings --- suggests that the software works. What remains is the harder question: does the market want it?

---

## Feature 3: "Founder Spotlight" Style (1,000 words)

### The Solo Developer Building a Rust Alternative to the Python AI Stack

Thibaut Melen has a problem with the way the AI industry builds tools. Not with the models --- he has praise for GPT-4o, Claude, Gemini, and the open-weight releases from Mistral and DeepSeek. The problem is everything around the models: the orchestration, the plumbing, the infrastructure that connects a model's capabilities to a user's actual workflow.

"The gap is not in the models," he says. "It is in the space between the model and the user. The plumbing. The 'how do I actually make this work without hiring a DevOps team' part."

His answer, developed over the past year, is Nika: a semantic YAML workflow engine for AI tasks, written entirely in Rust, that ships as a single binary with zero runtime dependencies. The project spans approximately 482,000 lines of code across 10 workspace crates. He wrote every line himself.

#### The Origin Story

Melen's journey to Nika started with frustration. As a software engineer working with AI APIs, he found himself writing the same Python boilerplate repeatedly: fetch data, call an LLM, parse the output, call another LLM, save the result. Each project required setting up virtual environments, managing dependencies, and deploying Docker containers.

"I kept thinking about Terraform," he recalls. "Terraform lets you define infrastructure in declarative files. Docker Compose lets you define containers in YAML. GitHub Actions lets you define CI/CD pipelines in YAML. But for AI workflows? You write Python scripts. That seemed like a gap."

The question that launched Nika was simple: what would Terraform for AI look like?

#### The Rust Decision

Choosing Rust was not the obvious move. Python dominates the AI ecosystem. The libraries, the community, the examples --- everything is Python. Going to Rust meant building from the foundation up, without the safety net of existing AI libraries.

Melen was undeterred. "The orchestration layer does not need to be in the same language as the models. The models run on CUDA. The orchestration runs on your laptop. These are different problems, and they deserve different tools."

Rust gave him three things he could not get from Python: a single binary with zero dependencies, a type system that enforces architectural correctness at compile time, and performance characteristics (SIMD text processing, concurrent HTTP handling, native media processing) that simply are not available in interpreted languages.

#### The Five-Verb Paradigm

Nika's design crystallized around a key insight: all AI workflows decompose into five operations. infer generates text or analyzes images via LLMs. exec runs shell commands. fetch makes HTTP requests. invoke calls MCP tools. agent runs autonomous multi-turn loops.

"I tested this against every workflow I could imagine," Melen says. "Content pipelines, competitive intelligence, media processing, code generation, research automation. Everything decomposed into these five verbs, composed into DAGs. I never found a counterexample."

The constraint --- exactly five verbs, no more --- is deliberate. It forces composability. Every task has exactly one verb. Complex behavior emerges from composition, not from an ever-growing verb vocabulary.

#### The AGPL Choice

Melen's most controversial decision may be the license. AGPL-3.0-or-later is the most restrictive widely-used open source license. It requires that anyone who modifies the code and provides it as a network service must share their modifications.

His reasoning is direct: "MIT and Apache 2.0 are invitations for cloud providers to enclose your work. We have seen this with Elasticsearch, MongoDB, Redis. I would rather have a smaller community that is genuinely free than a larger community that is one AWS announcement away from irrelevance."

The name reinforces the philosophy. Nika is the Sun God from One Piece, a manga character who embodies liberation and freedom. The project's butterfly symbol represents transformation and the impossibility of containing freedom.

"This is not just branding," Melen insists. "One Piece is fundamentally about the fight between freedom and enclosure. The World Government hoards knowledge. The pirates fight to free it. That is exactly the dynamic in the AI industry. Open source communities build the infrastructure. Cloud providers enclose it. AGPL is the legal structure that says: no, this stays free."

#### The Numbers

The scale of the project is difficult to reconcile with its solo development. 482,000 lines across 10 Rust crates. 8,300+ unit tests. A terminal UI with 42 widgets. A language server protocol implementation. 115 showcase workflows. A 12-level interactive learning course. Content-addressable storage for media. 24 built-in media processing tools. Support for 9 LLM providers.

When asked how one person builds this, Melen credits three things: Rust's compiler ("my pair programmer and code reviewer"), test-driven development ("not optional when you are the only person catching bugs"), and architectural discipline ("the three-phase AST pipeline means adding features is guided by the types, not by improvisation").

#### What Is Next

Nika is approaching its public launch. Distribution via Homebrew, GitHub releases, and crates.io is planned. The roadmap includes model routing (different models for different cognitive tasks), dynamic orchestration (LLM-driven task dispatch), and a three-tier memory architecture connecting to NovaNet, the companion knowledge graph.

But the real challenge is not technical. It is social. Nika exists in a market dominated by Python, and its success depends on finding the developers, teams, and organizations that are ready for a different approach.

"I am not trying to convert Python developers," Melen says. "I am trying to reach the people who are not Python developers --- the DevOps engineers, the data analysts, the content teams --- who need AI automation and do not want to learn a programming language to get it. For them, five verbs and a YAML file is not a limitation. It is liberation."

#### The Architecture in Brief

For the technically curious, Nika's internal architecture centers on a three-phase AST pipeline. Phase 1 (Raw) parses YAML into a representation where every field is optional, faithfully capturing user input with all its potential errors. Phase 2 (Analyzed) validates, type-checks, and interns identifiers. Phase 3 (Lower) converts to runtime-optimized representations. Rust's type system enforces the phase ordering --- you cannot skip analysis, because the runtime functions require analyzed types.

Tasks compose into directed acyclic graphs via `depends_on:` declarations. The engine validates the graph for cycles, resolves dependencies, and executes independent tasks in parallel using Tokio's work-stealing scheduler. A DashMap-backed concurrent store provides lock-free access to task results.

The media pipeline deserves special mention. Twenty-four built-in tools --- from SIMD-accelerated image resizing to C2PA content provenance verification --- run natively in the binary. Content-addressable storage (SHA-256 hashing, inspired by Git) ensures reproducibility: every media asset is identified by hash, not by mutable file path.

The terminal UI, implemented in a separate 92,000-line crate using ratatui, provides three views: Studio (workflow visualization), Command (interaction), and Control (configuration). It ships with 42 widgets and 2,117 of its own tests.

And the LSP --- a Language Server Protocol implementation split across two crates --- provides real-time YAML validation, completions, and diagnostics in any supporting editor: VS Code, Neovim, Zed, and others.

Every component reflects the same design principle: the infrastructure should be invisible. Users should think about workflows, not about the tool executing them.

---

*All three features are available for publication. Contact thibaut@supernovae.studio for interview requests or additional information.*
