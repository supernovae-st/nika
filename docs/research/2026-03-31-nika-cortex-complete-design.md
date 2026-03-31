# Nika Cortex — Complete Design Document

> Status: CONFIRMED — ready for implementation planning
> Date: 2026-03-31
> Research: 18 agents, 37+ papers, 80+ crates, 6 Rust projects analyzed

## 1. Vision

Nika Cortex is a **cognitive memory engine** embedded natively in Nika. It replaces the planned 3-tier HOT/WARM/COLD architecture and the NovaNet dependency with a self-contained, single-file memory system that combines 12 cognitive mechanisms from neuroscience and cutting-edge AI research.

**Tagline**: "The memory engine that doesn't exist yet."

**Key principle**: First-person memory — the agent DECIDES what to remember (via `nika:remember`), not passive extraction from conversations.

## 2. Confirmed Stack (FINAL — Grafeo-native)

| Layer | Crate | Version | Status | Binary Impact |
|-------|-------|---------|--------|---------------|
| **Graph+Vector+FTS** | `grafeo` | =0.5.30 | NEW — fork at SuperNovae-st/grafeo | +3MB |
| **Metadata+FSRS** | `rusqlite` (bundled) | 0.39 | Already in workspace | 0 |
| **Embeddings** | `fastembed` | 5.13 | NEW — opt-in feature flag | +3MB |

**New deps: 2 (grafeo + fastembed). Binary impact: +6 MB.**

Grafeo REPLACES what would have been 3 separate crates:
- ~~petgraph~~ → Grafeo has 22 built-in graph algorithms
- ~~usearch~~ → Grafeo has HNSW with i8/f16/binary quantization
- ~~FTS5~~ → Grafeo has BM25 full-text search built-in

### Fork strategy
- Upstream: github.com/GrafeoDB/grafeo (Apache-2.0)
- Fork: github.com/SuperNovae-st/grafeo
- Pin exact version, contribute upstream, maintain fork if needed

### Why Grafeo (revised from initial NO-GO)
1. Only pure Rust that does graph+vector+FTS in one engine
2. Cypher + SPARQL = ontological self-describing graph (impossible with SQLite)
3. Apache-2.0 = can fork freely, contribute back
4. Replaces 3 deps with 1 — less glue code, fewer bugs
5. Risk mitigated by fork + pin + trait CortexStore escape hatch

### Rejected alternatives
- **TurboQuant**: Wrong domain (KV cache compression, not vector search)
- **redb/fjall**: Wrong abstraction (KV-only, no FTS, no vector, no multi-process)
- **sqlite-vec/usearch**: Grafeo has HNSW built-in, no need for separate vector crate
- **Kuzu**: C++ FFI, stale (last commit Oct 2025)
- **SurrealDB**: BSL 1.1 license, AGPL-incompatible

## 3. Current Infrastructure (what exists today)

### Reusable (keep as-is or extend)

| Component | File | LOC | Reuse |
|-----------|------|-----|-------|
| RecordSpec (AST) | `nika-core/src/ast/record.rs` | 153 | 100% — extend with new fields |
| Record (struct) | `nika-engine/src/runtime/record.rs` | 225 | 70% — add cognitive fields |
| RecordCompressor | `nika-engine/src/runtime/record_compress.rs` | 360 | 90% — reuse for anticipation |
| RecordWriter | `nika-engine/src/store/record_writer.rs` | 253 | 95% — swap NDJSON→SQLite |
| RunContext records | `nika-engine/src/store/run_context.rs` | 80 | 80% — extend DashMap |
| ExecutorCompressorLlm | `nika-engine/src/runtime/executor_compressor.rs` | 100 | 100% — bridge pattern |
| nika:records tool | `nika-engine/src/runtime/builtin/records.rs` | 203 | Template for nika:recall |
| Output scanner | `nika-engine/src/runtime/output_scanner.rs` | 214 | 100% — security layer |
| Token budget | `nika-engine/src/binding/token_budget.rs` | 100+ | 100% — evidence packets |
| Daemon storage | `nika-daemon/src/storage.rs` | 500+ | 95% — add Cortex tables |
| EventLog | `nika-event/src/log.rs` | 400+ | 90% — add Cortex events |
| Introspect tools | Various | 300+ | 90% — extend with CSR |

**Total reusable: ~2,888 LOC across 12 files.**

### What changes (refactor)

| Current | Cortex Replacement |
|---------|-------------------|
| NDJSON files (`.nika/records/*.ndjson`) | SQLite `cortex.db` tables |
| Single Record struct per task | CortexNode with 6 memory types |
| No graph | petgraph in-memory + SQLite edges |
| No vector search | usearch HNSW i8 index |
| No FTS beyond daemon | FTS5 on nodes + anticipations |
| HOT/WARM/COLD tiers (not implemented) | Replaced by Cortex levels 0-5 |
| NovaNet as COLD tier | Removed — Cortex is self-contained |
| `nika:records` (single tool) | 9 Cortex builtin tools |

### What's removed

| Component | Why |
|-----------|-----|
| NDJSON persistence | Replaced by SQLite Cortex tables |
| NovaNet COLD tier dependency | Cortex is self-contained, NovaNet not available |
| 3-tier HOT/WARM/COLD naming | Replaced by 6-level memory hierarchy |

## 4. Architecture

### 6 Memory Levels (GAAMA-inspired hierarchy)

| Level | Type | What | Decay | Volume |
|-------|------|------|-------|--------|
| 0 | **Working** | Current context, evidence packets | Session-scoped | Transient |
| 1 | **Episodic** | Events, sessions, task results | FSRS-6 fast decay | High |
| 2 | **Semantic** | Facts, entities, typed relations | ACT-R slow decay | Medium |
| 3 | **Procedural** | Skills, patterns, Bayesian reliability | Utility-based | Low |
| 4 | **Reflective** | Meta-observations about patterns | Very slow | Low |
| 5 | **Conceptual** | Cross-cutting abstractions/themes | PageRank hubs | Very low |

**Consolidation flow**: Episodic → Semantic → Reflective → Conceptual (automatic via mechanism ⑤)

### 12 Cognitive Mechanisms

| # | Mechanism | Source | What |
|---|-----------|--------|------|
| ① | Hebbian strengthening | Shodh | Co-access → stronger links (+2.5%/-10%) |
| ② | Dual decay | FSRS-6 + ACT-R + Bjork | 3 decay models combined |
| ③ | Dopamine gate | D-MEM paper | Surprise × Utility threshold, -80% tokens |
| ④ | Prospective indexing | Kumiho (93.3%) | Write-time: anticipate future scenarios |
| ⑤ | Narrative consolidation | TraceMem + Vestige | Sleep replay, episode→semantic promotion |
| ⑥ | Contradiction detection | Kumiho AGM | Formal belief revision, supersedes |
| ⑦ | Salience encoding | Pensyve | 0.4×novelty+0.3×importance+0.1×ext+0.2×spec |
| ⑧ | Feedback correction | ICM | Learn from wrong recalls, closed-loop |
| ⑨ | Synaptic tagging | Vestige (Frey&Morris) | Retroactive importance boost (6h window) |
| ⑩ | Interference detection | Shodh | Proactive/retroactive, cosine>0.9 |
| ⑪ | Auto-linking | A-Mem Zettelkasten | Write-time: find related, create edges |
| ⑫ | Conditional triggers | Nocturne | Pattern-based auto-recall |

### 8-Signal Hybrid Retrieval

```
Query
  │
  ├→ ⓪ Trigger check (Nocturne conditional auto-recall)
  ├→ ① FTS5 BM25 (keywords + anticipations)
  ├→ ② usearch cosine i8 (semantic similarity, 98.9% recall)
  ├→ ③ Personalized PageRank (HippoRAG multi-hop)
  ├→ ④ ACT-R spreading activation (associative priming)
  ├→ ⑤ Query intent classification (5 intents)
  ├→ ⑥ Node confidence × FSRS retrievability
  ├→ ⑦ Interference penalty (suppress old similar)
  └→ ⑧ Salience boost
         │
         ▼
  RRF merge (adaptive k = max(1, count/10))
         │
         ▼
  Token budget filter → EvidencePackets
         │
         ▼
  RecallResult { packets, total_tokens, truncated }

  Recursive recall (RLM-inspired, max depth 3):
    If low relevance → extract entities → re-query → merge
```

### 8-Step Write Pipeline (nika:remember)

```
Input → 1. Dedup (blake3 + cosine>0.85)
      → 2. Dopamine gate (surprise × utility threshold)
      → 3. Salience encoding (4-factor score)
      → 4. Contradiction check (AGM revision)
      → 5. Auto-linking (Zettelkasten, cosine>0.6)
      → 6. Prospective indexing (LLM anticipation, if full processing)
      → 7. Synaptic tagging (retroactive boost, 6h window)
      → 8. Persist (SQLite + FTS5 + usearch + petgraph + provenance)
```

### 9 Builtin Tools

| Tool | Purpose |
|------|---------|
| `nika:remember` | Store with full 8-step write pipeline |
| `nika:recall` | 8-signal hybrid retrieval + recursive + assembly modes |
| `nika:revise` | Update with supersedes chain (never delete) |
| `nika:correct` | Feedback correction for wrong recalls |
| `nika:consolidate` | Merge similar, resolve contradictions, replay |
| `nika:cortex_schema` | Discover/create/evolve node types |
| `nika:cortex_audit` | CSR score, orphans, stale, integrity |
| `nika:cortex_export` | YAML/JSON/NDJSON export |
| `nika:cortex_history` | Changelog, diff, rollback |

### Context Assembly Modes (NovaNet-inspired)

| Mode | Description |
|------|-------------|
| `workflow` | All memory relevant to current workflow |
| `task` | Memory relevant to single task context |
| `knowledge` | Semantic facts only (no episodes) |
| `targeted` | Specific entity + N-hop graph neighborhood |

## 5. Data Model

### Core Structures

```rust
enum MemoryKind { Episodic, Semantic, Procedural, Working, Reflective, Conceptual }
enum Realm { System, User, Discovered }
enum EdgeType { Supports, Contradicts, Causes, DerivedFrom, SupersededBy, Refines, RelatedTo, PartOf, InstanceOf }
enum EdgeFamily { Causal, Semantic, Temporal, Structural }
```

### CortexNode
- id (blake3 hash), kind, node_type, realm, content, properties
- Provenance: source, confidence, created_at, updated_at, superseded_by
- Cognitive: activation (ACT-R), access_log, storage_strength, retrieval_strength, fsrs (FsrsState)
- Gating: surprise, utility
- Salience: composite score
- Kumiho: anticipations[]

### CortexEdge
- source, target, edge_type, family
- weight (Hebbian: +2.5%/-10%, floor 0.05, half-life 24h)

### NodeType (auto-evolving ontology)
- name, realm, parent, schema (JSON Schema), source, instance_count
- System types: fact, entity, event, skill, preference (readonly)
- Discovered types: auto-graduate after 10+ instances with confidence>0.8

### FsrsState
- difficulty, stability (half-life hours), elapsed, reps, lapses
- R(t,S) = (1+t/(9*S))^(-1)

### EvidencePacket
- node_id, content, relevance, distance, tokens, signal_scores
- SignalScores: bm25, cosine, pagerank, activation, intent, confidence, interference, salience

### SQLite Schema
See `2026-03-31-nika-cortex-data-model.md` for full CREATE TABLE statements.

## 6. Crate Structure

```
tools/nika-cortex/
├── Cargo.toml
└── src/
    ├── lib.rs                    # Cortex facade
    ├── store/
    │   ├── mod.rs                # trait CortexStore
    │   ├── sqlite.rs             # SQLite + FTS5 + schema migrations
    │   ├── vectors.rs            # usearch HNSW i8 wrapper
    │   ├── graph.rs              # petgraph in-memory + SQLite persistence
    │   └── dedup.rs              # blake3 + cosine 85% dedup
    ├── memory/
    │   ├── mod.rs                # MemoryKind enum
    │   ├── episodic.rs           # Events, sessions, timeline
    │   ├── semantic.rs           # Facts, entities, typed relations
    │   ├── procedural.rs         # Skills, Bayesian reliability
    │   ├── working.rs            # Evidence packets, token budgeting
    │   ├── reflective.rs         # Meta-observations
    │   └── conceptual.rs         # Cross-cutting themes
    ├── cognitive/
    │   ├── hebbian.rs            # ① Edge strengthening
    │   ├── decay.rs              # ② FSRS-6 + ACT-R + Bjork
    │   ├── gate.rs               # ③ Dopamine gate
    │   ├── anticipation.rs       # ④ Prospective indexing
    │   ├── consolidation.rs      # ⑤ Narrative threads + replay
    │   ├── contradiction.rs      # ⑥ AGM belief revision
    │   ├── salience.rs           # ⑦ Encoding importance
    │   ├── feedback.rs           # ⑧ Correction loop
    │   ├── tagging.rs            # ⑨ Synaptic tagging
    │   ├── interference.rs       # ⑩ Interference detection
    │   ├── autolink.rs           # ⑪ Zettelkasten auto-linking
    │   └── triggers.rs           # ⑫ Conditional auto-recall
    ├── retrieval/
    │   ├── signals.rs            # 8 signal extractors
    │   ├── rrf.rs                # Reciprocal Rank Fusion
    │   ├── pagerank.rs           # Personalized PageRank
    │   ├── activation.rs         # Spreading activation
    │   ├── recursive.rs          # RLM-style recursive recall
    │   └── assembly.rs           # Context assembly modes
    └── tools/
        ├── remember.rs           # nika:remember
        ├── recall.rs             # nika:recall
        ├── revise.rs             # nika:revise
        ├── correct.rs            # nika:correct
        ├── consolidate.rs        # nika:consolidate
        ├── schema.rs             # nika:cortex_schema
        ├── audit.rs              # nika:cortex_audit
        ├── export.rs             # nika:cortex_export
        └── history.rs            # nika:cortex_history
```

**~45 files, ~15-20K LOC estimated**

## 7. What's Better Than Everything Else

| Dimension | mem0 (51K) | Graphiti (24K) | Hermes (19K) | Nika Cortex |
|-----------|-----------|---------------|-------------|-------------|
| Hebbian learning | No | No | No | **Yes** |
| FSRS-6 + ACT-R decay | No | No | No | **Yes** |
| Dopamine gate (-80% tokens) | No | No | No | **Yes** |
| Prospective indexing (93.3%) | No | No | No | **Yes** |
| Causal edges | No | No | No | **Yes** |
| Auto-evolving ontology | No | No | No | **Yes** |
| Feedback correction loop | No | No | No | **Yes** |
| Synaptic tagging | No | No | No | **Yes** |
| Interference detection | No | No | No | **Yes** |
| Auto-linking (Zettelkasten) | No | No | No | **Yes** |
| Conditional triggers | No | No | No | **Yes** |
| 8-signal retrieval | No | No | No | **Yes** |
| Recursive recall (RLM) | No | No | No | **Yes** |
| Single binary | No | No | No | **Yes** |
| Zero LLM for store | No | No | Partial | **Yes** |
| Rust native | No | No | No | **Yes** |

## 8. Migration Plan (Current → Cortex)

### Phase 1: Foundation (coexist with current Records)
- Add `nika-cortex` crate to workspace
- SQLite schema + FTS5 + usearch + petgraph
- `nika:remember` + `nika:recall` + `nika:revise` tools
- Records continue working unchanged
- Cortex tools available alongside `nika:records`

### Phase 2: Integration
- RecordWriter migrates from NDJSON to Cortex SQLite
- `nika:records` becomes alias for `nika:recall --mode=workflow`
- Cognitive mechanisms (Hebbian, FSRS-6, gate, salience)
- Auto-linking + contradiction detection

### Phase 3: Advanced
- Prospective indexing + anticipations
- Consolidation engine (background daemon service)
- Feedback correction + synaptic tagging
- Interference detection + conditional triggers

### Phase 4: Polish
- 8-signal retrieval fully wired
- Recursive recall (RLM-style)
- Auto-evolving ontology
- CSR audit + history/rollback
- Export tool

### What gets deprecated
- `RecordWriter` NDJSON persistence → SQLite
- HOT/WARM/COLD tier naming → 6-level hierarchy
- NovaNet COLD tier dependency → removed entirely
- `nika:records` tool → alias to `nika:recall`

### What stays unchanged
- `RecordSpec` AST field (`record:` in YAML)
- `Record` struct (extended, not replaced)
- `RecordCompressor` (reused for anticipation extraction)
- `ExecutorCompressorLlm` (bridge pattern)
- `OutputScanner` (security layer)
- Token budget estimation
- EventLog (extended with new event types)
- Daemon SQLite storage (extended with Cortex tables)

## 9. Research Sources

### Papers (6 key)
- Kumiho (2603.17244) — Prospective indexing, AGM, 93.3% LoCoMo
- D-MEM (2603.14597) — Dopamine gate, -80% tokens
- GAAMA (2603.27910) — 4-node hierarchy + PageRank
- TraceMem (2602.09712) — Narrative consolidation
- AMA-Agent (2602.22769) — Causal graphs, +11%
- RLM (2512.24601) — Recursive self-invocation, 100x context

### Rust Projects (6 studied)
- Shodh (182 stars) — Hebbian constants, 55ms store
- Vestige (456 stars) — FSRS-6, prediction gating, synaptic tagging
- ICM (129 stars) — Dual model, typed relations, feedback
- Pensyve (1 star) — 6-signal RRF, Bayesian procedural, salience
- Nocturne (867 stars) — First-person sovereignty, conditional triggers
- Grafeo (463 stars) — Evaluated, NO-GO (too young)

### NovaNet Concepts Ported
- Evidence packets + token budgeting
- Arc families (causal, semantic, temporal, structural)
- Self-describing schema (novanet_describe/introspect)
- Provenance tracking (ADR-042)
- Quality audit (CSR score)
- Context assembly modes (4 modes adapted)
