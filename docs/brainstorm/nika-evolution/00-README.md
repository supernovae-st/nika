# Nika Evolution — Brainstorming

> Research and synthesis for Nika's next evolution phase.
> Date: 2026-03-14

---

## Documents

| # | Document | Content |
|---|----------|---------|
| 01 | [Current Features](./01-current-features.md) | Exhaustive Nika v0.27 + NovaNet v0.20 feature inventory |
| 02 | [Scientific Literature](./02-scientific-literature.md) | RLM, CodeAct, THREAD, Context-Folding, LLM Swarms |
| 03 | [Competitive Landscape](./03-competitive-landscape.md) | Slate (deep technical), Claude Code, Codex, LangGraph, CrewAI, protocols |
| 04 | [Nika x NovaNet Overlap](./04-nika-novanet-overlap.md) | Duplication risks, boundary rules, synergy opportunities |
| 05 | [Evolution Roadmap](./05-evolution-roadmap.md) | 6 priorities with Slate integration, 3 waves, full designs |
| 06 | [Research Synthesis](./06-research-synthesis-report.md) | Complete synthesis from 13 research agents |
| 07 | [Slate Deep Integration](./07-slate-deep-integration-strategy.md) | Thread/episode/weaving → Nika architecture mapping |

---

## TL;DR

### Where Nika Stands

Nika v0.27 is a mature YAML DAG workflow engine (371 files, 219K lines, 6,157 tests) with 5 semantic verbs, 7 LLM providers, MCP integration, spawn_agent recursion, 30+ transform operations, structured output, and a 4-view TUI. Combined with NovaNet's knowledge graph (59 NodeClasses, 8 MCP tools, 200+ locales), the ecosystem has no direct competitor.

### What the Literature Says

| Paper | Key Insight for Nika |
|-------|---------------------|
| **RLM** | Nika already has reference semantics (DataStore). Gap: dynamic DAG generation |
| **CodeAct** | Code execution is more expressive than JSON tools. Gap: no code sandbox |
| **THREAD** | Hierarchical decomposition works. Gap: no per-task model routing |
| **Context-Folding** | Sub-trajectory compression matters. Gap: no result folding |
| **LLM Swarms** | Hybrid DAG+LLM is optimal. Validation: Nika is already hybrid |

### What Competitors Do Differently

| Competitor | Key Differentiator | Nika's Response |
|-----------|-------------------|-----------------|
| **Slate** | Threads, episodes, thread weaving, 4 model slots | Integrate all 8 concepts (P-MODEL through P-MEMORY) |
| **LangGraph** | Python flexibility, checkpointing | Keep YAML-first, add DAG introspection (P-INTROSPECT) |
| **CrewAI** | 3-type memory system | Use NovaNet as memory backend (P-MEMORY) |
| **Claude Code** | Conversation-driven | Nika is Claude Code's workflow engine |

### The 6 Priorities

```
WAVE 1 — Foundation (v0.28, schema @0.12)
├── P-MODEL:   4-slot model routing (main/tactical/search/reasoning)
└── P-EPISODE: Episode compression at natural completion boundaries

WAVE 2 — Intelligence (v0.29, schema @0.13)
├── P-STRATEGY: Strategy orchestration with dynamic tactic dispatch
└── P-CONTEXT:  Context budget management (working memory awareness)

WAVE 3 — Memory (v0.30)
├── P-MEMORY:    NovaNet-backed episodic memory (cross-session, entity-linked)
└── P-INTROSPECT: Runtime introspection tools (6 new builtins)
```

### The Golden Rule

```
If it's about KNOWING things      → NovaNet
If it's about DOING things        → Nika
If it's about CONNECTING          → MCP
If it's about THINKING            → Episodes (strategy + model slots)
If it's about REMEMBERING         → Episodes (NovaNet persistence)
```

### Core Insight

```
Nika's DAG IS Slate's kernel.
Tasks ARE processes. TaskResult IS return values. DataStore IS RAM.
We don't BUILD Slate — we UPGRADE the kernel with 4 additions:
  1. Model slots (P-MODEL)
  2. Episode compression (P-EPISODE)
  3. Strategy orchestration (P-STRATEGY)
  4. Context budgeting (P-CONTEXT)
Then persist via NovaNet (P-MEMORY) and expose via tools (P-INTROSPECT).
```

---

## Sources

### Papers
- RLM (MIT 2025) — Recursive Language Models with REPL memory
- CodeAct (ICML 2024, arXiv:2402.01030) — Code actions for LLM agents
- THREAD (IJCAI 2025, arXiv:2405.17402) — Hierarchical agent decomposition
- Context-Folding (arXiv:2510.11967) — Branch/fold sub-trajectory compression
- LLM Swarms (arXiv:2506.14496) — Rule-based vs LLM swarm comparison
- Memory-R1 (2025) — RL-trained agent memory policies

### Products
- Slate by Random Labs (@realmcore_, v1.0.15, March 2026) — **Primary competitive reference**
- Claude Code (Anthropic)
- Codex (OpenAI)
- LangGraph (LangChain)
- CrewAI

### Protocols
- MCP (Anthropic) — Model Context Protocol
- A2A (Google → Linux Foundation) — Agent-to-Agent Protocol

### Codebase
- Nika v0.27.0 — `tools/nika/src/` (371 files, 219K lines)
- NovaNet v0.20.0 — 8 MCP tools, 59 NodeClasses, 159 ArcClasses

### Brainstorm Documents
- 7 documents (01-features through 07-slate-integration)
- 13 research agents deployed
- 12-step ultrathink sequential analysis (Slate → Nika concept mapping)
