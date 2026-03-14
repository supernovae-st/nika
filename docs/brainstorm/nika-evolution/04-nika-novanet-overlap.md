# 04 — Nika x NovaNet Overlap Analysis

> Identifying duplication, synergies, and boundary decisions.
> Date: 2026-03-14

---

## Architecture Recap

```
┌─────────────────────────────────────────────────────────────────────┐
│  ECOSYSTEM BOUNDARY                                                 │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  NovaNet (Brain)              │  Nika (Body)                        │
│  ─────────────────            │  ─────────────                      │
│  Knowledge storage            │  Workflow execution                 │
│  Schema management            │  LLM orchestration                  │
│  MCP Server (8 tools)         │  MCP Client (rmcp)                  │
│  Neo4j graph                  │  tokio runtime                      │
│  Locale intelligence          │  Multi-provider inference           │
│  Entity/Page/Block model      │  DAG + 5 verbs                     │
│  Quality audit (CSR)          │  Event sourcing + traces            │
│  Denomination forms           │  Transform engine                   │
│  Knowledge atoms              │  Agent loop + spawning              │
│                               │                                     │
│         MCP Protocol ◄────────┼────────►                            │
│                               │                                     │
└─────────────────────────────────────────────────────────────────────┘
```

**Rule (ADR-003):** Nika NEVER accesses Neo4j directly. All graph interaction flows through MCP.

---

## Overlap Areas

### 1. Context Assembly

| Capability | NovaNet | Nika | Verdict |
|-----------|---------|------|---------|
| Context assembly for LLM | `novanet_context` (4 modes) | `context:` file loading | **Complementary** |
| Token budgeting | `token_budget` param in context | None | NovaNet owns this |
| Evidence ranking | `novanet_context` auto-ranks | None | NovaNet owns this |
| File loading | None | `context: { files: ... }` | Nika owns this |
| Session restore | None | `context: { session: ... }` | Nika owns this |

**Analysis:** No duplication. NovaNet assembles **graph-based context** (entities, knowledge atoms, page structure). Nika assembles **file-based context** (markdown, JSON, YAML from disk). They serve different purposes and combine naturally:

```yaml
# Complementary context assembly
tasks:
  - id: graph_ctx
    invoke: novanet_context
    params: { focus_key: "homepage", locale: "fr-FR", mode: page }

  - id: file_ctx
    # Nika's own context (from disk)
    context:
      files:
        brand: ./context/brand.md

  - id: generate
    use:
      graph: $graph_ctx
      brand: "{{context.files.brand}}"
    infer: "Generate using both: {{use.graph}} {{use.brand}}"
```

### 2. Schema / Validation

| Capability | NovaNet | Nika | Verdict |
|-----------|---------|------|---------|
| Schema definition | 59 NodeClasses, YAML source | `nika-workflow.schema.json` | **No overlap** |
| Schema validation | `novanet_write(dry_run=true)` | Two-phase AST analyzer | **No overlap** |
| Schema introspection | `novanet_introspect` | LSP diagnostics | **Different domains** |

**Analysis:** NovaNet validates graph data schemas. Nika validates workflow YAML schemas. Zero overlap.

### 3. Search / Discovery

| Capability | NovaNet | Nika | Verdict |
|-----------|---------|------|---------|
| Entity search | `novanet_search` (5 modes) | None | NovaNet only |
| File search | None | `nika:glob`, `nika:grep` | Nika only |
| Tool discovery | `list_tools()` (MCP server) | `list_tools()` (MCP client) | **MCP protocol** |

**Analysis:** No duplication. NovaNet searches graph data. Nika searches filesystem.

### 4. State Management

| Capability | NovaNet | Nika | Verdict |
|-----------|---------|------|---------|
| Persistent state | Neo4j (durable) | DataStore (in-memory) | **Gap in Nika** |
| Cross-session | Graph persists forever | Session files only | **Gap in Nika** |
| Concurrent access | Neo4j transactions | DashMap lock-free | Both handle concurrency |

**Analysis:** NovaNet has durable state (Neo4j). Nika's state is ephemeral (DashMap, session files). This isn't overlap but a **complementary gap**: Nika could use NovaNet to persist cross-session agent memory.

### 5. Event / Audit

| Capability | NovaNet | Nika | Verdict |
|-----------|---------|------|---------|
| Quality audit | `novanet_audit` (CSR metrics) | None | NovaNet only |
| Event sourcing | None | 34 EventKind variants | Nika only |
| Trace storage | None | NDJSON files | Nika only |
| Data lineage | Arc-based provenance | None | NovaNet only |

**Analysis:** No overlap. NovaNet audits data quality. Nika traces execution. Could be synergistic: Nika could log generation events TO NovaNet for content lineage.

### 6. Locale / Content

| Capability | NovaNet | Nika | Verdict |
|-----------|---------|------|---------|
| Locale definitions | Shared realm locale layer | None | NovaNet only |
| Knowledge atoms | Expression, Pattern, CultureRef, Taboo | None | NovaNet only |
| Content generation | PageNative, BlockNative (output) | `infer:` verb | **Synergy point** |
| Content validation | Denomination forms (6 types) | Structured output (4 layers) | **Complementary** |

**Analysis:** NovaNet provides the locale intelligence. Nika provides the generation engine. The workflow is: NovaNet supplies context → Nika generates → NovaNet stores result. The only potential overlap is if Nika started building its own locale awareness, which it should NOT.

---

## Potential Duplication Risks

### Risk 1: Memory / State Duplication

```
RISK: Nika builds its own episodic memory system (P4 priority)
     that duplicates NovaNet's entity/knowledge storage.

RECOMMENDATION:
  - Nika's episodic memory should USE NovaNet as backend
  - New NodeClass: "AgentEpisode" in NovaNet Org realm
  - Nika writes episodes via novanet_write
  - Nika reads episodes via novanet_search
  - NovaNet handles persistence, search, and cleanup

WHY: NovaNet already has durable storage, fulltext search,
     and quality audit. Building a parallel system in Nika
     would duplicate all of this.
```

### Risk 2: Context Assembly Duplication

```
RISK: Nika evolves its context: system to include entity
     awareness, essentially rebuilding novanet_context.

RECOMMENDATION:
  - Keep context: for FILE-based context only
  - All graph-based context goes through novanet_context
  - Never add entity/locale/knowledge awareness to Nika
  - Nika is the orchestrator, NovaNet is the memory

WHY: NovaNet's context assembly has token budgeting,
     evidence ranking, and spreading activation.
     Rebuilding this in Nika would be inferior.
```

### Risk 3: Schema Duplication

```
RISK: Nika adds graph schema concepts (node types, arcs)
     to its workflow DSL for decompose: or routing.

RECOMMENDATION:
  - Keep graph schema knowledge in NovaNet only
  - Nika uses novanet_introspect to discover schema at runtime
  - decompose: strategy should call novanet_search(mode=walk)
  - Never hardcode NovaNet schema in Nika's AST

WHY: Schema changes in NovaNet (59→N nodes) shouldn't
     require Nika code changes. MCP is the abstraction.
```

### Risk 4: Audit Duplication

```
RISK: Nika builds its own quality scoring for generated content
     instead of using novanet_audit CSR metrics.

RECOMMENDATION:
  - Generated content quality → novanet_audit
  - Workflow execution quality → Nika event analysis
  - Content stored in NovaNet → NovaNet audits it
  - Execution traces in Nika → Nika monitors them

WHY: Content quality depends on entity coverage, locale
     completeness, and graph integrity — all NovaNet concerns.
```

---

## Synergy Opportunities

### Opportunity 1: Agent Episodes in NovaNet

```yaml
# Future: Agent writes its experience to NovaNet
tasks:
  - id: research
    agent:
      prompt: "Research QR code trends"
      episodic_memory: true  # NEW: persist to NovaNet

  # Automatically creates:
  # AgentEpisode node in NovaNet with:
  #   - task_summary, key_findings, tools_used
  #   - linked to Entity "qr-code" via EPISODE_OF arc
  #   - searchable for future similar tasks
```

### Opportunity 2: Generation Lineage

```yaml
# Future: Track what Nika generated and how
tasks:
  - id: generate_page
    invoke: novanet_context
    params: { focus_key: "homepage", locale: "fr-FR" }

  - id: write_content
    use:
      ctx: $generate_page
    infer: "Generate landing page"
    # After generation, auto-invoke:
    # novanet_write(class="PageNative", properties={content: result})
    # With provenance: generated_by="nika", workflow="generate-page.nika.yaml"
```

### Opportunity 3: Smart Model Routing via NovaNet

```yaml
# Future: NovaNet stores model performance data
# Nika queries it for routing decisions

tasks:
  - id: route
    invoke: novanet_search
    params:
      query: "model performance for translation tasks"
      kinds: ["ModelBenchmark"]  # New NodeClass

  - id: translate
    use:
      model: "$route.best_model"
    infer: "Translate to French"
    provider: "{{use.model.provider}}"
```

### Opportunity 4: Decompose via Graph Structure

Already partially implemented:

```yaml
# decompose: uses NovaNet's graph to expand DAG
tasks:
  - id: generate_all
    decompose:
      strategy: semantic
      traverse: HAS_CHILD  # NovaNet arc
      source: $entity
    infer: "Generate for {{use.item}}"
```

---

## Boundary Rules

```
┌─────────────────────────────────────────────────────────────────────┐
│  BOUNDARY RULES                                                     │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  NIKA OWNS:                                                         │
│  ├── Workflow definition and execution                              │
│  ├── LLM provider management and inference                         │
│  ├── DAG construction, validation, and scheduling                  │
│  ├── Agent loop, spawning, and tool routing                        │
│  ├── File-based context loading                                     │
│  ├── Execution traces and event sourcing                           │
│  ├── TUI and developer experience                                   │
│  ├── Transform engine and data binding                             │
│  └── Security (shell exec, path validation)                        │
│                                                                     │
│  NOVANET OWNS:                                                      │
│  ├── Entity and content storage (all NodeClasses)                  │
│  ├── Locale intelligence (atoms, forms, culture)                   │
│  ├── Graph-based context assembly (token budget, evidence)         │
│  ├── Schema definition and validation                              │
│  ├── Content quality audit (CSR metrics)                           │
│  ├── Search and discovery (fulltext, property, walk)               │
│  ├── Data lineage and provenance                                    │
│  └── Cross-session durable state                                   │
│                                                                     │
│  SHARED VIA MCP:                                                    │
│  ├── Tool discovery (list_tools)                                    │
│  ├── Context requests (novanet_context)                            │
│  ├── Data writes (novanet_write)                                    │
│  └── Schema introspection (novanet_introspect)                     │
│                                                                     │
│  NEVER DUPLICATE:                                                   │
│  ├── Entity/locale awareness in Nika                               │
│  ├── Graph schema in Nika AST                                       │
│  ├── Workflow execution in NovaNet                                  │
│  └── LLM inference in NovaNet                                       │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Summary

The Nika/NovaNet boundary is clean today. The main evolution risk is Nika building parallel systems (memory, context, quality) instead of leveraging NovaNet. The rule is simple:

- **If it's about knowing things** → NovaNet
- **If it's about doing things** → Nika
- **If it's about connecting knowing and doing** → MCP
