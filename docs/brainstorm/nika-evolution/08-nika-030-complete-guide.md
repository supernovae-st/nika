# 08 — Nika v0.30 Complete Guide

> Everything you need to understand Nika v0.30 + NovaNet — explained with real examples.
> What it is, how it works, what changed, and how to use it.

**Nika** v0.27.0 → v0.30 · **NovaNet** v0.20.0 · Updated 2026-03-14

---

## Table of Contents

1. [What is Nika?](#what-is-nika)
2. [What is NovaNet?](#what-is-novanet)
3. [How They Work Together](#how-nika--novanet-work-together)
4. [What's New in v0.30 — The 6 Features](#whats-new-in-v030)
5. [Feature 1: Model Slots](#feature-1-model-slots)
6. [Feature 2: Records](#feature-2-records)
7. [Feature 3: Shaka Orchestration](#feature-3-shaka-orchestration)
8. [Feature 4: Context Budget](#feature-4-context-budget)
9. [Feature 5: Persistent Memory](#feature-5-persistent-memory-novanet)
10. [Feature 6: Runtime Introspection](#feature-6-runtime-introspection)
11. [Before vs After — Side by Side](#before-vs-after)
12. [Feature Compatibility Matrix](#feature-compatibility-matrix)
13. [FAQ](#faq)

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

NovaNet is a **knowledge graph** powered by Neo4j. It stores structured knowledge about entities, locales, and content — and exposes it to Nika through 8 MCP tools.

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

### The 8 MCP Tools

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
        F3["✨ Shaka Mode\nLLM-driven orchestration"]
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
║  3. SHAKA          → Let an LLM decide what tasks to run dynamically          ║
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
| 2 | v0.29 | @0.13 | Shaka + Context Budget | Wave 1 |
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

## Feature 3: Shaka Orchestration

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

`orchestration: shaka` adds a new execution mode where the **Shaka** dynamically dispatches tasks:

```yaml
# v0.29 — The Shaka decides what to do
schema: nika/workflow@0.13

orchestration: shaka                   # ← NEW: enables shaka mode

model_slots:
  pythagoras: { provider: anthropic, model: claude-sonnet-4-6, extended_thinking: true }
  edison:     { provider: anthropic, model: claude-sonnet-4-6 }
  york:       { provider: groq,      model: llama-3.3-70b-versatile }

shaka:
  goal: |
    Generate a complete landing page for QR Code AI in French.
    Research trends, write sections, review quality.
    Iterate until quality score >= 0.85.
  model_slot: pythagoras
  max_rounds: 8
  record_budget: 15000

# These are TEMPLATES — the Shaka dispatches them
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
    participant S as 🎯 Shaka<br/>(Claude + thinking)
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
║  Mode 2: orchestration: shaka (new in v0.29)                                  ║
║  ─────────────────────────────────────────────────────────────                ║
║  • Tasks are TEMPLATES dispatched by the Shaka                                ║
║  • Shaka decides what to run, when, and with what params                      ║
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

Without limits, a task's context can grow unbounded — especially in shaka mode where multiple rounds accumulate records:

```
Round 1: research record       → 300 tokens
Round 2: write_hero record     → 800 tokens
Round 3: write_features record → 800 tokens
Round 4: review record         → 500 tokens
...
Round 8: Shaka has 5,000+ tokens of records
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
      - nika:shaka            # "What round are we on? Budget left?"
      - nika:cost             # "How many tokens/dollars spent so far?"
      - nika:dag_info         # "What tasks come after me?"
      - nika:task_status      # "Did task X succeed?"
```

### What each tool returns

| Tool | Returns | Example Use |
|------|---------|-------------|
| `nika:records` | List of all records with summaries and confidence | "Check if research was thorough enough" |
| `nika:threads` | Active, completed, and pending tasks | "Know what's left to do" |
| `nika:shaka` | Current round, max rounds, budget used/remaining | "Am I running out of budget?" |
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
orchestration: shaka

model_slots:
  pythagoras: { provider: anthropic,
    model: claude-sonnet-4-6,
    extended_thinking: true }
  edison: { provider: anthropic,
    model: claude-sonnet-4-6 }
  york: { provider: groq,
    model: llama-3.3-70b-versatile }

shaka:
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
- Shaka adds tasks dynamically
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
| Shaka | ❌ | Model Slots + Records | Content generation |
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
  Switch to orchestration: shaka for complex workflows

Level 4 — Add memory:
  Add record.persist: novanet for cross-session learning
```

---

## FAQ

### "Is v0.30 backward compatible?"

**Yes.** All new fields are optional. A v0.27 workflow runs unchanged on v0.30. You adopt features incrementally.

### "Do I need NovaNet to use v0.30?"

**No.** Features 1-4 (model slots, records, shaka, context budget) work without NovaNet. Only Feature 5 (persistent memory) requires NovaNet.

### "Is shaka mode deterministic?"

**No.** The Shaka makes decisions dynamically, so two runs may produce different task sequences. Use `orchestration: dag` when you need determinism.

### "How is this different from LangGraph?"

LangGraph is Python code that defines agent graphs. Nika is YAML that defines workflows. Key differences:
- Nika workflows are version-controlled YAML (auditable, reproducible)
- Nika has NovaNet integration (knowledge graph + 200 locales)
- Nika has real-time TUI for monitoring
- Nika has 5 semantic verbs (not arbitrary function nodes)

### "How is this different from CrewAI?"

CrewAI is multi-agent with role-based crews. Nika is workflow-first with optional shaka mode. Key differences:
- Nika's shaka mode is simpler (1 Shaka + N satellite templates)
- Nika records are compressed (CrewAI passes full outputs)
- Nika has NovaNet (no competitor has a knowledge graph)

### "What's the 'dumb zone'?"

The dumb zone (term from Dex Horthy / Slate) is the point where an LLM has so much context that its performance actually **degrades**. Think of it like trying to read a 100-page document while writing — you lose track. Records and context budgets prevent this.

### "Can I mix DAG and shaka mode?"

Not in the same workflow. But you can have a shaka workflow that `include:`s a DAG sub-workflow, or an `agent:` task that calls `nika:run` to execute a DAG workflow.

---

<div align="center">

[← 07 Slate Deep Integration](./07-slate-deep-integration.md) · [📋 Index](./00-README.md) · [09 Use Cases Cookbook →](./09-use-cases-cookbook.md)

</div>
