# Nika Vision

> "They built walls around intelligence. We compiled a door."

---

## The Problem

Six closed labs control frontier AI. Chips cost $6M per rack. LLM subscriptions run
$20-$200/month. The tools that promise to "democratize AI" charge $49/mo on their cloud,
under their terms. 57% of internet content is AI-generated, but most humans can't access
the tools.

Controlling access to LLMs is the new Newspeak. Not limiting vocabulary — limiting cognitive
augmentation at scale.

## The Response

**Nika** is a single Rust binary that reads a YAML file and executes AI workflows. No Python.
No Docker. No cloud. No subscription. AGPL-licensed so it stays free forever.

5 verbs. 22 providers. 451K lines. One binary.

## Guiding Documents

| Document | What it answers |
|----------|----------------|
| [The Manifesto](../MANIFESTO.md) | **WHY** — The mission, the problem, the stakes |
| [Drums of Liberation](./brainstorm-drums-of-liberation.md) | **HOW WE COMMUNICATE** — Three narrative layers, punchlines, audience messaging |
| [Design Philosophy](../content/learning/five-verbs-philosophy.md) | **WHY 5 VERBS** — Constraint as power, declarative vs imperative |
| [Why Rust](../content/learning/the-rust-engineering-story.md) | **WHY RUST** — Performance is freedom, the 10-crate architecture |
| [Competitive Positioning](../research/2026-03-23-competitive-positioning.md) | **VS WHO** — Benchmarks, moats, messaging by competitor |
| [Real-World Use Cases](../research/2026-03-23-real-world-use-cases.md) | **FOR WHOM** — 28 concrete use cases across 12 industries |
| [Competitive Landscape](../research/2026-03-23-competitive-landscape.md) | **THE MARKET** — 30+ tools mapped, SWOT, positioning matrix |
| [Brand Bible](../../tools/nika/context/brand.md) | **HOW WE SOUND** — Voice, taglines, punchlines, visual identity |

## Architecture: Brain + Body

```
NovaNet (Brain)         MCP Protocol          Nika (Body)
├── Knowledge Graph  <──────────────────────> ├── YAML Workflows
├── 59 NodeClasses                            ├── 5 Verbs
├── 200+ Locales                              ├── DAG Execution
└── MCP Tools                                 └── 22 Providers
```

**The Golden Rule:** NovaNet KNOWS things. Nika DOES things. MCP CONNECTS them.
Zero Cypher in Nika. Zero workflow execution in NovaNet.

## Technical Research Index

These documents cover deep technical research for Nika's evolution:

| Doc | Topic |
|-----|-------|
| [01](./01-nika-state-of-the-art.md) | Current features inventory |
| [03](./03-competitive-and-inspiration.md) | Competitive analysis + Slate deep integration |
| [05](./05-evolution-roadmap.md) | 6 priorities in 3 waves |
| [08](./08-nika-reference.md) | Complete reference guide |
| [09](./09-use-cases-cookbook.md) | Use cases with full YAML |
| [10](./10-jarvis-tui-vision.md) | TUI design vision |
| [12](./12-naming-and-design-decisions.md) | Naming system |
| [15](./15-ecosystem-coherence.md) | System topology |
| [20](./20-agent-and-orchestration-research.md) | Agent architecture + memory design |

## Video & Content

| Document | Purpose |
|----------|---------|
| [Google Flow Prompts](./prompts/google-flow-ready-to-paste.md) | 8 ready-to-paste video clips (hacker documentary style) |
| [Video Brainstorm](./prompts/google-flow-nika-video-brainstorm.md) | Full creative brief with 3 style variants |
| [Archive: One Piece Prompts](./archive/) | Original anime-style image prompts (archived) |
