# 08 — Nika Reference (Guide + API)

> This document combines the user-facing guide (Part 1) and the API-level technical reference (Part 2). Use Part 1 to understand features; use Part 2 for implementation details.

**Nika** v0.27.0 → v0.30 · **NovaNet** v0.20.0 · Updated 2026-03-20

---

## Table of Contents

### Part 1: User Guide

1. [What is Nika?](#what-is-nika)
2. [What is NovaNet?](#what-is-novanet)
3. [How They Work Together](#how-nika--novanet-work-together)
4. [What's New in v0.30 — The 6 Features](#whats-new-in-v030)
5. [Feature 1: Model Slots](#feature-1-model-slots)
6. [Feature 2: Records](#feature-2-records)
7. [Feature 3: Orchestrate Mode](#feature-3-orchestrate-mode)
8. [Feature 4: Context Budget](#feature-4-context-budget)
9. [Feature 5: Persistent Memory](#feature-5-persistent-memory-3-tier-punk-records)
10. [Feature 6: Runtime Introspection](#feature-6-runtime-introspection)
11. [Before vs After — Side by Side](#before-vs-after)
12. [Feature Compatibility Matrix](#feature-compatibility-matrix)
13. [FAQ](#faq)

### Part 2: Technical Reference

14. [What Is Nika (Architecture)](#1-what-is-nika-1)
15. [Architecture Overview](#2-architecture-overview)
16. [The 5 Semantic Verbs (API)](#3-the-5-semantic-verbs)
17. [DAG Execution Engine](#4-dag-execution-engine)
18. [Model Routing — 4 Slots (API)](#5-model-routing--4-slots)
19. [Agent System](#6-agent-system)
20. [Record Engine (API)](#7-record-engine)
21. [Orchestrate Mode (API)](#8-orchestrate-mode)
22. [Context Budget Management (API)](#9-context-budget-management)
23. [Structured Output Pipeline](#10-structured-output-pipeline)
24. [NovaNet Integration (MCP)](#11-novanet-integration-mcp)
25. [Persistent Records (API)](#12-persistent-records)
26. [Runtime Introspection (API)](#13-runtime-introspection)
27. [Binding System & Data Flow](#14-binding-system--data-flow)
28. [Artifact System](#15-artifact-system)
29. [Security Model](#16-security-model)
30. [Observability & Traces](#17-observability--traces)
31. [CLI](#18-cli)
32. [TUI — 3-View Architecture](#19-tui--3-view-architecture)
33. [LSP](#20-lsp)
34. [Stack & Numbers](#21-stack--numbers)
35. [Complete Module Map](#22-complete-module-map)

---

# Part 1: User Guide

> Everything you need to understand Nika v0.30 + NovaNet — explained with real examples.
> What it is, how it works, what changed, and how to use it.

---

## What is Nika?

Nika is a **workflow engine for AI tasks**. You write a YAML file describing what you want to do, and Nika executes it — calling LLMs, running shell commands, fetching URLs, and invoking MCP tools.

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  YOU WRITE THIS (YAML):              NIKA DOES THIS:                            │
│                                                                                 │
│  tasks:                              1. Parse YAML into a DAG                   │
│    - id: research                    2. Validate dependencies                   │
│      infer: "Research QR codes"      3. Execute tasks in order                  │
│                                      4. Pass outputs between tasks               │
│    - id: write                       5. Track tokens, costs, events             │
│      with:                           6. Write NDJSON trace file                  │
│        data: "$research"                                                        │
│      infer: "Write article from:                                                │
│              {{with.data}}"                                                     │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### The 5 Verbs

Every task in a Nika workflow uses exactly one of 5 verbs:

```mermaid
flowchart LR
    subgraph VERBS["Nika's 5 Semantic Verbs"]
        direction TB
        I["⚡ infer:\nAsk an LLM to generate text"]
        E["📟 exec:\nRun a shell command"]
        F["🛰️ fetch:\nMake an HTTP request"]
        V["🔌 invoke:\nCall an MCP tool"]
        A["🐔 agent:\nMulti-turn agentic loop"]
    end

    I --> |"Examples"| IE["'Summarize this article'\n'Generate a landing page'\n'Translate to French'"]
    E --> |"Examples"| EE["'npm run build'\n'git status'\n'python script.py'"]
    F --> |"Examples"| FE["'GET https://api.example.com'\n'POST with JSON body'"]
    V --> |"Examples"| VE["'novanet_context'\n'novanet_search'\n'novanet_write'"]
    A --> |"Examples"| AE["'Research and write a report'\n'Debug this code'\n'Build a feature'"]

    style VERBS fill:#ede9fe,stroke:#7c3aed
```

| Verb | What it does | When to use |
|------|-------------|-------------|
| `infer:` | Sends a prompt to an LLM, gets text back | Content generation, translation, summarization |
| `exec:` | Runs a shell command, captures stdout | Building, testing, file operations |
| `fetch:` | Makes an HTTP request, returns response | API calls, web scraping, webhooks |
| `invoke:` | Calls an MCP tool (NovaNet, etc.) | Knowledge graph queries, external tools |
| `agent:` | Multi-turn loop with tools (LLM decides) | Complex tasks requiring reasoning + tools |

### Minimal Example

```yaml
# hello.nika.yaml — The simplest Nika workflow
schema: nika/workflow@0.11

tasks:
  - id: greet
    infer: "Say hello in French, Japanese, and Swahili"
```

Run it:

```bash
nika hello.nika.yaml
```

Output:

```
Bonjour ! こんにちは！ Jambo!
```

That's it. Nika calls your default LLM provider, gets the response, writes a trace file.

---

## What is NovaNet?

NovaNet is a **knowledge graph** powered by Neo4j. It stores structured knowledge about entities, locales, and content — and exposes it to Nika through 7 MCP tools.

```mermaid
flowchart TB
    subgraph NOVANET["NovaNet — The Knowledge Graph"]
        direction TB

        subgraph SHARED["Shared Realm (universal)"]
            LOC["🌍 Locales\n200+ BCP-47 codes"]
            GEO["📍 Geography\ncountries, regions"]
            KNOW["📚 Knowledge Atoms\nterms, expressions,\ntaboos, culture refs"]
        end

        subgraph ORG["Org Realm (your project)"]
            ENT["🏷️ Entities\nsemantic concepts\n(qr-code, ai-generator...)"]
            PAGE["📄 Pages\nURL structure\n(/fr/qr-code, /en/pricing...)"]
            BLOCK["🧱 Blocks\npage sections\n(hero, features, FAQ...)"]
            NATIVE["🌐 *Native\nlocalized content\n(EntityNative, PageNative...)"]
        end
    end

    ENT -->|"HAS_NATIVE"| NATIVE
    PAGE -->|"HAS_BLOCK"| BLOCK
    PAGE -->|"REPRESENTS"| ENT
    NATIVE -->|"FOR_LOCALE"| LOC

    style SHARED fill:#ccfbf1,stroke:#0d9488
    style ORG fill:#dbeafe,stroke:#2563eb
```

### What's in it concretely?

For the QR Code AI project, NovaNet contains:

| What | Example | Count |
|------|---------|-------|
| **Entities** | `qr-code`, `ai-generator`, `dynamic-qr`, `qr-scanner` | ~30 |
| **Pages** | `homepage`, `pricing`, `blog/what-is-qr` | ~20 |
| **Blocks** | `hero`, `features`, `faq`, `testimonials` | ~80 |
| **Locales** | `fr-FR`, `en-US`, `de-DE`, `ja-JP`... | 200+ |
| **Knowledge atoms** | French expressions for "QR code", taboos for Japan, cultural refs for Germany | 1000+ |

### The 7 MCP Tools

NovaNet exposes its knowledge through MCP (Model Context Protocol):

| Tool | What it does | Example |
|------|-------------|---------|
| `novanet_describe` | Get an overview of the graph | "What's in this knowledge graph?" |
| `novanet_introspect` | Inspect schema (NodeClasses, ArcClasses) | "What types of nodes exist?" |
| `novanet_search` | Find nodes by text or properties | "Find all entities about QR codes" |
| `novanet_context` | Build LLM context from the graph | "Give me everything for homepage in French" |
| `novanet_write` | Create or update data | "Store this generated PageNative" |
| `novanet_audit` | Check data quality | "What's missing for French locale?" |
| `novanet_batch` | Run multiple operations | "Search 5 things in parallel" |
| `novanet_query` | Raw Cypher (last resort) | Custom analytics queries |

---

## How Nika + NovaNet Work Together

```mermaid
sequenceDiagram
    participant U as 👤 You
    participant N as 🦋 Nika
    participant MCP as 🔌 MCP Protocol
    participant NN as 🧠 NovaNet
    participant LLM as 🤖 LLM (Claude)

    U->>N: nika generate-page.nika.yaml
    Note over N: Parse YAML, build DAG

    N->>MCP: invoke: novanet_context
    MCP->>NN: focus_key="homepage", locale="fr-FR"
    NN-->>MCP: Entity context + knowledge atoms
    MCP-->>N: Structured context (2000 tokens)

    N->>LLM: infer: "Generate hero section" + context
    LLM-->>N: Generated content

    N->>MCP: invoke: novanet_write
    MCP->>NN: Store PageNative for homepage/fr-FR
    NN-->>MCP: Write confirmed

    N-->>U: ✅ Done — trace written to .nika/traces/
```

### The Golden Rule

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  THE GOLDEN RULE — 3 lines that govern everything                             ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  KNOWING things (entities, locales, knowledge) → NovaNet                      ║
║  DOING things (execution, LLM calls, DAG)      → Nika                        ║
║  CONNECTING them (protocol boundary)            → MCP                         ║
║                                                                               ║
║  Nika NEVER touches Neo4j directly.                                           ║
║  NovaNet NEVER executes workflows.                                            ║
║  MCP is the only bridge.                                                      ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### A Real Integration Workflow

```yaml
# generate-page.nika.yaml — Real-world Nika + NovaNet
schema: nika/workflow@0.11
provider: anthropic

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "-p", "novanet-mcp"]

tasks:
  # Step 1: Get context from NovaNet
  - id: get_context
    invoke:
      tool: novanet_context
      server: novanet
      params:
        focus_key: "homepage"
        locale: "fr-FR"
        mode: page

  # Step 2: Get locale-specific knowledge (expressions, taboos)
  - id: get_knowledge
    invoke:
      tool: novanet_context
      server: novanet
      params:
        focus_key: "homepage"
        locale: "fr-FR"
        mode: knowledge
        atom_type: expression

  # Step 3: Generate content using both
  - id: generate_hero
    with:
      entity: "$get_context"
      expressions: "$get_knowledge"
    infer: |
      Generate the hero section for the QR Code AI homepage in French.

      Entity context: {{with.entity}}
      French expressions to use: {{with.expressions}}

      Requirements:
      - Natural French (not translated)
      - Use the provided expressions
      - Include a CTA button text
    structured:
      schema:
        type: object
        properties:
          headline: { type: string }
          subheadline: { type: string }
          cta_text: { type: string }
          body: { type: string }
        required: [headline, body, cta_text]

  # Step 4: Store result back in NovaNet
  - id: save_to_novanet
    with:
      content: "$generate_hero"
    invoke:
      tool: novanet_write
      server: novanet
      params:
        operation: upsert_node
        class: BlockNative
        key: "homepage-hero-fr-FR"
        locale: "fr-FR"
        properties:
          content: "{{with.content}}"
          generated_by: "nika"
```

---

## What's New in v0.30

v0.30 adds **6 new features** across 3 versions (waves). Here's the big picture:

```mermaid
flowchart TB
    subgraph TODAY["v0.27 — Today"]
        T1["1 provider per workflow"]
        T2["Raw output passing\n(full text between tasks)"]
        T3["Static DAG\n(all tasks known at parse time)"]
        T4["No context limits\n(context grows unbounded)"]
        T5["In-memory only\n(lost after execution)"]
        T6["Agent is blind\n(can't see past tasks)"]
    end

    subgraph TOMORROW["v0.30 — Target"]
        F1["✨ Model Slots\n4 models per workflow"]
        F2["✨ Records\ncompressed task results"]
        F3["✨ Orchestrate Mode\nLLM-driven orchestration"]
        F4["✨ Context Budget\ntoken limits per task"]
        F5["✨ Persistent Memory\nrecords stored in NovaNet"]
        F6["✨ Introspection\n6 new runtime tools"]
    end

    T1 -->|"Feature 1"| F1
    T2 -->|"Feature 2"| F2
    T3 -->|"Feature 3"| F3
    T4 -->|"Feature 4"| F4
    T5 -->|"Feature 5"| F5
    T6 -->|"Feature 6"| F6

    style TODAY fill:#fee2e2,stroke:#dc2626
    style TOMORROW fill:#dcfce7,stroke:#16a34a
```

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  TL;DR — THE 6 FEATURES IN ONE SENTENCE EACH                                 ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  1. MODEL SLOTS    → Use different LLMs for different tasks (save 5-10x $)    ║
║  2. RECORDS        → Compress task outputs before passing downstream          ║
║  3. ORCHESTRATE    → Let an LLM decide what tasks to run dynamically          ║
║  4. CONTEXT BUDGET → Set token limits so LLMs never get confused              ║
║  5. MEMORY         → Save records to NovaNet for cross-session learning       ║
║  6. INTROSPECT     → Let agents query the workflow's own runtime state        ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Release Schedule

| Wave | Version | Schema | Features | Dependencies |
|:----:|---------|--------|----------|--------------|
| 1 | v0.28 | @0.12 | Model Slots + Records | None |
| 2 | v0.29 | @0.13 | Orchestrate + Context Budget | Wave 1 |
| 3 | v0.30 | @0.13 | Memory + Introspection | Wave 1 + 2 |

---

## Feature 1: Model Slots

### The Problem

Today, a workflow uses ONE LLM provider for everything:

```yaml
# v0.27 — All tasks use the same model
provider: anthropic  # ← Claude Sonnet for EVERYTHING

tasks:
  - id: research
    infer: "Research QR code trends"        # Claude Sonnet ($0.003/1K tokens)

  - id: format_list
    infer: "Format this as a bullet list"   # Claude Sonnet ($0.003/1K tokens)
    # ↑ This is a trivial task — why pay $0.003/1K for bullet formatting?
```

**Problem:** You're paying Claude Sonnet prices for trivial tasks that a $0.0001/1K model could handle.

### The Solution

`model_slots:` lets you define 4 named model slots per workflow, and assign tasks to the right slot:

```yaml
# v0.28+ — Different models for different tasks
schema: nika/workflow@0.12

model_slots:
  edison:                                    # For quality content generation
    provider: anthropic
    model: claude-sonnet-4-6
    # Cost: ~$0.003/1K tokens

  atlas:                                     # For simple formatting, parsing
    provider: deepseek
    model: deepseek-chat
    # Cost: ~$0.0001/1K tokens (30x cheaper!)

  york:                                      # For research, information retrieval
    provider: groq
    model: llama-3.3-70b-versatile
    # Cost: ~$0.0003/1K tokens (10x cheaper!)

  pythagoras:                                # For complex planning, review
    provider: anthropic
    model: claude-sonnet-4-6
    extended_thinking: true
    thinking_budget: 16384

default_model_slot: edison

tasks:
  - id: research
    model_slot: york                         # ← Groq: fast & cheap
    infer: "Research QR code trends"

  - id: plan
    model_slot: pythagoras                   # ← Claude + thinking: expensive but deep
    infer: "Create a content strategy"

  - id: write_hero
    model_slot: edison                       # ← Claude: quality content
    infer: "Write the hero section"

  - id: format_output
    model_slot: atlas                        # ← DeepSeek: trivial task, dirt cheap
    infer: "Format as HTML"
```

### Cost Impact

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  💰 COST COMPARISON (same workflow, same quality)                               │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  v0.27 (single provider):                                                       │
│  ─────────────────────────────────────────────────────────────                  │
│  research:  Claude Sonnet  2K tokens × $0.003  = $0.006                        │
│  plan:      Claude Sonnet  5K tokens × $0.003  = $0.015                        │
│  write:     Claude Sonnet  3K tokens × $0.003  = $0.009                        │
│  format:    Claude Sonnet  1K tokens × $0.003  = $0.003                        │
│                                         TOTAL  = $0.033                        │
│                                                                                 │
│  v0.28 (model slots):                                                           │
│  ─────────────────────────────────────────────────────────────                  │
│  research:  Groq           2K tokens × $0.0003 = $0.0006                       │
│  plan:      Claude+think   5K tokens × $0.003  = $0.015                        │
│  write:     Claude Sonnet  3K tokens × $0.003  = $0.009                        │
│  format:    DeepSeek       1K tokens × $0.0001 = $0.0001                       │
│                                         TOTAL  = $0.0247                       │
│                                                                                 │
│  Savings: 25% on a simple 4-task workflow                                       │
│  On a 50-task workflow with many atlas tasks: savings reach 60-80%              │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Feature 2: Records

### The Problem

Today, when Task B depends on Task A, it receives Task A's **entire raw output**:

```mermaid
flowchart LR
    A["Task A\nResearch QR codes\n→ 2,500 tokens output"] -->|"with: { data: $$A }"| B["Task B receives\nALL 2,500 tokens"]
    B -->|"with: { data: $$B }"| C["Task C receives\nA (2,500) + B (1,500)\n= 4,000 tokens"]
    C -->|"with: { data: $$C }"| D["Task D receives\nA+B+C = 6,000+ tokens"]
    D --> DEAD["💀 Dumb Zone\nLLM quality collapses"]

    style DEAD fill:#dc2626,color:#fff
```

With 10 tasks chained, Task 10 might receive 20,000+ tokens of accumulated context. The LLM enters the "dumb zone" — where it has so much context it performs **worse** than with less.

### The Solution

`record:` compresses a task's output at the completion boundary:

```yaml
tasks:
  - id: research
    model_slot: york
    infer: "Research QR code trends for 2026"
    record:
      compress: true              # ← LLM summarizes the output
      max_tokens: 300             # ← Summary must fit in 300 tokens
      retain: [key_findings]      # ← Also extract structured findings
      confidence_threshold: 0.8   # ← Self-assessed quality score
```

### How it works internally

```mermaid
flowchart TB
    EXEC["Task executes\n→ produces 2,500 tokens"] --> CHECK{"record.compress\n= true?"}

    CHECK -->|"No"| RAW["Store raw output\n(legacy behavior)"]
    CHECK -->|"Yes"| COMPRESS["Call atlas LLM:\n'Summarize this in 300 tokens.\nExtract key_findings.'"]

    COMPRESS --> RECORD["Record stored:\n• summary (300 tokens)\n• key_findings: ['dynamic QR', 'AI generation']\n• confidence: 0.92\n• model_used: deepseek\n• tokens_spent: 2,500"]

    RECORD --> DOWNSTREAM["Downstream tasks receive\nthe 300-token record\nNOT the 2,500-token raw output"]

    style EXEC fill:#dbeafe,stroke:#2563eb
    style COMPRESS fill:#fef3c7,stroke:#d97706
    style RECORD fill:#dcfce7,stroke:#16a34a
    style DOWNSTREAM fill:#dcfce7,stroke:#16a34a
```

### Before vs After

```
v0.27 (raw passing):                    v0.28 (records):
────────────────────                    ─────────────────
Task A → 2,500 tokens                  Task A → Record: 300 tokens
Task B gets 2,500                      Task B gets 300
Task C gets 4,000                      Task C gets 600
Task D gets 6,000                      Task D gets 900
Task E gets 8,000                      Task E gets 1,200
...                                    ...
Task J gets 20,000 → 💀 DUMB ZONE     Task J gets 3,000 → ✅ SHARP

Context growth: O(n²)                  Context growth: O(n) bounded
```

### Record Rust Struct

```rust
pub struct Record {
    pub task_id: TaskId,
    pub summary: String,            // LLM-compressed (max_tokens limit)
    pub key_findings: Vec<String>,  // Structured extraction (retain field)
    pub raw_output: Option<String>, // Debug only — never passed downstream
    pub model_used: String,         // Which model produced this
    pub tokens_spent: u64,          // Total tokens consumed
    pub confidence: f64,            // Self-assessed quality (0.0–1.0)
    pub artifacts: Vec<Artifact>,   // Files produced by this task
}
```

---

## Feature 3: Orchestrate Mode

### The Problem

Today, Nika's DAG is **static** — you must define ALL tasks at write time:

```yaml
# v0.27 — You must decide everything upfront
tasks:
  - id: write_hero
    infer: "Write hero section"
  - id: write_features
    infer: "Write features section"
  - id: write_pricing
    infer: "Write pricing section"
  # What if research reveals you need a "testimonials" section?
  # → You can't add it. The DAG is fixed.
```

### The Solution

`goal:` adds a new execution mode where the **orchestrator** dynamically dispatches tasks:

```yaml
# v0.29 — The orchestrator decides what to do
schema: nika/workflow@0.13

goal:                   # ← NEW: enables orchestrate mode

model_slots:
  pythagoras: { provider: anthropic, model: claude-sonnet-4-6, extended_thinking: true }
  edison:     { provider: anthropic, model: claude-sonnet-4-6 }
  york:       { provider: groq,      model: llama-3.3-70b-versatile }

goal:
  goal: |
    Generate a complete landing page for QR Code AI in French.
    Research trends, write sections, review quality.
    Iterate until quality score >= 0.85.
  model_slot: pythagoras
  max_rounds: 8
  record_budget: 15000

# These are TEMPLATES — the orchestrator dispatches them
tasks:
  - id: research
    model_slot: york
    infer: "Research: {{with.topic}}"
    record: { compress: true, max_tokens: 300 }

  - id: write_section
    model_slot: edison
    infer: "Write {{with.section}} section"
    record: { compress: true, max_tokens: 800 }
    structured:
      schema:
        type: object
        properties:
          content: { type: string }
        required: [content]

  - id: review
    model_slot: pythagoras
    infer: "Review drafts: {{with.drafts}}"
    record: { compress: true, retain: [score, issues] }
    structured:
      schema:
        type: object
        properties:
          score: { type: number }
          issues: { type: array, items: { type: string } }
        required: [score, issues]
```

### How it executes

```mermaid
sequenceDiagram
    participant S as 🎯 Orchestrator<br/>(Claude + thinking)
    participant R as 🔍 research<br/>(Groq — cheap)
    participant W as ✍️ write_section<br/>(Claude — quality)
    participant V as 🔬 review<br/>(Claude + thinking)

    Note over S: Round 1 — "I need to understand the topic first"
    S->>R: dispatch(topic="QR code trends 2026")
    R-->>S: Record{key_findings: ["dynamic QR growing 40%"], confidence: 0.91}

    Note over S: Round 2 — "I have context. Write hero + features in parallel"
    S->>W: dispatch(section="hero")
    S->>W: dispatch(section="features")
    Note right of W: Parallel execution!
    W-->>S: Record{content: "Créez des QR codes...", 800 tok}
    W-->>S: Record{content: "Fonctionnalités...", 800 tok}

    Note over S: Round 3 — "Research mentioned testimonials. Let me add that"
    S->>W: dispatch(section="testimonials")
    Note right of W: Dynamic! Not in original YAML
    W-->>S: Record{content: "Nos clients...", 800 tok}

    Note over S: Round 4 — "Review everything"
    S->>V: dispatch(drafts=[hero, features, testimonials])
    V-->>S: Record{score: 0.72, issues: ["hero needs CTA"]}

    Note over S: Round 5 — "Score 0.72 < 0.85. Fix hero."
    S->>W: dispatch(section="hero", feedback="add CTA button")
    W-->>S: Record{content: "hero v2...", 800 tok}

    Note over S: Round 6 — "Re-review"
    S->>V: dispatch(drafts=[hero_v2, features, testimonials])
    V-->>S: Record{score: 0.91}

    Note over S: ✅ Score 0.91 >= 0.85 — DONE
    S->>S: Synthesize all records → final output
```

### The Two Modes

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  NIKA HAS TWO EXECUTION MODES                                                ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Mode 1: orchestration: dag (default — like v0.27)                            ║
║  ─────────────────────────────────────────────────────────────                ║
║  • All tasks defined at write time                                            ║
║  • Execution order determined by DAG                                          ║
║  • Predictable, reproducible, deterministic                                   ║
║  • Best for: pipelines, batch processing, CI/CD                               ║
║                                                                               ║
║  Mode 2: goal: (new in v0.29)                                  ║
║  ─────────────────────────────────────────────────────────────                ║
║  • Tasks are TEMPLATES dispatched by the orchestrator                                ║
║  • Orchestrator decides what to run, when, and with what params                      ║
║  • Adaptive: adds tasks, retries on low quality, changes approach             ║
║  • Best for: content generation, research, complex multi-step reasoning       ║
║                                                                               ║
║  Same YAML format. Same 5 verbs. Same bindings.                              ║
║  Only difference: WHO decides the execution order.                            ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Feature 4: Context Budget

### The Problem

Without limits, a task's context can grow unbounded — especially in orchestrate mode where multiple rounds accumulate records:

```
Round 1: research record       → 300 tokens
Round 2: write_hero record     → 800 tokens
Round 3: write_features record → 800 tokens
Round 4: review record         → 500 tokens
...
Round 8: Orchestrator has 5,000+ tokens of records
→ LLM starts losing quality
```

### The Solution

`context_budget:` sets a hard token limit per task:

```yaml
tasks:
  - id: research
    model_slot: york
    context_budget: 4000          # ← Max 4K tokens in this task's prompt
    infer: "Research QR codes"
    record:
      compress: true
      max_tokens: 300

  - id: write_hero
    model_slot: edison
    context_budget: 8000          # ← Larger budget for generation
    with:
      trends: "$research"
      entity: "$get_context"
    infer: "Write hero: {{with.trends}} {{with.entity}}"
```

### How it works

```mermaid
flowchart TB
    PROMPT["Task prompt\n500 tokens"] --> BUDGET{"context_budget\n= 8000?"}
    RECORDS["Relevant records\n2,400 tokens"] --> BUDGET
    CONTEXT["NovaNet context\n3,000 tokens"] --> BUDGET
    FILES["File context\n1,800 tokens"] --> BUDGET

    BUDGET -->|"500+2400+3000+1800\n= 7,700 < 8,000"| OK["✅ Within budget\nSend to LLM"]
    BUDGET -->|"If total > 8,000"| TRIM["⚠️ Over budget\nTruncate oldest records\nKeep most relevant"]

    style OK fill:#dcfce7,stroke:#16a34a
    style TRIM fill:#fef3c7,stroke:#d97706
```

**Rules enforced by the runtime:**
1. Each task receives ONLY: its prompt + relevant records + context
2. Never raw history from other tasks
3. Budget enforced by truncation/selection — not rejection
4. Token count tracked in events for cost monitoring

---

## Feature 5: Persistent Memory (3-Tier Punk Records)

### The Problem

Today, everything Nika learns during a workflow is **lost** when execution ends:

```
Monday:   Workflow researches QR code trends → findings in Egghead → LOST
Wednesday: Same workflow runs again → starts from zero → pays for research again
```

### The Solution

Records live in a **3-tier architecture**:
- **HOT**: Egghead (DashMap RAM, one run) -- what exists today
- **WARM**: Punk Records (NDJSON on disk, TTL configurable, managed by `RecordLog`) -- records survive restarts locally
- **COLD**: NovaNet (`Record` node class, permanent, promoted records) -- cross-session learning

Records first live locally in Punk Records (WARM tier), then get promoted to NovaNet (COLD tier) when they prove valuable. `record.persist: novanet` promotes records to the COLD tier, linked to semantic entities:

```yaml
tasks:
  - id: research
    infer: "Research QR code trends"
    record:
      compress: true
      persist: novanet            # ← NEW: save to NovaNet
      entity_link: qr-code        # ← Link to the QR code entity
```

### How it works

```mermaid
flowchart LR
    subgraph SESSION_1["Monday — Session 1"]
        R1["research(qr-code)"]
        R1 --> E1["Record:\ntrends, findings"]
        E1 -->|"novanet_write"| AE1["Record\nin NovaNet"]
    end

    subgraph KG["NovaNet Knowledge Graph"]
        AE1 -->|"RECORD_OF"| ENT["Entity:\nqr-code"]
        AE1 -->|"FOR_LOCALE"| LOC["Locale:\nfr-FR"]
        AE1 -->|"PRECEDED_BY"| AE0["Older\nRecord"]
    end

    subgraph SESSION_2["Wednesday — Session 2"]
        G1["generate(qr-code)"]
        AE1 -->|"novanet_search:\npast records"| G1
        G1 --> OUT["Uses Monday's research\nNo need to re-research!"]
    end

    style SESSION_1 fill:#dbeafe,stroke:#2563eb
    style KG fill:#dcfce7,stroke:#16a34a
    style SESSION_2 fill:#fef3c7,stroke:#d97706
```

### NovaNet Schema Addition

```
Record (new NodeClass, agent layer)
├── key: string
├── workflow: string
├── task_id: string
├── summary: string
├── key_findings: string[]
├── confidence: float
├── tokens_spent: integer
├── timestamp: datetime
├── Arcs:
│   ├── RECORD_OF → Entity
│   ├── FOR_LOCALE → Locale
│   ├── SIMILAR_TO → Record
│   └── PRECEDED_BY → Record
```

This means you can query past experience:

```yaml
# "What did we learn about QR codes last week?"
- id: recall
  invoke:
    tool: novanet_search
    server: novanet
    params:
      query: "QR code research"
      kinds: ["Record"]
      # Returns: [{summary: "...", confidence: 0.92, timestamp: "2026-03-10"}]
```

---

## Feature 6: Runtime Introspection

### The Problem

Today, an `agent:` task is blind — it can't see what happened before it in the workflow:

```yaml
- id: my_agent
  agent:
    prompt: "Generate content"
    # The agent has NO IDEA:
    # - What other tasks produced
    # - How much budget is left
    # - What the DAG looks like
    # - What previous records contain
```

### The Solution

6 new builtin tools that let agents query the workflow's runtime state:

```yaml
- id: smart_agent
  agent:
    prompt: "Generate content, adapting to what came before"
    tools:
      - nika:records          # "What did previous tasks produce?"
      - nika:threads          # "What tasks are running/completed?"
      - nika:orchestrate            # "What round are we on? Budget left?"
      - nika:cost             # "How many tokens/dollars spent so far?"
      - nika:dag_info         # "What tasks come after me?"
      - nika:task_status      # "Did task X succeed?"
```

### What each tool returns

| Tool | Returns | Example Use |
|------|---------|-------------|
| `nika:records` | List of all records with summaries and confidence | "Check if research was thorough enough" |
| `nika:threads` | Active, completed, and pending tasks | "Know what's left to do" |
| `nika:orchestrate` | Current round, max rounds, budget used/remaining | "Am I running out of budget?" |
| `nika:cost` | Token counts and cost per model slot | "Switch to cheaper model if over budget" |
| `nika:dag_info` | Predecessors, successors, critical path | "Understand my position in the workflow" |
| `nika:task_status` | Single task's status and record | "Check if dependency succeeded" |

### Example: Cost-Aware Agent

```yaml
- id: cost_aware_writer
  agent:
    prompt: |
      Write landing page sections.
      Check your budget with nika:cost before each section.
      If budget is > 80% spent, use shorter summaries.
      If budget is > 95% spent, stop and output what you have.
    tools:
      - nika:cost
      - nika:records
      - nika:write
```

---

## Before vs After

### Complete Side-by-Side

Here's the SAME use case (generate a landing page) in v0.27 vs v0.30:

<table>
<tr><th>v0.27 (today)</th><th>v0.30 (target)</th></tr>
<tr>
<td>

```yaml
schema: nika/workflow@0.11
provider: anthropic

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "-p", "novanet-mcp"]

tasks:
  - id: ctx
    invoke:
      tool: novanet_context
      server: novanet
      params:
        focus_key: "homepage"
        locale: "fr-FR"
        mode: page

  - id: research
    use:
      ctx: $ctx
    infer: "Research trends: {{use.ctx}}"

  - id: hero
    use:
      ctx: $ctx
      research: $research
    infer: "Write hero: {{use.ctx}}
            {{use.research}}"

  - id: features
    use:
      ctx: $ctx
      research: $research
    infer: "Write features: {{use.ctx}}
            {{use.research}}"

  - id: assemble
    use:
      hero: $hero
      features: $features
    infer: "Assemble: {{use.hero}}
            {{use.features}}"
```

</td>
<td>

```yaml
schema: nika/workflow@0.13
goal:

model_slots:
  pythagoras: { provider: anthropic,
    model: claude-sonnet-4-6,
    extended_thinking: true }
  edison: { provider: anthropic,
    model: claude-sonnet-4-6 }
  york: { provider: groq,
    model: llama-3.3-70b-versatile }

goal:
  goal: |
    Generate French landing page
    for QR Code AI.
    Quality >= 0.85.
  model_slot: pythagoras
  max_rounds: 8
  record_budget: 15000

mcp:
  servers:
    novanet:
      command: cargo
      args: ["run", "-p", "novanet-mcp"]

tasks:
  - id: ctx
    invoke:
      tool: novanet_context
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
    with:
      topic: "$ctx"
    infer: "Research: {{with.topic}}"
    record:
      compress: true
      max_tokens: 300
      persist: novanet
      entity_link: qr-code

  - id: write_section
    model_slot: edison
    context_budget: 8000
    with:
      section: "$research"
      context: "$ctx"
    infer: "Write {{with.section}} using {{with.context}}"
    structured:
      schema:
        type: object
        properties:
          content: { type: string }
          word_count: { type: integer }
        required: [content]
    record:
      compress: true
      max_tokens: 800

  - id: review
    model_slot: pythagoras
    with:
      drafts: "$write_section"
    infer: "Review: {{with.drafts}}"
    structured:
      schema:
        type: object
        properties:
          score: { type: number }
          issues: { type: array, items: { type: string } }
        required: [score, issues]
    record:
      compress: true
      retain: [score, issues]
      confidence_threshold: 0.85
```

</td>
</tr>
<tr>
<td>

**Behavior:**
- 1 model for everything (expensive)
- Fixed 5 tasks, always the same
- Context grows linearly (dumb zone risk)
- Results lost after execution
- No quality feedback loop

</td>
<td>

**Behavior:**
- 3 models, right tool for each job
- Orchestrator adds tasks dynamically
- Context bounded by records + budget
- Records persisted in NovaNet
- Quality loop: review → retry if < 0.85

</td>
</tr>
</table>

---

## Feature Compatibility Matrix

All 6 features work independently. You can adopt them incrementally:

| Feature | Works alone? | Requires | Best with |
|---------|:----------:|----------|-----------|
| Model Slots | ✅ | Nothing | Any workflow |
| Records | ✅ | Nothing | Multi-step pipelines |
| Orchestrate | ❌ | Model Slots + Records | Content generation |
| Context Budget | ✅ | Nothing (better with Records) | Long pipelines |
| Memory | ❌ | Records + NovaNet | Cross-session workflows |
| Introspect | ✅ | Nothing (better with Records) | Agent tasks |

### Adoption Path

```
Level 1 — Start here:
  Add model_slots: to save money (zero risk, backward compatible)

Level 2 — Add compression:
  Add record: to tasks that pass data downstream

Level 3 — Go adaptive:
  Switch to goal: for complex workflows

Level 4 — Add memory:
  Add record.persist: novanet for cross-session learning
```

---

## FAQ

### "Is v0.30 backward compatible?"

**Yes.** All new fields are optional. A v0.27 workflow runs unchanged on v0.30. You adopt features incrementally.

### "Do I need NovaNet to use v0.30?"

**No.** Features 1-4 (model slots, records, orchestrator, context budget) work without NovaNet. Only Feature 5 (persistent memory) requires NovaNet.

### "Is orchestrate mode deterministic?"

**No.** The orchestrator makes decisions dynamically, so two runs may produce different task sequences. Use `orchestration: dag` when you need determinism.

### "How is this different from LangGraph?"

LangGraph is Python code that defines agent graphs. Nika is YAML that defines workflows. Key differences:
- Nika workflows are version-controlled YAML (auditable, reproducible)
- Nika has NovaNet integration (knowledge graph + 200 locales)
- Nika has real-time TUI for monitoring
- Nika has 5 semantic verbs (not arbitrary function nodes)

### "How is this different from CrewAI?"

CrewAI is multi-agent with role-based crews. Nika is workflow-first with optional orchestrate mode. Key differences:
- Nika's orchestrate mode is simpler (1 orchestrator + N satellite templates)
- Nika records are compressed (CrewAI passes full outputs)
- Nika has NovaNet (no competitor has a knowledge graph)

### "What's the 'dumb zone'?"

The dumb zone (term from Dex Horthy / Slate) is the point where an LLM has so much context that its performance actually **degrades**. Think of it like trying to read a 100-page document while writing — you lose track. Records and context budgets prevent this.

### "Can I mix DAG and orchestrate mode?"

Not in the same workflow. But you can have an orchestrate-mode workflow that `include:`s a DAG sub-workflow, or an `agent:` task that calls `nika:run` to execute a DAG workflow.

---

# Part 2: Technical Reference

> The complete Nika runtime, presented as a unified product.
> What it is, how it works, every feature, every mechanism.

---

## 1. What Is Nika

Nika is an AI workflow runtime written in Rust. You define tasks in a YAML file (`.nika.yaml`), Nika resolves the dependency graph, and executes them in parallel via tokio.

```mermaid
flowchart LR
    YAML["workflow.nika.yaml"] -->|parse| AST["Two-Phase AST\nRaw → Analyzed"]
    AST -->|validate| DAG["DAG Resolution\ncycle detection\ndependency sort"]
    DAG -->|execute| RT["Tokio Runtime\nparallel tasks\nfail_fast"]
    RT -->|trace| NDJSON["Trace File\n.ndjson events"]
    RT -->|output| RESULT["TaskResults\n+ Records\n+ Artifacts"]

    style YAML fill:#fef3c7,stroke:#d97706
    style AST fill:#dbeafe,stroke:#2563eb
    style DAG fill:#dcfce7,stroke:#16a34a
    style RT fill:#fce7f3,stroke:#db2777
    style NDJSON fill:#f3e8ff,stroke:#9333ea
    style RESULT fill:#ecfdf5,stroke:#059669
```

Two execution modes:

| Mode | Description | YAML field |
|------|-------------|------------|
| **dag** (default) | Static DAG — all tasks known at parse time, executed in dependency order | `orchestration: dag` or omitted |
| **orchestrate** | Dynamic — an orchestrator dispatches satellites across rounds | `goal:` |

---

## 2. Architecture Overview

```mermaid
flowchart TB
    subgraph NIKA["Nika Runtime"]
        direction TB
        CLI["CLI\nnika *.nika.yaml\nnika chat\nnika studio"]
        TUI["TUI\n3 views\nratatui"]
        LSP_MOD["LSP\nYAML validation\ncompletions"]

        subgraph CORE_EXEC["Execution Core"]
            AST_MOD["ast/\n30 files\n2-phase IR"]
            DAG_MOD["dag/\n4 files\ntoposort + DynamicDag"]
            RUNTIME["runtime/\n38 files\nexecutor + agents"]
            BINDING["binding/\n9 files\nlazy + templates"]
        end

        subgraph PROVIDERS["Provider Layer"]
            RIG["provider/rig.rs\nrig-core v0.32\n7 cloud providers"]
            NATIVE["provider/native/\nmistral.rs\nlocal GGUF"]
            SLOTS["Model Routing\n4 slots"]
        end

        subgraph IO_LAYER["I/O Layer"]
            MCP_CLIENT["mcp/\n12 files\nrmcp v0.16"]
            EVENT["event/\n4 files\nNDJSON traces"]
            ARTIFACT["io/\n5 files\natomic writes"]
            SECRETS["secrets/\n5 files\nkeychain + daemon"]
        end

        subgraph MGMT["Management (v0.27 spn fusion)"]
            PROV_CMD["nika provider\nAPI keys"]
            MODEL_CMD["nika model\nlocal models"]
            MCP_CMD["nika mcp\n100 aliases"]
            SYNC_CMD["nika sync\neditors"]
            JOBS_CMD["nika jobs\nbackground"]
        end
    end

    subgraph NOVANET["NovaNet (Knowledge Graph)"]
        NEO4J["Neo4j\n59 NodeClasses\n159 ArcClasses"]
        MCP_SRV["MCP Server\n7 tools"]
    end

    MCP_CLIENT <-->|"JSON-RPC 2.0\nstdio"| MCP_SRV
    MCP_SRV --> NEO4J

    style NIKA fill:#f8fafc,stroke:#334155
    style CORE_EXEC fill:#dbeafe,stroke:#2563eb
    style PROVIDERS fill:#fef3c7,stroke:#d97706
    style IO_LAYER fill:#dcfce7,stroke:#16a34a
    style MGMT fill:#f3e8ff,stroke:#9333ea
    style NOVANET fill:#ecfdf5,stroke:#059669
```

### The Brain & Body Pattern

| Role | System | Responsibility |
|------|--------|----------------|
| **Brain** | NovaNet | Knows things — entities, locales, knowledge atoms, SEO data, denomination forms |
| **Body** | Nika | Does things — runs workflows, calls LLMs, executes shell commands, manages tools |
| **Nervous System** | MCP | Connects them — JSON-RPC 2.0 protocol, 7 tools, bidirectional |

**The Golden Rule**: KNOWING goes to NovaNet. DOING goes to Nika. CONNECTING uses MCP.

---

## 3. The 5 Semantic Verbs

Every task uses exactly one verb. No more, no less.

```mermaid
flowchart LR
    subgraph VERBS["5 Semantic Verbs"]
        direction TB
        INFER["infer:\nLLM generation"]
        EXEC["exec:\nShell command"]
        FETCH["fetch:\nHTTP request"]
        INVOKE["invoke:\nMCP tool call"]
        AGENT["agent:\nMulti-turn loop"]
    end

    INFER -->|"rig-core"| LLM["7 Cloud Providers\n+ Native (GGUF)"]
    EXEC -->|"shlex"| SHELL["OS Process\nshell:false default"]
    FETCH -->|"reqwest"| HTTP["HTTP/S\nGET/POST/PUT/DELETE"]
    INVOKE -->|"rmcp"| MCP_T["MCP Servers\nJSON-RPC 2.0"]
    AGENT -->|"rig AgentBuilder"| LOOP["Agent Loop\ntools + records"]

    style VERBS fill:#dbeafe,stroke:#2563eb
```

### Verb Reference

| Verb | Purpose | Shorthand | Full Form |
|------|---------|-----------|-----------|
| `infer:` | One-shot LLM call | `infer: "prompt"` | `infer: { prompt, provider, model, temperature, system, max_tokens, response_format, extended_thinking }` |
| `exec:` | Shell command | `exec: "command"` | `exec: { command, shell, env, timeout }` |
| `fetch:` | HTTP request | -- | `fetch: { url, method, headers, body, json }` |
| `invoke:` | MCP tool call | -- | `invoke: { tool, server, params }` |
| `agent:` | Multi-turn agentic loop | -- | `agent: { prompt, system, provider, model, mcp, tools, max_turns, depth_limit, strategy, ... }` |

### YAML Examples

```yaml
# infer: — shorthand
- id: headline
  infer: "Generate a headline for QR Code AI"

# infer: — full form with LLM controls
- id: creative
  infer:
    prompt: "Write a tagline"
    provider: anthropic
    model: claude-sonnet-4-6
    temperature: 0.9
    system: "You are a creative copywriter"
    max_tokens: 100
    extended_thinking: true
    thinking_budget: 8192

# exec: — with env injection
- id: build
  exec:
    command: "npm run build"
    env:
      NODE_ENV: production
    shell: true  # Opt-in for pipes/redirects

# fetch: — with JSON body
- id: api_call
  fetch:
    url: "https://api.example.com/data"
    method: POST
    headers:
      Authorization: "Bearer $token"
    json:
      query: "QR code trends"

# invoke: — MCP tool call
- id: get_context
  invoke: novanet_context
  params:
    focus_key: "qr-code"
    locale: "fr-FR"
    mode: "page"

# agent: — multi-turn with tools
- id: researcher
  agent:
    prompt: "Research QR code trends and write a summary"
    provider: anthropic
    model: claude-sonnet-4-6
    mcp: [novanet]
    tools: [nika:read, nika:write, nika:glob]
    max_turns: 20
    depth_limit: 3
    completion:
      mode: explicit  # Must call nika:complete to finish
```

---

## 4. DAG Execution Engine

```mermaid
flowchart TB
    subgraph PARSE["Phase 1: Parse"]
        YAML_IN["YAML file"] --> RAW["Raw AST\nast/raw/"]
        RAW -->|"analyze"| ANALYZED["Analyzed AST\nast/analyzed/"]
    end

    subgraph VALIDATE["Phase 2: Validate"]
        ANALYZED --> TOPO["Topological Sort\ndag/stable.rs"]
        TOPO --> CYCLE["Cycle Detection\ndag/validate.rs"]
        CYCLE --> FLOW["Data Flow Analysis\ndag/flow.rs"]
    end

    subgraph EXECUTE["Phase 3: Execute"]
        FLOW --> SCHED["Scheduler\ntokio::spawn"]
        SCHED --> T1["Task A"]
        SCHED --> T2["Task B"]
        SCHED --> T3["Task C\nwaits on A"]
        T1 -->|"complete"| T3
        T2 -->|"complete"| T4["Task D\nwaits on B+C"]
        T3 -->|"complete"| T4
    end

    style PARSE fill:#dbeafe,stroke:#2563eb
    style VALIDATE fill:#fef3c7,stroke:#d97706
    style EXECUTE fill:#dcfce7,stroke:#16a34a
```

### Two-Phase IR Architecture

| Phase | Module | Input | Output |
|-------|--------|-------|--------|
| **Raw Parse** | `ast/raw/parser.rs` | YAML string | `RawWorkflow` — unvalidated struct with raw strings |
| **Analysis** | `ast/analyzer/analyze.rs` | `RawWorkflow` | `AnalyzedWorkflow` — resolved IDs, validated deps, typed actions |

### Parallel Execution

Tasks without dependencies execute concurrently via `tokio::spawn`. When a task completes, its dependents are released.

### fail_fast Behavior

When `fail_fast: true` (default):

```mermaid
sequenceDiagram
    participant S as Scheduler
    participant A as Task A
    participant B as Task B (depends on A)
    participant C as Task C (independent)

    S->>A: spawn
    S->>C: spawn
    A->>A: FAILS
    A-->>S: TaskFailed
    S->>C: tokio::select! cancel
    Note over C: TaskStatus::Skipped
    Note over B: TaskStatus::DependencyFailed
    S-->>S: WorkflowFailed
```

- `tokio::select!` races semaphore acquisition against a cancellation token
- In-flight tasks are cancelled immediately
- Unstarted dependents get `DependencyFailed { dependency: "task_a" }`
- Distinguishes true deadlock (NIKA-025) from dependency chain failure (NIKA-026)

### for_each Parallelism

```yaml
- id: generate_pages
  for_each: ["fr-FR", "en-US", "de-DE"]
  as: locale
  concurrency: 5
  fail_fast: true
  infer: "Generate page in {{use.locale}}"
```

Creates N task instances running in a `tokio::JoinSet` with bounded concurrency.

### DynamicDag (v0.30 — orchestrate mode)

In `goal:` mode, the DAG is mutable at runtime. The orchestrator can dispatch new satellites that get inserted into the live DAG.

| DAG Type | When | Mutability |
|----------|------|------------|
| `StableDag` | `orchestration: dag` (default) | Immutable after parse |
| `DynamicDag` | `goal:` | Tasks added at runtime by the orchestrator |

Source: `dag/stable.rs` (existing), `dag/dynamic.rs` (v0.30)

---

## 5. Model Routing — 4 Slots

Route different cognitive tasks to different providers/models within a single workflow.

```mermaid
flowchart LR
    subgraph SLOTS["model_slots:"]
        EDISON["edison\nclaude-sonnet\n$$$$"]
        ATLAS["atlas\nllama-3.3-70b\n$"]
        YORK["york\ndeepseek-chat\n$$"]
        PYTHAGORAS["pythagoras\nclaude + thinking\n$$$$$"]
    end

    subgraph TASKS["Task Routing"]
        T1["plan → pythagoras"]
        T2["generate → edison"]
        T3["classify → atlas"]
        T4["research → york"]
    end

    PYTHAGORAS -.-> T1
    EDISON -.-> T2
    ATLAS -.-> T3
    YORK -.-> T4

    style SLOTS fill:#dbeafe,stroke:#2563eb
    style TASKS fill:#fef3c7,stroke:#d97706
```

### YAML Configuration

```yaml
schema: nika/workflow@0.12

model_slots:
  edison:
    provider: anthropic
    model: claude-sonnet-4-6
    # Primary content generation, complex reasoning

  atlas:
    provider: groq
    model: llama-3.3-70b-versatile
    # Simple decisions, classifications, formatting

  york:
    provider: deepseek
    model: deepseek-chat
    # Research, search synthesis, information retrieval

  pythagoras:
    provider: anthropic
    model: claude-sonnet-4-6
    extended_thinking: true
    thinking_budget: 16384
    # Strategy, planning, review, critique

default_model_slot: edison

tasks:
  - id: plan
    model_slot: pythagoras
    infer: "Create a content plan"

  - id: classify
    model_slot: atlas
    infer: "Is this about QR codes? Answer yes/no"

  - id: generate
    model_slot: edison
    infer: "Generate the landing page"
```

### Provider Inventory

| Provider | Type | Models | Use Case |
|----------|------|--------|----------|
| **Anthropic** | Cloud | Claude Sonnet, Haiku, Opus | General purpose, reasoning |
| **OpenAI** | Cloud | GPT-4o, GPT-4o-mini | General purpose |
| **Mistral** | Cloud | Mistral Large, Medium, Small | European AI |
| **Groq** | Cloud | Llama 3.3 70B, Mixtral | Fast inference |
| **DeepSeek** | Cloud | DeepSeek-Chat, DeepSeek-R1 | Reasoning, search |
| **Google** | Cloud | Gemini 2.0 Flash, Pro | Multimodal, grounded search |
| **Native** | Local | Any GGUF model via mistral.rs | Zero API key, Metal/CUDA |

### Cost Impact

Model routing enables significant cost reduction:

```
WITHOUT routing (v0.27):
  All tasks use Claude Sonnet → $$$$ per task
  200 pages × 5 locales × 5 tasks = 5,000 calls at $$$$ each

WITH routing (v0.30):
  Planning (5%) → pythagoras ($$$$)
  Generation (35%) → edison ($$$$)
  Classification/formatting (60%) → atlas ($)

  Result: ~60% cost reduction on same pipeline
```

Source: `provider/rig.rs` (existing), `ast/raw/model_slot.rs` (v0.28)

---

## 6. Agent System

The `agent:` verb creates a multi-turn execution loop where the LLM iterates with tool access until task completion.

```mermaid
sequenceDiagram
    participant RT as Nika Runtime
    participant LLM as LLM Provider
    participant TOOLS as Tool Layer

    RT->>LLM: System prompt + Goal + Available tools
    loop Each Turn (max_turns)
        LLM->>TOOLS: tool_call(name, args)
        TOOLS-->>LLM: tool_result
        Note over LLM: Decide: more tools or done?
    end
    LLM-->>RT: Final response or nika:complete
    RT->>RT: Compress to Record
```

### Tool Inventory

**12 Builtin Tools (7 core + 5 file):**

| Tool | Category | Description |
|------|----------|-------------|
| `nika:sleep` | Core | Pause execution (max 5 min) |
| `nika:log` | Core | Emit log event at level |
| `nika:emit` | Core | Emit custom event to trace |
| `nika:assert` | Core | Validate condition, fail if false |
| `nika:prompt` | Core | Human-in-the-loop user input |
| `nika:run` | Core | Execute nested workflow |
| `nika:complete` | Core | Signal agent task completion |
| `nika:read` | File | Read file with line numbers |
| `nika:write` | File | Create/overwrite file |
| `nika:edit` | File | Modify file (old_string → new_string) |
| `nika:glob` | File | Find files by pattern |
| `nika:grep` | File | Search content with regex |

**6 Introspection Tools (v0.30):**

| Tool | Returns |
|------|---------|
| `nika:records` | Accumulated records `[{task_id, summary, confidence, tokens}]` |
| `nika:threads` | Active/completed threads `[{task_id, status, model_slot}]` |
| `nika:orchestrate` | Orchestration progress `{round, max_rounds, budget_used, budget_total}` |
| `nika:cost` | Token usage and cost report `{total_tokens, total_cost, per_model}` |
| `nika:dag_info` | DAG structure `{predecessors, successors, critical_path}` |
| `nika:task_status` | Individual task status `{task_id, status, record}` |

**+ All MCP tools** from configured servers (NovaNet, Neo4j, GitHub, Slack, etc.)

### spawn_agent — Nested Agents

An agent can launch sub-agents via the `spawn_agent` internal tool.

```mermaid
flowchart TB
    AGENT["Agent (depth=3)\nprompt: 'Research and summarize'"]
    SUB1["Sub-agent (depth=2)\nprompt: 'Fetch arxiv papers'"]
    SUB2["Sub-agent (depth=2)\nprompt: 'Parse results'"]
    BLOCK["BLOCKED (depth=0)\nspawn_agent refused"]

    AGENT -->|spawn_agent| SUB1
    AGENT -->|spawn_agent| SUB2
    SUB1 -->|spawn_agent| SUB3["Sub-sub (depth=1)"]
    SUB3 -->|"spawn_agent"| BLOCK

    style AGENT fill:#dbeafe,stroke:#2563eb
    style SUB1 fill:#e0f2fe,stroke:#0284c7
    style SUB2 fill:#e0f2fe,stroke:#0284c7
    style SUB3 fill:#f0f9ff,stroke:#38bdf8
    style BLOCK fill:#fee2e2,stroke:#dc2626
```

- `depth_limit` decremented at each spawn (default: 3, max: 10)
- Sub-agent inherits parent's MCP connections
- Result returned to parent agent

### decompose: — Runtime DAG Expansion

The agent can call `nika:decompose` to break a task into sub-tasks at runtime. The DAG is re-resolved dynamically. Used when the agent discovers the problem is more complex than anticipated.

### Completion Modes

| Mode | Behavior |
|------|----------|
| `natural` (default) | Completes when LLM has no more tool calls |
| `explicit` | Agent must call `nika:complete` tool |
| `pattern` | Completes on pattern match in output |

Source: `runtime/rig_agent_loop/` (7 files), `runtime/builtin/` (13 files), `runtime/spawn.rs`

---

## 7. Record Engine

Records are compressed representations of task execution, generated at the natural completion boundary. Downstream tasks receive records, not raw output.

```mermaid
stateDiagram-v2
    [*] --> Executing: Task starts
    Executing --> Completed: Task finishes
    Completed --> Compressing: record.compress = true
    Compressing --> RecordStored: atlas LLM summarizes
    Completed --> RawStored: record.compress = false

    RecordStored --> [*]: summary + key_findings + confidence
    RawStored --> [*]: raw TaskResult (legacy behavior)
```

### Record Data Structure

```rust
pub struct Record {
    pub task_id: TaskId,
    pub summary: String,           // LLM-compressed summary
    pub key_findings: Vec<String>, // Extracted key points
    pub raw_output: Option<String>,// Debug only, not passed downstream
    pub model_used: String,        // Which model produced this
    pub tokens_spent: u64,         // Cost tracking
    pub confidence: f64,           // Self-assessed 0.0-1.0
    pub artifacts: Vec<Artifact>,  // Files produced
}
```

### YAML Configuration

```yaml
tasks:
  - id: research
    model_slot: york
    infer: "Research QR code trends in 2026"
    record:
      compress: true            # Generate record summary
      retain: [key_findings]    # What to extract from raw output
      max_tokens: 500           # Record summary size limit
      confidence_threshold: 0.8 # Orchestrator can escalate if below
```

### How It Works

1. Task executes normally (any verb)
2. At completion, if `record.compress: true`:
   - The raw output is sent to the **atlas** model slot (cheap, fast)
   - The LLM produces a structured summary + key_findings + confidence score
   - The Record struct replaces the raw TaskResult for downstream bindings
3. Downstream tasks referencing this task via `$research` get the record, not the raw output

### Impact on Context Growth

```
WITHOUT records:
  Task A output: 2,000 tokens
  Task B receives: 2,000 tokens (full A)
  Task C receives: 4,000 tokens (A + B)
  Task D receives: 6,000+ tokens → context degradation

WITH records:
  Task A → Record: 300 tokens
  Task B receives: 300 tokens
  Task C receives: 600 tokens (A_rec + B_rec)
  Task D receives: relevant records only → always within budget
```

Source: `runtime/record.rs`, `runtime/record_compress.rs` (v0.28)

---

## 8. Orchestrate Mode

A new execution mode where an orchestrator dynamically dispatches satellites based on the goal and accumulated records.

```mermaid
sequenceDiagram
    participant STR as Orchestrator (pythagoras slot)
    participant R as research (york slot)
    participant W as write_section (edison slot)
    participant V as review (pythagoras slot)

    Note over STR: Round 1 — Gather information
    STR->>R: dispatch(topic="QR trends 2026")
    R-->>STR: Record{summary, confidence: 0.9}

    Note over STR: Round 2 — Generate content (parallel)
    STR->>W: dispatch(section="hero")
    STR->>W: dispatch(section="features")
    W-->>STR: Record{content: hero_draft}
    W-->>STR: Record{content: features_draft}

    Note over STR: Round 3 — Review
    STR->>V: dispatch(draft=hero+features)
    V-->>STR: Record{issues: [...], score: 0.85}

    Note over STR: Round 4 — Synthesize
    STR->>STR: All records assembled
    Note over STR: DONE — complete page generated
```

### YAML Configuration

```yaml
schema: nika/workflow@0.13
workflow: landing-page-generator

goal:              # Enables orchestrator/satellites mode

model_slots:
  pythagoras: { provider: anthropic, model: claude-sonnet-4-6, extended_thinking: true }
  edison: { provider: anthropic, model: claude-sonnet-4-6 }
  york: { provider: groq, model: llama-3.3-70b-versatile }
  atlas: { provider: deepseek, model: deepseek-chat }

goal:
  goal: "Generate a complete French landing page for QR Code AI"
  model_slot: pythagoras
  max_rounds: 10
  record_budget: 15000            # Total token budget across all records

# Satellite templates — dispatched dynamically by the orchestrator
tasks:
  - id: research
    model_slot: york
    infer: "Research: {{with.topic}}"
    record: { compress: true, max_tokens: 300 }

  - id: write_section
    model_slot: edison
    infer: "Write: {{with.section}} using context: {{with.context}}"
    record: { compress: true, retain: [content], max_tokens: 800 }

  - id: review
    model_slot: pythagoras
    infer: "Review and critique: {{with.draft}}"
    record: { compress: true, retain: [issues, suggestions] }
```

### DAG vs Orchestrate — When to Use Which

| Criterion | DAG Mode | Orchestrate Mode |
|-----------|----------|------------|
| Tasks known at design time | Yes | No — discovered at runtime |
| Execution order | Fixed by depends_on | Dynamic per round |
| Stopping condition | All tasks done | orchestrator says "DONE" |
| Cost | Predictable | Variable (max_rounds bounds it) |
| Inter-task data | Raw bindings or records | Always records |
| Use case | Pipelines, ETL, deterministic flows | Creative generation, research, iterative refinement |

### Components

| Component | File | Purpose |
|-----------|------|---------|
| `Orchestrator` | `runtime/orchestrator.rs` | Main loop — sends goal + records to the orchestrator, dispatches satellites |
| `SatelliteTemplate` | `runtime/satellite.rs` | Parsed task definitions available as satellites |
| `SatelliteInstance` | `runtime/satellite.rs` | Concrete instantiation with runtime parameters |
| `DynamicDag` | `dag/dynamic.rs` | Mutable DAG that accepts new tasks at runtime |

Source: `runtime/orchestrator.rs`, `runtime/satellite.rs`, `dag/dynamic.rs` (v0.29)

---

## 9. Context Budget Management

Working memory awareness at the runtime level. Each task declares its context budget. The runtime enforces this via record summaries.

```mermaid
flowchart TB
    subgraph BEFORE["Without Context Budgets"]
        B1["Task A output\n2,000 tokens"] --> B2["Task B\n2,000 tokens context"]
        B2 --> B3["Task C\n4,000 tokens"]
        B3 --> B4["Task D\n6,000+ tokens"]
        B4 --> B5["Context degradation\nLLM in 'dumb zone'"]
    end

    subgraph AFTER["With Context Budgets"]
        A1["Task A → Record\n300 tokens"] --> A2["Task B\n300 tokens context"]
        A2 --> A3["Task C\n600 tokens"]
        A3 --> A4["Task D\nrelevant records only"]
        A4 --> A5["Always within budget"]
    end

    style BEFORE fill:#fee2e2,stroke:#dc2626
    style AFTER fill:#dcfce7,stroke:#16a34a
    style B5 fill:#dc2626,color:#fff
    style A5 fill:#16a34a,color:#fff
```

### YAML Configuration

```yaml
tasks:
  - id: research
    model_slot: york
    context_budget: 4000        # Max tokens in this task's context
    infer: "Research QR code trends"
    record:
      compress: true
      max_tokens: 300           # Record must fit in 300 tokens

  - id: generate
    model_slot: edison
    context_budget: 8000        # Larger budget for generation
    with:
      trends: "$research"       # Receives record, not raw output
    infer: "Generate landing page based on: {{with.trends}}"
```

### Rules

1. Each task receives ONLY: its prompt + relevant records + NovaNet context
2. Never raw history from other tasks
3. `context_budget` enforced by runtime (truncate/warn if exceeded)
4. In orchestrate mode, the orchestrator selects which records to include per round
5. Token budget tracked in events for observability

### Agent Budget Awareness

Agents can call `nika:get_budget` to check remaining tokens and adapt:

```
Agent: "I have 30% budget remaining → I'll simplify my approach"
Agent: "Budget critical → switching to atlas model"
Agent: "Insufficient budget for spawn_agent → handling inline"
```

Source: `runtime/context_budget.rs` (v0.29), `ast/raw/task.rs`

---

## 10. Structured Output Pipeline

When an `infer:` or `agent:` task declares an `output.schema` (JSON Schema), Nika enforces the structure through a 4-layer pipeline.

```mermaid
flowchart TB
    LLM_OUT["LLM Output"] --> L1{"Layer 1\nValidate"}
    L1 -->|"valid"| DONE["TaskCompleted"]
    L1 -->|"invalid"| L2{"Layer 2\nRetry (×3)"}
    L2 -->|"valid"| DONE
    L2 -->|"still invalid"| L3{"Layer 3\nLLM Repair"}
    L3 -->|"valid"| DONE
    L3 -->|"still invalid"| L4["Layer 4\nTaskFailed"]

    style L1 fill:#dcfce7,stroke:#16a34a
    style L2 fill:#fef3c7,stroke:#d97706
    style L3 fill:#fce7f3,stroke:#db2777
    style L4 fill:#fee2e2,stroke:#dc2626
    style DONE fill:#ecfdf5,stroke:#059669
```

| Layer | Mechanism | Model Used |
|-------|-----------|------------|
| **1 — Validate** | JSON Schema validation against output | None |
| **2 — Retry** | Re-prompt LLM with validation error | Same as task (edison slot) |
| **3 — Repair** | Separate LLM call to fix invalid JSON | Atlas slot (cheap) |
| **4 — Fallback** | Task fails with structured error | None |

### YAML Configuration

```yaml
- id: generate
  infer: "Generate product data"
  output:
    schema:
      type: object
      properties:
        title: { type: string }
        description: { type: string }
        price: { type: number }
      required: [title, description, price]
```

No silent truncation. No corrupted data. The pipeline guarantees valid JSON or explicit failure.

Source: `runtime/structured_output.rs`, `ast/output.rs`, `ast/structured.rs`

---

## 11. NovaNet Integration (MCP)

Nika connects to NovaNet exclusively via MCP protocol. Zero Cypher in Nika.

```mermaid
flowchart LR
    subgraph NIKA_SIDE["Nika (MCP Client)"]
        INV["invoke: novanet_*"]
        AGT["agent: with mcp: [novanet]"]
    end

    subgraph MCP_PROTO["MCP Protocol"]
        JSONRPC["JSON-RPC 2.0\nover stdio"]
    end

    subgraph NOVANET_SIDE["NovaNet (MCP Server)"]
        SEARCH["novanet_search\nfind nodes"]
        CONTEXT["novanet_context\nassemble LLM context"]
        DESCRIBE["novanet_describe\nschema overview"]
        INTROSPECT["novanet_introspect\nclass/arc details"]
        WRITE["novanet_write\ncreate/update"]
        AUDIT["novanet_audit\nquality checks"]
        BATCH["novanet_batch\nparallel ops"]
        QUERY["novanet_query\nraw Cypher (last resort)"]
    end

    INV --> JSONRPC
    AGT --> JSONRPC
    JSONRPC --> SEARCH
    JSONRPC --> CONTEXT
    JSONRPC --> DESCRIBE
    JSONRPC --> INTROSPECT
    JSONRPC --> WRITE
    JSONRPC --> AUDIT
    JSONRPC --> BATCH
    JSONRPC --> QUERY

    style NIKA_SIDE fill:#dbeafe,stroke:#2563eb
    style MCP_PROTO fill:#fef3c7,stroke:#d97706
    style NOVANET_SIDE fill:#dcfce7,stroke:#16a34a
```

### NovaNet Tool Reference

| Tool | Mode | Purpose |
|------|------|---------|
| `novanet_search` | fulltext, property, hybrid, walk, triggers | Find nodes, traverse graph |
| `novanet_context` | page, block, knowledge, assemble | Build LLM generation context |
| `novanet_describe` | schema, entity, category, relations, locales, stats | Understand the graph |
| `novanet_introspect` | classes, class, arcs, arc | Schema details with relationships |
| `novanet_write` | upsert_node, create_arc, update_props | Mutate graph (dry_run to validate) |
| `novanet_audit` | coverage, orphans, integrity, freshness, all | Data quality checks |
| `novanet_batch` | parallel operations | Multiple ops in one call |
| `novanet_query` | raw Cypher | Analytics only (LAST RESORT) |

### NovaNet Knowledge Structure

```mermaid
flowchart TB
    subgraph SHARED["Shared Realm (36 nodes, read-only)"]
        LOCALE["Locale\nfr-FR, en-US, ja-JP..."]
        GEO["Geography\ncountries, regions"]
        KNOWLEDGE["Knowledge\nterms, expressions, patterns"]
    end

    subgraph ORG["Org Realm (23 nodes)"]
        ENTITY["Entity\nqr-code, barcode..."]
        ENTITY_NATIVE["EntityNative\nfr-FR content for qr-code"]
        PAGE["Page\nhomepage, features..."]
        PAGE_NATIVE["PageNative\nfr-FR generated page"]
        BLOCK["Block\nhero, features-list..."]
    end

    ENTITY -->|HAS_NATIVE| ENTITY_NATIVE
    ENTITY_NATIVE -->|FOR_LOCALE| LOCALE
    PAGE -->|REPRESENTS| ENTITY
    PAGE -->|HAS_NATIVE| PAGE_NATIVE
    PAGE -->|HAS_BLOCK| BLOCK

    style SHARED fill:#e0f2fe,stroke:#0284c7
    style ORG fill:#fef3c7,stroke:#d97706
```

### Key Concepts

| Concept | Description |
|---------|-------------|
| **Entity** | Semantic concept (e.g., "qr-code") — defined, universal |
| **EntityNative** | Locale-specific content for an entity (e.g., fr-FR text for "qr-code") — authored |
| **Page** | URL-owning structure — owns slug, has blocks |
| **PageNative** | Generated locale-specific page content |
| **Denomination Forms** | 6 canonical forms per entity: text, title, abbrev, mixed, base, url |
| **Knowledge Atoms** | Locale knowledge: expressions, patterns, culture refs, taboos, audience traits |

### MCP Client Architecture

| Component | File | Purpose |
|-----------|------|---------|
| `McpClient` | `mcp/client.rs` | Main client with DashMap + OnceCell caching |
| `ConnectionPool` | `mcp/pool.rs` | Pool for multiple MCP servers |
| `RetryPolicy` | `mcp/retry.rs` | Retry logic for transient failures |
| `SchemaCache` | `mcp/validation/schema_cache.rs` | Cache JSON schemas from tool definitions |
| `ParameterValidator` | `mcp/validation/validator.rs` | Validate params before sending |

**Timeouts:**
- `INVOKE_TASK_DEADLINE`: 5 minutes per MCP operation
- `RECONNECT_TIMEOUT`: Auto-reconnect if server crashes
- JSON-RPC error codes preserved from server

Source: `mcp/` (12 files)

---

## 12. Persistent Records

Records compressed at task completion don't die with the workflow. Nika persists them in NovaNet's knowledge graph, enabling cross-session learning.

```mermaid
flowchart LR
    subgraph SESSION1["Session 1"]
        T1["research(qr-code, fr-FR)"] --> EP1["Record\ncompressed"]
    end

    EP1 -->|"novanet_write"| KG

    subgraph KG["NovaNet Knowledge Graph"]
        AE["Record\nsummary, findings,\nconfidence, tokens"]
        AE -->|RECORD_OF| ENT["Entity\nqr-code"]
        AE -->|FOR_LOCALE| LOC["Locale\nfr-FR"]
        AE -->|PRECEDED_BY| AE_OLD["Previous\nRecord"]
    end

    subgraph SESSION2["Session 2"]
        T2["generate(qr-code, fr-FR)"]
    end

    KG -->|"novanet_search\n(recall records)"| T2

    style SESSION1 fill:#dbeafe,stroke:#2563eb
    style KG fill:#dcfce7,stroke:#16a34a
    style SESSION2 fill:#fef3c7,stroke:#d97706
```

### How It Works

1. Task completes → record compressed (P-RECORD)
2. If `record.persist: novanet`, the record is written to NovaNet via `novanet_write`
3. NovaNet stores it as a `Record` node, linked to the relevant Entity and Locale
4. On next run, Nika calls `novanet_search` to recall relevant records
5. Recalled records are injected into the agent's context

### Record Node (NovaNet Schema)

```
Record (NodeClass, org realm, agent layer)
├── key: string                 # Unique identifier
├── workflow: string            # Source workflow name
├── task_id: string             # Source task
├── summary: string             # Compressed record
├── key_findings: string[]      # Extracted points
├── model_used: string          # Which model produced this
├── tokens_spent: integer       # Cost
├── confidence: float           # Self-assessed 0.0-1.0
├── timestamp: datetime         # When created
└── Arcs:
    ├── RECORD_OF → Entity      # Semantic link
    ├── FOR_LOCALE → Locale     # Locale-specific
    ├── SIMILAR_TO → Record
    └── PRECEDED_BY → Record (temporal chain)
```

### Cross-Session Learning

```
Run 1: Agent generates fr-FR content for "qr-code"
  → Tone too formal, user corrects to casual
  → Record: "tone was too formal, user preferred casual register"

Run 2: Agent receives this record in context
  → Adapts tone directly, no repeated mistake
  → Record: "casual tone applied successfully, user approved"

Run 3: Agent has both records
  → Knows the pattern, applies immediately
```

Memory is scoped per entity × locale — no cross-locale pollution.

### YAML Configuration

```yaml
tasks:
  - id: research
    infer: "Research QR code trends"
    record:
      compress: true
      persist: novanet          # Store in NovaNet
      entity_link: qr-code     # Link to semantic entity
```

Source: `runtime/record_memory.rs` (v0.30), `mcp/client.rs`

---

## 13. Runtime Introspection

6 read-only builtin tools that let agents examine their own state.

```mermaid
flowchart TB
    AGENT["Agent running in workflow"]

    AGENT -->|"nika:records"| EP["Records accumulated\nfrom completed tasks"]
    AGENT -->|"nika:threads"| TH["Active/completed threads\nwith status + model_slot"]
    AGENT -->|"nika:orchestrate"| SS["Orchestration progress\nround, budget used/total"]
    AGENT -->|"nika:cost"| CO["Token usage report\ntotal, per-model, per-task"]
    AGENT -->|"nika:dag_info"| DG["DAG structure\npredecessors, successors, path"]
    AGENT -->|"nika:task_status"| TS["Individual task\nstatus + record"]

    style AGENT fill:#dbeafe,stroke:#2563eb
    style EP fill:#fef3c7,stroke:#d97706
    style TH fill:#fef3c7,stroke:#d97706
    style SS fill:#fef3c7,stroke:#d97706
    style CO fill:#fef3c7,stroke:#d97706
    style DG fill:#fef3c7,stroke:#d97706
    style TS fill:#fef3c7,stroke:#d97706
```

### Why This Matters

Agents make better decisions when they can observe their own state:

| Observation | Agent Decision |
|-------------|----------------|
| "30% budget remaining" | Simplify approach, use atlas model |
| "Previous task failed at confidence 0.4" | Change approach, retry with more context |
| "3 sub-agents already spawned" | Don't spawn a 4th, handle inline |
| "Round 7 of 10 in orchestrate mode" | Start synthesizing, stop researching |
| "DAG critical path has 2 remaining tasks" | Focus on unblocking them |

These tools join the existing 12 builtins, bringing the total to **18 builtin tools**.

Source: `runtime/builtin/` (v0.30 additions)

---

## 14. Binding System & Data Flow

The binding system connects task outputs to task inputs.

```mermaid
flowchart LR
    TA["Task A\nuse.ctx: result_a"] -->|"$result_a"| TB["Task B\nwith: { data: '$result_a' }"]
    TA -->|"record"| TC["Task C\ngets Record, not raw"]

    subgraph EGGHEAD["Egghead (in-memory)"]
        DS1["result_a: { ... }"]
        DS2["result_b: { ... }"]
    end

    TA -->|store| DS1
    TB -->|store| DS2

    style EGGHEAD fill:#f3e8ff,stroke:#9333ea
```

### Binding Types

| Syntax | Type | Description |
|--------|------|-------------|
| `use.ctx: name` | Store | Store task result in Egghead under `name` |
| `$name` | Reference | Reference a stored value |
| `{{use.name}}` | Template | Inline template interpolation |
| `{{inputs.locale}}` | Input | Access workflow inputs |
| `{{context.files.brand}}` | Context | Access loaded context files |
| `for_each: $items` | Iteration | Iterate over stored array |
| `lazy: true` | Deferred | Resolve only when accessed, not at planning time |

### Lazy Bindings (ADR-006)

```yaml
- id: maybe_needed
  infer: "Generate optional content"
  use.ctx: optional_data

- id: consumer
  with:
    data:
      binding: "$optional_data"
      lazy: true              # Only resolved if {{with.data}} is actually used
  infer: "Use if needed: {{with.data}}"
```

Lazy bindings avoid loading data that may not be needed, reducing context size.

### Binding Resolution Pipeline

| Module | File | Purpose |
|--------|------|---------|
| Entry parsing | `binding/entry.rs` | Parse binding expressions |
| JSONPath | `binding/jsonpath.rs` | `$.field.subfield` access |
| Mentions | `binding/mention.rs` | `@task_id` references |
| Resolution | `binding/resolve.rs` | Resolve bindings to values |
| Templates | `binding/template.rs` | `{{var}}` interpolation |
| Transforms | `binding/transform.rs` | jq-style transforms |
| Validation | `binding/validate.rs` | Validate at parse time |

Source: `binding/` (9 files)

---

## 15. Artifact System

Secure file persistence for task outputs.

```mermaid
flowchart LR
    TASK["Task Output"] --> TEMPLATE["Template Engine\n{{task_id}}, {{date}}, {{locale}}"]
    TEMPLATE --> SECURITY["Path Validation\nno traversal, no symlinks"]
    SECURITY --> ATOMIC["Atomic Write\ntemp + fsync + rename"]
    ATOMIC --> FILE["Output File\non disk"]
    ATOMIC --> EVENT["ArtifactWritten\nevent in trace"]

    style SECURITY fill:#fee2e2,stroke:#dc2626
    style ATOMIC fill:#dcfce7,stroke:#16a34a
```

### Architecture

| Module | File | Purpose |
|--------|------|---------|
| `io::atomic` | `io/atomic.rs` | Atomic writes: temp file → fsync → rename |
| `io::security` | `io/security.rs` | Path validation, traversal prevention (`../../` blocked) |
| `io::template` | `io/template.rs` | Variable interpolation in output paths |
| `io::writer` | `io/writer.rs` | `ArtifactWriter` combining all modules |

### Security Guarantees

- **No directory traversal**: `../../etc/passwd` is rejected
- **No symlink following**: Symlinks in output paths are blocked
- **Atomic writes**: Temp file → fsync → rename prevents partial writes
- **TOCTOU mitigation**: Checks performed just before write
- **Template injection prevented**: Variables sanitized before interpolation

Source: `io/` (5 files), error codes NIKA-280 to NIKA-289

---

## 16. Security Model

```mermaid
flowchart TB
    subgraph EXEC_SECURITY["exec: Security"]
        SHLEX["shell:false (default)\nshlex parsing"]
        BLOCKLIST["Command blocklist\nrm -rf, sudo..."]
    end

    subgraph AGENT_SECURITY["Agent Security"]
        DEPTH["depth_limit\nmax spawn depth"]
        SLEEP_LIM["sleep limit\n5 min max"]
        TIMEOUT["MCP timeout\n5 min deadline"]
    end

    subgraph IO_SECURITY["I/O Security"]
        PATH["Path validation\nno traversal"]
        ATOMIC_SEC["Atomic writes\nno corruption"]
        TOCTOU["TOCTOU mitigation"]
    end

    subgraph SECRET_SECURITY["Secrets Security"]
        KEYCHAIN["OS Keychain\nnot .env files"]
        MLOCK["mlock()\nno swap"]
        ZERO["Zeroizing<T>\nauto-clear on drop"]
        PEER["Peer credentials\nsocket verification"]
    end

    subgraph YAML_SECURITY["YAML Security"]
        BOMB["YAML bomb protection\nbudget system"]
        DEPTH_Y["Max depth: 100"]
        ANCHOR["Max anchors: 200"]
        SCALAR["Max scalars: 1 MiB"]
    end

    style EXEC_SECURITY fill:#fee2e2,stroke:#dc2626
    style AGENT_SECURITY fill:#fef3c7,stroke:#d97706
    style IO_SECURITY fill:#dbeafe,stroke:#2563eb
    style SECRET_SECURITY fill:#dcfce7,stroke:#16a34a
    style YAML_SECURITY fill:#f3e8ff,stroke:#9333ea
```

### Security Summary

| Vector | Protection | Error Code |
|--------|-----------|------------|
| Shell injection | `shell: false` default, shlex parsing | NIKA-053 |
| Directory traversal | Path validation in artifacts | NIKA-280+ |
| YAML bombs | Budget system (depth, anchors, aliases, scalars) | Parse error |
| Runaway agents | `depth_limit`, `max_turns`, `token_budget` | Agent timeout |
| Unbounded sleep | 5 minute max on `nika:sleep` | NIKA tool error |
| MCP hangs | 5 minute deadline per operation | Timeout |
| Secret exposure | OS Keychain, mlock(), Zeroizing<T>, daemon socket 0600 | N/A |
| Blocked commands | Blocklist for dangerous binaries | NIKA-053 |

Source: `runtime/security.rs`, `ast/budget.rs`, `io/security.rs`, `secrets/`

---

## 17. Observability & Traces

Every workflow run produces a NDJSON trace file with structured events.

### Event Inventory (22+ event types)

| Event | When |
|-------|------|
| `WorkflowStarted` | Workflow begins |
| `WorkflowCompleted` | All tasks done |
| `WorkflowFailed` | Workflow failed |
| `WorkflowAborted` | User cancelled |
| `TaskScheduled` | Task queued |
| `TaskStarted` | Task begins |
| `TaskCompleted` | Task done (includes record if compressed) |
| `TaskFailed` | Task failed |
| `ProviderCalled` | LLM API call made |
| `ProviderResponded` | LLM response received |
| `McpInvoke` | MCP tool called |
| `McpResponse` | MCP result received |
| `McpConnected` | MCP server connected |
| `McpError` | MCP error |
| `AgentStart` | Agent loop begins |
| `AgentTurn` | Agent turn (includes thinking if extended_thinking) |
| `AgentComplete` | Agent done |
| `AgentSpawned` | Sub-agent created |
| `ContextAssembled` | Context loaded |
| `TemplateResolved` | Template variables resolved |
| `ArtifactWritten` | File written |
| `ArtifactFailed` | File write failed |
| `RecordCreated` | Record compressed (v0.28) |
| `BudgetExceeded` | Context budget warning (v0.29) |

### CLI Trace Commands

```bash
nika trace list              # List all trace files
nika trace show <id>         # Display events for a run
nika trace export <id>       # Export as JSON/YAML
```

Source: `event/` (4 files): `emitter.rs`, `log.rs`, `trace.rs`

---

## 18. CLI

Complete CLI with spn features merged (v0.27).

### Command Reference

```
nika                              # TUI Home view
nika <workflow.nika.yaml>         # Run workflow (positional arg)
nika chat                         # Chat view
nika studio [file]                # Studio view (YAML editor)
nika check <file> [--strict]      # Validate workflow
nika new                          # Interactive workflow wizard
nika trace list|show|export       # Trace inspection

nika provider list                # Show all providers + status
nika keys set <name>              # Store API key in encrypted vault
nika provider get <name>          # Retrieve (masked)
nika provider test <name>         # Validate key with API call
nika provider migrate             # Migrate env vars → keychain

nika model list                   # List local models
nika model pull <name>            # Download from HuggingFace
nika model info <name>            # Model details

nika mcp add <name>               # Add MCP server (100 aliases)
nika mcp remove <name>            # Remove server
nika mcp list                     # List configured
nika mcp test <name>              # Test connection
nika mcp tools <name>             # List available tools

nika sync                         # Sync to enabled editors
nika sync --status                # Show sync status
nika sync --enable <editor>       # Enable editor (claude-code, cursor, windsurf, vscode)

nika daemon start|stop|status     # Background daemon (keychain relay)
nika jobs submit|list|output|cancel  # Background workflow execution
nika backup create|list|restore|prune  # Unified backup
nika setup [nika|novanet|claude-code]  # Interactive onboarding wizard
```

### Provider Resolution Priority

1. NikaVault (most secure) — via `nika keys set`
2. Environment variable — `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, etc.
3. `.env` file — via dotenvy

Source: `main.rs`, `core/` (8 files), `secrets/` (5 files)

---

## 19. TUI — 4-View Architecture

```mermaid
flowchart LR
    subgraph TUI["Nika TUI (ratatui)"]
        HOME["Home\n(1)\nbrowse workflows"]
        STUDIO["Studio\n(2/s)\n3-panel workspace"]
        RUNNER["Runner\n(3/r)\nreal-time monitoring"]
        CHAT["Chat\n(4/c)\nagent conversation"]
    end

    STUDIO --> BROWSER["File Browser\ntui-tree-widget"]
    STUDIO --> EDITOR["YAML Editor\nsyntax highlighting\nundo/redo"]
    STUDIO --> DAG_PREV["DAG Preview\nASCII visualization"]

    CHAT --> NIKA_MSG["Nika (runtime)"]
    CHAT --> AGENT_MSG["Agent"]
    CHAT --> SUB_MSG["Sub-agent"]

    style TUI fill:#dbeafe,stroke:#2563eb
    style STUDIO fill:#fef3c7,stroke:#d97706
```

### Views

| View | Shortcut | Purpose |
|------|----------|---------|
| **Home** | `1` | Browse workflows, recent files |
| **Studio** | `2` or `s` | 3-panel workspace: Browser \| Editor \| DAG Preview |
| **Runner** | `3` or `r` | Real-time execution monitoring: tasks, status, logs |
| **Chat** | `4` or `c` | Conversational agent interface |

### Studio Features

- **File browser**: VS Code-like tree widget, navigate project files
- **YAML editor**: Syntax highlighting, schema validation inline, tab management
- **DAG preview**: ASCII visualization of task dependencies
- **Undo/Redo**: Ctrl+Z / Ctrl+Y with 500ms coalescing
- **Fuzzy search**: Ctrl+P quick open
- **Sessions**: Auto-save to `.nika/sessions/` (max 50, auto-cleanup)

### Chat Interface

```
┌────────────────────────────────────────────┐
│  Nika Chat                                  │
│                                             │
│  User: /agent "Research QR trends"          │
│                                             │
│  Nika: Je lance un agent...                 │
│    ├─ Agent: Searching for papers...        │
│    │  ├─ Sub-agent: Fetching arxiv...       │
│    │  └─ Sub-agent: Parsing results...      │
│    └─ Agent: Found 15 papers.               │
│                                             │
│  Nika: L'agent a terminé. Résultats...      │
└────────────────────────────────────────────┘
```

### Theme

Solarized Dark/Light unified palette. Config in `.nika/config.toml`.

Source: `tui/` (153 files), `tui/views/` (11 files), `tui/widgets/` (100+ files)

---

## 20. LSP

Language Server Protocol for `.nika.yaml` files.

| Feature | Description |
|---------|-------------|
| **Completion** | Verb names, task IDs, binding references, provider names |
| **Hover** | Documentation on hover for verbs, fields, providers |
| **Go-to-definition** | Jump to task definition from `depends_on` references |
| **Code actions** | Quick fixes for common errors |
| **Document symbols** | Task and workflow outline |
| **Diagnostics** | Real-time YAML validation against workflow schema |

Source: `lsp/` (13 files)

---

## 21. Stack & Numbers

### Tech Stack

| Component | Library | Version |
|-----------|---------|---------|
| Language | Rust | 2024 edition |
| Async runtime | tokio | latest |
| LLM abstraction | rig-core | v0.32 |
| MCP client | rmcp | v0.16 |
| Terminal UI | ratatui | latest |
| Error reporting | miette | latest |
| Local inference | mistral.rs | latest |
| YAML parsing | serde_saphyr | with budget protection |
| Tree widget | tui-tree-widget | v0.24 |

### Numbers

| Metric | Value |
|--------|-------|
| Lines of Rust | ~220,000 |
| Source files | 378 |
| Modules | 29 |
| Tests | 6,600+ |
| Clippy warnings | 0 |
| Semantic verbs | 5 |
| Builtin tools | 12 (+ 6 introspection in v0.30 = 18) |
| Cloud providers | 7 (Anthropic, OpenAI, Mistral, Groq, DeepSeek, Gemini, Perplexity) |
| Local provider | 1 (mistral.rs / GGUF) |
| MCP aliases | 100 preconfigured |
| Known models | 16+ curated |
| Known providers | 20 (8 LLM + 11 MCP + 1 local) |
| TUI views | 3 (Studio, Command, Control) |
| TUI widgets | 100+ |
| Event types | 22+ |
| Error codes | 40+ (NIKA-025 through NIKA-289) |
| Test workflows | 103 |

---

## 22. Complete Module Map

```mermaid
flowchart TB
    subgraph SRC["tools/nika/src/ — 378 files, 29 modules"]
        direction TB

        subgraph PARSE_LAYER["Parsing Layer"]
            AST_M["ast/ (30 files)\n2-phase IR\nRaw → Analyzed"]
            DAG_M["dag/ (4 files)\ntoposort, cycle detection\nflow analysis, DynamicDag"]
        end

        subgraph EXEC_LAYER["Execution Layer"]
            RUNTIME_M["runtime/ (38 files)\nexecutor, runner, agents\nrecords, orchestrator, builtins"]
            PROVIDER_M["provider/ (7 files)\nrig-core, native/mistral.rs\ncost tracking"]
            BINDING_M["binding/ (9 files)\nlazy, templates, JSONPath\nmentions, transforms"]
        end

        subgraph IO_LAYER_M["I/O Layer"]
            MCP_M["mcp/ (12 files)\nclient, pool, retry\nvalidation, protocol"]
            EVENT_M["event/ (4 files)\nemitter, trace\nNDJSON writer"]
            IO_M["io/ (5 files)\natomic, security\ntemplate, writer"]
            SECRETS_M["secrets/ (5 files)\nkeychain, daemon\nfallback"]
        end

        subgraph CORE_LAYER["Core Definitions (v0.27)"]
            CORE_M["core/ (8 files)\nproviders, models\nMCP aliases, paths"]
        end

        subgraph UI_LAYER["UI Layer"]
            TUI_M["tui/ (153 files)\n3 views, 100+ widgets\nstate, themes, sessions"]
            LSP_M["lsp/ (13 files)\ncompletion, hover\ndefinition, diagnostics"]
            INIT_M["init/ (10 files)\nworkflow wizard\n6 template tiers"]
        end

        subgraph MGMT_LAYER["Management (v0.27 spn fusion)"]
            JOBS_M["jobs/ (8 files)\nscheduler, daemon\nretry, notify"]
            SYNC_M["sync/ (4 files)\neditor sync\nconfig, operations"]
            SETUP_M["setup/ (2 files)\nonboarding wizard"]
            REGISTRY_M["registry/ (6 files)\npackage management\nlockfile, resolver"]
        end

        subgraph SUPPORT["Support"]
            STORE_M["store/ (3 files)\nSQLite egghead\nsessions, traces"]
            SOURCE_M["source/ (3 files)\nspan tracking\nerror reporting"]
            TOOLS_M["tools/ (9 files)\nfile tool impls\nread, write, edit, glob, grep"]
            UTIL_M["util/ (5 files)\nconstants, fs\ninterner, system"]
        end
    end

    AST_M --> DAG_M
    DAG_M --> RUNTIME_M
    RUNTIME_M --> PROVIDER_M
    RUNTIME_M --> BINDING_M
    RUNTIME_M --> MCP_M
    RUNTIME_M --> EVENT_M
    RUNTIME_M --> IO_M
    CORE_M --> PROVIDER_M
    CORE_M --> SECRETS_M
    TUI_M --> RUNTIME_M

    style SRC fill:#f8fafc,stroke:#334155
    style PARSE_LAYER fill:#dbeafe,stroke:#2563eb
    style EXEC_LAYER fill:#dcfce7,stroke:#16a34a
    style IO_LAYER_M fill:#fef3c7,stroke:#d97706
    style CORE_LAYER fill:#f3e8ff,stroke:#9333ea
    style UI_LAYER fill:#fce7f3,stroke:#db2777
    style MGMT_LAYER fill:#ecfdf5,stroke:#059669
    style SUPPORT fill:#f1f5f9,stroke:#94a3b8
```

---

## Feature Status Matrix

| Feature | Status | Version | Source Files |
|---------|--------|---------|--------------|
| 5 semantic verbs | Shipped | v0.1+ | `ast/action.rs` |
| DAG execution + toposort | Shipped | v0.1+ | `dag/`, `runtime/executor/` |
| 7 cloud providers (rig-core) | Shipped | v0.6-v0.15 | `provider/rig.rs` |
| Native inference (mistral.rs) | Shipped | v0.26 | `provider/native/` |
| Agent loop (multi-turn) | Shipped | v0.4+ | `runtime/rig_agent_loop/` |
| spawn_agent (nested agents) | Shipped | v0.5 | `runtime/spawn.rs` |
| decompose: (runtime DAG expansion) | Shipped | v0.5 | `runtime/executor/decompose.rs` |
| Lazy bindings | Shipped | v0.5 | `binding/types.rs` |
| Structured output (4 layers) | Shipped | v0.19-v0.24 | `runtime/structured_output.rs` |
| fail_fast + DependencyFailed | Shipped | v0.24 | `runtime/executor/` |
| Artifact system (atomic writes) | Shipped | v0.18 | `io/` |
| 3-view TUI | Shipped | v0.20-v0.22 | `tui/views/` |
| Studio (editor + browser + DAG) | Shipped | v0.8-v0.22 | `tui/views/studio.rs` |
| spn→nika CLI fusion | Shipped | v0.27 | `core/`, `secrets/`, `jobs/`, `sync/`, `setup/` |
| 12 builtin tools | Shipped | v0.15 | `runtime/builtin/` |
| LSP | Shipped | v0.19+ | `lsp/` |
| YAML bomb protection | Shipped | v0.27+ | `ast/budget.rs` |
| **Model routing (4 slots)** | **Planned** | **v0.28** | `ast/raw/model_slot.rs`, `provider/rig.rs` |
| **Record engine** | **Planned** | **v0.28** | `runtime/record.rs`, `runtime/record_compress.rs` |
| **Orchestrate Mode** | **Planned** | **v0.29** | `runtime/orchestrator.rs`, `runtime/satellite.rs`, `dag/dynamic.rs` |
| **Context budget management** | **Planned** | **v0.29** | `runtime/context_budget.rs` |
| **Persistent Records (NovaNet)** | **Planned** | **v0.30** | `runtime/record_memory.rs` |
| **6 introspection tools** | **Planned** | **v0.30** | `runtime/builtin/` (6 new tools) |

---

## Version Mapping

```mermaid
gantt
    title Nika v0.28 → v0.30 Feature Roadmap
    dateFormat YYYY-MM-DD
    axisFormat %b

    section Wave 1 (v0.28)
    P-MODEL: 4-slot model routing          :pm, 2026-04-01, 30d
    P-RECORD: Record compression           :pe, 2026-04-01, 30d

    section Wave 2 (v0.29)
    P-ORCHESTRATE: orchestrate mode           :ps, after pe, 45d
    P-CONTEXT: Context budgets             :pc, after pe, 30d

    section Wave 3 (v0.30)
    P-MEMORY: NovaNet persistent records   :pmem, after ps, 30d
    P-INTROSPECT: 6 introspection tools    :pi, after ps, 20d
```

### Dependencies

```mermaid
flowchart LR
    PM["P-MODEL\n4 slots"] --> PS["P-ORCHESTRATE\nneeds slots for routing"]
    PE["P-RECORD\ncompression"] --> PS
    PE --> PC["P-CONTEXT\nneeds records for budgets"]
    PS --> PMEM["P-MEMORY\nneeds records stable"]
    PC --> PMEM
    PMEM --> PI["P-INTROSPECT\nneeds runtime state"]

    style PM fill:#dbeafe,stroke:#2563eb
    style PE fill:#dbeafe,stroke:#2563eb
    style PS fill:#fef3c7,stroke:#d97706
    style PC fill:#fef3c7,stroke:#d97706
    style PMEM fill:#dcfce7,stroke:#16a34a
    style PI fill:#dcfce7,stroke:#16a34a
```

---

<div align="center">

[← 05 Roadmap](./05-roadmap.md) · [Index](./00-README.md) · [09 Cookbook →](./09-use-cases-cookbook.md)

</div>
