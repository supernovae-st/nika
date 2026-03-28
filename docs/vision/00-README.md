# Nika Vision

> "They built walls around intelligence. We compiled a door."

---

## Current State

| Metric | Value |
|--------|-------|
| **Version** | v0.49.3 |
| **Schema** | `nika/workflow@0.12` |
| **Tests** | 8457 |
| **Crates** | 10 workspace crates |
| **Verbs** | 5 (infer, exec, fetch, invoke, agent) |
| **Providers** | 9+ (7 cloud + native GGUF + custom endpoints) |
| **MCP aliases** | 100+ |

---

## The Problem

Six closed labs control frontier AI. Chips cost $6M per rack. LLM subscriptions run
$20-$200/month. The tools that promise to "democratize AI" charge $49/mo on their cloud,
under their terms. 57% of internet content is AI-generated, but most humans can't access
the tools.

Controlling access to LLMs is the new Newspeak. Not limiting vocabulary -- limiting cognitive
augmentation at scale.

## The Response

**Nika** is a single Rust binary that reads a YAML file and executes AI workflows. No Python.
No Docker. No cloud. No subscription. AGPL-licensed so it stays free forever.

5 verbs. 9+ providers. 10 crates. One binary.

## Guiding Documents

| Document | What it answers |
|----------|----------------|
| [The Manifesto](../MANIFESTO.md) | **WHY** -- The mission, the problem, the stakes |
| [v1 Master Plan](../plans/2026-03-28-v1-master-plan.md) | **WHAT'S NEXT** -- Complete roadmap to v1.0 |
| [Drums of Liberation](./brainstorm-drums-of-liberation.md) | **HOW WE COMMUNICATE** -- Three narrative layers, punchlines, audience messaging |
| [Design Philosophy](../content/learning/five-verbs-philosophy.md) | **WHY 5 VERBS** -- Constraint as power, declarative vs imperative |
| [Why Rust](../content/learning/the-rust-engineering-story.md) | **WHY RUST** -- Performance is freedom, the 10-crate architecture |
| [Competitive Positioning](../research/2026-03-23-competitive-positioning.md) | **VS WHO** -- Benchmarks, moats, messaging by competitor |
| [Real-World Use Cases](../research/2026-03-23-real-world-use-cases.md) | **FOR WHOM** -- 28 concrete use cases across 12 industries |
| [Competitive Landscape](../research/2026-03-23-competitive-landscape.md) | **THE MARKET** -- 30+ tools mapped, SWOT, positioning matrix |
| [Brand Bible](../../tools/nika/context/brand.md) | **HOW WE SOUND** -- Voice, taglines, punchlines, visual identity |

## Architecture: Brain + Body

```
NovaNet (Brain)         MCP Protocol          Nika (Body)
+-+ Knowledge Graph  <---------------------> +-+ YAML Workflows
+-+ 59 NodeClasses                           +-+ 5 Verbs
+-+ 200+ Locales                             +-+ DAG Execution
+-- MCP Tools                                +-- 9+ Providers
```

**The Golden Rule:** NovaNet KNOWS things. Nika DOES things. MCP CONNECTS them.
Zero Cypher in Nika. Zero workflow execution in NovaNet.

## Technical Research Index

These documents cover deep technical research for Nika's evolution:

| Doc | Topic | Status |
|-----|-------|--------|
| [01](./01-nika-state-of-the-art.md) | Current features inventory | Stale (pre-v0.34) |
| [03](./03-competitive-and-inspiration.md) | Competitive analysis + Slate deep integration | Stale (pre-v0.34) |
| [05](./05-evolution-roadmap.md) | 6 priorities in 3 waves | Stale -- superseded by v1 master plan |
| [08](./08-nika-reference.md) | Complete reference guide | Stale (pre-v0.34) |
| [09](./09-use-cases-cookbook.md) | Use cases with full YAML | Stale (pre-v0.34) |
| [10](./10-jarvis-tui-vision.md) | TUI design vision | Partially current |
| [12](./12-naming-and-design-decisions.md) | Naming system + agent presets | Updated v0.49 |
| [15](./15-ecosystem-coherence.md) | System topology | Stale (pre-v0.34) |
| [20](./20-agent-and-orchestration-research.md) | Agent architecture + memory design | Stale (pre-v0.34) |

## Research Documents (March 2026)

Recent deep-dive research informing the v1 roadmap:

| Document | Topic |
|----------|-------|
| [Hermes Agent Deep-Dive](../research/2026-03-27-hermes-agent-deep-dive.md) | NousResearch Hermes architecture, function calling, structured output |
| [Agent Framework Landscape](../research/2026-03-27-agent-framework-landscape.md) | Survey of agent frameworks: LangGraph, CrewAI, AutoGen, Semantic Kernel |
| [AI Orchestrator Landscape](../research/2026-03-27-ai-orchestrator-landscape.md) | Orchestrator survey: Temporal, Prefect, Windmill, Argo |
| [Mega Stack Brainstorm](../research/2026-03-27-mega-stack-brainstorm.md) | Full-stack AI architecture brainstorm for SuperNovae |

## Video & Content

| Document | Purpose |
|----------|---------|
| [Google Flow Prompts](./prompts/google-flow-ready-to-paste.md) | 8 ready-to-paste video clips (hacker documentary style) |
| [Video Brainstorm](./prompts/google-flow-nika-video-brainstorm.md) | Full creative brief with 3 style variants |
| [Archive: One Piece Prompts](./archive/) | Original anime-style image prompts (archived) |
