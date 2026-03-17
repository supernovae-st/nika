# 05 — Evolution Roadmap

> 6 priorities in 3 waves — from Nika v0.27 to v0.30.
> Centered on Slate's thread/record architecture, adapted for YAML-first declarative workflows.

**Nika** v0.27.0 · **NovaNet** v0.20.0 · Updated 2026-03-14

---

## Overview

```mermaid
flowchart LR
    subgraph TODAY["v0.27 — Today"]
        T1["Single provider per workflow"]
        T2["Raw TaskResult passing"]
        T3["Static DAG execution"]
        T4["No context budgeting"]
        T5["In-memory Egghead"]
    end

    subgraph TOMORROW["v0.30 — Target"]
        F1["4-slot model routing"]
        F2["Record compression"]
        F3["Shaka orchestration"]
        F4["Context budget mgmt"]
        F5["3-tier Punk Records"]
    end

    T1 -->|"P-MODEL"| F1
    T2 -->|"P-RECORD"| F2
    T3 -->|"P-SHAKA"| F3
    T4 -->|"P-CONTEXT"| F4
    T5 -->|"P-MEMORY"| F5

    style TODAY fill:#fee2e2,stroke:#dc2626
    style TOMORROW fill:#dcfce7,stroke:#16a34a
```

> [!IMPORTANT]
> **Core insight** — Nika's DAG IS Slate's kernel. `AnalyzedWorkflow` IS the OS. `TaskResult` IS return values. `Egghead` IS RAM. We don't BUILD Slate — we UPGRADE the kernel with 4 additions, then persist via NovaNet.

---

## The 6 Priorities

```mermaid
flowchart TD
    PM["🎛️ P-MODEL\n4-slot model routing"]
    PE["📦 P-RECORD\nRecord compression"]
    PS["🎯 P-SHAKA\nShaka orchestration"]
    PC["📊 P-CONTEXT\nContext budgeting"]
    PMEM["🧠 P-MEMORY\n3-tier Punk Records"]
    PI["🔍 P-INTROSPECT\nRuntime introspection"]

    PM --> PS
    PE --> PS
    PE --> PC
    PS --> PMEM
    PC --> PMEM
    PMEM --> PI

    subgraph W1["Wave 1 · v0.28 · schema @0.12"]
        PM
        PE
    end

    subgraph W2["Wave 2 · v0.29 · schema @0.13"]
        PS
        PC
    end

    subgraph W3["Wave 3 · v0.30"]
        PMEM
        PI
    end

    style W1 fill:#dbeafe,stroke:#2563eb
    style W2 fill:#fef3c7,stroke:#d97706
    style W3 fill:#dcfce7,stroke:#16a34a
```

> [!NOTE]
> **Old P3 (ConfidenceRouter) absorbed** — Confidence is now a record property. The Shaka LLM handles escalation naturally, with full context, rather than a rigid router with fixed rules.

---

## Wave 1: Thread Foundation (v0.28, schema @0.12)

### P-MODEL: 4-Slot Model Architecture

Route different cognitive tasks to different providers/models. Inspired by Slate's cross-model composition (Sonnet + Codex)[^1], adapted as named slots per-workflow.

```mermaid
flowchart LR
    subgraph SLOTS["model_slots:"]
        M["🧠 edison\nclaude-sonnet-4-6"]
        T["⚡ atlas\nllama-3.3-70b"]
        S["🔍 york\ndeepseek-chat"]
        R["🤔 pythagoras\nclaude + thinking"]
    end

    subgraph TASKS["tasks:"]
        T1["plan\n→ pythagoras"]
        T2["generate\n→ edison"]
        T3["fetch data\n→ york"]
        T4["format\n→ atlas"]
    end

    M --> T2
    T --> T4
    S --> T3
    R --> T1

    style SLOTS fill:#f0f9ff,stroke:#0284c7
    style TASKS fill:#fefce8,stroke:#ca8a04
```

**Current state** → After:

| Aspect | Today (v0.27) | After (v0.28) |
|--------|---------------|---------------|
| Provider scope | Single per workflow | 4 named slots per workflow |
| Per-task override | Not supported | `model_slot:` reference |
| Cost optimization | None | Route simple tasks to cheap models |
| Provider resolution | `RigProvider::auto()` | `RigProvider::from_slot()` |

<details>
<summary>📐 YAML Design</summary>

```yaml
schema: nika/workflow@0.12

model_slots:
  edison:
    provider: anthropic
    model: claude-sonnet-4-6
    # For: primary content generation, complex reasoning

  atlas:
    provider: groq
    model: llama-3.3-70b-versatile
    # For: simple thread execution, tactical actions

  york:
    provider: deepseek
    model: deepseek-chat
    # For: research, search synthesis, information retrieval

  pythagoras:
    provider: anthropic
    model: claude-sonnet-4-6
    extended_thinking: true
    thinking_budget: 16384
    # For: planning, review, critique

default_model_slot: edison

tasks:
  - id: plan
    model_slot: pythagoras         # Expensive deep-thinking model
    infer: "Create a content plan for {{with.entity}}"

  - id: generate_pages
    model_slot: atlas              # Cheap fast model
    for_each: $pages
    infer: "Generate page {{with.item}}"

  - id: review
    model_slot: pythagoras         # Back to expensive model
    infer: "Review all generated pages for quality"
```

**Why per-workflow, not global**: Different workflows have different cost/quality tradeoffs. A content generation workflow uses expensive models. An audit workflow uses cheap models. Slate's global config can't express this.

</details>

<details>
<summary>🔧 Implementation</summary>

| Change | Location | Effort |
|--------|----------|--------|
| `ModelSlot` struct | `ast/raw/model_slot.rs` (NEW) | Low |
| `model_slots:` field | `ast/raw/workflow.rs` | Low |
| `model_slot:` field | `ast/raw/task.rs` | Low |
| Slot validation | `ast/analyzer/analyze.rs` | Low |
| `from_slot()` constructor | `provider/rig.rs` | Medium |
| Slot resolution per task | `runtime/executor.rs` | Medium |
| Schema bump | `schemas/nika-workflow.schema.json` → @0.12 | Low |
| Feature gate | `ast/analyzer/feature_gate.rs` | Low |

**Breaking changes:** None — new optional fields, old workflows unchanged.

</details>

<details>
<summary>📚 Source Inspiration</summary>

- **Slate:** Cross-model composition — uses Sonnet + Codex together for different cognitive tasks[^1]. The "4 named slots" design (edison/atlas/york/pythagoras) is our proposal.
- **THREAD:** Resource-aware model allocation per subtask[^2]
- **SWE-bench:** Different models excel at different cognitive tasks

</details>

---

### P-RECORD: Record Engine

Compressed representation of a task's execution, generated at the natural completion boundary. Downstream tasks receive **records**, not raw output. This is Slate's core innovation[^1].

```mermaid
stateDiagram-v2
    [*] --> Executing: Task starts
    Executing --> Completed: Task finishes
    Completed --> Compressing: record.compress = true
    Compressing --> RecordStored: LLM summarizes
    Completed --> RawStored: record.compress = false

    RecordStored --> [*]: summary + key_findings + confidence
    RawStored --> [*]: raw TaskResult (legacy)

    note right of Compressing
        Uses cheap model (atlas slot)
        Max tokens configurable
        Confidence self-assessed
    end note
```

**Current state** → After:

| Aspect | Today (v0.27) | After (v0.28) |
|--------|---------------|---------------|
| Task output | Raw `TaskResult` in `Egghead` | `Record` struct with compression |
| Context passing | Full output via `with:` bindings | Record summaries via `with:` bindings |
| Context growth | Linear with pipeline depth | Bounded by record `max_tokens` |
| Observability | `TaskCompleted` event | `TaskCompleted` + `RecordCreated` events |
| Confidence | Not tracked | Self-assessed `0.0-1.0` per record |

<details>
<summary>📐 YAML Design</summary>

```yaml
tasks:
  - id: research_trends
    model_slot: york
    infer: "Research QR code trends in 2026"
    record:
      compress: true           # Generate record summary after execution
      retain: [key_findings]   # What to keep from raw output
      max_tokens: 500          # Record summary size limit
      confidence_threshold: 0.8 # Shaka can escalate if below
```

</details>

<details>
<summary>🦀 Rust Data Structure</summary>

```rust
/// Compressed representation of a task's execution.
/// Generated at the natural completion boundary (not mid-stream).
pub struct Record {
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

**Location:** `src/runtime/record.rs` (NEW)

</details>

<details>
<summary>🔁 Confidence Replaces Old P3 (ConfidenceRouter)</summary>

```
Old approach (ConfidenceRouter):
  Task → Tier 1 model → confidence < threshold? → Tier 2 model
  Rigid. Fixed rules. No context.

New approach (Record confidence):
  Task → Record (with confidence) → Shaka LLM sees low confidence
  → Shaka DECIDES: retry with better model? get more context? skip?
  Adaptive. Full context. Natural escalation.
```

</details>

<details>
<summary>🔧 Implementation</summary>

| Change | Location | Effort |
|--------|----------|--------|
| `Record` struct | `runtime/record.rs` (NEW) | Medium |
| `RecordCompressor` (LLM-based) | `runtime/record_compress.rs` (NEW) | Medium |
| `record:` field | `ast/raw/task.rs` | Low |
| Record generation after completion | `runtime/executor.rs` | Medium |
| Record storage | `store/mod.rs` | Low |
| Record-aware binding resolution | `binding/resolve.rs` | Medium |
| `RecordCreated` event kind | `event/log.rs` | Low |

**Quality risk:** Compression quality depends on LLM summarization ability. Mitigated by using the `retain:` field for explicit key extraction.

</details>

<details>
<summary>📚 Source Inspiration</summary>

- **Slate:** Records — compressed at natural completion boundary[^1]
- **Context-Folding:** Sub-trajectory compression for reduced context[^3]
- **Memory-R1:** RL-trained memory policies with confidence scoring[^4]

</details>

---

## Wave 2: Shaka Intelligence (v0.29, schema @0.13)

### P-SHAKA: Shaka Orchestration

A new workflow execution mode where a **Shaka LLM** dynamically dispatches **satellites** based on the goal and accumulated records. This is Slate's thread weaving[^1]: implicit adaptive decomposition via an orchestrator loop.

```mermaid
sequenceDiagram
    participant S as 🎯 Shaka LLM
    participant R as 🔍 research
    participant W as ✍️ write_section
    participant V as 🔬 review

    Note over S: Round 1
    S->>R: dispatch(topic="QR trends")
    R-->>S: Record{summary, confidence: 0.9}

    Note over S: Round 2
    S->>W: dispatch(section="hero")
    S->>W: dispatch(section="features")
    Note right of W: Parallel execution
    W-->>S: Record{content: hero_draft}
    W-->>S: Record{content: features_draft}

    Note over S: Round 3
    S->>V: dispatch(draft=hero+features)
    V-->>S: Record{issues: [...], score: 0.85}

    Note over S: Round 4
    S->>S: All records synthesized
    Note over S: ✅ DONE — assembled page
```

**Current state** → After:

| Aspect | Today (v0.27) | After (v0.29) |
|--------|---------------|---------------|
| Execution mode | Static DAG only | `dag` (default) or `shaka` |
| Task dispatch | All known at parse time | Dynamic by Shaka LLM |
| Inter-task data | Raw `with:` bindings | Record synthesis between rounds |
| Stopping condition | DAG completed | Shaka LLM decides "DONE" |
| DAG mutation | Immutable after parse | `DynamicDag` adds tasks at runtime |

<details>
<summary>📐 YAML Design</summary>

```yaml
schema: nika/workflow@0.13
workflow: landing-page-generator

orchestration: shaka    # NEW: enables shaka/satellites mode

model_slots:
  pythagoras: { provider: anthropic, model: claude-sonnet-4-6, extended_thinking: true }
  edison: { provider: anthropic, model: claude-sonnet-4-6 }
  york: { provider: groq, model: llama-3.3-70b-versatile }
  atlas: { provider: deepseek, model: deepseek-chat }

shaka:
  goal: "Generate a complete French landing page for QR Code AI"
  model_slot: pythagoras
  max_rounds: 10
  record_budget: 15000    # Total token budget across all records

# Satellite templates — dispatched dynamically by Shaka
tasks:
  - id: research
    model_slot: york
    infer: "Research: {{with.topic}}"
    record: { compress: true, max_tokens: 300 }

  - id: write_section
    model_slot: edison
    infer: "Write: {{with.section}} using context: {{with.context}}"
    record: { compress: true, retain: [content], max_tokens: 800 }
    structured:
      schema:
        type: object
        properties:
          content: { type: string }
          word_count: { type: integer }
        required: [content]

  - id: review
    model_slot: pythagoras
    infer: "Review and critique: {{with.draft}}"
    record: { compress: true, retain: [issues, suggestions] }
    structured:
      schema:
        type: object
        properties:
          issues: { type: array, items: { type: string } }
          suggestions: { type: array, items: { type: string } }
          score: { type: number }
        required: [issues, score]
```

</details>

<details>
<summary>🔧 Implementation</summary>

| Change | Location | Effort |
|--------|----------|--------|
| `ShakaOrchestrator` struct | `runtime/shaka.rs` (NEW) | **High** |
| `SatelliteTemplate`, `SatelliteInstance` | `runtime/satellite.rs` (NEW) | Medium |
| `DynamicDag` (mutable DAG) | `dag/dynamic.rs` (NEW) | **High** |
| `orchestration:` + `shaka:` fields | `ast/raw/workflow.rs` | Low |
| Shaka mode routing | `runtime/runner.rs` | Medium |
| Mutable DAG operations | `dag/mod.rs` | Medium |
| Shaka visualization in TUI | `tui/views/runner.rs` | Medium |
| Schema bump | `schemas/nika-workflow.schema.json` → @0.13 | Low |

**Dependencies:** Requires P-MODEL + P-RECORD (Wave 1) as foundation.

</details>

<details>
<summary>📚 Source Inspiration</summary>

- **Slate:** Thread weaving — implicit adaptive decomposition[^1]
- **Slate:** Shaka/satellites separation, AlphaZero mapping (value network = Shaka, policy network = satellites)[^5]
- **THREAD:** Hierarchical decomposition with resource-aware model selection[^2]
- **RLM:** Recursive sub-LM calls with external working memory[^6]

</details>

---

### P-CONTEXT: Context Budget Management

Working memory awareness at the runtime level. Each task declares its context budget. The runtime enforces this by passing only record summaries — never raw history.

```mermaid
flowchart TB
    subgraph BEFORE["Without P-CONTEXT"]
        B1["Task A output\n2,000 tokens"] --> B2["Task B receives\nfull 2,000 tokens"]
        B2 --> B3["Task C receives\nA + B = 4,000 tokens"]
        B3 --> B4["Task D receives\nA+B+C = 6,000+ tokens"]
        B4 --> B5["💀 Dumb Zone\nContext degradation"]
    end

    subgraph AFTER["With P-CONTEXT"]
        A1["Task A → Record\n300 tokens"] --> A2["Task B receives\nrecord = 300 tokens"]
        A2 --> A3["Task C receives\nA_rec + B_rec = 600 tokens"]
        A3 --> A4["Task D receives\nrelevant records only"]
        A4 --> A5["✅ Working Memory\nAlways within budget"]
    end

    style BEFORE fill:#fee2e2,stroke:#dc2626
    style AFTER fill:#dcfce7,stroke:#16a34a
    style B5 fill:#dc2626,color:#fff
    style A5 fill:#16a34a,color:#fff
```

> [!WARNING]
> **Context degradation** is the root cause of agent failure. LLM performance degrades past the "dumb zone" threshold (Dex Horthy's term[^1]). P-CONTEXT prevents this structurally.

<details>
<summary>📐 YAML Design</summary>

```yaml
tasks:
  - id: research
    model_slot: york
    context_budget: 4000     # Max tokens in this task's context
    infer: "Research QR code trends"
    record:
      compress: true
      max_tokens: 300        # Record must fit in 300 tokens

  - id: generate
    model_slot: edison
    context_budget: 8000     # Larger budget for generation
    with:
      trends: "$research"     # Receives record, not raw output
    infer: "Generate landing page based on: {{with.trends}}"
```

**Rules:**
1. Each task receives ONLY: its prompt + relevant records + NovaNet context
2. Never raw history from other tasks
3. `context_budget` enforced by the runtime (truncate/warn if exceeded)
4. Shaka orchestrator manages which records to include per thread
5. Token budget tracked in events for observability

</details>

<details>
<summary>🔧 Implementation</summary>

| Change | Location | Effort |
|--------|----------|--------|
| `context_budget:` field | `ast/raw/task.rs` | Low |
| Budget enforcement | `runtime/executor.rs` | Medium |
| Token counting utilities | `runtime/context_budget.rs` (NEW) | Medium |
| Budget tracking in events | `event/log.rs` | Low |
| Shaka record selection | `runtime/shaka.rs` | Medium |

**Accuracy note:** Token counting is approximate (tokenizer-dependent). Use conservative estimates.

</details>

---

## Wave 3: Persistent Memory (v0.30)

### P-MEMORY: 3-Tier Punk Records

Records live in a 3-tier architecture: **HOT** (Egghead DashMap RAM, one run), **WARM** (Punk Records NDJSON on disk, TTL configurable, managed by `RecordLog`), and **COLD** (NovaNet `Record` node class, permanent, promoted records). Records first live locally in Punk Records (WARM tier), then get promoted to NovaNet (COLD tier) when they prove valuable — enabling cross-session learning and knowledge overhang activation[^1].

```mermaid
flowchart LR
    subgraph S1["Session 1"]
        R1["research(qr-code)"] --> E1["Record"]
        E1 -->|"novanet_write"| AE1["Record\nin NovaNet"]
    end

    subgraph KG["NovaNet Knowledge Graph"]
        AE1 --- ENT["Entity\nqr-code"]
        AE1 --- LOC["Locale\nfr-FR"]
        AE1 --> AE0["Previous\nRecord"]
    end

    subgraph S2["Session 2"]
        G1["generate(qr-code)"] -->|"novanet_search"| AE1
        AE1 -->|"records as context"| G1
    end

    style S1 fill:#dbeafe,stroke:#2563eb
    style KG fill:#f0fdf4,stroke:#16a34a
    style S2 fill:#fef3c7,stroke:#d97706
```

> [!TIP]
> **Knowledge overhang** — Models have knowledge they can't access without scaffolding. Cross-session records provide that scaffolding, activating latent capabilities across sessions.

<details>
<summary>📐 YAML Design</summary>

```yaml
tasks:
  - id: research
    infer: "Research QR code trends"
    record:
      compress: true
      persist: novanet        # Store record in NovaNet
      entity_link: qr-code    # Link to semantic entity

  - id: generate
    with:
      past_experience: "$recall_records"  # Retrieved from NovaNet
    infer: |
      Generate a QR code landing page.
      Previous experience: {{with.past_experience}}
```

</details>

<details>
<summary>🏗️ NovaNet Schema Additions</summary>

```
Record (NodeClass, org realm, agent layer)
├── Properties:
│   ├── key: string (unique identifier)
│   ├── workflow: string (source workflow name)
│   ├── task_id: string (source task)
│   ├── summary: string (compressed record)
│   ├── key_findings: string[] (extracted points)
│   ├── model_used: string
│   ├── tokens_spent: integer
│   ├── confidence: float
│   └── timestamp: datetime
├── Arcs:
│   ├── RECORD_OF → Entity (semantic link)
│   ├── FOR_LOCALE → Locale (if locale-specific)
│   ├── SIMILAR_TO → Record (similarity)
│   └── PRECEDED_BY → Record (temporal chain)
```

**Requires:** NovaNet schema ADR + coordinated Nika/NovaNet development.

</details>

<details>
<summary>🔧 Implementation (5 Phases)</summary>

| Phase | What | Location |
|-------|------|----------|
| 1 | Record data model | NovaNet schema YAML (ADR required) |
| 2 | Write records | Nika calls `novanet_write` after completion |
| 3 | Recall records | `novanet_search` for similar past runs |
| 4 | Inject in context | Recalled records in agent system prompt |
| 5 | Auto-learning | Pattern extraction from success/failure |

</details>

---

### P-INTROSPECT: Runtime Introspection Tools

New builtin tools that let agents query the current workflow's runtime state. The DAG becomes a first-class data structure that agents can reason about.

| Tool | Returns | Use Case |
|------|---------|----------|
| `nika:records` | `[{task_id, summary, confidence, tokens}]` | Query accumulated records |
| `nika:threads` | `[{task_id, status, model_slot}]` | List active/completed threads |
| `nika:shaka` | `{round, max_rounds, budget_used, budget_total}` | Check Shaka progress |
| `nika:cost` | `{total_tokens, total_cost, per_model}` | Token usage and cost report |
| `nika:dag_info` | `{predecessors, successors, critical_path}` | DAG structure queries |
| `nika:task_status` | `{task_id, status, record}` | Individual task status |

> [!NOTE]
> These 6 new tools join the existing 11 builtin tools (6 core + 5 file), bringing the total to 17. All read-only — agents cannot modify runtime state via introspection tools.

<details>
<summary>📐 YAML Design</summary>

```yaml
tasks:
  - id: adaptive_step
    agent:
      prompt: "Generate content, adapting based on what came before"
      tools:
        - nika:records       # Query accumulated records
        - nika:cost           # Check remaining budget
        - nika:shaka          # Know current round
        - nika:dag_info       # Understand DAG structure
```

</details>

---

## Priority Matrix

```mermaid
quadrantChart
    title Impact vs Effort
    x-axis "Low Effort" --> "High Effort"
    y-axis "Medium Impact" --> "High Impact"
    quadrant-1 "Strategic Investments"
    quadrant-2 "Quick Wins"
    quadrant-3 "Low Priority"
    quadrant-4 "Consider Carefully"
    "P-MODEL": [0.30, 0.75]
    "P-RECORD": [0.45, 0.85]
    "P-CONTEXT": [0.50, 0.70]
    "P-INTROSPECT": [0.40, 0.50]
    "P-SHAKA": [0.80, 0.90]
    "P-MEMORY": [0.75, 0.95]
```

---

## Version Mapping

| Priority | Version | Schema | New Files | Modified Files | Dependencies |
|----------|---------|--------|-----------|----------------|--------------|
| P-MODEL | v0.28.0 | @0.12 | 2 | 6 | None |
| P-RECORD | v0.28.0 | @0.12 | 2 | 5 | None (ships with P-MODEL) |
| P-SHAKA | v0.29.0 | @0.13 | 3 | 5 | P-MODEL + P-RECORD |
| P-CONTEXT | v0.29.0 | @0.13 | 1 | 3 | P-RECORD |
| P-MEMORY | v0.30.0 | @0.13 + NovaNet | 1 | 3 | P-RECORD |
| P-INTROSPECT | v0.30.0 | — | 6 tools | 3 | P-RECORD + P-SHAKA |

---

## File Change Summary

<details>
<summary>📁 New Files (8)</summary>

| File | Priority | Purpose |
|------|----------|---------|
| `src/ast/raw/model_slot.rs` | P-MODEL | `ModelSlot` struct |
| `src/ast/analyzed/model_slot.rs` | P-MODEL | Analyzed slot with validation |
| `src/runtime/record.rs` | P-RECORD | `Record` struct + lifecycle |
| `src/runtime/record_compress.rs` | P-RECORD | LLM-based compression |
| `src/runtime/shaka.rs` | P-SHAKA | `ShakaOrchestrator` |
| `src/runtime/satellite.rs` | P-SHAKA | `SatelliteTemplate`, `SatelliteInstance` |
| `src/dag/dynamic.rs` | P-SHAKA | `DynamicDag` for runtime mutation |
| `src/runtime/episodic_memory.rs` | P-MEMORY | `EpisodicMemoryManager` |

</details>

<details>
<summary>📝 Modified Files (11)</summary>

| File | Priorities | Changes |
|------|-----------|---------|
| `src/ast/raw/workflow.rs` | P-MODEL, P-SHAKA | `model_slots`, `orchestration`, `shaka` fields |
| `src/ast/raw/task.rs` | P-MODEL, P-RECORD, P-CONTEXT | `model_slot`, `record`, `context_budget` fields |
| `src/ast/analyzer/analyze.rs` | P-MODEL, P-RECORD | Slot validation, record config validation |
| `src/provider/rig.rs` | P-MODEL | `from_slot()` constructor |
| `src/runtime/executor.rs` | P-MODEL, P-RECORD, P-CONTEXT | Slot routing, record gen, budget enforcement |
| `src/runtime/runner.rs` | P-SHAKA | Shaka mode routing |
| `src/store/mod.rs` | P-RECORD | Record storage in `Egghead` |
| `src/binding/resolve.rs` | P-RECORD | Record-aware resolution |
| `src/event/log.rs` | P-RECORD, P-CONTEXT | `RecordCreated`, `BudgetExceeded` events |
| `src/dag/mod.rs` | P-SHAKA | Mutable operations |
| `src/mcp/client.rs` | P-MEMORY | `Record` read/write |

</details>

---

## Cross-Cutting Concerns

### Context Compression (from literature)

Not a separate priority — woven into P-RECORD and P-CONTEXT:

- **P-RECORD:** Sub-DAG results auto-compressed at completion boundary (Context-Folding[^3])
- **P-CONTEXT:** Working memory budget prevents degradation (Slate's dumb zone[^1])

### A2A Protocol

Future consideration beyond these 6 priorities. If Nika agents need to coordinate with external runtimes (LangGraph, Slate), A2A is the protocol. Not urgent for QR Code AI target.

### Code Execution Sandbox

Potential future priority. A `code:` verb with Pyodide/Deno sandbox would give agents CodeAct-level expressivity[^7]. Lower priority because Nika's 5 semantic verbs + `exec:` cover most needs.

---

## Sequencing Rationale

> [!TIP]
> **Why this order?** Each wave builds the foundation for the next. You can't have Shaka without model slots and records. You can't have memory without records being stable.

1. **P-MODEL first** — Low-effort, high-value, prerequisite for everything (Shaka needs model slots to route satellites)
2. **P-RECORD with P-MODEL** — Records are the core primitive. Everything downstream depends on compressed task results
3. **P-SHAKA after Wave 1** — Shaka orchestration REQUIRES both model slots (routing) and records (inter-round communication)
4. **P-CONTEXT with P-SHAKA** — Context budgeting makes Shaka mode practical (without budgets, rounds accumulate unbounded context)
5. **P-MEMORY last** — Requires cross-project NovaNet schema changes (ADR, NodeClass, ArcClasses) and builds on records being stable
6. **P-INTROSPECT with P-MEMORY** — Introspection tools are simple once runtime state (records, Shaka, cost) is already tracked

---

<div align="center">

[← 04 Nika × NovaNet Overlap](./04-nika-novanet-overlap.md) · [📋 Index](./00-README.md) · [06 Research Synthesis →](./06-research-synthesis-report.md)

</div>

---

[^1]: Slate by Random Labs — [Technical blog post](https://randomlabs.ai/blog/slate). Records, thread weaving, working memory, and cross-model composition are Slate's architectural innovations. The "4 named slots" (edison/atlas/york/pythagoras) is our design, inspired by Slate's approach.
[^2]: THREAD: Thinking Deeper with Recursive Spawning — [arXiv:2405.17402](https://arxiv.org/abs/2405.17402). Hierarchical agent decomposition with resource-aware model selection.
[^3]: Context-Folding: Scaling Long-Horizon LLM Agent — [arXiv:2510.11967](https://arxiv.org/abs/2510.11967). Branch/fold sub-trajectory compression.
[^4]: Memory-R1: RL-trained agent memory policies — [arXiv:2508.19828](https://arxiv.org/abs/2508.19828). Confidence scoring and memory retention.
[^5]: McGrath et al., "Acquisition of Chess Knowledge in AlphaZero" — [PNAS 2022](https://www.pnas.org/doi/10.1073/pnas.2206625119). Shaka/satellites separation cited in Slate blog.
[^6]: RLM: Recursive Language Models — [arXiv:2512.24601](https://arxiv.org/abs/2512.24601) (MIT, 2025). Recursive sub-LM calls with external working memory.
[^7]: CodeAct: Code Actions for LLM Agents — [arXiv:2402.01030](https://arxiv.org/abs/2402.01030) (ICML 2024).
