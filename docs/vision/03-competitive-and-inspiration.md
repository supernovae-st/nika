# 03 — Competitive Analysis & Inspiration

> Part 1 maps the market: who competes with Nika, where each player sits, and what differentiates them.
> Part 2 deep-dives the most relevant inspiration source — Slate by Random Labs — and designs how Nika absorbs its architectural insights while going beyond.

**Nika** v0.49 · Updated 2026-03-28

---

# Part 1 — Competitive Landscape

> 8 competitors mapped. 2 protocols analyzed. 1 unique position defined.

---

## Market Map

```mermaid
quadrantChart
    title Declarative vs Imperative x Simple vs Complex
    x-axis "Simple" --> "Complex"
    y-axis "Imperative" --> "Declarative"
    quadrant-1 "Declarative + Complex"
    quadrant-2 "Declarative + Simple"
    quadrant-3 "Imperative + Simple"
    quadrant-4 "Imperative + Complex"
    "Nika": [0.80, 0.85]
    "Rein": [0.50, 0.75]
    "Kestra": [0.55, 0.80]
    "Dify": [0.30, 0.75]
    "LangGraph": [0.70, 0.25]
    "CrewAI": [0.45, 0.35]
    "AutoGen": [0.55, 0.30]
    "OpenClaw": [0.65, 0.15]
    "Hermes Agent": [0.60, 0.20]
```

```mermaid
flowchart TB
    subgraph ENGINES["Workflow Engines (framework)"]
        direction LR
        NIKA["Nika\nYAML DAG + MCP + Media"]
        LG["LangGraph\nPython, stateful graphs"]
        CREW["CrewAI\nRole-based multi-agent"]
        AUTO["AutoGen\nConversational agents"]
        DIFY["Dify\nVisual workflow builder"]
        REIN["Rein\nYAML AI orchestrator"]
        KESTRA["Kestra\nYAML data pipelines"]
    end

    subgraph AGENTS["AI Agents (product)"]
        direction LR
        CC["Claude Code\n#1 coding tool, Auto mode"]
        OC["OpenClaw\n338K stars, 24+ channels"]
        HERMES["Hermes Agent\nSelf-improving, RL training"]
        SLATE["Slate\nSwarm-native, episodes"]
        DEVIN["Devin\nFull dev environment"]
        OH["OpenHands\nSWE-Bench 77.6%"]
    end

    subgraph PROTOCOLS["Protocols"]
        direction LR
        MCP["MCP (Anthropic)\n97M monthly SDK downloads"]
        A2A["A2A (Google -> LF)\nAgent-to-Agent"]
    end

    style ENGINES fill:#dbeafe,stroke:#2563eb
    style AGENTS fill:#fef3c7,stroke:#d97706
    style PROTOCOLS fill:#dcfce7,stroke:#16a34a
```

> [!NOTE]
> Nika sits in the **Declarative + Complex** quadrant — a unique position. No other framework combines YAML-first workflows, knowledge graph integration, multi-provider orchestration, and a 24-tool media pipeline.

---

## 1. Slate by Random Labs[^1]

**Source:** Blog post + documentation + npm `@randomlabs/slate` v1.0.15
**Config:** `slate.json` / `slate.jsonc` with 3-level merge (global -> project -> inline)

### Architecture: 8 Interconnected Concepts

Slate solves the **context window degradation problem**: past a threshold (the "dumb zone"), LLM performance degrades. Every existing approach fails for different reasons. Slate's answer is **threads + episodes + thread weaving**.

```mermaid
flowchart TB
    WM["1. Working Memory\n& Dumb Zone"] --> TH["2. Threads\n(one-shot, NOT subagents)"]
    TH --> EP["3. Episodes\n(completion boundary\ncompression)"]
    EP --> TW["4. Thread Weaving\n(implicit adaptive\ndecomposition)"]
    TW --> ST["5. Strategy / Tactics\n(AlphaZero mapping)"]
    ST --> KO["6. Knowledge\nOverhang"]
    KO --> CO["7. Composability\n(episodes as inputs)"]
    CO --> OS["8. OS Framing\n(kernel metaphor)"]

    style WM fill:#fecaca,stroke:#dc2626
    style TH fill:#dbeafe,stroke:#2563eb
    style EP fill:#dcfce7,stroke:#16a34a
    style TW fill:#fef3c7,stroke:#d97706
    style ST fill:#ede9fe,stroke:#7c3aed
    style KO fill:#ccfbf1,stroke:#0d9488
    style CO fill:#dbeafe,stroke:#2563eb
    style OS fill:#fef3c7,stroke:#d97706
```

### Slate's Critique of Existing Approaches

| Approach | Failure Mode | Slate's Alternative |
|----------|-------------|---------------------|
| **Compaction** | Lossy, unpredictable information loss | Episode compression at completion boundary |
| **Subagents** | Context isolated, can't transfer info | Threads (one-shot) + episodes (compressed handoff) |
| **Markdown Plans** | Underspecified, incomplete execution, stale | No explicit plans — implicit adaptive decomposition |
| **Task Decomposition** | Rigid, can't adapt, low expressivity | Thread weaving — orchestrator adapts per round |
| **RLM** | Blind N-step execution, no intermediate feedback | Every thread produces episode, orchestrator reviews |
| **Devin/Manus** | Context lost at compress boundary | Episodes preserve key info, entity-linked persistence |

### Slate's Model Slots vs Nika's Agent Presets

> [!IMPORTANT]
> **Attribution note:** Slate configures 4 model slots: **main**, **subagent**, **search**, **reasoning**. Nika uses `agents:` with 8 named presets: **default**, **lite**, **think**, **search**, **vision**, **judge**, **coder**, **summary**. The slot concept is inspired by Slate; the specific taxonomy and declarative YAML approach is Nika's design.

```mermaid
flowchart LR
    subgraph SLATE_SLOTS["Slate's Slots"]
        SM["main\n(expensive, capable)"]
        SS["subagent\n(cheaper, faster)"]
        SSE["search\n(fast, cheap)"]
        SR["reasoning\n(deep thinking)"]
    end

    subgraph NIKA_SLOTS["Nika's Agent Presets"]
        ND["default\n(orchestration)"]
        NL["lite\n(fast, cheap)"]
        NT["think\n(extended thinking)"]
        NS["search\n(retrieval)"]
        NV["vision\n(multimodal)"]
        NJ["judge\n(review)"]
        NC["coder\n(code gen)"]
        NSM["summary\n(compression)"]
    end

    SLATE_SLOTS -.->|"inspired by"| NIKA_SLOTS

    style SLATE_SLOTS fill:#fef3c7,stroke:#d97706
    style NIKA_SLOTS fill:#dbeafe,stroke:#2563eb
```

### Slate's Architecture Comparison

| Dimension | ReAct | Plan | Task Trees | RLM | Devin | Claude Code | **Slate** |
|-----------|:-----:|:----:|:----------:|:---:|:-----:|:-----------:|:---------:|
| Planning | Implicit | Explicit | Explicit | None | Explicit | Implicit | Implicit |
| Decomposition | None | Manual | Static | None | Static | None | Implicit |
| Feedback | Per-step | End | End | None | Per-task | Per-step | Per-episode |
| Context isolation | None | None | Partial | None | Full | None | Per-thread |
| Compression | Compact | Compact | None | None | Compress | Compact | Episode |
| Parallelism | None | None | None | None | Multi-agent | None | Native |
| Adaptability | Low | Low | Low | Low | Medium | High | High |

### Head-to-Head: Slate vs Nika

<details>
<summary>Where Slate leads (gaps Nika must close)</summary>

| Feature | Slate | Nika | Priority |
|---------|-------|------|----------|
| Context management | Working memory awareness, episode compression | None — full context carried | P-CONTEXT |
| Model routing | 4 model slots (main/subagent/search/reasoning) | Single provider per workflow | P-MODEL |
| Episodic memory | Cross-session persistence (session files) | In-memory only | P-MEMORY |
| Strategy/tactics | Orchestrator dispatches threads, synthesizes episodes | Flat agent loop | P-ORCHESTRATE |
| Thread model | One-shot threads with episode compression | Persistent subagents | P-RECORD |
| Adaptive planning | Implicit via thread weaving, no stale plans | Static DAG, no runtime adaptation | P-ORCHESTRATE |
| Long-running tasks | Hours to days | Single session | P-MEMORY |

</details>

<details>
<summary>Where Nika leads (moat Slate cannot replicate)</summary>

| Feature | Nika | Slate |
|---------|------|-------|
| Declarative workflows | YAML DAG, version-controlled, auditable | TypeScript, imperative, opaque |
| Knowledge graph | NovaNet (59 NodeClasses, 200+ locales) | None |
| Reproducibility | NDJSON traces, deterministic DAG replay | Thread-based, non-deterministic |
| Security | Shell-free exec, command blocklist, path validation | Not documented |
| Structured output | 5-layer validation (inject -> extract -> validate -> retry -> repair) | Not documented |
| Multi-provider | 7 cloud + local GGUF + custom endpoints (9 total) | Cross-model but fewer providers |
| Multi-locale | 200+ locales via NovaNet knowledge atoms | English-focused |
| Observability | 43 event types, NDJSON traces, TUI with DAG view | Basic logging |
| Cost control | Token tracking per task, budget awareness | No token budgeting |
| Media pipeline | 24 builtin tools (CAS, thumbnail, convert, chart, provenance) | None |
| Interactive TUI | 3-view ratatui with live DAG, spinners, progress | None |
| LSP | Completions, diagnostics, hover for .nika.yaml | None |
| Testing | 8457 tests, mock provider, dry-run mode | Not documented |
| Course | 12 levels, 44 exercises, constellation progress | None |

</details>

### Features to Steal from Slate

| Feature | What Slate Does | How Nika Could Adapt |
|---------|----------------|---------------------|
| Episode compression | Compress at completion boundary, not mid-stream | `record:` block on tasks with LLM summarization |
| Thread weaving | Orchestrator dispatches then synthesizes | `goal:` mode with dynamic DAG |
| Working memory budget | Tracks usable context vs dumb zone | `context_budget:` per task |
| Cross-model composition | Different models for different cognitive tasks | Already have `agents:` — need orchestrate routing |

> [!TIP]
> **Key takeaway:** Nika's DAG IS already Slate's kernel. Tasks ARE processes. `TaskResult` IS return values. `RunContext` IS RAM. We don't BUILD Slate — we UPGRADE the kernel with 4 additions (agent presets, records, orchestrate mode, context budgets), then persist via NovaNet. See [Part 2](#part-2--slate-deep-integration-strategy) for the complete integration strategy.

---

## 2. Claude Code (Anthropic)

**Type:** CLI coding agent — #1 AI coding tool (March 2026)
**Traction:** 95% of surveyed engineers use AI tools weekly; Claude Code leads ahead of GitHub Copilot and Cursor

| Aspect | Claude Code | Nika |
|--------|------------|------|
| Interaction | Conversational + Auto mode | Workflow-defined |
| Reproducibility | Low (conversations) | High (YAML DAG + traces) |
| Extensibility | Hooks + skills + MCP | Verbs + MCP + includes |
| Multi-step | Ad-hoc via conversation | Structured via DAG |
| Multi-model | Claude only | 7 cloud + local + custom endpoints |
| Knowledge graph | None | NovaNet integration |
| MCP | Client (97M monthly SDK downloads) | Client + 24 native builtins |
| Structured output | Ad-hoc JSON | 5-layer schema validation |
| Media | None | 24 builtin media tools |

**Key developments (2026):**
- **Auto mode**: AI executes safe actions without per-step approval. Safety layer reviews actions before running.
- **MCP integration**: Connects to 6,000+ apps via MCP servers (Zapier, etc.). MCP SDK has 97M monthly downloads.
- **Claude Cowork**: Desktop tool for non-developers to build web applications via plain language.
- **IDE integration**: VS Code, JetBrains, GitHub (@claude mentions on PRs).
- **CLAUDE.md**: Project-level instructions as persistent context.

> [!NOTE]
> **Relationship:** Claude Code is Nika's **user**, not competitor. Nika workflows are authored and invoked from Claude Code sessions. The relationship is symbiotic — Claude Code provides the interactive shell, Nika provides the reproducible workflow engine.

---

## 3. OpenClaw (~338K stars)

**Type:** Personal AI assistant / gateway — largest open-source AI project by stars
**License:** MIT | **Language:** TypeScript (Node.js)

| Aspect | OpenClaw | Nika |
|--------|----------|------|
| Architecture | WebSocket gateway, multi-channel inbox | YAML DAG, local-first binary |
| Channels | 24+ (WhatsApp, Telegram, Slack, Discord, Signal, etc.) | CLI + TUI + MCP |
| Agent model | Single agent, autonomous decisions | 5 semantic verbs, structured DAG |
| Voice | Wake words, Talk Mode | None |
| Visual | A2UI Live Canvas | TUI with DAG visualization |
| Workflow engine | None (agent decides next steps) | Full DAG with data flow |
| Media | None | 24 builtin media tools |
| Structured output | None | 5-layer schema validation |
| Reproducibility | None | NDJSON traces, deterministic |

**Key innovations:**
- 24+ messaging platform integration in one binary
- Voice wake + Talk Mode (macOS/iOS/Android)
- A2UI Live Canvas (agent-driven visual workspace)
- Browser control via Chromium CDP
- Agent-to-agent communication (sessions_send)
- ClawHub skills registry
- Docker sandboxing for non-main sessions

### Features to Steal from OpenClaw

| Feature | What OpenClaw Does | How Nika Could Adapt |
|---------|-------------------|---------------------|
| Multi-channel delivery | 24+ messaging platforms | Telegram webhook trigger (post-MVP) |
| Skills registry | ClawHub for community skills | Nika package ecosystem (`nika pkg`) |
| Voice interaction | Wake words, continuous voice | Not in scope (different paradigm) |

---

## 4. Hermes Agent (Nous Research)

**Type:** Self-improving personal AI agent
**License:** MIT | **Language:** Python | **Stars:** ~14.5K

| Aspect | Hermes Agent | Nika |
|--------|-------------|------|
| Language | Python | Rust (single binary, no runtime) |
| Paradigm | Imperative, autonomous | Declarative YAML |
| Self-improvement | Native (learns from experience, creates skills) | Not yet (potential future feature) |
| Memory | 4-layer hierarchy (short-term -> skills -> nudges -> RL) | Context files, skills, artifacts |
| User modeling | Honcho dialectic (builds model of who you are) | None |
| Multi-model | Nous Portal, OpenRouter (200+), OpenAI, custom | 7 cloud + local GGUF + custom endpoints |
| Tool calling | Python RPC + MCP | MCP protocol (standard) |
| Media | None | 24 builtin media tools |
| Determinism | Low (autonomous agent) | High (DAG + structured output) |
| RL Training | Atropos environments, trajectory generation | None |
| Backends | 6 (local, Docker, SSH, Daytona, Singularity, Modal) | Local + custom endpoints |

**Key innovations:**
- **4-layer self-improvement loop**: Memory (seconds) -> Skills (minutes) -> Background Review/Nudges (async) -> RL Training (Atropos)
- **agentskills.io** open standard for skill sharing
- **FTS5 session search** with LLM summarization for cross-session recall
- **Honcho dialectic** user modeling (builds persistent model of user preferences)
- **Subagent delegation** for parallel workstreams
- **Zero-context-cost turns** via Python scripts that call tools via RPC

### Features to Steal from Hermes Agent

| Feature | What Hermes Does | How Nika Could Adapt |
|---------|-----------------|---------------------|
| Self-improvement | Background review agent creates skills from experience | Layer 6: workflow self-analysis via `agent:` verb |
| Skills standard | agentskills.io open standard | Nika skills already exist — align with standard |
| User modeling | Honcho dialectic builds user profile | NovaNet entity for user preferences |
| Trajectory generation | Every session becomes RL training data | Nika traces are already structured — export to training format |

> [!WARNING]
> **Hermes's biggest strength** is self-improvement and persistent memory. This is a genuine gap in Nika — the ability for workflows to learn from past executions and evolve their own skills. See Layer 6 in Part 2.

---

## 5. Rein (YAML AI Orchestrator)

**Type:** Open-source YAML AI workflow orchestrator — closest direct competitor
**Status:** Early stage, limited ecosystem

| Aspect | Rein | Nika |
|--------|------|------|
| Config format | YAML | YAML (.nika.yaml) |
| Workflow model | Linear steps | DAG with parallel + for_each |
| Multi-agent | 8 agents debating in 97 YAML steps | `agent:` verb with guardrails, max_turns, tools |
| Providers | Unknown | 7 cloud + local GGUF + custom endpoints |
| Media | None | 24 builtin media tools |
| Structured output | None | 5-layer schema validation |
| MCP | Unknown | Native rmcp client + 24 builtins |
| Testing | Unknown | 8457 tests, mock provider, dry-run |
| TUI | None | 3-view ratatui |
| LSP | None | Completions + diagnostics |
| Course | None | 12 levels, 44 exercises |

> [!NOTE]
> Rein validates the YAML-for-AI-orchestration thesis but is far less mature. Nika's DAG execution, structured output, media pipeline, and developer tooling (TUI, LSP, course) create a significant moat.

---

## 6. Codex (OpenAI)

**Type:** Cloud-based coding agent — sandboxed environment, PR-oriented

| Aspect | Codex | Nika |
|--------|-------|------|
| Execution | Cloud sandbox (firecracker) | Local + cloud |
| Workflow | Single PR task | Multi-step DAG |
| Multi-model | OpenAI only | 7 cloud + local + custom endpoints |
| Trace | GitHub PR diffs | NDJSON events |
| Cost model | Per-task billing | Pay-per-API-call |

> [!NOTE]
> **Different markets:** Codex focuses on code-change-as-output (PRs). Nika focuses on arbitrary AI workflow orchestration.

---

## 7. LangGraph (LangChain)

**Type:** Python framework for stateful agent graphs — most adopted multi-agent framework (27,100 monthly searches)

| Aspect | LangGraph | Nika |
|--------|-----------|------|
| Language | Python | Rust (YAML DSL) |
| Graph model | StateGraph with conditional edges | DAG with 5 verbs |
| State | Shared state dict | RunContext + bindings |
| Checkpointing | Built-in persistence | Event log (replay) |
| Performance | Python (slow) | Rust + tokio (fast) |
| MCP | Via langchain-mcp | Native rmcp |
| Knowledge graph | Manual integration | NovaNet built-in |
| Structured output | LangChain output parsers | 5-layer validation |
| Media | None | 24 builtin media tools |
| Model support | 100+ models via LangChain | 7 providers + local + custom |

**Strengths:** 40-50% LLM call savings via state persistence. LangSmith monitoring. Huge ecosystem.
**Weaknesses:** Steep learning curve, Python-only, heavy dependency chain.

> [!NOTE]
> **Tradeoff:** LangGraph is more flexible (arbitrary Python) but slower, harder to reproduce, and lacks YAML-first philosophy. Nika's YAML workflows are version-controllable artifacts; LangGraph's Python graphs are code.

---

## 8. CrewAI

**Type:** Role-based multi-agent framework (Python) — #2 for rapid multi-agent prototyping

| Aspect | CrewAI | Nika |
|--------|--------|------|
| Agent model | Role-based (researcher, writer, etc.) | Verb-based (infer, exec, agent) |
| Coordination | Sequential/hierarchical | DAG with parallel + for_each |
| Memory | Short/long-term/entity memory (3 types) | RunContext (session) + NovaNet (persistent) |
| Tools | Custom tool definitions | MCP tools + 24 builtins |
| Structured output | Pydantic models | 5-layer schema validation |

**Strengths:** 2-4 hours to prototype multi-agent systems. Role-based collaboration is intuitive.
**Weaknesses:** Less suited for stateful/non-collaborative flows.

> [!WARNING]
> CrewAI's **3-type memory system** (short-term, long-term, entity) is more mature than Nika's single RunContext. This gap is addressed by P-MEMORY and P-RECORD in the [Evolution Roadmap](./05-evolution-roadmap.md).

---

## 9. SWE-bench Leaderboard (March 2026)

| Agent | Score | Model |
|-------|-------|-------|
| GPT-5.4 Pro | 95% | OpenAI |
| Claude Opus 4.6 | 91% | Anthropic |
| Devin | ~85% | Multi-model |
| OpenHands | 77.6% | Multi-model |
| SWE-Agent | ~80% | Various |

> [!TIP]
> The models themselves are converging at high capability. The differentiator is now **orchestration quality** — how effectively you chain, route, and compose LLM calls. This validates Nika's focus on workflow engineering over model capability.

---

## 10. Protocol Landscape

```mermaid
flowchart LR
    subgraph MCP_BOX["MCP (Anthropic)"]
        direction TB
        M1["Agent <-> Tools"]
        M2["97M monthly SDK downloads"]
        M3["6,000+ app integrations"]
    end

    subgraph A2A_BOX["A2A (Google -> Linux Foundation)"]
        direction TB
        A1["Agent <-> Agent"]
        A2["Agent Cards at /.well-known/agent.json"]
        A3["JSON-RPC 2.0, SSE, OAuth 2.0"]
    end

    NIKA_NOW["Nika today:\nMCP client +\n24 builtin tools +\nrmcp v0.16"]
    NIKA_FUT["Nika future:\nMCP client +\nA2A for discovery\n& delegation"]

    MCP_BOX --> NIKA_NOW
    MCP_BOX --> NIKA_FUT
    A2A_BOX -.->|"future"| NIKA_FUT

    style MCP_BOX fill:#dbeafe,stroke:#2563eb
    style A2A_BOX fill:#dcfce7,stroke:#16a34a
    style NIKA_NOW fill:#fef3c7,stroke:#d97706
    style NIKA_FUT fill:#ede9fe,stroke:#7c3aed
```

### MCP (Model Context Protocol)

- **Created by:** Anthropic
- **Status:** De facto standard for AI tool calling. 97M monthly SDK downloads.
- **Supported by:** Claude, GPT-4, Gemini, LLaMA, and hundreds of third-party tools.
- **Prediction:** "Does it have an MCP server?" becomes procurement question by 2027.
- **Nika uses:** rmcp v0.16 as MCP client, NovaNet as MCP server, 24 builtin tools (`nika:*`)

### A2A (Agent-to-Agent Protocol)

- **Created by:** Google (April 2025), donated to Linux Foundation (June 2025)
- **Key features:** Agent Cards at `/.well-known/agent.json`, JSON-RPC 2.0, SSE streaming, OAuth 2.0
- **Role:** Inter-agent communication — complements MCP (horizontal vs vertical)

> [!NOTE]
> If Nika wants to support multi-runtime agent coordination (e.g., a Nika agent delegating to an external LangGraph agent), A2A is the protocol to adopt.

---

## Competitive Positioning

```mermaid
quadrantChart
    title Expressivity vs Memory Sophistication
    x-axis "Basic Memory" --> "Episodic Memory"
    y-axis "Low Expressivity" --> "High Expressivity"
    quadrant-1 "Leader Zone"
    quadrant-2 "Expressive but Forgetful"
    quadrant-3 "Rigid & Forgetful"
    quadrant-4 "Smart Memory, Low Flex"
    "Nika v0.49": [0.40, 0.85]
    "Slate": [0.75, 0.85]
    "Hermes Agent": [0.80, 0.50]
    "OpenClaw": [0.45, 0.40]
    "Claude Code": [0.35, 0.65]
    "LangGraph": [0.45, 0.50]
    "CrewAI": [0.55, 0.40]
    "Codex": [0.20, 0.55]
    "Rein": [0.25, 0.55]
```

### Nika's Moat

No competitor has **all** of these:

```mermaid
mindmap
    root((Nika's Moat))
        NovaNet Knowledge Graph
            Curated, not auto-generated
            59 NodeClasses
            200+ locales
        YAML-First Workflows
            Version-controlled
            Auditable DAG
            Reproducible traces
            8457 tests
        Rust Performance
            tokio concurrency
            Sub-millisecond DAG validation
            Single binary, no runtime
        Developer Experience
            TUI (3 views, ratatui)
            LSP (completions, diagnostics)
            Course (12 levels, 44 exercises)
            115 showcase workflows
        Media Pipeline
            24 builtin tools
            CAS content store
            Vision support (6 providers)
            C2PA provenance
        Observability
            43 event types
            NDJSON traces
            Full token tracking
            Daemon with secrets/cache
        Security
            Shell-free exec default
            Command blocklist
            Path traversal prevention
            SSRF validation
        Structured Output
            5-layer validation pipeline
            Inject -> Extract -> Validate -> Retry -> Repair
```

### NovaNet's Unique Position

Research confirmed that NovaNet has **no direct competitors** in the curated-graph native-generation space:

| Closest Alternative | Why It's Different |
|--------------------|--------------------|
| GraphRAG (Microsoft) | Auto-built from documents, not curated |
| Knowledge graph + LLM research | Academic, no product |
| Wikidata / DBpedia | General purpose, not per-org content generation |

> [!IMPORTANT]
> NovaNet's combination of per-locale knowledge atoms (Expression, Pattern, CultureRef, Taboo) with entity-based content generation is **unique in the market**. No competitor offers curated, locale-aware knowledge graph integration with workflow orchestration.

---

## Master Comparison Table

| Capability | Nika | Slate | Claude Code | OpenClaw | Hermes | Rein | LangGraph | CrewAI |
|-----------|:----:|:-----:|:-----------:|:--------:|:------:|:----:|:---------:|:------:|
| Declarative YAML | Yes | -- | -- | -- | -- | Yes | -- | -- |
| DAG execution | Yes | -- | -- | -- | -- | Linear | Graph | Seq/Hier |
| Multi-provider (7+) | Yes | Partial | Claude | Multi | Multi | ? | Yes | Yes |
| Local inference | GGUF | -- | -- | -- | Via OpenRouter | ? | -- | -- |
| Custom endpoints | Yes | -- | -- | -- | OpenAI-compat | ? | -- | -- |
| MCP native | Yes | -- | Client | -- | Client | ? | Via adapter | -- |
| 24 media tools | Yes | -- | -- | -- | -- | -- | -- | -- |
| Structured output | 5-layer | -- | Ad-hoc | -- | -- | -- | Parsers | Pydantic |
| Knowledge graph | NovaNet | -- | -- | -- | -- | -- | -- | -- |
| TUI | 3-view | -- | Terminal | -- | CLI | -- | -- | -- |
| LSP | Yes | -- | -- | -- | -- | -- | -- | -- |
| Course/learning | 44 exercises | -- | -- | -- | -- | -- | Docs | Docs |
| Self-improvement | -- | -- | -- | -- | 4-layer | -- | -- | -- |
| Multi-channel | -- | -- | IDE+GitHub | 24+ | 5 gateways | -- | -- | -- |
| Voice | -- | -- | -- | Yes | -- | -- | -- | -- |
| Episodic memory | -- | Yes | -- | -- | Skills+Memory | -- | Checkpoint | 3-type |
| Agent parallelism | for_each | Native | -- | Sessions | Subagents | Steps | Nodes | Crews |
| RL training | -- | -- | -- | -- | Atropos | -- | -- | -- |
| Tests | 8457 | ? | ? | ? | ? | ? | Yes | Yes |

---

# Part 2 — Slate Deep Integration Strategy

> Copying Slate's thread/record architecture into Nika, then going beyond.
> Every concept mapped. Every claim verified. Every design grounded in existing code.

---

## Why This Part Exists

Slate (Random Labs)[^1] introduced an architecture — threads, records, thread weaving, orchestrator/agent dispatch — that solves the fundamental problems of long-running AI agents. This section maps every Slate concept to Nika's existing architecture, identifies what needs to change, and designs how Nika goes **beyond** Slate by leveraging the NovaNet knowledge graph, YAML declarative workflows, and full observability.

> [!IMPORTANT]
> **Guiding principle** — We are not building feature parity with Slate. We are taking Slate's **architectural insights** and implementing them in a way that is **declaratively superior** — auditable, reproducible, version-controlled, and knowledge-graph-powered.

---

## Slate's Core Architecture

### The Problem Slate Solves

LLM context windows are not uniformly useful. Performance degrades past a threshold — the "dumb zone" (Dex Horthy's term[^1]). Every existing approach fails in a specific way:

```mermaid
flowchart TB
    PROBLEM["Context Degradation\nLLMs lose quality past working memory"]

    subgraph FAILS["Approaches That Fail"]
        C["Compaction\nlossy, unpredictable"]
        S["Subagents\nisolated, no sharing"]
        P["Plans\nunderspecified, forgotten"]
        T["Task Decomp\nrigid, can't adapt"]
        R["RLM\nblind N-step"]
        D["Devin/Manus\ncompress-boundary loss"]
    end

    PROBLEM --> FAILS

    subgraph SOLUTION["Slate's Solution"]
        TH["Threads\none-action workers"]
        EP["Records\ncompletion-boundary compression"]
        TW["Thread Weaving\nimplicit adaptive decomposition"]
    end

    FAILS -->|"all fail at"| SOLUTION

    style PROBLEM fill:#dc2626,color:#fff
    style FAILS fill:#fee2e2,stroke:#dc2626
    style SOLUTION fill:#dcfce7,stroke:#16a34a
```

### The 8 Interconnected Concepts

```mermaid
flowchart TD
    WM["1. Working Memory\n& Dumb Zone"] --> TH["2. Threads\nOne-action workers"]
    TH --> EP["3. Records\nCompletion-boundary compression"]
    EP --> TW["4. Thread Weaving\nOrchestrator loop"]
    TW --> ST["5. Orchestrator/Agents\nAlphaZero mapping"]
    EP --> KO["6. Knowledge Overhang\nScaffolding activates latent knowledge"]
    EP --> CO["7. Composability\nRecords flow between threads"]
    TW --> OS["8. OS Framing\nKernel + processes + return values"]

    style WM fill:#dbeafe,stroke:#2563eb
    style TH fill:#dbeafe,stroke:#2563eb
    style EP fill:#fef3c7,stroke:#d97706
    style TW fill:#fef3c7,stroke:#d97706
    style ST fill:#dcfce7,stroke:#16a34a
    style KO fill:#dcfce7,stroke:#16a34a
    style CO fill:#dcfce7,stroke:#16a34a
    style OS fill:#f3e8ff,stroke:#7c3aed
```

<details>
<summary>Concept Details</summary>

| # | Concept | Mechanism | Key Insight |
|---|---------|-----------|-------------|
| 1 | **Working Memory** | Context has a usable zone and a degraded "dumb zone" | Never exceed working memory threshold |
| 2 | **Threads** | Each thread executes ONE action, then pauses. NOT persistent subagents | Context isolated per thread |
| 3 | **Records** | Compressed representation at completion boundary (not mid-stream) | Only important results retained |
| 4 | **Thread Weaving** | Orchestrator: dispatch threads -> collect records -> synthesize -> dispatch | Implicit adaptive decomposition |
| 5 | **Orchestrator/Agents** | Orchestrator = open-ended planning. Agents = learned action sequences | AlphaZero mapping (value + policy networks)[^2] |
| 6 | **Knowledge Overhang** | Models have knowledge they can't access without scaffolding | Records provide the scaffolding |
| 7 | **Composability** | Records flow between threads as handoff boundary | Cross-model composition |
| 8 | **OS Framing** | Orchestrator = kernel, Threads = processes, Records = return values | Karpathy's LLM OS framing |

</details>

> [!NOTE]
> **Slate's model configuration** — Slate supports cross-model composition (e.g., Sonnet + Codex for different cognitive tasks)[^1]. The "8 agent presets" (default/lite/think/search/vision/judge/coder/summary) is our design, inspired by this capability.

---

## The Critical Realization: Nika IS Already an OS

```mermaid
flowchart LR
    subgraph SLATE["Slate OS Concepts"]
        SK["Kernel\n(scheduler)"]
        SP["Processes\n(threads)"]
        SRV["Return Values"]
        SRAM["RAM\n(context)"]
        SFS["File System"]
        SIPC["IPC"]
        SFJ["Fork/Join"]
    end

    subgraph NIKA["Nika Already Has"]
        NK["DAG Scheduler\n(runner.rs)"]
        NP["Tasks\n(5 verbs)"]
        NRV["TaskResult\nin RunContext"]
        NRAM["Agent context\nwindow"]
        NFS["NovaNet\nknowledge graph"]
        NIPC["with: bindings\n(A.output -> B)"]
        NFJ["for_each +\nconcurrency"]
    end

    SK -.->|"="| NK
    SP -.->|"="| NP
    SRV -.->|"="| NRV
    SRAM -.->|"="| NRAM
    SFS -.->|"="| NFS
    SIPC -.->|"="| NIPC
    SFJ -.->|"="| NFJ

    style SLATE fill:#f0f9ff,stroke:#0284c7
    style NIKA fill:#f0fdf4,stroke:#16a34a
```

**Current codebase evidence (v0.49):**
- `RunContext` in `store/` — runtime state container (replaces old Egghead naming)
- 43 `EventKind` variants — full event sourcing
- 8457 tests across 10 workspace crates
- `runner.rs` — DAG scheduler with topological execution
- `executor/` — verb dispatch (infer, exec, fetch, invoke, agent)
- `for_each` — parallel fan-out with `concurrency:` control

> [!TIP]
> **What's missing is NOT the kernel** — it's 4 kernel upgrades: record compression, dynamic process creation (orchestrator), memory budgets, and agent routing. The kernel itself (`Runner` + `TaskExecutor` + `RunContext`) already works.

---

## Concept-by-Concept Mapping

### Complete Mapping Table

| # | Slate Concept | Nika Existing | Nika Needed | Nika Goes Beyond |
|:-:|---------------|---------------|-------------|------------------|
| 1 | Working Memory | No awareness | Context budget per task | Budget is declarative YAML |
| 2 | Dumb Zone | N/A | Working memory boundary | Token budget in events |
| 3 | Threads | Tasks in DAG (partial) | Dynamic dispatch by orchestrator | Tasks referencing agents in YAML |
| 4 | Records | `TaskResult` (raw) | Record compression at boundary | NovaNet persistence |
| 5 | Thread Weaving | DAG execution (static) | Dynamic DAG + orchestrator loop | Real-time TUI visualization |
| 6 | Orchestrator/Agents | Flat agent loop | `goal:` | Declarative YAML orchestrators |
| 7 | Knowledge Overhang | NovaNet context + files | Record-based scaffolding | 200+ locale knowledge atoms |
| 8 | Episodic Memory | In-memory `RunContext` | NovaNet `Record` | Graph-queryable, entity-linked |
| 9 | Agent Presets | Single provider | `agents:` in YAML | Per-workflow agents (8 presets) |
| 10 | Composability | `with:` bindings | Record-aware bindings | Structured output + records |
| 11 | Parallel Threads | `for_each` + concurrency | Orchestrator parallel dispatch | Token budget + cost tracking |
| 12 | Cross-model | Multi-provider (7+local+custom) | Agent preset per task | YAML-declared routing |
| 13 | OS Framing | DAG = kernel | Orchestrator = kernel upgrade | NovaNet = persistent storage |
| 14 | Permissions | Command blocklist + shell-free + SSRF | Already better | 4-layer security model |
| 15 | build/plan agents | `agent:` verb | orchestrate mode selection | Multiple orchestrator configurations |
| 16 | Custom /commands | Skills via `include:` | Already exists | YAML skills merged via DAG |
| 17 | .env config | `.nika/config.toml` | Already exists | 3-level config merge |

---

## Design: The 7-Layer Architecture

```mermaid
flowchart TB
    subgraph L1["Layer 1 -- Agent Presets"]
        MS["agents:\n8 named presets per workflow"]
    end

    subgraph L2["Layer 2 -- Record Engine"]
        EE["record:\ncompression at completion boundary"]
    end

    subgraph L3["Layer 3 -- Orchestrate Mode"]
        SO["goal:\ndynamic agent dispatch"]
    end

    subgraph L4["Layer 4 -- Context Budget"]
        CB["context_budget:\nworking memory enforcement"]
    end

    subgraph L5["Layer 5 -- NovaNet Memory"]
        NM["record.persist: novanet\ncross-session, entity-linked"]
    end

    subgraph L6["Layer 6 -- Self-Improvement"]
        SI["Hermes-inspired learning:\nworkflow self-analysis + skill evolution"]
    end

    subgraph L7["Layer 7 -- Package Ecosystem"]
        PK["nika pkg:\ncommunity workflows + skills + agents"]
    end

    L1 --> L2
    L2 --> L3
    L3 --> L4
    L4 --> L5
    L5 --> L6
    L6 --> L7

    style L1 fill:#dbeafe,stroke:#2563eb
    style L2 fill:#dbeafe,stroke:#2563eb
    style L3 fill:#fef3c7,stroke:#d97706
    style L4 fill:#fef3c7,stroke:#d97706
    style L5 fill:#dcfce7,stroke:#16a34a
    style L6 fill:#ede9fe,stroke:#7c3aed
    style L7 fill:#ccfbf1,stroke:#0d9488
```

### Layer 1: Agent Presets

Per-workflow agent definitions that route different cognitive tasks to different providers. 8 presets available: default, lite, think, search, vision, judge, coder, summary. See [05-evolution-roadmap.md P-MODEL](./05-evolution-roadmap.md#p-model-4-slot-model-architecture) for full design.

```yaml
# models: layer defines reusable model references
models:
  sonnet: { provider: anthropic, model: claude-sonnet-4-20250514 }
  groq-llama: { provider: groq, model: llama-3.3-70b-versatile }
  deepseek: { provider: deepseek, model: deepseek-chat }
  gpt4o: { provider: openai, model: gpt-4o }

# agents: reference models by alias
agents:
  default: { model: sonnet }
  lite:    { model: groq-llama }
  search:  { model: deepseek }
  think:   { model: sonnet, extended_thinking: true }
  vision:  { model: gpt4o }
  judge:   { model: sonnet }
  coder:   { model: sonnet }
  summary: { model: groq-llama }
```

### Layer 2: Record Engine

See [05-evolution-roadmap.md P-RECORD](./05-evolution-roadmap.md#p-record-record-engine) for full design.

```yaml
record:
  compress: true           # LLM compression at completion boundary
  retain: [key_findings]   # Explicit key extraction
  max_tokens: 500          # Size limit
  confidence_threshold: 0.8
```

### Layer 3: Orchestrate Mode

The core upgrade. See [05-evolution-roadmap.md P-ORCHESTRATE](./05-evolution-roadmap.md#p-orchestrate-orchestrate-mode) for full design.

```mermaid
sequenceDiagram
    participant S as Orchestrator
    participant T as Tasks with agent: ref

    loop Until DONE or max_rounds
        S->>S: Review accumulated records
        S->>T: Dispatch task(s) with params
        T-->>S: Record(s) with confidence
        S->>S: Synthesize records
        alt confidence >= threshold
            S->>S: Continue or DONE
        else confidence < threshold
            S->>T: Retry with better agent
        end
    end
```

### Layer 4: Context Budget

See [05-evolution-roadmap.md P-CONTEXT](./05-evolution-roadmap.md#p-context-context-budget-management) for full design.

### Layer 5: NovaNet Episodic Memory

See [05-evolution-roadmap.md P-MEMORY](./05-evolution-roadmap.md#p-memory-novanet-episodic-memory) for full design.

### Layer 6: Self-Improvement (Hermes-Inspired)

Inspired by Hermes Agent's 4-layer learning loop. Nika can implement workflow self-analysis using its own `agent:` verb:

```yaml
# Post-execution analysis task (future)
- id: self_review
  depends_on: [main_workflow]
  agent:
    system: "You are a workflow optimization analyst."
    prompt: |
      Review the execution trace of {{with.trace}}.
      Identify: slow tasks, redundant steps, failed retries.
      Suggest improvements as a YAML diff.
    tools: [nika:*]
    max_turns: 5
    completion:
      mode: explicit
```

**Key differences from Hermes:**
- Hermes learns autonomously in background threads; Nika's self-improvement would be an explicit, auditable workflow step
- Hermes uses Python skills; Nika would produce YAML workflow patches
- Hermes trains RL models; Nika would optimize DAG structure and agent routing

### Layer 7: Package Ecosystem

Community-driven workflow, skill, and agent sharing via `nika pkg`:

- **Workflows:** Reusable .nika.yaml files (already 115 showcases)
- **Skills:** Prompt augmentation files (already supported via `skills:`)
- **Agents:** Pre-configured agent presets
- **Media pipelines:** Reusable processing chains
- **Registry:** `nika pkg list`, `nika pkg install`, `nika pkg publish`

---

## Why Nika Goes Beyond Slate

```mermaid
flowchart LR
    subgraph SLATE_HAS["Slate Has"]
        S1["TypeScript threads"]
        S2["In-memory sessions"]
        S3["Basic logging"]
        S4["No cost control"]
        S5["English only"]
    end

    subgraph NIKA_HAS["Nika Has (after integration)"]
        N1["YAML tasks with agent: refs"]
        N2["NovaNet graph memory"]
        N3["43 events + NDJSON"]
        N4["Record budget + tokens"]
        N5["200+ locales"]
    end

    S1 -.->|"upgraded to"| N1
    S2 -.->|"upgraded to"| N2
    S3 -.->|"upgraded to"| N3
    S4 -.->|"upgraded to"| N4
    S5 -.->|"upgraded to"| N5

    style SLATE_HAS fill:#fee2e2,stroke:#dc2626
    style NIKA_HAS fill:#dcfce7,stroke:#16a34a
```

| Dimension | Slate | Nika (after integration) |
|-----------|-------|--------------------------|
| Thread definition | TypeScript code | YAML tasks with agent: refs |
| Record storage | In-memory session | `RunContext` + NovaNet graph |
| Cross-session | Session files | Knowledge graph (queryable) |
| Observability | Basic logging | 43 EventKind variants[^4] + NDJSON |
| Cost control | None | Record budget + token tracking |
| Knowledge source | None | NovaNet atoms (200+ locales) |
| Reproducibility | Non-deterministic | DAG traces + replay |
| Multi-locale | English only | 200+ locales |
| Model routing | Global config | Per-workflow `agents:` (8 presets) |
| Orchestration | Imperative code | Declarative YAML |
| DAG visualization | None | Real-time TUI (3 views) |
| Structured output | Not documented | 5-layer validation |
| Security | Not documented | Shell-free + blocklist + SSRF |
| Media pipeline | None | 24 builtin tools + CAS |
| LSP support | None | Completions + diagnostics |
| Self-improvement | None | Layer 6 (Hermes-inspired) |
| Package ecosystem | None | Layer 7 (`nika pkg`) |

> [!IMPORTANT]
> **Unique to Nika** (no competitor has equivalent):
> - NovaNet knowledge graph (59 NodeClasses, 159 ArcClasses)
> - Entity-linked episodic memory with graph queries
> - Knowledge atoms (Expression, Pattern, CultureRef, Taboo) across 200+ locales
> - NDJSON trace files with full event sourcing (43 EventKind variants)
> - 5-layer structured output (inject -> extract -> validate -> retry -> repair)
> - DAG visualization in TUI with real-time thread/record view
> - Record budget with per-workflow cost prediction
> - 24 media tools with CAS content store and C2PA provenance
> - LSP intelligence for .nika.yaml files
> - 12-level course with 44 exercises

---

## Architecture Comparison Matrix

For each dimension in Slate's comparison table[^1], here's where Nika lands:

| Dimension | ReAct | Plan | Task Trees | RLM | Devin | Claude Code | Slate | **Nika** |
|-----------|:-----:|:----:|:----------:|:---:|:-----:|:-----------:|:-----:|:--------:|
| Planning | Implicit | Explicit | Explicit | None | Explicit | Implicit | Implicit | **Implicit** |
| Decomposition | None | Manual | Static | None | Static | None | Implicit | **Implicit** |
| Feedback | Per-step | End | End | None | Per-task | Per-step | Per-record | **Per-record** |
| Context isolation | None | None | Partial | None | Full | None | Per-thread | **Per-task** |
| Compression | Compact | Compact | None | None | Compress | Compact | Record | **Record+NovaNet** |
| Parallelism | None | None | None | None | Multi-agent | None | Native | **for_each+orchestrate** |
| Adaptability | Low | Low | Low | Low | Medium | High | High | **High** |
| **Reproducibility** | Low | Low | Low | Low | Low | Low | Low | **High** |
| **Observability** | Low | Low | Low | Low | Medium | Medium | Low | **High** |
| **Cost control** | None | None | None | None | None | None | None | **Record budget** |
| **Knowledge graph** | None | None | None | None | None | None | None | **NovaNet** |
| **Media pipeline** | None | None | None | None | None | None | None | **24 tools** |
| **Structured output** | None | None | None | None | None | Ad-hoc | None | **5-layer** |

---

## Complete Example

<details>
<summary>Full Orchestrate Workflow -- Landing Page Generation</summary>

```yaml
schema: "nika/workflow@0.12"
workflow: generate-landing-page
provider: anthropic
model: claude-sonnet-4-20250514

models:
  sonnet: { provider: anthropic, model: claude-sonnet-4-20250514 }
  groq-llama: { provider: groq, model: llama-3.3-70b-versatile }
  deepseek: { provider: deepseek, model: deepseek-chat }
  gpt4o: { provider: openai, model: gpt-4o }

agents:
  think:
    model: sonnet
    extended_thinking: true
    thinking_budget: 16384
  default:  { model: sonnet }
  search:   { model: groq-llama }
  lite:     { model: deepseek }
  vision:   { model: gpt4o }
  judge:
    provider: anthropic
    model: claude-sonnet-4-20250514
  coder:
    provider: anthropic
    model: claude-sonnet-4-20250514
  summary:
    provider: groq
    model: llama-3.3-70b-versatile

inputs:
  locale: "fr-FR"
  focus: "homepage"

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "--manifest-path", "/path/to/novanet/Cargo.toml"]

tasks:
  - id: get_context
    invoke:
      tool: "novanet::novanet_context"
      params:
        focus_key: "{{inputs.focus}}"
        locale: "{{inputs.locale}}"
        mode: page
    record:
      compress: true
      max_tokens: 500

  - id: research
    depends_on: [get_context]
    with:
      context: $get_context
    infer:
      prompt: |
        Research current QR code AI trends for a French landing page.
        Entity context: {{with.context}}
      temperature: 0.7
    record:
      compress: true
      max_tokens: 300
      retain: [key_findings]

  - id: write_hero
    depends_on: [research]
    with:
      context: $get_context
      research: $research
    infer:
      prompt: |
        Write the hero section for the landing page.
        Entity context: {{with.context}}
        Research findings: {{with.research}}
      max_tokens: 1000
    record:
      compress: true
      retain: [content]
      max_tokens: 800

  - id: write_features
    depends_on: [research]
    with:
      context: $get_context
      research: $research
    infer:
      prompt: |
        Write the features section for the landing page.
        Entity context: {{with.context}}
        Research findings: {{with.research}}
      max_tokens: 1500
    record:
      compress: true
      retain: [content]
      max_tokens: 800

  - id: review
    depends_on: [write_hero, write_features]
    with:
      hero: $write_hero
      features: $write_features
    infer:
      prompt: |
        Review the following draft sections for quality and coherence:

        ## Hero
        {{with.hero}}

        ## Features
        {{with.features}}

        Check against QR Code AI brand guidelines and French locale conventions.
    structured:
      schema:
        type: object
        properties:
          score: { type: number }
          issues: { type: array, items: { type: string } }
          suggestions: { type: array, items: { type: string } }
        required: [score, issues, suggestions]

  - id: persist_records
    depends_on: [review]
    with:
      review: $review
    invoke:
      tool: "novanet::novanet_write"
      params:
        class: Record
        key: "landing-page-{{inputs.locale}}"
    record:
      persist: novanet
      entity_link: qr-code-ai
```

</details>

---

## Confidence -> Natural Escalation

The old P3 ConfidenceRouter is absorbed into records. Confidence is a property, not a system:

```mermaid
flowchart LR
    subgraph OLD["Old: ConfidenceRouter"]
        O1["Task"] --> O2["Tier 1 Model"]
        O2 -->|"confidence < 0.8"| O3["Tier 2 Model"]
        O3 -->|"confidence < 0.8"| O4["Tier 3 Model"]
    end

    subgraph NEW["New: Record Confidence"]
        N1["Task"] --> N2["Record\nconfidence: 0.6"]
        N2 --> N3["orchestrator\nsees low confidence"]
        N3 -->|"retry?"| N4["Better agent"]
        N3 -->|"more context?"| N5["Add research"]
        N3 -->|"good enough?"| N6["Accept & continue"]
    end

    style OLD fill:#fee2e2,stroke:#dc2626
    style NEW fill:#dcfce7,stroke:#16a34a
```

> [!TIP]
> The orchestrator has **full context** to decide how to handle low confidence. A rigid router has fixed rules. This is simpler AND more powerful.

---

## Summary

```mermaid
mindmap
    root((Nika Evolution))
        Slate's Insights
            Threads -> Tasks
            Records -> Compressed results
            Weaving -> orchestrate mode
            Agent presets -> Per-workflow routing
        Hermes's Insights
            Self-improvement loop
            Skill creation from experience
            User modeling
            RL training pipeline
        Nika's Additions
            YAML declarative
            NovaNet knowledge graph
            43 events observability
            5-layer structured output
            24 media tools + CAS
            TUI + LSP + Course
            Record budget cost control
            200+ locales
            8457 tests
        Result
            Most advanced RLM implementation
            Auditable, reproducible
            Knowledge-graph-powered
            Self-improving (future)
```

> [!IMPORTANT]
> **The Golden Rule (extended):**
>
> | Concern | Owner | Why |
> |---------|-------|-----|
> | **KNOWING** things | NovaNet | Knowledge graph, entities, locales, semantics |
> | **DOING** things | Nika | Workflow execution, DAG, verbs, providers |
> | **CONNECTING** | MCP | Protocol boundary, zero Cypher in Nika |
> | **THINKING** | Records | orchestrate mode, agent routing, confidence |
> | **REMEMBERING** | Records -> NovaNet | Cross-session memory, entity-linked persistence |
> | **LEARNING** | Self-improvement | Hermes-inspired workflow analysis + skill evolution |

---

<div align="center">

[<- 01 State of the Art](./01-current-features.md) . [Index](./00-README.md) . [05 Roadmap ->](./05-evolution-roadmap.md)

</div>

---

[^1]: Slate by Random Labs — [Technical blog post](https://randomlabs.ai/blog/slate) with 26 academic references. Thread-based episodic memory architecture. The "8 agent presets" design is our proposal, inspired by Slate's cross-model composition (Sonnet + Codex). [Documentation](https://docs.randomlabs.ai). npm: `@randomlabs/slate` v1.0.15.
[^2]: McGrath et al., "Acquisition of Chess Knowledge in AlphaZero" — [PNAS 2022](https://www.pnas.org/doi/10.1073/pnas.2206625119). Hierarchical decomposition with per-task model routing. orchestrator/agents separation cited in Slate blog.
[^3]: THREAD: Thinking Hierarchically for Resource-Efficient Agent Decision-making — [arXiv:2405.17402](https://arxiv.org/abs/2405.17402). Hierarchical decomposition with per-task model routing.
[^4]: Verified via engine event system — 43 `EventKind` variants as of v0.49.
