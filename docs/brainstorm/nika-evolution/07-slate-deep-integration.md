# 07 — Slate Deep Integration Strategy

> Copying Slate's thread/episode architecture into Nika, then going beyond.
> Date: 2026-03-14

---

## Why This Document Exists

Slate (Random Labs) introduced an architecture — threads, episodes, thread weaving, strategy/tactics — that solves the fundamental problems of long-running AI agents. This document maps every Slate concept to Nika's existing architecture, identifies what needs to change, and designs how Nika goes **beyond** Slate by leveraging the NovaNet knowledge graph, YAML declarative workflows, and full observability.

**Guiding principle**: We are not building feature parity with Slate. We are taking Slate's **architectural insights** and implementing them in a way that is **declaratively superior** — auditable, reproducible, version-controlled, and knowledge-graph-powered.

---

## Slate's Core Architecture (Deep Technical Analysis)

### The Problem Slate Solves

LLM context windows are not uniformly useful. Performance degrades past a threshold (the "dumb zone"). Every existing approach to manage this fails:

```
┌─────────────────────────────────────────────────────────────────────┐
│  APPROACHES THAT FAIL (Slate's Critique)                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Compaction           Subagents             Markdown Plans          │
│  ─────────            ─────────             ──────────────          │
│  • Lossy              • Context isolated    • Underspecified        │
│  • Unpredictable      • Can't share info    • Incomplete execution  │
│  • Mid-stream loss    • No cross-boundary   • Agent forgets to      │
│                         transfer              update                │
│                                                                     │
│  Task Decomposition   RLM                   Devin/Manus            │
│  ──────────────────   ───                   ──────────             │
│  • Rigid structure    • Blind N-step        • Context lost at       │
│  • Can't adapt          execution             compress boundary     │
│  • Low expressivity   • No intermediate     • Strategize-delegate-  │
│                         feedback              compress cycle        │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Slate's Solution: 8 Interconnected Concepts

```
┌─────────────────────────────────────────────────────────────────────┐
│  SLATE ARCHITECTURE (Deep Mechanics)                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  1. WORKING MEMORY & DUMB ZONE                                      │
│     Context has a usable zone (working memory) and a degraded       │
│     zone (dumb zone). Solution: never exceed working memory.        │
│                                                                     │
│  2. THREADS (NOT subagents)                                         │
│     Each thread executes ONE action, then pauses and returns        │
│     control to orchestrator. Context isolated per thread.           │
│     ≠ subagents: threads are one-shot, not persistent.              │
│                                                                     │
│  3. EPISODES                                                        │
│     Compressed representation of a thread's execution.              │
│     Generated AT the completion boundary (not mid-stream).          │
│     Only important results retained. Natural compression.           │
│                                                                     │
│  4. THREAD WEAVING                                                  │
│     Orchestrator loop: dispatch threads → collect episodes →        │
│     synthesize → dispatch next threads. Implicit adaptive           │
│     decomposition. No explicit plan.                                │
│                                                                     │
│  5. STRATEGY / TACTICS (AlphaZero mapping)                          │
│     Strategy = open-ended planning (value network)                  │
│     Tactics = learned action sequences (policy network)             │
│     Orchestrator = strategist. Threads = tacticians.                │
│                                                                     │
│  6. KNOWLEDGE OVERHANG                                              │
│     Models have knowledge they can't access without scaffolding.    │
│     Episodes provide the scaffolding that activates latent          │
│     capabilities.                                                   │
│                                                                     │
│  7. COMPOSABILITY                                                   │
│     Episodes flow between threads (A's episode → B's input).        │
│     Cross-model composition via episodes as handoff boundary.       │
│     Parallel execution with episode synthesis.                      │
│                                                                     │
│  8. OS FRAMING                                                      │
│     Orchestrator = kernel, Threads = processes,                     │
│     Episodes = return values, Context = RAM.                        │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Slate's 4 Model Slots

```
┌─────────────────────────────────────────────────────────────────────┐
│  MODEL SLOT ARCHITECTURE                                            │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  main       → Primary reasoning (expensive, most capable)           │
│  subagent   → Thread execution (can be cheaper/faster)              │
│  search     → Information retrieval (fast, cheap)                   │
│  reasoning  → Planning, review, critique (deep thinking)            │
│                                                                     │
│  Configured globally in slate.json, one model per slot.             │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Slate's Architecture Comparison

| Dimension | ReAct | Plan | Task Trees | RLM | Devin | Claude Code | **Slate** |
|-----------|:-----:|:----:|:----------:|:---:|:-----:|:-----------:|:---------:|
| Planning | Implicit | Explicit | Explicit | None | Explicit | Implicit | **Implicit** |
| Decomposition | None | Manual | Static | None | Static | None | **Implicit** |
| Feedback | Per-step | End | End | None | Per-task | Per-step | **Per-episode** |
| Context isolation | None | None | Partial | None | Full | None | **Per-thread** |
| Compression | Compact | Compact | None | None | Compress | Compact | **Episode** |
| Parallelism | None | None | None | None | Multi-agent | None | **Native** |
| Adaptability | Low | Low | Low | Low | Medium | High | **High** |

---

## The Critical Realization: Nika IS Already an OS

```
┌─────────────────────────────────────────────────────────────────────┐
│  SLATE OS FRAMING → NIKA EXISTING ARCHITECTURE                      │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Slate Concept          Nika Already Has                            │
│  ──────────────         ────────────────                            │
│  Kernel (scheduler)  →  DAG scheduler (runner.rs)                   │
│  Processes (threads) →  Tasks (infer/exec/fetch/invoke)             │
│  Return values       →  TaskResult in DataStore                     │
│  Process isolation   →  Each task gets own execution context        │
│  RAM (context)       →  Agent context window                        │
│  File system         →  NovaNet knowledge graph                     │
│  IPC (inter-process) →  use: bindings (task A output → task B)      │
│  Fork/join           →  for_each + concurrency                      │
│  Process priority    →  flows: dependency ordering                  │
│                                                                     │
│  MISSING:                                                           │
│  ├── Episode compression (return value summarization)               │
│  ├── Dynamic process creation (strategy dispatching tactics)        │
│  ├── Memory budget (working memory management)                      │
│  └── Model routing (different CPU types for different processes)     │
│                                                                     │
│  NOT missing: The kernel itself. We upgrade it, not rebuild it.     │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Concept-by-Concept Mapping

### Complete Mapping Table

| # | Slate Concept | Nika Existing | Nika Needed | Nika Goes Beyond |
|---|---------------|---------------|-------------|------------------|
| 1 | Working Memory | No awareness | Context budget per task | Budget is declarative YAML |
| 2 | Dumb Zone | N/A | Working memory boundary | Token budget in events |
| 3 | Threads | Tasks in DAG (partial) | Dynamic dispatch by strategy | Tactic TEMPLATES in YAML |
| 4 | Episodes | TaskResult (raw) | Episode compression at boundary | NovaNet persistence |
| 5 | Thread Weaving | DAG execution (static) | Dynamic DAG + strategy loop | Real-time TUI visualization |
| 6 | Strategy/Tactics | Flat agent loop | `orchestration: strategy` | Declarative YAML strategies |
| 7 | Knowledge Overhang | NovaNet context + files | Episode-based scaffolding | 200+ locale knowledge atoms |
| 8 | Episodic Memory | In-memory DataStore | NovaNet AgentEpisode | Graph-queryable, entity-linked |
| 9 | 4 Model Slots | Single provider | `model_slots:` in YAML | Per-workflow slots |
| 10 | Composability | `use:` bindings | Episode-aware bindings | Structured output + episodes |
| 11 | Parallel Threads | `for_each` + concurrency | Strategy parallel dispatch | Token budget + cost tracking |
| 12 | Cross-model | Multi-provider (7) | Model slot per task | YAML-declared routing |
| 13 | OS Framing | DAG = kernel | Strategy = kernel upgrade | NovaNet = persistent storage |
| 14 | Permission system | Command blocklist + shell-free | Already better | 4-layer security model |
| 15 | build/plan agents | `agent:` verb | Strategy mode selection | Multiple strategy templates |
| 16 | Custom /commands | Skills via include: | Already exists | YAML skills merged via DAG |
| 17 | Server mode | N/A | Future consideration | MCP server mode |
| 18 | .env config | .nika/config.toml | Already exists | 3-level config merge |

---

## Design: The Nika Thread/Episode Architecture

### Layer 1: Model Slots (Foundation)

```yaml
# 4-slot architecture in YAML (per-workflow, not global)
schema: nika/workflow@0.13

model_slots:
  main:
    provider: anthropic
    model: claude-sonnet-4-6
    # For: primary content generation, complex reasoning

  tactical:
    provider: groq
    model: llama-3.3-70b-versatile
    # For: simple thread execution, tactical actions

  search:
    provider: deepseek
    model: deepseek-chat
    # For: research, search synthesis, information retrieval

  reasoning:
    provider: anthropic
    model: claude-sonnet-4-6
    extended_thinking: true
    thinking_budget: 16384
    # For: strategy, planning, review, critique

default_model_slot: main
```

Per-task reference:

```yaml
tasks:
  - id: analyze
    model_slot: reasoning
    infer: "Analyze the architecture..."

  - id: research
    model_slot: search
    infer: "Find information about..."

  - id: generate
    # Uses default_model_slot (main)
    infer: "Generate content..."
```

**Why per-workflow, not global**: Different workflows have different cost/quality tradeoffs. A content generation workflow uses expensive models. An audit workflow uses cheap models. Slate's global config can't express this.

### Layer 2: Episode Engine

```yaml
tasks:
  - id: research_trends
    model_slot: search
    infer: "Research QR code trends in 2026"
    episode:
      compress: true           # Generate episode summary after execution
      retain: [key_findings]   # What to keep from raw output
      max_tokens: 500          # Episode summary size limit
```

Episode data structure:

```rust
pub struct Episode {
    pub task_id: TaskId,
    pub summary: String,           // LLM-compressed summary
    pub key_findings: Vec<String>, // Extracted key points
    pub raw_output: Option<String>,// Original (debug only)
    pub model_used: String,        // Which model produced this
    pub tokens_spent: u64,         // Cost tracking
    pub confidence: f64,           // Self-assessed (0.0-1.0)
    pub artifacts: Vec<Artifact>,  // Files produced
}
```

Episode compression flow:

```
Task executes → Full output → LLM compresses → Episode stored
                                    │
                                    ├── Summary (always kept)
                                    ├── Key findings (configurable)
                                    └── Raw output (optional, debug)

Downstream tasks receive EPISODE, not raw output.
```

### Layer 3: Strategy Orchestration

```yaml
schema: nika/workflow@0.13
workflow: landing-page-generator

orchestration: strategy    # NEW: enables strategy/tactics mode

model_slots:
  reasoning: { provider: anthropic, model: claude-sonnet-4-6 }
  main: { provider: anthropic, model: claude-sonnet-4-6 }
  search: { provider: groq, model: llama-3.3-70b-versatile }

strategy:
  goal: "Generate a complete landing page for QR Code AI"
  model_slot: reasoning
  max_rounds: 10
  episode_budget: 10000    # Total token budget across all episodes

# Tactic templates — dispatched dynamically by strategy
tasks:
  - id: research
    model_slot: search
    infer: "Research: {{use.topic}}"
    episode: { compress: true, max_tokens: 300 }

  - id: write
    model_slot: main
    infer: "Write: {{use.section}} using context: {{use.context}}"
    episode: { compress: true, retain: [content] }

  - id: review
    model_slot: reasoning
    infer: "Review and critique: {{use.draft}}"
    episode: { compress: true, retain: [issues, suggestions] }
```

Strategy orchestration loop:

```
┌─────────────────────────────────────────────────────────────────────┐
│  STRATEGY ORCHESTRATION LOOP                                        │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Round 1:                                                           │
│  ┌──────────────┐                                                   │
│  │  Strategy LLM │──► "Dispatch: research(topic='QR trends')"       │
│  └──────┬───────┘                                                   │
│         │                                                           │
│         ▼                                                           │
│  ┌──────────────┐     ┌──────────┐                                  │
│  │ research task │────►│ Episode  │                                  │
│  └──────────────┘     └────┬─────┘                                  │
│                            │                                        │
│  Round 2:                  ▼                                        │
│  ┌──────────────┐     ┌──────────────────────────────────┐          │
│  │  Strategy LLM │◄───│ Episodes: [research_ep]          │          │
│  └──────┬───────┘     └──────────────────────────────────┘          │
│         │                                                           │
│         ▼                                                           │
│  "Dispatch: write(section='hero'), write(section='features')"       │
│         │                                                           │
│         ├──► ┌──────────┐     ┌──────────┐                          │
│         │    │ write hero│────►│ Episode  │                          │
│         │    └──────────┘     └──────────┘  (parallel)              │
│         └──► ┌──────────┐     ┌──────────┐                          │
│              │write feat.│────►│ Episode  │                          │
│              └──────────┘     └──────────┘                          │
│                                                                     │
│  Round 3:                                                           │
│  Strategy LLM ◄── [research_ep, hero_ep, features_ep]               │
│  "Dispatch: review(draft=hero_ep+features_ep)"                      │
│         │                                                           │
│         ▼                                                           │
│  ┌──────────────┐     ┌──────────┐                                  │
│  │ review task   │────►│ Episode  │                                  │
│  └──────────────┘     └──────────┘                                  │
│                                                                     │
│  Round 4:                                                           │
│  Strategy LLM ◄── [all episodes]                                    │
│  "DONE. Final output: [assembled page]"                             │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Layer 4: Context Budget Management

```yaml
tasks:
  - id: research
    model_slot: search
    context_budget: 4000     # Max tokens in this task's context
    infer: "Research..."
    episode:
      compress: true
      max_tokens: 300        # Episode must fit in 300 tokens
```

Rules:
- Each thread/task receives ONLY: its prompt + relevant episodes + NovaNet context
- Never raw history from other tasks
- Context budget enforced by the runtime (truncate/error if exceeded)
- Strategy orchestrator manages what episodes to include per thread

### Layer 5: NovaNet Episodic Memory

```yaml
tasks:
  - id: research
    infer: "Research QR code trends"
    episode:
      compress: true
      persist: novanet        # NEW: persist episode to NovaNet
      entity_link: qr-code    # Link to semantic entity
```

NovaNet schema additions:

```
AgentEpisode (NodeClass, org realm, output layer)
├── Properties:
│   ├── key: string (unique identifier)
│   ├── workflow: string (source workflow name)
│   ├── task_id: string (source task)
│   ├── summary: string (compressed episode)
│   ├── key_findings: string[] (extracted points)
│   ├── model_used: string
│   ├── tokens_spent: integer
│   ├── confidence: float
│   └── timestamp: datetime
├── Arcs:
│   ├── EPISODE_OF → Entity (semantic link)
│   └── FOR_LOCALE → Locale (if locale-specific)
```

Cross-session knowledge overhang:

```
Session 1: research(qr-code) → Episode → novanet_write(AgentEpisode)
Session 2: generate(qr-code) → novanet_search(AgentEpisode, entity=qr-code)
                                     ↓
                              Previous episodes surface as context
                              Knowledge overhang ACTIVATED
```

---

## Why Nika Goes Beyond Slate

```
┌─────────────────────────────────────────────────────────────────────┐
│  NIKA vs SLATE — WHERE NIKA IS SUPERIOR                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Dimension          │ Slate              │ Nika (after this)        │
│  ───────────────────┼────────────────────┼──────────────────────    │
│  Thread definition  │ TypeScript code    │ YAML tactic templates    │
│  Episode storage    │ In-memory session  │ DataStore + NovaNet      │
│  Cross-session      │ Session files      │ Knowledge graph (query)  │
│  Observability      │ Basic logging      │ 34 events + NDJSON       │
│  Cost control       │ None               │ Episode budget + tokens  │
│  Knowledge source   │ None               │ NovaNet atoms (200+loc)  │
│  Reproducibility    │ Non-deterministic  │ DAG traces + replay      │
│  Multi-locale       │ English only       │ 200+ locales             │
│  Model routing      │ 4 global slots     │ 4 per-workflow slots     │
│  Orchestration      │ Imperative code    │ Declarative YAML         │
│  DAG visualization  │ None               │ Real-time TUI            │
│  Structured output  │ Not documented     │ 4-layer validation       │
│  Security           │ Not documented     │ Shell-free + blocklist   │
│                                                                     │
│  UNIQUE TO NIKA (Slate has NO equivalent):                          │
│  ├── NovaNet knowledge graph (59 NodeClasses)                       │
│  ├── Entity-linked episodic memory                                  │
│  ├── Knowledge atoms (Expression, Pattern, CultureRef, Taboo)       │
│  ├── Denomination forms (text, title, abbrev, url)                  │
│  ├── NDJSON trace files with full event sourcing                    │
│  ├── 4-layer structured output (parse → validate → retry → repair)  │
│  ├── DAG visualization in TUI with real-time thread/episode view    │
│  └── Episode budget with per-workflow cost prediction               │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Revised Implementation Waves

### WAVE 1: Thread Foundation (v0.28)

```
┌─────────────────────────────────────────────────────────────────────┐
│  WAVE 1: MODEL SLOTS + EPISODE ENGINE                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  P-MODEL: 4-Slot Architecture                                       │
│  ├── model_slots: in workflow YAML                                  │
│  ├── Per-task model_slot: reference                                 │
│  ├── Provider routing via slot resolution                           │
│  ├── default_model_slot: for tasks without explicit slot            │
│  └── Schema version: @0.12                                          │
│                                                                     │
│  P-EPISODE: Episode Engine                                          │
│  ├── Episode struct (summary, key_findings, tokens, confidence)     │
│  ├── episode: config per task in YAML                               │
│  ├── LLM-based compression at task completion boundary              │
│  ├── Episode stored in DataStore                                    │
│  ├── use: bindings prefer episodes over raw output                  │
│  ├── New event: EpisodeCreated                                      │
│  └── Schema version: @0.12                                          │
│                                                                     │
│  Files changed:                                                     │
│  ├── NEW: src/ast/raw/model_slot.rs                                 │
│  ├── NEW: src/ast/analyzed/model_slot.rs                            │
│  ├── NEW: src/runtime/episode.rs                                    │
│  ├── NEW: src/runtime/episode_compress.rs                           │
│  ├── MOD: src/ast/raw/workflow.rs (model_slots field)               │
│  ├── MOD: src/ast/raw/task.rs (model_slot, episode fields)          │
│  ├── MOD: src/ast/analyzer/analyze.rs (slot validation)             │
│  ├── MOD: src/provider/rig.rs (from_slot constructor)               │
│  ├── MOD: src/runtime/executor.rs (slot routing, episode gen)       │
│  ├── MOD: src/store/mod.rs (Episode storage)                        │
│  ├── MOD: src/binding/resolve.rs (episode-aware resolution)         │
│  └── MOD: src/event/log.rs (EpisodeCreated event)                   │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### WAVE 2: Strategy Intelligence (v0.29)

```
┌─────────────────────────────────────────────────────────────────────┐
│  WAVE 2: STRATEGY ORCHESTRATION + CONTEXT BUDGET                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  P-STRATEGY: Strategy Orchestration                                 │
│  ├── orchestration: strategy | dag workflow mode                    │
│  ├── strategy: block (goal, model_slot, max_rounds, budget)         │
│  ├── Tasks become tactic templates (dispatched dynamically)         │
│  ├── Dynamic DAG mutation (add tasks at runtime)                    │
│  ├── Episode synthesis between rounds                               │
│  ├── Strategy LLM decides next tactics via structured output        │
│  └── Schema version: @0.13                                          │
│                                                                     │
│  P-CONTEXT: Context Budget Management                               │
│  ├── context_budget: per task (max tokens in context)               │
│  ├── Episode-only context passing (not raw history)                 │
│  ├── Strategy decides which episodes each thread receives           │
│  ├── Token budget tracking in events                                │
│  └── Working memory boundary enforcement                            │
│                                                                     │
│  Files changed:                                                     │
│  ├── NEW: src/runtime/strategy.rs (StrategyOrchestrator)            │
│  ├── NEW: src/runtime/tactic.rs (TacticTemplate, TacticInstance)    │
│  ├── NEW: src/dag/dynamic.rs (DynamicDag - mutable DAG)             │
│  ├── MOD: src/ast/raw/workflow.rs (orchestration, strategy fields)  │
│  ├── MOD: src/runtime/runner.rs (strategy mode routing)             │
│  ├── MOD: src/dag/mod.rs (mutable operations)                       │
│  └── MOD: src/tui/views/runner.rs (strategy visualization)          │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### WAVE 3: Persistent Memory (v0.30)

```
┌─────────────────────────────────────────────────────────────────────┐
│  WAVE 3: NOVANET EPISODIC MEMORY + INTROSPECTION                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  P-MEMORY: NovaNet Episodic Memory                                  │
│  ├── AgentEpisode NodeClass in NovaNet (org/output)                 │
│  ├── episode.persist: novanet in workflow YAML                      │
│  ├── entity_link: semantic entity association                       │
│  ├── Cross-session episode retrieval via novanet_search             │
│  ├── Auto-surfacing relevant episodes as context                    │
│  └── Knowledge overhang activation across sessions                  │
│                                                                     │
│  P-INTROSPECT: Runtime Introspection                                │
│  ├── nika:episodes — list episodes from current workflow            │
│  ├── nika:threads — list active/completed threads                   │
│  ├── nika:strategy_state — current strategy round and budget        │
│  └── nika:cost — token usage and cost report                        │
│                                                                     │
│  Files changed:                                                     │
│  ├── NEW: src/runtime/episodic_memory.rs                            │
│  ├── MOD: src/runtime/episode.rs (persist_to_novanet)               │
│  ├── MOD: src/mcp/client.rs (AgentEpisode read/write)               │
│  └── MOD: src/runtime/builtin/*.rs (new introspection tools)        │
│                                                                     │
│  NovaNet changes:                                                   │
│  ├── NEW: AgentEpisode NodeClass                                    │
│  ├── NEW: EPISODE_OF ArcClass (→ Entity)                            │
│  └── NEW: HAS_EPISODE ArcClass (Entity →)                           │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## What Old P3 (ConfidenceRouter) Becomes

The ConfidenceRouter is no longer a separate priority. Confidence is now a **property of episodes**:

```yaml
tasks:
  - id: generate
    infer: "Generate landing page hero section"
    episode:
      compress: true
      confidence_threshold: 0.8  # If episode confidence < 0.8, strategy escalates
```

Strategy escalation logic (built into StrategyOrchestrator):

```
Episode confidence < threshold
    → Strategy LLM receives: "Thread produced low-confidence result"
    → Strategy decides: retry with better model slot? get more context? skip?
    → Natural escalation without a separate ConfidenceRouter system
```

This is simpler AND more powerful: the strategy LLM has full context to decide how to handle low confidence, rather than a rigid router with fixed rules.

---

## Complete Example: Content Generation Workflow

```yaml
schema: nika/workflow@0.13
workflow: generate-landing-page

orchestration: strategy

model_slots:
  reasoning:
    provider: anthropic
    model: claude-sonnet-4-6
    extended_thinking: true
    thinking_budget: 16384
  main:
    provider: anthropic
    model: claude-sonnet-4-6
  search:
    provider: groq
    model: llama-3.3-70b-versatile
  tactical:
    provider: deepseek
    model: deepseek-chat

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "--manifest-path", "/path/to/novanet/Cargo.toml"]

strategy:
  goal: |
    Generate a complete French landing page for QR Code AI.
    Use NovaNet for entity context and locale knowledge.
    Research current trends, write sections, review quality.
  model_slot: reasoning
  max_rounds: 8
  episode_budget: 15000

tasks:
  - id: get_context
    model_slot: tactical
    invoke:
      tool: novanet_generate
      server: novanet
      params:
        focus_key: "homepage"
        locale: "fr-FR"
        mode: page
    episode:
      compress: true
      max_tokens: 500

  - id: research
    model_slot: search
    infer: "Research: {{use.topic}}"
    episode:
      compress: true
      max_tokens: 300
      retain: [key_findings]

  - id: write_section
    model_slot: main
    use:
      context: $get_context
    infer: |
      Write the {{use.section}} section for the landing page.
      Entity context: {{use.context}}
      Research: {{use.research_episodes}}
    episode:
      compress: true
      retain: [content]
      max_tokens: 800

  - id: review
    model_slot: reasoning
    infer: |
      Review the following draft sections for quality and coherence:
      {{use.drafts}}

      Check against QR Code AI brand guidelines and French locale conventions.
    episode:
      compress: true
      retain: [issues, suggestions, score]
      confidence_threshold: 0.85

  - id: persist_episodes
    model_slot: tactical
    invoke:
      tool: novanet_write
      server: novanet
      params:
        class: AgentEpisode
        key: "landing-page-{{date}}"
    episode:
      persist: novanet
      entity_link: qr-code-ai
```

---

## Addressing Slate's Architecture Comparison

For each dimension in Slate's comparison table, here's where Nika lands:

| Dimension | Slate | Nika (after integration) |
|-----------|:-----:|:------------------------:|
| Planning | Implicit (orchestrator) | Implicit (strategy LLM) |
| Decomposition | Implicit (thread weaving) | Implicit (tactic dispatch) |
| Feedback | Per-episode | Per-episode + events |
| Context isolation | Per-thread | Per-task (DAG native) |
| Compression | Episode | Episode + NovaNet persist |
| Parallelism | Native threads | for_each + strategy dispatch |
| Adaptability | High (TypeScript) | High (YAML + strategy) |
| **Reproducibility** | **Low** | **High (DAG traces)** |
| **Observability** | **Low** | **High (34 events)** |
| **Cost control** | **None** | **Episode budget** |
| **Knowledge graph** | **None** | **NovaNet** |
| **Multi-locale** | **None** | **200+ locales** |

---

## Summary

Nika copies Slate's core insights (threads, episodes, thread weaving, strategy/tactics, model slots) and implements them as declarative YAML constructs, backed by the NovaNet knowledge graph. The result is architecturally superior to Slate in reproducibility, observability, cost control, and knowledge integration.

```
THE FORMULA:
  Slate's thread/episode architecture
+ Nika's YAML declarative workflows
+ NovaNet's knowledge graph
+ Full event sourcing observability
+ Episode budget cost control
= The most advanced RLM implementation
```

```
THE GOLDEN RULE (extended):
  If it's about KNOWING things → NovaNet
  If it's about DOING things  → Nika
  If it's about CONNECTING    → MCP
  If it's about THINKING      → Episodes (strategy + model slots)
  If it's about REMEMBERING   → Episodes (NovaNet persistence)
```
