# Conference Speaking Proposals --- Nika

> Five conference talk proposals for different audiences.
> Each includes: title, abstract, outline, speaker bio, and technical requirements.

---

## Proposal 1: RustConf

### Title

**Building a 482K-Line Workflow Engine in Rust: Architecture Decisions, Type-Driven Design, and Lessons from Solo Development**

### Abstract

Nika is a semantic YAML workflow engine for AI tasks: 482,000 lines of Rust across 10 workspace crates, compiled into a single binary that orchestrates LLM inference, HTTP fetching, shell execution, MCP protocol calls, and autonomous agent loops. The entire codebase was written by one developer.

This talk is not about AI. It is about Rust.

I will cover the architecture decisions that made this scale of solo development possible: a three-phase AST pipeline (Raw, Analyzed, Lower) enforced by the type system, DAG validation using Kahn's algorithm in a concurrent Tokio runtime, content-addressable storage inspired by Git, a media processing pipeline using SIMD-accelerated image operations, structured error types with namespaced error codes (NIKA-000 through NIKA-319), and the crate boundary decisions that split 10 independent workspace members with clean dependency graphs.

I will also cover what went wrong: the patterns that did not scale, the trait designs I had to refactor, the moment I realized 92,000 lines of TUI code needed its own crate, and why I abandoned anyhow in favor of a custom error type that now has 65 variants.

No AI knowledge required. This is a Rust architecture talk.

### Outline (40 minutes)

1. **Introduction** (3 min) --- What Nika does, why one person built it, why Rust
2. **Three-Phase AST Pipeline** (8 min)
   - Raw AST: all fields `Option<T>`, span preservation with marked_yaml
   - Analyzed AST: semantic validation, TaskId interning via string interner
   - Lower phase: runtime type conversion, template pre-compilation
   - How the type system prevents phase-skipping
3. **DAG Execution** (7 min)
   - Cycle detection with Kahn's algorithm (topological sort)
   - Concurrent execution via Tokio JoinSet
   - CancellationToken for fail-fast mode
   - DashMap-backed RunContext for lock-free concurrent task result storage
4. **Content-Addressable Storage** (5 min)
   - SHA-256 hashing for media identity
   - File import with path traversal validation
   - Pre-read size checks (50 MB default)
   - How CAS enables reproducible workflows
5. **Error Architecture** (5 min)
   - Why I abandoned anyhow for 65 NikaError variants
   - Namespaced NIKA-XXX codes by subsystem
   - Source span attachment for YAML-line-level diagnostics
   - How structured errors improved the LSP implementation
6. **Crate Boundaries** (5 min)
   - The 10-crate workspace and dependency graph
   - When to split: the 92K-line TUI crate decision
   - nika-core's zero-I/O constraint
   - Feature flags for opt-in media tools
7. **Mistakes and Refactors** (5 min)
   - The DataStore-to-RunContext rename (205 call sites)
   - The Vegapunk-to-descriptive naming migration
   - Why the first provider abstraction was wrong
   - The timeout unit bug (seconds vs. milliseconds)
8. **Q&A** (2 min)

### Speaker Bio

Thibaut Melen is the founder of SuperNovae Studio and the sole developer of Nika, a 482,000-line Rust project that compiles a YAML workflow language into DAG-scheduled AI task execution. He is an advocate for typed error handling, test-driven development, and the AGPL license. He has written more Rust than he has read manga, but not by much.

### Technical Requirements

- Screen for slides and live terminal demos
- No network required (all demos run locally)
- 40-minute slot preferred, 30-minute minimum

---

## Proposal 2: KubeCon / AI Dev Day

### Title

**Semantic YAML for AI Orchestration: How 5 Verbs Replace Your AI Framework**

### Abstract

The AI orchestration landscape has a deployment problem. LangChain needs Python. Dify needs Docker. Prefect needs a server. Argo needs Kubernetes. Every tool in the space adds infrastructure complexity that contradicts the promise of accessible AI.

Nika takes the opposite approach: five semantic verbs (infer, exec, fetch, invoke, agent) compose into DAG-scheduled YAML workflows, executed by a single binary with zero runtime dependencies. This talk demonstrates how declarative YAML --- the format Kubernetes engineers already know --- can define complete AI pipelines, from web scraping to LLM inference to media processing to MCP tool orchestration.

I will show live demos of:
- A competitive intelligence pipeline (fetch + infer + structured output)
- A multimodal image analysis workflow (CAS + vision + agent)
- An MCP-connected knowledge graph pipeline (invoke + NovaNet)

This is infrastructure-as-code for AI. Five verbs. One file. One binary.

### Outline (30 minutes)

1. **The Problem** (5 min)
   - Every AI tool adds infrastructure: Python, Docker, servers, databases
   - The "just write a Docker Compose file for your AI pipeline" trap
   - What if AI workflows were as simple as Kubernetes YAML, but without Kubernetes?
2. **Five Verbs** (5 min)
   - infer: LLM generation (text, vision, structured output)
   - exec: Shell commands with security controls
   - fetch: HTTP requests with 9 extraction modes
   - invoke: MCP tool calls (24 built-in + external services)
   - agent: Multi-turn autonomous loops
3. **Live Demo 1: Content Pipeline** (5 min)
   - Fetch a news page, extract as markdown, analyze with Claude, generate report
   - Show DAG parallel execution in the terminal UI
4. **Live Demo 2: Media Processing** (5 min)
   - Import image to CAS, generate thumbnail, extract colors, run vision analysis
   - Show content-addressable storage and hash-based references
5. **Live Demo 3: MCP Integration** (5 min)
   - Connect to an MCP server, invoke tools, chain results
   - Demonstrate the protocol bridge between YAML workflows and tool ecosystems
6. **Architecture** (3 min)
   - Single binary: how Rust eliminates dependencies
   - DAG validation: cycle detection, typed bindings
   - 9 providers: cloud + local GGUF in one binary
7. **Q&A** (2 min)

### Speaker Bio

Thibaut Melen is the founder of SuperNovae Studio and creator of Nika, the first YAML-native AI workflow engine that ships as a single binary. He believes AI orchestration does not need Python, Docker, or Kubernetes --- just five verbs and a good compiler. His tool supports 9 LLM providers, 24 media tools, and the MCP protocol, all in 482,000 lines of Rust. He chose AGPL because commons need protection.

### Technical Requirements

- Screen for slides and live terminal demos
- Internet connection for cloud LLM demos (with offline fallback using local GGUF model)
- 30-minute slot

---

## Proposal 3: FOSDEM (Free and Open Source Developers' European Meeting)

### Title

**AGPL as Liberation: The Open Source License Debate in the Age of AI**

### Abstract

The open source AI ecosystem has a licensing problem. Most AI tools use MIT or Apache 2.0 --- permissive licenses that allow cloud providers to take community-built software, deploy it as a proprietary service, and capture value without contributing back. This pattern has already forced Elasticsearch, MongoDB, and Redis to change their licenses.

This talk argues that AGPL-3.0 is the right license for AI infrastructure tools. I will present the case through the lens of Nika, a 482,000-line Rust workflow engine licensed AGPL, drawing parallels to the themes of liberation and freedom in Eiichiro Oda's One Piece manga --- where the fight between pirates (open source) and the World Government (big tech) mirrors the dynamics of the AI industry.

I will cover:
- Why permissive licenses fail for cloud-era infrastructure
- How AGPL's network copyleft provision specifically addresses SaaS exploitation
- The practical impact of AGPL on a CLI tool (spoiler: minimal for 99% of users)
- Why Grafana Labs at $6B+ proves AGPL is not anti-business
- A call for more AGPL AI tools

This is a talk about freedom, enclosure, and the legal structures that protect one from the other.

### Outline (25 minutes)

1. **The Enclosure Pattern** (5 min)
   - Linux, Elasticsearch, MongoDB, Redis: the repeating cycle
   - Cloud providers as value extractors
   - The difference between "open source" and "open for exploitation"
2. **Why AGPL** (5 min)
   - Network copyleft: the provision designed for SaaS
   - MIT/Apache vs. AGPL: what each protects and what each allows
   - The practical impact on CLI tools (almost none for local use)
   - Grafana Labs, Nextcloud, GitLab: AGPL success stories
3. **The One Piece Parallel** (5 min)
   - World Government = big tech hoarding knowledge
   - Pirates = open source communities building in the open
   - Sun God Nika = liberation through joy (and AGPL)
   - Whitebeard's last words: "Open Source AI does exist!"
   - Why cultural framing matters for community building
4. **Nika as a Case Study** (5 min)
   - 482K lines, AGPL from day one, zero regrets
   - Why the license choice shaped the architecture (single binary = minimal AGPL friction)
   - How to communicate AGPL to enterprises
5. **Call to Action** (3 min)
   - AI tools are infrastructure; infrastructure needs copyleft
   - Practical guidance for choosing AGPL
   - The butterfly cannot be contained
6. **Q&A** (2 min)

### Speaker Bio

Thibaut Melen is the founder of SuperNovae Studio and creator of Nika, a semantic YAML workflow engine for AI tasks licensed AGPL-3.0-or-later. He is an open source advocate who believes that AI infrastructure should be free --- not just free to use, but free to remain free. He chose to name his project after the Sun God from One Piece because the fight for open source freedom and the fight against corporate enclosure are, at their core, the same story.

### Technical Requirements

- Screen for slides (no live demos for this talk)
- 25-minute slot preferred
- FOSDEM Legal and Policy devroom or Community devroom

---

## Proposal 4: PyCon

### Title

**Why a Rust Engine Beats Python for AI Workflows (From Someone Who Loves Python)**

### Abstract

This is a deliberately provocative talk, and I want to be upfront about that.

I built Nika, a 482,000-line Rust workflow engine for AI tasks. It ships as a single binary, supports 9 LLM providers, and executes YAML-defined workflows as DAGs. It does not use Python anywhere.

This talk is not an attack on Python. Python is extraordinary for data science, machine learning research, and prototyping. But I will argue that the orchestration layer --- the plumbing that connects LLMs to each other, to tools, and to the outside world --- is better served by a different kind of tool.

I will present five specific areas where Rust provides advantages for AI orchestration:

1. **Deployment:** Single binary vs. virtualenv + pip + Docker
2. **Type safety:** Compile-time AST validation vs. runtime TypeErrors
3. **Concurrency:** Tokio work-stealing vs. asyncio + GIL
4. **Performance:** SIMD text processing and native media tools vs. C-extension wrappers
5. **Reproducibility:** Content-addressable storage vs. mutable file paths

I will then concede five areas where Python wins:

1. **Ecosystem depth** (more libraries, more integrations)
2. **Community size** (more developers, more Stack Overflow answers)
3. **Prototyping speed** (REPL, dynamic typing, Jupyter)
4. **ML framework integration** (PyTorch, TensorFlow, JAX)
5. **Approachability** (lower learning curve)

The conclusion is not "use Rust instead of Python" but "use each for what it is best at." Models in Python. Orchestration in Rust. YAML for the interface.

I expect this talk to generate spirited discussion. That is the point.

### Outline (30 minutes)

1. **Disclaimer** (2 min) --- I love Python. This talk is friendly fire.
2. **The Orchestration Layer Problem** (3 min)
   - Orchestration != model training
   - Different requirements: reliability, deployment, type safety
   - Why the same language for both is not obvious
3. **Five Things Rust Does Better for Orchestration** (12 min)
   - Single binary deployment (demo: download and run in 10 seconds)
   - Three-phase type-checked AST (demo: compile error vs. runtime TypeError)
   - Tokio concurrency (demo: 10 parallel tasks, no GIL)
   - SIMD media processing (demo: image resize without Pillow)
   - CAS reproducibility (demo: hash-based media references)
4. **Five Things Python Does Better** (8 min)
   - Ecosystem, community, prototyping, ML frameworks, approachability
   - Why I would not write a training loop in Rust
   - Why Jupyter notebooks exist and are good
5. **The Synthesis** (3 min)
   - Models in Python. Orchestration in Rust. Interface in YAML.
   - Nika as a bridge, not a replacement
   - Interop via exec: and fetch: verbs
6. **Q&A** (2 min)

### Speaker Bio

Thibaut Melen is the founder of SuperNovae Studio and creator of Nika, the only Rust-based AI workflow engine. He chose to submit this talk to PyCon specifically because the most productive conversations happen between communities, not within them. He has deep respect for Python's role in democratizing programming and machine learning, and he thinks the orchestration layer deserves its own tool. He expects to be challenged. He looks forward to it.

### Technical Requirements

- Screen for slides and live terminal demos
- Internet connection preferred (for cloud LLM demo)
- 30-minute slot
- Note: this is designed to be a constructive debate, not a flame talk

---

## Proposal 5: AI Engineer Summit

### Title

**5 Verbs to Replace Your AI Framework: Declarative YAML Workflows for the Post-LangChain Era**

### Abstract

The AI framework landscape is fragmenting. LangChain, LlamaIndex, LangGraph, CrewAI, AutoGen, DSPy, Haystack, Semantic Kernel --- each with its own abstractions, APIs, and breaking changes. Developers spend more time learning framework abstractions than building AI applications.

What if the abstraction layer was just five verbs?

Nika is a workflow engine where AI pipelines are YAML files with five operations: infer (LLM generation), exec (shell commands), fetch (HTTP requests), invoke (MCP tool calls), and agent (autonomous loops). Tasks compose into DAGs. The engine handles concurrency, type checking, structured output validation, and multi-provider dispatch. No framework to learn. No API to memorize. No breaking changes to track.

This talk demonstrates:
- How any AI pipeline decomposes into five verbs
- Live coding: build a complete agent workflow in under 5 minutes
- Structured output: JSON Schema validation on LLM responses
- MCP integration: connecting to external tool ecosystems
- The case for YAML over Python for production AI pipelines

The thesis: frameworks add complexity. Verbs add clarity. Five is enough.

### Outline (25 minutes)

1. **The Framework Problem** (4 min)
   - Framework proliferation: 8+ major options, none compatible
   - Abstraction tax: learning LangChain does not teach you LlamaIndex
   - Breaking changes: how many times has LangChain's API changed?
   - The alternative: what if the interface was just five verbs?
2. **The Five Verbs** (4 min)
   - infer: prompt → LLM → response (text, vision, structured output)
   - exec: command → shell → result
   - fetch: URL → HTTP → content (9 extraction modes)
   - invoke: tool → MCP → result (24 built-in + any MCP server)
   - agent: goal → LLM loop → outcome
3. **Live Coding: Build a Workflow** (6 min)
   - Start from empty YAML file
   - Add tasks, dependencies, bindings
   - Run with `nika run`
   - Show parallel execution in terminal UI
4. **Structured Output and Guardrails** (4 min)
   - JSON Schema validation on infer: tasks
   - How structured output eliminates downstream parsing errors
   - Guardrails for content safety
5. **MCP: The Protocol Bridge** (3 min)
   - invoke: as the universal tool-calling interface
   - 100+ pre-configured aliases
   - Connecting YAML workflows to any MCP server
6. **The Production Case** (2 min)
   - Version-controlled YAML in git
   - Reproducible via CAS
   - No server, no Docker, no framework upgrades
   - AGPL: the code stays free
7. **Q&A** (2 min)

### Speaker Bio

Thibaut Melen is the founder of SuperNovae Studio and creator of Nika, the first declarative AI workflow engine that ships as a single binary. His 482,000-line Rust project supports 9 LLM providers, 24 media tools, and the MCP protocol --- all defined in YAML with five verbs. He believes AI engineering has a framework problem, and the solution is not another framework --- it is a simpler abstraction.

### Technical Requirements

- Screen for slides and live coding
- Internet connection for LLM API calls
- Terminal with large font for live coding visibility
- 25-minute slot

---

## General Speaker Information

**Speaker:** Thibaut Melen
**Title:** Founder, SuperNovae Studio
**Location:** Available for European and international conferences
**Email:** thibaut@supernovae.studio
**GitHub:** [@ThibautMelen](https://github.com/ThibautMelen)
**Organization:** [@supernovae-st](https://github.com/supernovae-st)

**Speaker experience:** Available for keynotes, breakout sessions, panels, and lightning talks. All talks include live demos unless otherwise specified.

**A/V requirements:** Standard presentation setup (screen, microphone). All demos run locally with optional internet for cloud LLM calls. Backup offline demos available for all talks.

---

*For booking inquiries, contact thibaut@supernovae.studio. Speaker headshot and logo assets available upon request.*
