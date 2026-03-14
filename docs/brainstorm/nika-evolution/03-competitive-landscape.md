# 03 — Competitive Landscape

> Analysis of competing agent runtimes and coding agents.
> Date: 2026-03-14

---

## Market Map

```
┌─────────────────────────────────────────────────────────────────────┐
│  AGENT RUNTIME LANDSCAPE (March 2026)                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  CODING AGENTS (product)                                            │
│  ├── Claude Code (Anthropic) — CLI agent with hooks/skills          │
│  ├── Codex (OpenAI) — cloud sandbox, PR-oriented                    │
│  ├── Devin (Cognition) — full dev environment                       │
│  ├── Cursor / Windsurf / Cline — IDE-embedded agents                │
│  └── Slate (Random Labs) — swarm-native coding agent                │
│                                                                     │
│  WORKFLOW ENGINES (framework)                                       │
│  ├── LangGraph (LangChain) — Python, stateful graphs                │
│  ├── CrewAI — role-based multi-agent                                │
│  ├── AutoGen (Microsoft) — conversational agents                    │
│  ├── Dify — visual workflow builder                                 │
│  └── Nika — YAML DAG + MCP + knowledge graph                       │
│                                                                     │
│  PROTOCOLS                                                          │
│  ├── MCP (Anthropic) — tool provision (intra-agent)                 │
│  ├── A2A (Google → Linux Foundation) — agent coordination           │
│  └── ACP (various) — agent communication                            │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 1. Slate by Random Labs

**Source:** Blog post (randomlabs.ai/blog/slate) + full documentation (docs.randomlabs.ai) + Twitter thread by @realmcore_ (March 12, 2026)
**Package:** `@randomlabs/slate` v1.0.15 (npm)
**Config:** `slate.json` / `slate.jsonc` with 3-level merge (global → project → inline)

### Architecture (Deep Technical Analysis)

Slate is built on 8 interconnected concepts. At its core, it solves the **context window degradation problem**: past a threshold (the "dumb zone"), LLM performance degrades. Every existing approach (compaction, subagents, markdown plans, task decomposition, RLM) fails for different reasons. Slate's answer is **threads + episodes + thread weaving**.

```
┌─────────────────────────────────────────────────────────────────────┐
│  SLATE ARCHITECTURE (Deep Mechanics)                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  1. WORKING MEMORY & DUMB ZONE                                      │
│     Context has a usable zone (working memory) and a degraded       │
│     zone (dumb zone). Solution: NEVER exceed working memory.        │
│     ≠ compaction (lossy, unpredictable information loss)             │
│                                                                     │
│  2. THREADS (NOT subagents)                                         │
│     Each thread executes ONE action, then pauses and returns        │
│     control to orchestrator. Context is isolated per thread.        │
│     KEY: threads are one-shot, not persistent like subagents.       │
│     One action → episode → return to orchestrator.                  │
│                                                                     │
│  3. EPISODES (Compression at Completion Boundary)                   │
│     Compressed representation of a thread's execution.              │
│     Generated AT the natural completion boundary (not mid-stream).  │
│     The agent that ran the thread decides what's important.         │
│     ≠ compaction (lossy mid-stream) or subagent return (raw dump)   │
│                                                                     │
│  4. THREAD WEAVING (Implicit Adaptive Decomposition)                │
│     Orchestrator loop: dispatch threads → collect episodes →        │
│     synthesize → dispatch next threads. No explicit plan needed.    │
│     The orchestrator adapts based on episode results.               │
│     ≠ markdown planning (gets stale, 3 failure modes)               │
│     ≠ task decomposition (rigid, can't adapt)                       │
│                                                                     │
│  5. STRATEGY / TACTICS (AlphaZero Mapping)                          │
│     Strategy = open-ended planning, value network                   │
│     Tactics = learned action sequences, policy network              │
│     Software engineering is an "open-ended infinite game."          │
│     Orchestrator = strategist. Threads = tacticians.                │
│                                                                     │
│  6. KNOWLEDGE OVERHANG                                              │
│     Models have knowledge they can't access without scaffolding.    │
│     The gap is a systems problem, not capability.                   │
│     Episodes provide scaffolding that activates latent knowledge.   │
│                                                                     │
│  7. COMPOSABILITY                                                   │
│     Episodes as inputs to other threads (A's ep → B's input).      │
│     Cross-model composition: different models across threads,       │
│     episodes as clean handoff boundary.                             │
│     Parallel thread execution with episode synthesis.               │
│                                                                     │
│  8. OS FRAMING                                                      │
│     Orchestrator = kernel, Threads = processes,                     │
│     Episodes = process return values, Context = RAM.                │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Slate's Critique of Existing Approaches

| Approach | Failure Mode | Slate's Alternative |
|----------|-------------|---------------------|
| **Compaction** | Lossy, unpredictable information loss | Episode compression at completion boundary |
| **Subagents** | Context isolated, can't transfer info across boundaries | Threads (one-shot) + episodes (compressed handoff) |
| **Markdown Plans** | Underspecified, incomplete execution, agent forgets to update | No explicit plans — implicit adaptive decomposition |
| **Task Decomposition** | Rigid, can't adapt, low expressivity | Thread weaving — orchestrator adapts per round |
| **RLM** | Blind N-step execution, no intermediate feedback | Every thread produces episode, orchestrator reviews between |
| **Devin/Manus** | Context lost at compress boundary, strategize-delegate-compress | Episodes preserve key info, entity-linked persistence |

### Slate's 4 Model Slots

```
┌─────────────────────────────────────────────────────────────────────┐
│  MODEL SLOT ARCHITECTURE (slate.json config)                        │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  main       → Primary reasoning (expensive, most capable)           │
│  subagent   → Thread execution (can be cheaper/faster)              │
│  search     → Information retrieval (fast, cheap)                   │
│  reasoning  → Planning, review, critique (deep thinking)            │
│                                                                     │
│  Configured globally. Two primary agents: "build" and "plan".       │
│  Permission system: allow/ask/deny per action type.                 │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Slate's Architecture Comparison Table

| Dimension | ReAct | Plan | Task Trees | RLM | Devin | Claude Code | **Slate** |
|-----------|:-----:|:----:|:----------:|:---:|:-----:|:-----------:|:---------:|
| Planning | Implicit | Explicit | Explicit | None | Explicit | Implicit | Implicit |
| Decomposition | None | Manual | Static | None | Static | None | Implicit |
| Feedback | Per-step | End | End | None | Per-task | Per-step | Per-episode |
| Context isolation | None | None | Partial | None | Full | None | Per-thread |
| Compression | Compact | Compact | None | None | Compress | Compact | Episode |
| Parallelism | None | None | None | None | Multi-agent | None | Native |
| Adaptability | Low | Low | Low | Low | Medium | High | High |

### What Slate Does Better Than Nika

| Feature | Slate | Nika |
|---------|-------|------|
| Context management | Working memory awareness, episode compression | None — full context carried |
| Model routing | 4 model slots (main/subagent/search/reasoning) | Single provider per workflow |
| Episodic memory | Cross-session persistence (session files) | In-memory only |
| Strategy/tactics | Orchestrator dispatches threads, synthesizes episodes | Flat agent loop |
| Thread model | One-shot threads with episode compression | Persistent subagents |
| Adaptive planning | Implicit via thread weaving, no stale plans | Static DAG, no runtime adaptation |
| Long-running tasks | Hours to days | Single session |

### What Nika Does Better Than Slate

| Feature | Nika | Slate |
|---------|------|-------|
| Declarative workflows | YAML DAG, version-controlled, auditable | TypeScript, imperative, opaque |
| Knowledge graph | NovaNet (59 NodeClasses, 200+ locales) | None |
| Reproducibility | NDJSON traces, deterministic DAG replay | Thread-based, non-deterministic |
| Security | Shell-free exec, command blocklist, path validation | Not documented |
| Structured output | 4-layer validation (parse → validate → retry → repair) | Not documented |
| Multi-locale | 200+ locales via NovaNet knowledge atoms | English-focused |
| Observability | 34 event types, NDJSON traces, TUI with DAG view | Basic logging |
| Cost control | Token tracking per task, budget awareness | No token budgeting |
| Episode persistence | NovaNet = graph-queryable, entity-linked (future) | Session files only |

### Key Takeaways from Slate

1. **Thread model is the core innovation** — One-shot threads (not persistent subagents) with episode compression at natural completion boundaries solve context bloat without lossy compaction
2. **Thread weaving > explicit planning** — Implicit adaptive decomposition via orchestrator loop avoids all 3 failure modes of markdown planning
3. **4 model slots is the right abstraction** — Different cognitive tasks need different models (strategy vs tactics vs search vs reasoning)
4. **Episodes are composable** — They flow between threads as clean handoff boundaries, enabling cross-model composition
5. **Nika's DAG IS the orchestrator** — Nika doesn't need to BUILD Slate's architecture from scratch. The DAG engine is already the kernel. Tasks are already processes. We need to ADD episode compression, dynamic dispatch, and model slots.
6. **Nika goes beyond Slate** via NovaNet (persistent entity-linked memory), declarative YAML (auditable, version-controlled), and full observability (34 events + traces)

> **See doc 07 for the complete Slate → Nika integration strategy.**

---

## 2. Claude Code (Anthropic)

**Type:** CLI coding agent
**Traction:** Dominant in developer workflows (March 2026)

### Architecture

- Hook system for extensibility (PreToolUse, PostToolUse, etc.)
- Skill files for reusable capabilities
- MCP server integration
- Conversation-based context management
- No workflow definition format — conversational only

### Comparison

| Aspect | Claude Code | Nika |
|--------|------------|------|
| Interaction | Conversational | Workflow-defined |
| Reproducibility | Low (conversations) | High (YAML DAG + traces) |
| Extensibility | Hooks + skills | Verbs + MCP + includes |
| Multi-step | Ad-hoc via conversation | Structured via DAG |
| Multi-model | Claude only | 7 providers |
| Knowledge graph | None | NovaNet integration |

**Insight:** Claude Code is Nika's user, not competitor. Nika workflows are authored and invoked from Claude Code sessions. The relationship is symbiotic.

---

## 3. Codex (OpenAI)

**Type:** Cloud-based coding agent
**Architecture:** Sandboxed cloud environment, PR-oriented

### Comparison

| Aspect | Codex | Nika |
|--------|-------|------|
| Execution | Cloud sandbox (firecracker) | Local + cloud |
| Workflow | Single PR task | Multi-step DAG |
| Multi-model | OpenAI only | 7 providers |
| Trace | GitHub PR diffs | NDJSON events |
| Cost model | Per-task billing | Pay-per-API-call |

**Insight:** Codex focuses on code-change-as-output (PRs). Nika focuses on arbitrary AI workflow orchestration. Different target markets.

---

## 4. LangGraph (LangChain)

**Type:** Python framework for stateful agent graphs
**Traction:** Large Python ecosystem

### Comparison

| Aspect | LangGraph | Nika |
|--------|-----------|------|
| Language | Python | Rust (YAML DSL) |
| Graph model | StateGraph with conditional edges | DAG with 5 verbs |
| State | Shared state dict | DataStore + bindings |
| Checkpointing | Built-in persistence | Event log (replay) |
| Performance | Python (slow) | Rust + tokio (fast) |
| MCP | Via langchain-mcp | Native rmcp |
| Knowledge graph | Manual integration | NovaNet built-in |

**Insight:** LangGraph is more flexible (arbitrary Python) but slower, harder to reproduce, and lacks Nika's YAML-first philosophy. Nika's YAML workflows are version-controllable artifacts; LangGraph's Python graphs are code.

---

## 5. CrewAI

**Type:** Role-based multi-agent framework (Python)

### Comparison

| Aspect | CrewAI | Nika |
|--------|--------|------|
| Agent model | Role-based (researcher, writer, etc.) | Verb-based (infer, exec, agent) |
| Coordination | Sequential/hierarchical | DAG with parallel + for_each |
| Memory | Short/long-term/entity memory | DataStore (session only) |
| Tools | Custom tool definitions | MCP tools + 11 builtins |

**Insight:** CrewAI's role-based model is intuitive but less precise than Nika's verb-based approach. CrewAI's memory system (3 types) is more mature than Nika's (single DataStore).

---

## 6. SWE-bench Leaderboard (March 2026)

Current agent performance benchmarks:

| Agent | Score | Model |
|-------|-------|-------|
| GPT-5.4 Pro | 95% | OpenAI |
| Claude Opus 4.6 | 91% | Anthropic |
| Devin | ~85% | Multi-model |
| SWE-Agent | ~80% | Various |

**Insight for Nika:** The models themselves are converging at high capability. The differentiator is now **orchestration quality** — how effectively you chain, route, and compose LLM calls. This validates Nika's focus on workflow engineering over model capability.

---

## 7. Protocol Landscape

### MCP (Model Context Protocol)

- **Created by:** Anthropic
- **Focus:** Tool provision — exposing capabilities TO agents
- **Nika uses:** rmcp v0.16 as MCP client, NovaNet as MCP server
- **Role:** Intra-agent tooling (agent ↔ tools)

### A2A (Agent-to-Agent Protocol)

- **Created by:** Google (April 2025), donated to Linux Foundation (June 2025)
- **Focus:** Inter-agent coordination and task delegation
- **Key features:** Agent Cards at `/.well-known/agent.json`, JSON-RPC 2.0, SSE streaming, OAuth 2.0
- **Role:** Inter-agent communication (agent ↔ agent)

### Relationship

```
MCP and A2A are complementary:

  MCP:  Agent ←→ Tools (what an agent can DO)
  A2A:  Agent ←→ Agent (how agents COORDINATE)

  Nika today: MCP client (tools) + spawn_agent (basic coordination)
  Nika future: MCP client + A2A for agent discovery and delegation
```

**Insight:** If Nika wants to support multi-runtime agent coordination (e.g., a Nika agent delegating to an external agent running on LangGraph), A2A is the protocol to adopt.

---

## Competitive Positioning

```
┌─────────────────────────────────────────────────────────────────────┐
│  WHERE NIKA STANDS                                                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  UNIQUE STRENGTHS (no competitor has all of these)                  │
│  ├── YAML-first declarative workflows → reproducibility             │
│  ├── NovaNet knowledge graph integration → multi-locale content     │
│  ├── Rust performance + tokio concurrency → fast execution          │
│  ├── 5 semantic verbs → clear action taxonomy                       │
│  ├── Event sourcing + NDJSON traces → full observability            │
│  └── 7 LLM providers + native inference → provider independence    │
│                                                                     │
│  GAPS TO CLOSE                                                      │
│  ├── Context compression (Slate, Context-Folding)                   │
│  ├── Multi-model routing per task (THREAD, Slate)                   │
│  ├── Episodic memory (Slate, CrewAI)                                │
│  ├── Strategy/tactics separation (Slate, THREAD)                    │
│  ├── Code execution sandbox (CodeAct)                               │
│  └── Inter-agent protocol (A2A)                                     │
│                                                                     │
│  MOAT                                                               │
│  ├── NovaNet — no competitor has a curated knowledge graph          │
│  ├── YAML DSL — workflows as artifacts, not code                    │
│  └── Multi-locale — 200+ locales, no one else does this             │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### NovaNet's Unique Position

Research confirmed that NovaNet has **no direct competitors** in the curated-graph native-generation space. The closest are:
- **GraphRAG (Microsoft)** — auto-built from documents, not curated
- **Knowledge graph + LLM** research — academic, no product
- **Wikidata/DBpedia** — general purpose, not per-org content generation

NovaNet's combination of per-locale knowledge atoms (Expression, Pattern, CultureRef, Taboo) with entity-based content generation is unique in the market.
