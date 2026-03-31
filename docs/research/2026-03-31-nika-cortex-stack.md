# Nika Cortex — Confirmed Stack & Architecture

> Decision date: 2026-03-31
> Status: LOCKED
> Research: 18 agents, 37+ papers, 80+ crates examined

## Stack Decision (FINAL)

| Layer | Crate | Version | Status | Impact |
|-------|-------|---------|--------|--------|
| **Graph + Vector + FTS** | `grafeo` | =0.5.30 | NEW — fork at SuperNovae-st/grafeo | +3MB |
| **Metadata + FSRS** | `rusqlite` (bundled) | 0.39 | Already in workspace | 0 |
| **Embeddings** | `fastembed` | 5.13 | NEW — opt-in feature flag | +3MB |

**New deps: 2 (grafeo + fastembed). Binary impact: +6 MB (45→51 MB).**

Grafeo REPLACES what would have been 3 separate crates:
- ~~petgraph~~ → Grafeo has 22 built-in graph algorithms (PageRank, Louvain, SSSP...)
- ~~usearch~~ → Grafeo has HNSW with f16/i8/binary quantization + SimSIMD
- ~~FTS5~~ → Grafeo has BM25 full-text search built-in

petgraph stays in workspace for DAG engine (not Cortex).

## Why Grafeo (revised decision)

### The case FOR (why we switched from NO-GO to GO)

1. **It's the ONLY pure Rust embeddable that does graph+vector+FTS in one**
   Without it, we build a "poor man's graph" on SQLite adjacency tables with manual traversal.
   That's not the ambition level we want.

2. **Cypher + SPARQL = ontological self-describing graph**
   SPARQL/RDF is EXACTLY what we need for auto-evolving ontology.
   SQLite will NEVER have this. petgraph will never have this.

3. **Apache-2.0 = we can fork freely**
   Fork at SuperNovae-st/grafeo. If author abandons → we maintain.
   We contribute fixes upstream. Open source spirit.

4. **Nika has bus factor ~2 too**
   Can't criticize Grafeo for bus factor 1 when we're similar.

5. **We'd be one of the first serious users**
   We influence the project direction. Early adopter advantage.
   Our contributions (benchmarks, tests, fixes) make it better for everyone.

### Risks mitigated

| Risk | Mitigation |
|------|-----------|
| Author abandons | Fork at SuperNovae-st/grafeo, maintain ourselves |
| Breaking changes | Pin exact version `=0.5.30` in Cargo.toml |
| License flip | Our fork stays Apache-2.0 forever (pinned) |
| Snapshot format breaks | Pin version, test upgrades before adopting |
| Bugs | We contribute fixes upstream via PRs |
| Compile time | Feature flags: `embedded` profile only (~3MB) |

### What Grafeo gives us (concrete)

```cypher
-- Instead of 50 lines of SQL + petgraph glue:
MATCH (fact:Fact)-[:CAUSES]->(effect:Fact)
WHERE fact.confidence > 0.8
RETURN fact.content, effect.content, fact.surprise
ORDER BY fact.salience DESC
LIMIT 10

-- Ontology self-description:
SELECT ?type ?count WHERE {
  ?node a ?type .
} GROUP BY ?type ORDER BY DESC(?count)

-- Hybrid search (vector + graph):
CALL grafeo.vector.search('Fact', 'embedding', $query_vec, 20)
YIELD node, score
MATCH (node)-[:RELATED_TO*1..3]-(related)
RETURN node, related, score
```

vs what we'd have to write with SQLite+petgraph:
```rust
// 1. Query FTS5 separately
// 2. Query usearch separately
// 3. Load petgraph from SQLite edges
// 4. Run PageRank on petgraph
// 5. Manually merge results with RRF
// 6. Convert between 3 different ID systems
// → 200+ lines of glue code per query
```

## Fork Strategy

```
UPSTREAM : github.com/GrafeoDB/grafeo (Apache-2.0, 463 stars, 10 forks)
FORK    : github.com/SuperNovae-st/grafeo

supernovae-hq/
├── nika/            [submodule] workflow engine
├── novanet/         [submodule] knowledge graph
├── grafeo/          [submodule] graph DB fork ← NEW
├── homebrew-tap/    [submodule]
└── ...

Cargo.toml (nika workspace):
  # Start with crates.io pinned
  grafeo = { version = "=0.5.30", features = ["embedded", "ai"] }

  # If we need patches:
  # grafeo = { git = "https://github.com/SuperNovae-st/grafeo",
  #            rev = "abc123", features = ["embedded", "ai"] }

Git workflow:
  1. Fork → SuperNovae-st/grafeo
  2. Branches: main (sync upstream) + sn/nika (our patches)
  3. Upstream sync: periodic merge from GrafeoDB/grafeo main
  4. Contributions: PR from sn/nika → GrafeoDB/grafeo
  5. If upstream dies: sn/nika becomes source of truth
```

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                    NIKA CORTEX                            │
│                                                           │
│  ┌─────────────────────────────────────────────────────┐ │
│  │              GRAFEO (graph engine)                   │ │
│  │  ├── Property Graph (nodes, edges, properties)      │ │
│  │  ├── Cypher queries (traversal, pattern matching)   │ │
│  │  ├── SPARQL (RDF ontology, self-describing schema)  │ │
│  │  ├── HNSW vector index (i8/f16 quantization)        │ │
│  │  ├── BM25 full-text search                          │ │
│  │  ├── Hybrid search RRF (vector + text + graph)      │ │
│  │  ├── 22 graph algorithms (PageRank, Louvain, ...)   │ │
│  │  ├── MVCC transactions + WAL persistence            │ │
│  │  └── Single file: ~/.nika/egghead.grafeo             │ │
│  └─────────────────────────────────────────────────────┘ │
│                         +                                 │
│  ┌─────────────────────────────────────────────────────┐ │
│  │           RUSQLITE (metadata store)                  │ │
│  │  ├── FSRS-6 scheduler state (per-node)              │ │
│  │  ├── Access logs (timestamps for ACT-R)             │ │
│  │  ├── Trigger rules (conditional auto-recall)        │ │
│  │  ├── Memory changelog (rollback/versioning)         │ │
│  │  ├── Daemon job queue (existing)                    │ │
│  │  └── Single file: ~/.nika/egghead-meta.db            │ │
│  └─────────────────────────────────────────────────────┘ │
│                         +                                 │
│  ┌─────────────────────────────────────────────────────┐ │
│  │         FASTEMBED (embedding engine, opt-in)         │ │
│  │  ├── BGE-small-en-v1.5 (384d, 33MB model)          │ │
│  │  ├── Or multilingual-e5-small (384d)                │ │
│  │  ├── ONNX Runtime inference                         │ │
│  │  └── Feature flag: cortex-embed                     │ │
│  └─────────────────────────────────────────────────────┘ │
│                                                           │
│  12 cognitive mechanisms + 8-signal retrieval             │
│  + 9 builtin tools + auto-evolving ontology              │
└──────────────────────────────────────────────────────────┘
```

## Rejected Alternatives (with reasoning)

| Crate | Decision | Why |
|-------|----------|-----|
| ~~petgraph~~ (for Cortex) | REPLACED by Grafeo | Grafeo has 22 graph algos built-in |
| ~~usearch~~ | REPLACED by Grafeo | Grafeo has HNSW with quantization |
| ~~sqlite-vec~~ | REPLACED by Grafeo | Grafeo has vector search integrated |
| ~~FTS5~~ (for Cortex) | REPLACED by Grafeo | Grafeo has BM25 built-in |
| ~~turbo-quant~~ | WRONG DOMAIN | KV cache compression, not vector search |
| ~~redb/fjall/sled~~ | WRONG ABSTRACTION | KV-only, no FTS, no vector, no graph |
| ~~Kuzu~~ | STALE | C++ FFI, last commit Oct 2025 |
| ~~CozoDB~~ | STALE | Last release Dec 2023 |
| ~~SurrealDB~~ | LICENSE | BSL 1.1, AGPL-incompatible |

## 12 Cognitive Mechanisms

| # | Mechanism | Source | What |
|---|-----------|--------|------|
| ① | Hebbian strengthening | Shodh | Co-access → stronger links (+2.5%/-10%) |
| ② | Dual decay | FSRS-6 + ACT-R + Bjork | 3 decay models combined |
| ③ | Dopamine gate | D-MEM paper | Surprise × Utility threshold, -80% tokens |
| ④ | Prospective indexing | Kumiho (93.3%) | Write-time anticipation of future scenarios |
| ⑤ | Narrative consolidation | TraceMem + Vestige | Sleep replay, episode→semantic promotion |
| ⑥ | Contradiction detection | Kumiho AGM | Formal belief revision, supersedes |
| ⑦ | Salience encoding | Pensyve | 0.4×novelty+0.3×importance+0.1×ext+0.2×spec |
| ⑧ | Feedback correction | ICM | Learn from wrong recalls, closed-loop |
| ⑨ | Synaptic tagging | Vestige (Frey&Morris) | Retroactive importance boost (6h window) |
| ⑩ | Interference detection | Shodh | Proactive/retroactive, cosine>0.9 |
| ⑪ | Auto-linking | A-Mem Zettelkasten | Write-time: find related, create edges |
| ⑫ | Conditional triggers | Nocturne | Pattern-based auto-recall |

## 6 Memory Levels

| Level | Type | What | Decay |
|-------|------|------|-------|
| 0 | Working | Evidence packets (session-scoped) | Session end |
| 1 | Episodic | Events, sessions, timeline | FSRS-6 fast |
| 2 | Semantic | Facts, entities, typed relations | ACT-R slow |
| 3 | Procedural | Skills, patterns, Bayesian reliability | Utility-based |
| 4 | Reflective | Meta-observations about patterns | Very slow |
| 5 | Conceptual | Cross-cutting abstractions/themes | PageRank hubs |

## 8-Signal Hybrid Retrieval

```
Query → ⓪ Trigger check (conditional auto-recall)
      → ① Grafeo BM25 (keywords + anticipations)
      → ② Grafeo HNSW cosine (semantic similarity)
      → ③ Grafeo PageRank (multi-hop graph relevance)
      → ④ ACT-R spreading activation (associative priming)
      → ⑤ Query intent classification (5 intents)
      → ⑥ Node confidence × FSRS retrievability
      → ⑦ Interference penalty (suppress old similar)
      → ⑧ Salience boost
            │
      Grafeo native RRF merge (adaptive k)
            │
      Token budget → EvidencePackets → RecallResult
```

Note: signals ①②③ are ALL inside Grafeo (one query). Only ④⑤⑥⑦⑧ are computed in Rust.

## 9 Builtin Tools

| Tool | Purpose |
|------|---------|
| `nika:remember` | Store with 8-step write pipeline |
| `nika:recall` | 8-signal hybrid retrieval + recursive + assembly |
| `nika:revise` | Update with supersedes (never delete) |
| `nika:correct` | Feedback correction for wrong recalls |
| `nika:consolidate` | Merge similar, resolve contradictions, replay |
| `nika:egghead_schema` | Discover/create/evolve node types (SPARQL) |
| `nika:egghead_audit` | CSR score, orphans, stale, integrity |
| `nika:egghead_export` | YAML/JSON/NDJSON export |
| `nika:egghead_history` | Changelog, diff, rollback |

## New Ideas & Improvements

### Grafeo-enabled features (impossible without it)

1. **Cypher-native retrieval** — queries are readable, debuggable, composable
2. **SPARQL ontology** — self-describing schema queries in standard W3C language
3. **Graph-vector hybrid queries** — vector search WITHIN graph traversal (native)
4. **Community detection** — Louvain clustering for automatic concept node generation
5. **Shortest path** — causal chain discovery between any two facts
6. **Graph visualization** — Grafeo's viz output → Nika TUI graph panel

### Nika-specific innovations (beyond any existing system)

7. **Workflow-as-procedural-memory** — successful .nika.yaml workflows auto-stored as procedural memories with Bayesian reliability
8. **Memory-guided orchestration** — P-ORCHESTRATE queries Cortex to decide next steps based on past workflow results
9. **Cortex MCP server** — expose Cortex as MCP so OTHER tools (Claude Code, Cursor) can query Nika's memory
10. **Memory import** — import from Hermes SKILL.md, Claude CLAUDE.md/MEMORY.md, ICM format
11. **Cross-workflow memory** — workflow A's learnings available in workflow B automatically
12. **TUI memory panel** — visualize the knowledge graph in Nika's terminal UI (Grafeo → ASCII graph)
13. **Embedding cache** — cache embeddings in Grafeo nodes to avoid re-computing

## Crate Structure (FINAL)

```
tools/nika-cortex/
├── Cargo.toml
└── src/
    ├── lib.rs                    # Cortex facade
    ├── store/
    │   ├── mod.rs                # trait CortexStore (escape hatch)
    │   ├── grafeo.rs             # Grafeo graph engine wrapper
    │   ├── meta.rs               # SQLite metadata (FSRS, triggers, changelog)
    │   └── dedup.rs              # blake3 + cosine dedup
    ├── memory/
    │   ├── mod.rs                # MemoryKind enum (6 levels)
    │   ├── episodic.rs           # Events, sessions, narrative threads
    │   ├── semantic.rs           # Facts, entities, typed relations
    │   ├── procedural.rs         # Skills, Bayesian reliability
    │   ├── working.rs            # Evidence packets, token budgeting
    │   ├── reflective.rs         # Meta-observations (auto-generated)
    │   └── conceptual.rs         # Cross-cutting themes (PageRank hubs)
    ├── cognitive/
    │   ├── hebbian.rs            # ① Edge strengthening
    │   ├── decay.rs              # ② FSRS-6 + ACT-R + Bjork
    │   ├── gate.rs               # ③ Dopamine gate
    │   ├── anticipation.rs       # ④ Prospective indexing
    │   ├── consolidation.rs      # ⑤ Narrative consolidation + replay
    │   ├── contradiction.rs      # ⑥ AGM belief revision
    │   ├── salience.rs           # ⑦ Encoding importance
    │   ├── feedback.rs           # ⑧ Correction loop
    │   ├── tagging.rs            # ⑨ Synaptic tagging
    │   ├── interference.rs       # ⑩ Interference detection
    │   ├── autolink.rs           # ⑪ Zettelkasten auto-linking
    │   └── triggers.rs           # ⑫ Conditional auto-recall
    ├── retrieval/
    │   ├── signals.rs            # 8 signal extractors
    │   ├── rrf.rs                # RRF merge (3 from Grafeo + 5 from Rust)
    │   ├── activation.rs         # ACT-R spreading activation
    │   ├── recursive.rs          # RLM-style recursive recall
    │   └── assembly.rs           # Context assembly modes (4)
    └── tools/
        ├── remember.rs           # nika:remember (8-step pipeline)
        ├── recall.rs             # nika:recall (8-signal + recursive)
        ├── revise.rs             # nika:revise (supersedes chain)
        ├── correct.rs            # nika:correct (feedback loop)
        ├── consolidate.rs        # nika:consolidate (merge + replay)
        ├── schema.rs             # nika:egghead_schema (SPARQL ontology)
        ├── audit.rs              # nika:egghead_audit (CSR score)
        ├── export.rs             # nika:egghead_export
        └── history.rs            # nika:egghead_history (rollback)
```

## Key Research Sources

### Papers (6)
- Kumiho (2603.17244) — Prospective indexing, AGM, 93.3% LoCoMo
- D-MEM (2603.14597) — Dopamine gate, -80% tokens
- GAAMA (2603.27910) — 4-node hierarchy + PageRank
- TraceMem (2602.09712) — Narrative consolidation
- AMA-Agent (2602.22769) — Causal graphs, +11%
- RLM (2512.24601) — Recursive self-invocation, 100x context

### Rust Projects Studied (6)
- Shodh (182 stars) — Hebbian constants, 55ms store
- Vestige (456 stars) — FSRS-6, prediction gating, synaptic tagging
- ICM (129 stars) — Dual model, typed relations, feedback
- Pensyve (1 star) — 6-signal RRF, Bayesian procedural, salience
- Nocturne (867 stars) — First-person sovereignty, conditional triggers
- Grafeo (463 stars) — 6 query langs, HNSW+BM25 in one (ADOPTED)

### NovaNet Concepts Ported
- Evidence packets + token budgeting
- Arc families → EdgeFamily enum
- Self-describing schema → SPARQL on Grafeo
- Provenance tracking (ADR-042)
- Quality audit (CSR score)
- Context assembly modes (4 adapted)
