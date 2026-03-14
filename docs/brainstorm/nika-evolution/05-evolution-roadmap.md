# 05 — Evolution Roadmap

> 6 priorities in 3 waves, centered on Slate's thread/episode architecture.
> Date: 2026-03-14

---

## The 6 Priorities

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  EVOLUTION PRIORITIES (Slate Integration)                                        │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  P-MODEL      4-slot model architecture (main/tactical/search/reasoning)        │
│  P-EPISODE    Episode engine (compression at completion boundary)               │
│  P-STRATEGY   Strategy orchestration (dynamic tactic dispatch)                  │
│  P-CONTEXT    Context budget management (working memory awareness)              │
│  P-MEMORY     NovaNet episodic memory (cross-session, entity-linked)            │
│  P-INTROSPECT Runtime introspection tools (episodes, threads, cost)             │
│                                                                                 │
│  OLD P3 (ConfidenceRouter) → ABSORBED into P-EPISODE (confidence is an          │
│  episode property; strategy LLM handles escalation naturally)                   │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## P-MODEL: 4-Slot Model Architecture

### What It Is

Per-workflow model slot definitions that route different cognitive tasks to different providers/models. Slate's 4 global slots, made declarative and per-workflow.

### Current State

```
TODAY:
  ├── Single provider: per workflow (all tasks use same model)
  ├── Per-task provider/model override: NOT supported
  └── RigProvider::auto() detects from env vars

AFTER:
  ├── model_slots: block in YAML (4 named slots)
  ├── Per-task model_slot: reference
  ├── default_model_slot: fallback
  └── Different models for strategy vs tactics vs search vs reasoning
```

### Proposed Design

```yaml
schema: nika/workflow@0.12

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

tasks:
  - id: plan
    model_slot: reasoning         # Uses expensive deep-thinking model
    infer: "Create a content plan for {{use.entity}}"

  - id: generate_pages
    model_slot: tactical          # Uses cheap fast model
    for_each: $pages
    infer: "Generate page {{use.item}}"

  - id: review
    model_slot: reasoning         # Back to expensive model for review
    infer: "Review all generated pages for quality"
```

**Why per-workflow, not global**: Different workflows have different cost/quality tradeoffs. A content generation workflow uses expensive models. An audit workflow uses cheap models. Slate's global config can't express this.

### Implementation

| Change | Location | Effort |
|--------|----------|--------|
| `ModelSlot` struct (provider, model, params) | `ast/raw/model_slot.rs` (NEW) | Low |
| `model_slots:` field in `RawWorkflow` | `ast/raw/workflow.rs` | Low |
| `model_slot:` field in `RawTask` | `ast/raw/task.rs` | Low |
| Analyze slot validation | `ast/analyzer/analyze.rs` | Low |
| `from_slot()` constructor on RigProvider | `provider/rig.rs` | Medium |
| Resolve slot per task in executor | `runtime/executor.rs` | Medium |
| Schema bump to @0.12 | `schemas/nika-workflow.schema.json` | Low |
| Feature gate for @0.12 | `ast/analyzer/feature_gate.rs` | Low |

### Source Inspiration

- **Slate:** 4 model slots (main/subagent/search/reasoning) — global config
- **THREAD:** Resource-aware model allocation per subtask
- **SWE-bench:** Different models excel at different cognitive tasks

### Risk Assessment

- **Complexity:** LOW-MEDIUM — mostly AST + provider plumbing
- **Breaking changes:** None (new optional fields, old workflows unchanged)
- **Cost impact:** Significant savings by routing simple tasks to cheaper models

---

## P-EPISODE: Episode Engine

### What It Is

Compressed representation of a task's execution, generated at the natural completion boundary (not mid-stream). This is Slate's core innovation: tasks produce **episodes**, not raw output. Downstream tasks receive episodes, keeping context within working memory.

### Current State

```
TODAY:
  ├── TaskResult: raw output stored in DataStore
  ├── use: bindings pass raw output between tasks
  ├── No compression — full output carried forward
  └── Context grows linearly with pipeline depth

AFTER:
  ├── Episode struct: summary + key_findings + confidence + tokens
  ├── LLM compression at task completion boundary
  ├── use: bindings prefer episodes over raw output
  ├── Context stays within working memory budget
  └── EpisodeCreated event for observability
```

### Proposed Design

```yaml
tasks:
  - id: research_trends
    model_slot: search
    infer: "Research QR code trends in 2026"
    episode:
      compress: true           # Generate episode summary after execution
      retain: [key_findings]   # What to keep from raw output
      max_tokens: 500          # Episode summary size limit
      confidence_threshold: 0.8 # Strategy can escalate if below threshold
```

Episode data structure:

```rust
pub struct Episode {
    pub task_id: TaskId,
    pub summary: String,           // LLM-compressed summary
    pub key_findings: Vec<String>, // Extracted key points
    pub raw_output: Option<String>,// Original (debug only, not passed downstream)
    pub model_used: String,        // Which model produced this
    pub tokens_spent: u64,         // Cost tracking
    pub confidence: f64,           // Self-assessed (0.0-1.0)
    pub artifacts: Vec<Artifact>,  // Files produced
}
```

Episode compression flow:

```
Task executes → Full output → LLM compresses → Episode stored in DataStore
                                    │
                                    ├── Summary (always kept)
                                    ├── Key findings (configurable via retain:)
                                    ├── Confidence score (self-assessed)
                                    └── Raw output (optional, debug mode only)

Downstream tasks receive EPISODE, not raw output.
Context stays within working memory.
```

### How Confidence Replaces Old P3 (ConfidenceRouter)

The old ConfidenceRouter was a rigid tiered escalation system. Episodes make this unnecessary:

```
Old approach (ConfidenceRouter):
  Task → Tier 1 model → confidence < threshold? → Tier 2 model
  Rigid. Fixed rules. No context.

New approach (Episode confidence):
  Task → Episode (with confidence) → Strategy LLM sees low confidence
  → Strategy DECIDES: retry with better model? get more context? skip?
  Adaptive. Full context. Natural escalation.
```

### Implementation

| Change | Location | Effort |
|--------|----------|--------|
| `Episode` struct | `runtime/episode.rs` (NEW) | Medium |
| `EpisodeCompressor` (LLM-based) | `runtime/episode_compress.rs` (NEW) | Medium |
| `episode:` field in `RawTask` | `ast/raw/task.rs` | Low |
| Episode generation after task completion | `runtime/executor.rs` | Medium |
| Episode storage in DataStore | `store/mod.rs` | Low |
| Episode-aware binding resolution | `binding/resolve.rs` | Medium |
| `EpisodeCreated` event kind | `event/log.rs` | Low |

### Source Inspiration

- **Slate:** Episodes — the core innovation. Compressed at natural completion boundary.
- **Context-Folding:** Sub-trajectory compression for reduced context.
- **Memory-R1:** RL-trained memory policies (confidence scoring).

### Risk Assessment

- **Complexity:** MEDIUM — new module, LLM compression logic, binding changes
- **Quality:** Episode compression quality depends on LLM summarization ability
- **Cost:** Compression adds one extra LLM call per task (use cheap model)
- **Value:** HIGH — solves context degradation, enables strategy mode

---

## P-STRATEGY: Strategy Orchestration

### What It Is

A new workflow execution mode where a **strategy LLM** dynamically dispatches **tactic tasks** based on the goal and accumulated episodes. This is Slate's thread weaving: implicit adaptive decomposition via an orchestrator loop.

### Current State

```
TODAY:
  ├── Static DAG execution (all tasks and flows known at parse time)
  ├── decompose: modifier for graph-based expansion
  ├── spawn_agent for recursive delegation
  └── No dynamic task creation at runtime

AFTER:
  ├── orchestration: strategy mode (new workflow execution path)
  ├── Strategy LLM decides which tasks to dispatch per round
  ├── Tasks become tactic TEMPLATES (dispatched dynamically)
  ├── Dynamic DAG mutation (add tasks at runtime)
  ├── Episode synthesis between rounds
  └── Strategy LLM decides when to stop
```

### Proposed Design

```yaml
schema: nika/workflow@0.13
workflow: landing-page-generator

orchestration: strategy    # NEW: enables strategy/tactics mode

model_slots:
  reasoning: { provider: anthropic, model: claude-sonnet-4-6, extended_thinking: true }
  main: { provider: anthropic, model: claude-sonnet-4-6 }
  search: { provider: groq, model: llama-3.3-70b-versatile }
  tactical: { provider: deepseek, model: deepseek-chat }

strategy:
  goal: "Generate a complete French landing page for QR Code AI"
  model_slot: reasoning
  max_rounds: 10
  episode_budget: 15000    # Total token budget across all episodes

# Tactic templates — dispatched dynamically by strategy
tasks:
  - id: research
    model_slot: search
    infer: "Research: {{use.topic}}"
    episode: { compress: true, max_tokens: 300 }

  - id: write_section
    model_slot: main
    infer: "Write: {{use.section}} using context: {{use.context}}"
    episode: { compress: true, retain: [content], max_tokens: 800 }

  - id: review
    model_slot: reasoning
    infer: "Review and critique: {{use.draft}}"
    episode: { compress: true, retain: [issues, suggestions] }
```

### Strategy Orchestration Loop

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  STRATEGY ORCHESTRATION LOOP                                                    │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Round 1:                                                                       │
│  ┌──────────────┐                                                               │
│  │  Strategy LLM │──► "Dispatch: research(topic='QR trends')"                   │
│  └──────┬───────┘                                                               │
│         │                                                                       │
│         ▼                                                                       │
│  ┌──────────────┐     ┌──────────┐                                              │
│  │ research task │────►│ Episode  │                                              │
│  └──────────────┘     └────┬─────┘                                              │
│                            │                                                    │
│  Round 2:                  ▼                                                    │
│  ┌──────────────┐     ┌──────────────────────────────────┐                      │
│  │  Strategy LLM │◄───│ Episodes: [research_ep]          │                      │
│  └──────┬───────┘     └──────────────────────────────────┘                      │
│         │                                                                       │
│         ▼                                                                       │
│  "Dispatch: write_section(section='hero'), write_section(section='features')"   │
│         │                                                                       │
│         ├──► ┌──────────────┐     ┌──────────┐                                  │
│         │    │ write (hero)  │────►│ Episode  │  (parallel)                      │
│         │    └──────────────┘     └──────────┘                                  │
│         └──► ┌──────────────┐     ┌──────────┐                                  │
│              │ write (feat.) │────►│ Episode  │                                  │
│              └──────────────┘     └──────────┘                                  │
│                                                                                 │
│  Round 3:                                                                       │
│  Strategy LLM ◄── [research_ep, hero_ep, features_ep]                           │
│  "Dispatch: review(draft=hero_ep+features_ep)"                                  │
│                                                                                 │
│  Round N:                                                                       │
│  Strategy LLM: "DONE. Final output: [assembled page]"                           │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Implementation

| Change | Location | Effort |
|--------|----------|--------|
| `StrategyOrchestrator` struct | `runtime/strategy.rs` (NEW) | High |
| `TacticTemplate`, `TacticInstance` | `runtime/tactic.rs` (NEW) | Medium |
| `DynamicDag` (mutable DAG) | `dag/dynamic.rs` (NEW) | High |
| `orchestration:` + `strategy:` fields | `ast/raw/workflow.rs` | Low |
| Strategy mode routing in runner | `runtime/runner.rs` | Medium |
| Mutable DAG operations | `dag/mod.rs` | Medium |
| Strategy visualization in TUI | `tui/views/runner.rs` | Medium |
| Schema bump to @0.13 | `schemas/nika-workflow.schema.json` | Low |

### Source Inspiration

- **Slate:** Thread weaving — implicit adaptive decomposition via orchestrator loop
- **Slate:** Strategy/tactics separation (AlphaZero mapping)
- **THREAD:** Hierarchical decomposition with resource-aware model selection
- **RLM:** Recursive sub-LM calls with external working memory

### Risk Assessment

- **Complexity:** HIGH — touches DAG, runtime, AST, provider, TUI
- **Security:** Must validate dynamically generated task parameters
- **Scope creep:** Start with sequential dispatch, add parallel later
- **Dependencies:** Requires P-MODEL + P-EPISODE (Wave 1) as foundation

---

## P-CONTEXT: Context Budget Management

### What It Is

Working memory awareness at the runtime level. Each task declares how much context it can consume (context budget), and the runtime enforces this by passing only episode summaries and relevant context — never raw history from other tasks.

### Current State

```
TODAY:
  ├── No context budgeting — full output passed between tasks
  ├── No working memory awareness
  ├── Context grows linearly with pipeline depth
  └── Agent context window filled until degradation

AFTER:
  ├── context_budget: per task in YAML
  ├── Episode-only passing (not raw history)
  ├── Strategy decides which episodes each thread receives
  ├── Token budget tracking in events
  └── Working memory boundary enforcement
```

### Proposed Design

```yaml
tasks:
  - id: research
    model_slot: search
    context_budget: 4000     # Max tokens in this task's context
    infer: "Research QR code trends"
    episode:
      compress: true
      max_tokens: 300        # Episode must fit in 300 tokens

  - id: generate
    model_slot: main
    context_budget: 8000     # Larger budget for generation
    use:
      trends: $research      # Receives episode, not raw output
    infer: "Generate landing page based on: {{use.trends}}"
```

Rules:
1. Each task receives ONLY: its prompt + relevant episodes + NovaNet context
2. Never raw history from other tasks
3. `context_budget` enforced by the runtime (truncate/warn if exceeded)
4. Strategy orchestrator manages which episodes to include per thread
5. Token budget tracked in events for observability

### Implementation

| Change | Location | Effort |
|--------|----------|--------|
| `context_budget:` field in `RawTask` | `ast/raw/task.rs` | Low |
| Budget enforcement in executor | `runtime/executor.rs` | Medium |
| Token counting utilities | `runtime/context_budget.rs` (NEW) | Medium |
| Budget tracking in events | `event/log.rs` | Low |
| Strategy episode selection | `runtime/strategy.rs` | Medium |

### Source Inspiration

- **Slate:** Working memory / dumb zone — never exceed usable context
- **Context-Folding:** Sub-trajectory compression to stay within budget
- **RLM:** Token-aware external memory management

### Risk Assessment

- **Complexity:** MEDIUM — new field, budget logic, token counting
- **Accuracy:** Token counting is approximate (tokenizer-dependent)
- **Dependencies:** Benefits greatly from P-EPISODE (episodes are budget-friendly)
- **Value:** HIGH — prevents context degradation, the root cause of agent failure

---

## P-MEMORY: NovaNet Episodic Memory

### What It Is

Persistent episodes stored in NovaNet's knowledge graph, linked to semantic entities. Episodes survive across sessions, enabling cross-session learning, knowledge overhang activation, and experience accumulation.

### Current State

```
TODAY:
  ├── DataStore (DashMap) — in-memory, dies with process
  ├── Session files (.nika/sessions/) — editor state only
  ├── NDJSON traces — raw events, not queryable
  └── NovaNet — has durable storage but no agent memory model

AFTER:
  ├── AgentEpisode NodeClass in NovaNet
  ├── episode.persist: novanet in workflow YAML
  ├── entity_link: semantic entity association
  ├── Cross-session episode retrieval via novanet_search
  ├── Auto-surfacing relevant episodes as context
  └── Knowledge overhang activation across sessions
```

### Proposed Design

```yaml
tasks:
  - id: research
    infer: "Research QR code trends"
    episode:
      compress: true
      persist: novanet        # Store episode in NovaNet
      entity_link: qr-code    # Link to semantic entity

  - id: generate
    use:
      past_experience: $recall_episodes  # Retrieved from NovaNet
    infer: |
      Generate a QR code landing page.
      Previous experience: {{use.past_experience}}
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
│   ├── FOR_LOCALE → Locale (if locale-specific)
│   ├── SIMILAR_TO → AgentEpisode (similarity arcs)
│   └── PRECEDED_BY → AgentEpisode (temporal chain)
```

Cross-session knowledge overhang:

```
Session 1: research(qr-code) → Episode → novanet_write(AgentEpisode)
Session 2: generate(qr-code) → novanet_search(AgentEpisode, entity=qr-code)
                                     ↓
                              Previous episodes surface as context
                              Knowledge overhang ACTIVATED
```

### Implementation Phases

1. **Episode data model** — New NodeClass in NovaNet schema (ADR required)
2. **Write episodes** — Nika calls `novanet_write` after workflow completion
3. **Recall episodes** — `novanet_search` for similar past runs
4. **Inject in context** — Recalled episodes added to agent system prompt
5. **Auto-learning** — Pattern extraction from success/failure episodes

### Implementation

| Change | Location | Effort |
|--------|----------|--------|
| `EpisodicMemoryManager` struct | `runtime/episodic_memory.rs` (NEW) | Medium |
| `persist_to_novanet()` on Episode | `runtime/episode.rs` | Medium |
| AgentEpisode read/write via MCP | `mcp/client.rs` | Medium |
| NovaNet AgentEpisode NodeClass | NovaNet schema YAML | Medium |
| NovaNet EPISODE_OF ArcClass | NovaNet schema YAML | Low |
| NovaNet HAS_EPISODE ArcClass | NovaNet schema YAML | Low |
| ADR for episode schema | `dx/adr/novanet/` | Low |

### Source Inspiration

- **Slate:** Episodic memory, cross-session learning via session files
- **Memory-R1:** RL-trained memory policies for what to remember
- **CrewAI:** Short/long-term/entity memory (3-type model)

### Risk Assessment

- **Complexity:** HIGH — requires NovaNet schema changes + new Nika module
- **Cross-project:** Needs coordinated NovaNet + Nika development
- **Privacy:** Episodes may contain sensitive data — need retention policies
- **Value:** VERY HIGH — learning from experience is the biggest differentiator

---

## P-INTROSPECT: Runtime Introspection Tools

### What It Is

New builtin tools that let agents query the current workflow's runtime state: episodes, threads, strategy state, and cost. The DAG becomes a first-class data structure that agents can reason about.

### Current State

```
TODAY:
  ├── AnalyzedWorkflow has TaskTable (O(1) lookup)
  ├── DAG validation checks cycles and dependencies
  ├── Event log captures execution order
  ├── TUI displays DAG visually (DAG Preview panel)
  └── Agents CANNOT query "what episodes exist?" or "what's the cost so far?"

AFTER:
  ├── nika:episodes — list episodes from current workflow
  ├── nika:threads — list active/completed threads
  ├── nika:strategy_state — current round, budget, accumulated episodes
  ├── nika:cost — token usage and cost report
  ├── nika:dag_info — predecessors, successors, critical path
  └── nika:task_status — status of specific tasks
```

### Proposed Design

```yaml
tasks:
  - id: adaptive_step
    agent:
      prompt: "Generate content, adapting based on what came before"
      tools:
        - nika:episodes       # Query accumulated episodes
        - nika:cost           # Check remaining budget
        - nika:strategy_state # Know current round
        - nika:dag_info       # Understand DAG structure
```

### New Builtin Tools

| Tool | Description | Returns |
|------|-------------|---------|
| `nika:episodes` | List episodes from current workflow | `[{task_id, summary, confidence, tokens}]` |
| `nika:threads` | List active/completed threads | `[{task_id, status, model_slot}]` |
| `nika:strategy_state` | Current strategy round and budget | `{round, max_rounds, budget_used, budget_total}` |
| `nika:cost` | Token usage and cost report | `{total_tokens, total_cost, per_model}` |
| `nika:dag_info` | Query DAG structure | `{predecessors, successors, critical_path}` |
| `nika:task_status` | Check status of specific tasks | `{task_id, status, episode}` |

### Implementation

| Change | Location | Effort |
|--------|----------|--------|
| 6 new builtin tools | `runtime/builtin/*.rs` | Medium |
| DAG query API | `dag/mod.rs` | Medium |
| Episode registry accessible to tools | `runtime/executor.rs` | Low |
| Strategy state accessible to tools | `runtime/strategy.rs` | Low |
| Cost tracking aggregation | `event/log.rs` | Low |

### Source Inspiration

- **RLM:** Self-referential computation (model reasons about its own process)
- **Slate:** DAG introspection for strategy adjustment
- **THREAD:** Resource allocation based on task graph structure

### Risk Assessment

- **Complexity:** MEDIUM — mostly new builtin tools + query APIs
- **Security:** Read-only access (no modification via tools)
- **Value:** MEDIUM — enables self-aware agents and adaptive strategies

---

## Priority Matrix

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  IMPACT vs EFFORT                                                               │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  HIGH     │  P-MEMORY        │  P-STRATEGY      │                               │
│  IMPACT   │  (NovaNet+Nika)  │  (dynamic DAG +  │                               │
│           │                  │   orchestration)  │                               │
│           ├──────────────────┼──────────────────┤                               │
│           │  P-MODEL         │  P-CONTEXT        │                               │
│           │  (4-slot)        │  (budget mgmt)    │                               │
│           │                  │                   │                               │
│           │  P-EPISODE       │                   │                               │
│           │  (compression)   │                   │                               │
│           ├──────────────────┼──────────────────┤                               │
│  MEDIUM   │  P-INTROSPECT    │                   │                               │
│  IMPACT   │  (runtime tools) │                   │                               │
│           │                  │                   │                               │
│           ├──────────────────┼──────────────────┤                               │
│           │  LOW EFFORT      │  HIGH EFFORT      │                               │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Recommended Sequence

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  EXECUTION ORDER (3 Waves)                                                      │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  WAVE 1: Thread Foundation (v0.28)                                              │
│  ├── P-MODEL: 4-slot model architecture                                         │
│  │   └── model_slots: in YAML, per-task model_slot: reference                   │
│  └── P-EPISODE: Episode engine                                                  │
│      └── Episode struct, LLM compression, episode-aware bindings                │
│                                                                                 │
│  WAVE 2: Strategy Intelligence (v0.29)                                          │
│  ├── P-STRATEGY: Strategy orchestration                                         │
│  │   └── orchestration: strategy mode, dynamic tactic dispatch                  │
│  └── P-CONTEXT: Context budget management                                       │
│      └── context_budget: per task, working memory enforcement                   │
│                                                                                 │
│  WAVE 3: Persistent Memory (v0.30)                                              │
│  ├── P-MEMORY: NovaNet episodic memory                                          │
│  │   └── AgentEpisode NodeClass, entity-linked, cross-session                   │
│  └── P-INTROSPECT: Runtime introspection tools                                  │
│      └── nika:episodes, nika:threads, nika:cost, nika:dag_info                  │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### Rationale

1. **P-MODEL first** because it's low-effort, high-value, and prerequisite for everything else (strategy needs model slots to route tactics)
2. **P-EPISODE with P-MODEL** because episodes are the core primitive — everything downstream depends on compressed task results
3. **P-STRATEGY after Wave 1** because strategy orchestration REQUIRES both model slots (for routing) and episodes (for inter-round communication)
4. **P-CONTEXT with P-STRATEGY** because context budgeting makes strategy mode practical (without budgets, strategy rounds accumulate unbounded context)
5. **P-MEMORY last** because it requires cross-project NovaNet schema changes (ADR, NodeClass, ArcClasses) and builds on episodes being stable
6. **P-INTROSPECT with P-MEMORY** because introspection tools are simple once the runtime state (episodes, strategy, cost) is already tracked

---

## Cross-Cutting Concerns

### Context Compression (from literature)

Woven into P-EPISODE and P-CONTEXT, not a separate priority:
- **P-EPISODE:** Sub-DAG results auto-compressed at completion boundary (Context-Folding paper)
- **P-CONTEXT:** Working memory budget prevents degradation (Slate's dumb zone concept)

### Old P3 (ConfidenceRouter) — Absorbed

Confidence is now an episode property. The strategy LLM naturally handles escalation:

```
Episode confidence < threshold
    → Strategy LLM receives: "Thread produced low-confidence result"
    → Strategy decides: retry with better model slot? get more context? skip?
    → Natural escalation — no separate router needed
```

This is simpler AND more powerful: the strategy LLM has full context to decide how to handle low confidence, rather than a rigid router with fixed rules.

### A2A Protocol (from competitive analysis)

Future consideration beyond these 6 priorities. If Nika agents need to coordinate with external runtimes (LangGraph, Slate), A2A is the protocol. Not urgent for QR Code AI target.

### Code Execution Sandbox (from CodeAct)

Potential future priority. A `code:` verb with Pyodide/Deno sandbox would give agents CodeAct-level expressivity. Lower priority because Nika's transform engine + exec: verb cover most needs.

---

## Version Mapping

| Priority | Version | Schema Bump | Dependencies |
|----------|---------|-------------|--------------|
| P-MODEL | v0.28.0 | @0.12 | None |
| P-EPISODE | v0.28.0 | @0.12 | None (ships with P-MODEL) |
| P-STRATEGY | v0.29.0 | @0.13 | P-MODEL + P-EPISODE |
| P-CONTEXT | v0.29.0 | @0.13 (extends) | P-EPISODE |
| P-MEMORY | v0.30.0 | @0.13 + NovaNet | P-EPISODE |
| P-INTROSPECT | v0.30.0 | (new builtin tools) | P-EPISODE + P-STRATEGY |

---

## File Change Summary

### New Files (8)

| File | Priority | Purpose |
|------|----------|---------|
| `src/ast/raw/model_slot.rs` | P-MODEL | ModelSlot struct |
| `src/ast/analyzed/model_slot.rs` | P-MODEL | Analyzed slot with validation |
| `src/runtime/episode.rs` | P-EPISODE | Episode struct + lifecycle |
| `src/runtime/episode_compress.rs` | P-EPISODE | LLM-based compression |
| `src/runtime/strategy.rs` | P-STRATEGY | StrategyOrchestrator |
| `src/runtime/tactic.rs` | P-STRATEGY | TacticTemplate, TacticInstance |
| `src/dag/dynamic.rs` | P-STRATEGY | DynamicDag for runtime mutation |
| `src/runtime/episodic_memory.rs` | P-MEMORY | EpisodicMemoryManager |

### Modified Files (11)

| File | Priorities | Changes |
|------|-----------|---------|
| `src/ast/raw/workflow.rs` | P-MODEL, P-STRATEGY | model_slots, orchestration, strategy fields |
| `src/ast/raw/task.rs` | P-MODEL, P-EPISODE, P-CONTEXT | model_slot, episode, context_budget fields |
| `src/ast/analyzer/analyze.rs` | P-MODEL, P-EPISODE | Slot validation, episode config validation |
| `src/provider/rig.rs` | P-MODEL | from_slot() constructor |
| `src/runtime/executor.rs` | P-MODEL, P-EPISODE, P-CONTEXT | Slot routing, episode gen, budget enforcement |
| `src/runtime/runner.rs` | P-STRATEGY | Strategy mode routing |
| `src/store/mod.rs` | P-EPISODE | Episode storage |
| `src/binding/resolve.rs` | P-EPISODE | Episode-aware resolution |
| `src/event/log.rs` | P-EPISODE, P-CONTEXT | EpisodeCreated, BudgetExceeded events |
| `src/dag/mod.rs` | P-STRATEGY | Mutable operations |
| `src/mcp/client.rs` | P-MEMORY | AgentEpisode read/write |
