# Media Pitch Emails --- Nika

> Five ready-to-send pitch emails for different audiences.
> Each includes: subject line, body, key hook, and follow-up strategy.

---

## Pitch 1: TechCrunch Reporter

### Subject Line

Solo dev built a 482K-line Rust alternative to LangChain --- ships as one binary, zero dependencies, AGPL licensed

### Body

Hi [Reporter Name],

I am reaching out about a project that sits at an unusual intersection: a solo developer who built a 482,000-line Rust codebase to solve what he sees as a fundamental problem in the AI orchestration space.

**The core idea:** Every major AI orchestration tool (LangChain, Dify, CrewAI, Prefect) requires Python, Docker, or a server. Nika is a semantic YAML workflow engine that compiles to a single binary with zero runtime dependencies. Users write declarative YAML files with five verbs (infer, exec, fetch, invoke, agent) and run them with one command. No Python. No Docker. No server.

**Why it matters:**
- Occupies a competitive position that research across 80+ sources confirms is unique --- no other tool combines YAML-native definitions with single-binary deployment
- First CLI tool to implement Anthropic's MCP protocol (all other MCP clients are AI assistants or IDEs)
- Supports 9 LLM providers including local GGUF inference in the same binary
- Licensed AGPL-3.0 as a deliberate choice to prevent cloud exploitation
- Named after the Sun God from One Piece (yes, the manga)

**The human story:** Thibaut Melen built the entire codebase himself. 337,000 lines of Rust. 7,700+ tests. 10 workspace crates. A terminal UI, a language server, a 12-level learning course, a media pipeline with content-addressable storage. Zero clippy warnings. The project approaches the scale of a small company's output, built by one person with a strong opinion about what AI tooling should look like.

**The market angle:** The AI orchestration market is growing at 20%+ annually. Every major player is Python-based. Nika is the only Rust-based entry, arguing that the orchestration layer should be a compiled binary, not an interpreted script.

I would be happy to arrange an interview with Thibaut, provide a demo, or share the competitive landscape research document. The project is approaching its public launch and has not been covered by any publication yet.

Best,
[Your name]

### Key Hook

The scale vs. solo dynamic: 482K lines built by one person, occupying a competitive position that no funded team has addressed.

### Follow-up Strategy

- Wait 5 business days
- Follow up with a specific angle: "The AGPL debate in AI" or "the One Piece-inspired naming" depending on the reporter's beat
- Offer a live demo over video call

---

## Pitch 2: Hacker News (Show HN Post)

### Title

Show HN: Nika -- Semantic YAML workflow engine for AI tasks (Rust, single binary, AGPL)

### Body

Hi HN,

I have been building Nika for the past year. It is a workflow engine for AI tasks where you define pipelines in YAML using five verbs and run them from a single Rust binary. No Python, no Docker, no server.

**What it does:**

```yaml
tasks:
  - id: research
    fetch:
      url: https://news.ycombinator.com
      extract: markdown

  - id: analyze
    infer:
      model: claude-sonnet-4-20250514
      prompt: "Identify the top 3 tech trends: {{with.page.body}}"
    with: { page: $research }
    depends_on: [research]
```

That is a complete workflow. `nika run research.nika.yaml` executes it.

**Five verbs, that is it:**
- `infer:` -- LLM generation (9 providers including local GGUF)
- `exec:` -- Shell commands
- `fetch:` -- HTTP + 9 extraction modes (markdown, RSS, metadata, etc.)
- `invoke:` -- MCP tool calls (24 built-in media tools + external)
- `agent:` -- Multi-turn autonomous loops

**Technical stats:**
- ~482K total lines (337K Rust, 570 YAML workflow files, docs)
- 10 workspace crates (engine 134K, TUI 92K, core 23K, etc.)
- 7,700+ tests, zero clippy warnings
- Three-phase AST pipeline (Raw -> Analyzed -> Lower)
- DAG execution with cycle detection and parallel scheduling
- Content-addressable storage for media (SHA-256, like Git)
- Built-in terminal UI (ratatui, 42 widgets)
- LSP for editor integration
- 12-level interactive learning course

**Why Rust:** Single binary deployment, type-enforced AST phases, SIMD media processing, Tokio async for parallel DAG execution.

**Why AGPL:** I believe AI tools are infrastructure, and infrastructure commons need copyleft protection against cloud exploitation. AGPL's network provision rarely triggers for a CLI tool that runs locally.

**Why the name:** Nika is the Sun God from One Piece, whose power is "limited only by imagination." The butterfly is our symbol --- freedom spreading.

Solo project. Would love feedback on the architecture and paradigm.

github.com/supernovae-st/nika

### Key Hook

HN values technical depth, honest tradeoffs, and solo achievement. Lead with the code, not the marketing.

### Follow-up Strategy

- Post at 9 AM ET on a weekday (optimal HN posting time)
- Be present in the comments for the first 4 hours
- Respond to every technical question with specific code references
- If asked about benchmarks, be honest: none exist yet
- If challenged on YAML vs. code, reference the Terraform/Docker Compose/GitHub Actions precedent

---

## Pitch 3: Rust Community Blogs (This Week in Rust / Rust Blog)

### Subject Line

Project spotlight: Nika, a 482K-line YAML workflow engine for AI --- architecture walkthrough

### Body

Hi [Editor],

I would like to propose a project spotlight for [Publication] about Nika, a semantic YAML workflow engine for AI tasks that I believe showcases several Rust patterns that the community would find interesting.

**The project:** Nika compiles declarative YAML files into DAG-scheduled AI workflows. It supports 9 LLM providers, 24 built-in media tools, the MCP protocol, a ratatui TUI, and an LSP implementation. 482,000 total lines across 10 workspace crates.

**Rust patterns worth discussing:**

1. **Three-phase AST pipeline with type-enforced phase separation.** Raw AST (all `Option<T>`) -> Analyzed AST (validated, non-optional) -> Lower (runtime types). The compiler ensures you cannot execute an unvalidated workflow. This pattern could benefit any project with multi-phase data transformation.

2. **Structured error types at scale.** 65 NikaError variants organized into namespaced NIKA-XXX codes (000-319). Each error carries source spans from the YAML parser. I chose this over anyhow/thiserror for diagnostics quality, and I can discuss the tradeoffs.

3. **DashMap-backed concurrent task store.** The RunContext uses DashMap for lock-free concurrent access from Tokio tasks executing DAG nodes in parallel. This pattern works well for producer-consumer workflows where tasks produce results that downstream tasks consume.

4. **Content-addressable storage in Rust.** SHA-256 hashing with path traversal validation and pre-read size checks. The CAS pattern (from Git/Docker) applied to media assets in a workflow engine.

5. **Feature-gated media tools.** The 24 media tools are organized into 3 tiers using Cargo feature flags. Always-on (5 tools), default `media-core` (6 tools), opt-in (13 tools). This keeps compile times reasonable for users who do not need the full media pipeline.

6. **10-crate workspace architecture.** Clean dependency boundaries: nika-core has zero I/O, nika-engine is the embeddable runtime, nika-tui is fully independent. I can discuss when and why to split crates.

I can provide either a self-contained article (2,000-3,000 words) or interview-format Q&A. Happy to adapt to your editorial format.

The project is licensed AGPL-3.0-or-later and approaching its public launch.

Best,
Thibaut Melen
SuperNovae Studio

### Key Hook

Specific, actionable Rust patterns that readers can apply to their own projects, demonstrated at scale in a real codebase.

### Follow-up Strategy

- For "This Week in Rust": Submit the project URL for the newsletter's project section
- For the Rust Blog: Propose a guest post focused on architectural patterns
- For r/rust: Cross-post with a focus on the Rust-specific technical decisions
- Engage with any community discussion within 24 hours

---

## Pitch 4: AI/ML Newsletters (The Batch, AI Weekly, TLDR AI)

### Subject Line

New category: "Declarative CLI AI Workflow Engine" --- Nika ships AI orchestration as a single Rust binary

### Body

Hi [Editor],

Quick pitch for your next issue: a project that creates a new category in AI tooling.

**One-liner:** Nika is a YAML workflow engine for AI tasks --- a single Rust binary where five verbs (infer, exec, fetch, invoke, agent) replace the Python + Docker + server stack that every other AI orchestration tool requires.

**Why your readers care:**

The AI orchestration market has a blind spot. Every tool --- LangChain, Dify, CrewAI, Prefect --- requires Python. Research across 80+ sources confirmed that no tool combines YAML-native workflow definitions with single-binary deployment. Nika occupies this gap.

**Key differentiators for an AI audience:**
- **9 LLM providers in one binary** --- cloud (OpenAI, Anthropic, Gemini, Mistral, Groq, xAI, DeepSeek) + local GGUF via mistral.rs. Switch providers by changing one YAML field.
- **First CLI tool with MCP support** --- connects YAML workflows to any MCP server. Not just chatbots --- workflow orchestration via the protocol.
- **Structured output validation** --- JSON Schema on infer: tasks catches malformed LLM responses before they propagate.
- **9 fetch extraction modes** --- markdown, article, metadata, links, feeds, jsonpath, llm_txt, and more. Web scraping + API calling built into the workflow engine.
- **24 media tools** --- image resize, PDF extract, chart generation, C2PA provenance. All native Rust, no external dependencies.
- **Content-addressable storage** --- media assets addressed by SHA-256 hash, like Git. Reproducible, portable, deduplicated.

**For a "Tool of the Week" or "New Launch" section:**

Nika / supernovae-st/nika / AGPL-3.0 / Rust / 482K lines / 9 providers / single binary / zero dependencies

**For a longer feature:** I can provide a technical deep dive, competitive analysis, or interview with the creator.

Happy to provide whatever format works for your editorial needs.

Best,
[Your name]

### Key Hook

The "new category" framing gives newsletter editors a fresh angle. The specific numbers (9 providers, 24 tools, 482K lines) provide credibility.

### Follow-up Strategy

- Wait 3 business days (newsletter editors work on tight cycles)
- Follow up with a specific section suggestion: "Tool of the Week" or "New Launch" or "Deep Dive"
- Offer a 200-word blurb that the editor can use with minimal editing

---

## Pitch 5: Open Source Foundations (Linux Foundation, OSI, FSFE, Software Freedom Conservancy)

### Subject Line

AGPL-3.0 AI infrastructure project seeking community partnership --- Nika semantic workflow engine

### Body

Dear [Foundation Contact],

I am writing to introduce Nika, an AGPL-3.0-or-later licensed AI workflow engine, and to explore potential partnership or community alignment with [Foundation Name].

**Project Overview:**

Nika is a semantic YAML workflow engine for AI tasks, written in Rust and distributed as a single binary. It allows users to define AI pipelines (LLM inference, web fetching, shell commands, MCP tool calls, autonomous agent loops) in declarative YAML files and execute them without Python, Docker, or server infrastructure.

The project spans approximately 482,000 lines of code across 10 workspace crates, includes 8,300+ unit tests, and supports 9 LLM providers.

**Why AGPL:**

The license choice is deliberate and philosophical. AI orchestration tools are infrastructure --- they connect users to AI capabilities. Like databases, web servers, and container runtimes, this infrastructure should be commons. AGPL-3.0 provides copyleft protection that prevents cloud providers from enclosing the software as a proprietary service without contributing modifications back to the community.

We chose AGPL specifically because:
- The SaaS delivery model means permissive licenses provide no protection at the service layer
- The AI industry's current trajectory favors value concentration in cloud platforms
- AGPL's network copyleft provision directly addresses this dynamic
- For a CLI tool that users run locally, AGPL's practical impact is minimal for end users

**Why This Matters for [Foundation Name]:**

The AI tooling ecosystem is consolidating rapidly. Most AI orchestration tools are either proprietary SaaS offerings or open source projects under permissive licenses that are vulnerable to cloud enclosure. AGPL AI infrastructure projects are rare. Nika represents a proof point that copyleft AI tools can be technically competitive while maintaining strong freedom guarantees.

**What We Are Looking For:**

- Community alignment and visibility through foundation channels
- Guidance on sustainable governance for a growing AGPL project
- Connection with other AGPL / copyleft AI projects
- Potential inclusion in foundation project listings or directories

**About the Creator:**

I am Thibaut Melen, the sole developer and founder of SuperNovae Studio. I built the entire codebase myself, driven by the conviction that AI infrastructure should be free --- not just free to use, but free to remain free. The project name comes from the Sun God Nika in One Piece, a symbol of liberation. The butterfly is our symbol, representing transformation and the impossibility of containing freedom.

I would welcome a conversation about how Nika can contribute to [Foundation Name]'s mission and how we might collaborate to promote copyleft AI infrastructure.

Best regards,
Thibaut Melen
Founder, SuperNovae Studio
thibaut@supernovae.studio
github.com/supernovae-st/nika

### Key Hook

AGPL AI infrastructure is rare. This positions Nika as a case study for the foundation's advocacy work.

### Follow-up Strategy

- Wait 2 weeks (foundation timelines are longer than media)
- Follow up with a specific ask: listing in project directory, blog post opportunity, or speaker slot at a foundation event
- For FSFE specifically: emphasize the European angle (French developer, European open source)
- For Linux Foundation: frame around MCP protocol adoption and YAML workflow standards
- For OSI: frame around the "open source AI" definition debate and AGPL's role

---

## General Pitching Guidelines

### Before Sending

- Research the recipient's recent coverage / editorial focus
- Customize the opening sentence to reference their work
- Replace [Reporter Name] / [Editor] / [Foundation Contact] with actual names
- Add the direct GitHub link
- Check that all statistics are current (update line counts, test counts, provider counts)

### Timing

- **TechCrunch:** Tuesday--Thursday morning ET
- **Hacker News:** Tuesday--Thursday 9 AM ET
- **Newsletters:** Monday--Wednesday (ahead of their publication day)
- **Rust community:** Anytime (community is global)
- **Foundations:** Early week, business hours in their timezone

### What to Prepare Before Pitching

- [ ] Project is publicly accessible on GitHub
- [ ] README is complete and professional
- [ ] At least one "Getting Started" tutorial exists
- [ ] Installation instructions work (Homebrew, GitHub releases, or cargo install)
- [ ] Demo workflow files are included and tested
- [ ] Screenshots of the terminal UI are available
- [ ] The competitive landscape research document is accessible

### Response Templates

**If asked for a demo:** "I can provide a live walkthrough over video call (30 minutes) or a recorded demo video. I can show workflow execution, the terminal UI, media processing, and MCP integration."

**If asked about funding:** "Nika is a bootstrapped project by SuperNovae Studio. There is no venture capital, no employees, and no revenue model yet. The focus is on building the best AI workflow engine possible and releasing it under AGPL."

**If asked about users:** "The project is approaching its public launch. It has been developed privately and has no external users yet. The 115 showcase workflows and 12-level course were built to ensure a strong onboarding experience from day one."

**If asked about sustainability:** "The AGPL license enables dual licensing: enterprises that need a non-AGPL option can purchase a commercial license. This is the model used by Grafana Labs, Qt, and many other successful AGPL projects."

---

*All pitch materials are ready for customization and sending. Review recipient-specific details before each send. Contact thibaut@supernovae.studio for additional materials or coordination.*
