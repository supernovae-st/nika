# 07 — Slate Deep Integration Strategy

> Copying Slate's thread/record architecture into Nika, then going beyond.
> Every concept mapped. Every claim verified. Every design grounded in existing code.

**Nika** v0.30.3 · **NovaNet** v0.20.0 · Updated 2026-03-14

---

## Why This Document Exists

Slate (Random Labs)[^1] introduced an architecture — threads, records, thread weaving, shaka/satellites — that solves the fundamental problems of long-running AI agents. This document maps every Slate concept to Nika's existing architecture, identifies what needs to change, and designs how Nika goes **beyond** Slate by leveraging the NovaNet knowledge graph, YAML declarative workflows, and full observability.

> [!IMPORTANT]
> **Guiding principle** — We are not building feature parity with Slate. We are taking Slate's **architectural insights** and implementing them in a way that is **declaratively superior** — auditable, reproducible, version-controlled, and knowledge-graph-powered.

---

## Slate's Core Architecture

### The Problem Slate Solves

LLM context windows are not uniformly useful. Performance degrades past a threshold — the "dumb zone" (Dex Horthy's term[^1]). Every existing approach fails in a specific way:

```mermaid
flowchart TB
    PROBLEM["💀 Context Degradation\nLLMs lose quality past working memory"]

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
    TW --> ST["5. Shaka/Satellites\nAlphaZero mapping"]
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
<summary>📖 Concept Details</summary>

| # | Concept | Mechanism | Key Insight |
|---|---------|-----------|-------------|
| 1 | **Working Memory** | Context has a usable zone and a degraded "dumb zone" | Never exceed working memory threshold |
| 2 | **Threads** | Each thread executes ONE action, then pauses. NOT persistent subagents | Context isolated per thread |
| 3 | **Records** | Compressed representation at completion boundary (not mid-stream) | Only important results retained |
| 4 | **Thread Weaving** | Orchestrator: dispatch threads → collect records → synthesize → dispatch | Implicit adaptive decomposition |
| 5 | **Shaka/Satellites** | Shaka = open-ended planning. Satellites = learned action sequences | AlphaZero mapping (value + policy networks)[^2] |
| 6 | **Knowledge Overhang** | Models have knowledge they can't access without scaffolding | Records provide the scaffolding |
| 7 | **Composability** | Records flow between threads as handoff boundary | Cross-model composition |
| 8 | **OS Framing** | Orchestrator = kernel, Threads = processes, Records = return values | Karpathy's LLM OS framing |

</details>

> [!NOTE]
> **Slate's model configuration** — Slate supports cross-model composition (e.g., Sonnet + Codex for different cognitive tasks)[^1]. The "4 named slots" (edison/atlas/york/pythagoras) is our design, inspired by this capability.

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
        NIPC["with: bindings\n(A.output → B)"]
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

> [!TIP]
> **What's missing is NOT the kernel** — it's 4 kernel upgrades: record compression, dynamic process creation (shaka), memory budgets, and model routing. The kernel itself (`Runner` + `TaskExecutor` + `RunContext`) already works.

---

## Concept-by-Concept Mapping

### Complete Mapping Table

| # | Slate Concept | Nika Existing | Nika Needed | Nika Goes Beyond |
|:-:|---------------|---------------|-------------|------------------|
| 1 | Working Memory | No awareness | Context budget per task | Budget is declarative YAML |
| 2 | Dumb Zone | N/A | Working memory boundary | Token budget in events |
| 3 | Threads | Tasks in DAG (partial) | Dynamic dispatch by shaka | Satellite TEMPLATES in YAML |
| 4 | Records | `TaskResult` (raw) | Record compression at boundary | NovaNet persistence |
| 5 | Thread Weaving | DAG execution (static) | Dynamic DAG + shaka loop | Real-time TUI visualization |
| 6 | Shaka/Satellites | Flat agent loop | `orchestration: shaka` | Declarative YAML shakas |
| 7 | Knowledge Overhang | NovaNet context + files | Record-based scaffolding | 200+ locale knowledge atoms |
| 8 | Episodic Memory | In-memory `RunContext` | NovaNet `Record` | Graph-queryable, entity-linked |
| 9 | Model Slots | Single provider | `model_slots:` in YAML | Per-workflow slots |
| 10 | Composability | `with:` bindings | Record-aware bindings | Structured output + records |
| 11 | Parallel Threads | `for_each` + concurrency | Shaka parallel dispatch | Token budget + cost tracking |
| 12 | Cross-model | Multi-provider (7+native) | Model slot per task | YAML-declared routing |
| 13 | OS Framing | DAG = kernel | Shaka = kernel upgrade | NovaNet = persistent storage |
| 14 | Permissions | Command blocklist + shell-free | Already better | 4-layer security model |
| 15 | build/plan agents | `agent:` verb | Shaka mode selection | Multiple shaka templates |
| 16 | Custom /commands | Skills via `include:` | Already exists | YAML skills merged via DAG |
| 17 | .env config | `.nika/config.toml` | Already exists | 3-level config merge |

---

## Design: The 5-Layer Architecture

```mermaid
flowchart TB
    subgraph L1["Layer 1 — Model Slots"]
        MS["model_slots:\n4 named slots per workflow"]
    end

    subgraph L2["Layer 2 — Record Engine"]
        EE["record:\ncompression at completion boundary"]
    end

    subgraph L3["Layer 3 — Shaka Orchestration"]
        SO["orchestration: shaka\ndynamic satellite dispatch"]
    end

    subgraph L4["Layer 4 — Context Budget"]
        CB["context_budget:\nworking memory enforcement"]
    end

    subgraph L5["Layer 5 — NovaNet Memory"]
        NM["record.persist: novanet\ncross-session, entity-linked"]
    end

    L1 --> L2
    L2 --> L3
    L3 --> L4
    L4 --> L5

    style L1 fill:#dbeafe,stroke:#2563eb
    style L2 fill:#dbeafe,stroke:#2563eb
    style L3 fill:#fef3c7,stroke:#d97706
    style L4 fill:#fef3c7,stroke:#d97706
    style L5 fill:#dcfce7,stroke:#16a34a
```

### Layer 1: Model Slots

Per-workflow model slot definitions that route different cognitive tasks to different providers. See [05-evolution-roadmap.md § P-MODEL](./05-evolution-roadmap.md#p-model-4-slot-model-architecture) for full design.

```yaml
model_slots:
  edison:     { provider: anthropic, model: claude-sonnet-4-6 }
  atlas:      { provider: groq,     model: llama-3.3-70b-versatile }
  york:       { provider: deepseek, model: deepseek-chat }
  pythagoras: { provider: anthropic, model: claude-sonnet-4-6, extended_thinking: true }
```

### Layer 2: Record Engine

See [05-evolution-roadmap.md § P-RECORD](./05-evolution-roadmap.md#p-record-record-engine) for full design.

```yaml
record:
  compress: true           # LLM compression at completion boundary
  retain: [key_findings]   # Explicit key extraction
  max_tokens: 500          # Size limit
  confidence_threshold: 0.8
```

### Layer 3: Shaka Orchestration

The core upgrade. See [05-evolution-roadmap.md § P-SHAKA](./05-evolution-roadmap.md#p-shaka-shaka-orchestration) for full design.

```mermaid
sequenceDiagram
    participant S as 🎯 Shaka Orchestrator
    participant T as ⚡ Satellite Templates

    loop Until DONE or max_rounds
        S->>S: Review accumulated records
        S->>T: Dispatch satellite(s) with params
        T-->>S: Record(s) with confidence
        S->>S: Synthesize records
        alt confidence >= threshold
            S->>S: Continue or DONE
        else confidence < threshold
            S->>T: Retry with better model_slot
        end
    end
```

### Layer 4: Context Budget

See [05-evolution-roadmap.md § P-CONTEXT](./05-evolution-roadmap.md#p-context-context-budget-management) for full design.

### Layer 5: NovaNet Episodic Memory

See [05-evolution-roadmap.md § P-MEMORY](./05-evolution-roadmap.md#p-memory-novanet-episodic-memory) for full design.

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
        N1["YAML satellite templates"]
        N2["NovaNet graph memory"]
        N3["32 events + NDJSON"]
        N4["Record budget + tokens"]
        N5["200+ locales"]
    end

    S1 -.->|"→"| N1
    S2 -.->|"→"| N2
    S3 -.->|"→"| N3
    S4 -.->|"→"| N4
    S5 -.->|"→"| N5

    style SLATE_HAS fill:#fee2e2,stroke:#dc2626
    style NIKA_HAS fill:#dcfce7,stroke:#16a34a
```

| Dimension | Slate | Nika (after integration) |
|-----------|-------|--------------------------|
| Thread definition | TypeScript code | YAML satellite templates |
| Record storage | In-memory session | `RunContext` + NovaNet graph |
| Cross-session | Session files | Knowledge graph (queryable) |
| Observability | Basic logging | 32 EventKind variants[^3] + NDJSON |
| Cost control | None | Record budget + token tracking |
| Knowledge source | None | NovaNet atoms (200+ locales) |
| Reproducibility | Non-deterministic | DAG traces + replay |
| Multi-locale | English only | 200+ locales |
| Model routing | Global config | Per-workflow `model_slots:` |
| Orchestration | Imperative code | Declarative YAML |
| DAG visualization | None | Real-time TUI |
| Structured output | Not documented | 4-layer validation |
| Security | Not documented | Shell-free + blocklist |

> [!IMPORTANT]
> **Unique to Nika** (Slate has NO equivalent):
> - NovaNet knowledge graph (59 NodeClasses, 159 ArcClasses)
> - Entity-linked episodic memory with graph queries
> - Knowledge atoms (Expression, Pattern, CultureRef, Taboo) across 200+ locales
> - NDJSON trace files with full event sourcing (32 EventKind variants)
> - 4-layer structured output (parse → validate → retry → repair)
> - DAG visualization in TUI with real-time thread/record view
> - Record budget with per-workflow cost prediction

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
| Parallelism | None | None | None | None | Multi-agent | None | Native | **for_each+shaka** |
| Adaptability | Low | Low | Low | Low | Medium | High | High | **High** |
| **Reproducibility** | Low | Low | Low | Low | Low | Low | Low | **High** |
| **Observability** | Low | Low | Low | Low | Medium | Medium | Low | **High** |
| **Cost control** | None | None | None | None | None | None | None | **Record budget** |
| **Knowledge graph** | None | None | None | None | None | None | None | **NovaNet** |

---

## Complete Example

<details>
<summary>📋 Full Shaka Workflow — Landing Page Generation</summary>

```yaml
schema: nika/workflow@0.14
workflow: generate-landing-page

orchestration: shaka

model_slots:
  pythagoras:
    provider: anthropic
    model: claude-sonnet-4-6
    extended_thinking: true
    thinking_budget: 16384
  edison:
    provider: anthropic
    model: claude-sonnet-4-6
  york:
    provider: groq
    model: llama-3.3-70b-versatile
  atlas:
    provider: deepseek
    model: deepseek-chat

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "--manifest-path", "/path/to/novanet/Cargo.toml"]

shaka:
  goal: |
    Generate a complete French landing page for QR Code AI.
    Use NovaNet for entity context and locale knowledge.
    Research current trends, write sections, review quality.
  model_slot: pythagoras
  max_rounds: 8
  record_budget: 15000

satellites:
  - id: get_context
    model_slot: atlas
    invoke:
      tool: novanet_generate
      server: novanet
      params:
        focus_key: "homepage"
        locale: "fr-FR"
        mode: page
    record:
      compress: true
      max_tokens: 500

  - id: research
    model_slot: york
    context_budget: 4000
    infer: "Research: {{with.topic}}"
    record:
      compress: true
      max_tokens: 300
      retain: [key_findings]

  - id: write_section
    model_slot: edison
    context_budget: 8000
    with:
      context: $get_context
    infer: |
      Write the {{with.section}} section for the landing page.
      Entity context: {{with.context}}
      Research: {{with.research_records}}
    record:
      compress: true
      retain: [content]
      max_tokens: 800

  - id: review
    model_slot: pythagoras
    infer: |
      Review the following draft sections for quality and coherence:
      {{with.drafts}}
      Check against QR Code AI brand guidelines and French locale conventions.
    record:
      compress: true
      retain: [issues, suggestions, score]
      confidence_threshold: 0.85

  - id: persist_records
    model_slot: atlas
    invoke:
      tool: novanet_write
      server: novanet
      params:
        class: Record
        key: "landing-page-{{date}}"
    record:
      persist: novanet
      entity_link: qr-code-ai
```

</details>

---

## Confidence → Natural Escalation

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
        N2 --> N3["Shaka orchestrator\nsees low confidence"]
        N3 -->|"retry?"| N4["Better model_slot"]
        N3 -->|"more context?"| N5["Add research"]
        N3 -->|"good enough?"| N6["Accept & continue"]
    end

    style OLD fill:#fee2e2,stroke:#dc2626
    style NEW fill:#dcfce7,stroke:#16a34a
```

> [!TIP]
> The Shaka orchestrator has **full context** to decide how to handle low confidence. A rigid router has fixed rules. This is simpler AND more powerful.

---

## Summary

```mermaid
mindmap
    root((Nika Evolution))
        Slate's Insights
            Threads → Tasks
            Records → Compressed results
            Weaving → Shaka orchestration
            Model slots → Per-workflow routing
        Nika's Additions
            YAML declarative
            NovaNet knowledge graph
            32 events observability
            Record budget cost control
            200+ locales
        Result
            Most advanced RLM implementation
            Auditable, reproducible
            Knowledge-graph-powered
```

> [!IMPORTANT]
> **The Golden Rule (extended):**
>
> | Concern | Owner | Why |
> |---------|-------|-----|
> | **KNOWING** things | NovaNet | Knowledge graph, entities, locales, semantics |
> | **DOING** things | Nika | Workflow execution, DAG, verbs, providers |
> | **CONNECTING** | MCP | Protocol boundary, zero Cypher in Nika |
> | **THINKING** | Records | Shaka orchestration, model routing, confidence |
> | **REMEMBERING** | Records → NovaNet | Cross-session memory, entity-linked persistence |

---

<div align="center">

[← 06 Research Synthesis](./06-research-synthesis-report.md) · [📋 Index](./00-README.md) · [08 v0.30 Guide →](./08-nika-030-complete-guide.md)

</div>

---

[^1]: Slate by Random Labs — [Technical blog post](https://randomlabs.ai/blog/slate) with 26 academic references. Thread-based episodic memory architecture. The "4 model slots" design is our proposal, inspired by Slate's cross-model composition (Sonnet + Codex).
[^2]: McGrath et al., "Acquisition of Chess Knowledge in AlphaZero" — [PNAS 2022](https://www.pnas.org/doi/10.1073/pnas.2206625119). Shaka/satellites separation cited in Slate blog.
[^3]: Verified via `src/event/log.rs` — 32 `EventKind` variants as of v0.30.3 (LimitReached and PartialCompletion removed).
