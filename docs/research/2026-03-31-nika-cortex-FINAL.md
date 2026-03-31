# Nika Cortex — FINAL Design Report

> Status: **LOCKED** — Ready for implementation planning
> Date: 2026-03-31
> Research: 19 agents, 37+ papers, 80+ crates, verified from source code
> This document supersedes all previous Cortex docs.

---

## 1. Vision

Nika Cortex is a **cognitive memory engine** embedded natively in Nika. It replaces the planned 3-tier HOT/WARM/COLD architecture and the NovaNet dependency with a self-contained system combining 12 cognitive mechanisms from neuroscience and cutting-edge AI research.

**Tagline**: "The memory engine that doesn't exist yet."

**Principle**: First-person memory — the agent DECIDES what to remember via `nika:remember`, not passive extraction.

---

## 2. Confirmed Stack (FINAL — verified from source code)

| Layer | Crate | Version | What it provides | Impact |
|-------|-------|---------|-----------------|--------|
| **Graph+Vector+FTS** | `grafeo` | =0.5.30 | Cypher, GQL, SPARQL, HNSW (Scalar/Binary/PQ), BM25, RRF hybrid, 22 graph algos, WAL, `.grafeo` single-file | +3MB |
| **Metadata** | `rusqlite` (bundled) | 0.39 | FSRS-6 state, access logs, trigger rules, memory changelog, daemon jobs | 0 (already in workspace) |
| **Embeddings** | `fastembed` | 5.13 | 35+ ONNX models, static ONNX Runtime, reranking, sparse SPLADE | +3MB (opt-in) |

### Why 3 crates, not 2?

**Grafeo HAS built-in ONNX embeddings** (`embed` feature). BUT it uses `ort` with `load-dynamic` — the ONNX Runtime shared library must be installed on the system separately. `fastembed` bundles the runtime statically.

**Decision**: Use Grafeo for graph+vector+FTS. Use `fastembed` for embeddings (static, portable, no system dependency). `fastembed` remains opt-in via feature flag `cortex-embed`. Without it, Cortex works with text-only search (FTS5 BM25 via Grafeo). If Grafeo's `embed` feature matures with static linking, we can drop fastembed later.

### Grafeo capabilities (verified from source code)

| Feature | Details |
|---------|---------|
| **Query languages** | GQL (ISO 39075) default. Cypher, SPARQL, Gremlin, GraphQL, SQL/PGQ with feature flags |
| **Vector index** | HNSW with 3 quantization types: Scalar (f32→u8, 4x), Binary (1-bit, 32x), Product (k-means, 8-32x) |
| **Full-text** | BM25 with Unicode tokenizer |
| **Hybrid search** | Native RRF fusion (vector + text + graph) |
| **Graph algos** | PageRank, SSSP, Louvain community detection, centrality, BFS, DFS, WCC, CDLP, LCC |
| **Persistence** | `.grafeo` single-file + WAL sidecar during writes. Crash-safe dual-header. |
| **Concurrent access** | Writer = exclusive lock, Readers = shared lock. CLI reads last checkpoint while daemon writes. |
| **Memory-mapped** | Optional `mmap` feature for large vector datasets |
| **ONNX embeddings** | Built-in via `embed` feature (load-dynamic). 3 presets + any HF model. |

### Concurrent access model (CRITICAL — verified)

```
Daemon (write)     → exclusive lock (fs2::try_lock_exclusive)
CLI (read-only)    → shared lock (fs2::try_lock_shared) → sees last checkpoint
TUI (read-only)    → shared lock → sees last checkpoint
Multiple readers   → OK (shared locks coexist)
```

This works for our use case:
- Daemon writes memories during/after workflow execution
- CLI/TUI read memories for display/export
- Readers see last checkpoint, not in-flight mutations → acceptable latency

### What Grafeo replaces

| Before (3 crates) | After (1 crate) |
|-------------------|-----------------|
| ~~petgraph~~ (graph algorithms) | Grafeo: 22 built-in algos |
| ~~usearch~~ (HNSW vector search) | Grafeo: HNSW with Scalar/Binary/PQ |
| ~~FTS5~~ (full-text search) | Grafeo: BM25 built-in |
| + manual RRF glue code | Grafeo: native hybrid RRF |

petgraph stays in workspace for Nika's DAG engine (not Cortex).

### Feature flags

```toml
[dependencies]
grafeo = { version = "=0.5.30", default-features = false, features = [
    "embedded",    # GQL + HNSW + BM25 + hybrid + algos + .grafeo file
    # "cypher",    # Cypher parser (add when needed for NovaNet compat)
    # "sparql",    # SPARQL parser (add for ontology self-description)
    # "embed",     # ONNX embedding generation (load-dynamic)
] }
rusqlite = { workspace = true }
fastembed = { version = "5", optional = true }  # feature: cortex-embed

[features]
default = ["cortex"]
cortex = ["grafeo/embedded"]
cortex-embed = ["dep:fastembed"]
cortex-cypher = ["grafeo/cypher"]
cortex-sparql = ["grafeo/sparql"]
cortex-full = ["cortex-embed", "cortex-cypher", "cortex-sparql"]
```

---

## 3. Fork Strategy

```
UPSTREAM : github.com/GrafeoDB/grafeo (Apache-2.0, 463 stars, 10 forks)
FORK    : github.com/SuperNovae-st/grafeo

supernovae-hq/
├── nika/              [submodule] workflow engine
├── novanet/           [submodule] knowledge graph (legacy, not for memory)
├── grafeo/            [submodule] graph DB fork ← NEW
├── homebrew-tap/      [submodule]
└── ...

Cargo.toml phases:
  Phase 1: grafeo = { version = "=0.5.30", ... }      (crates.io pinned)
  Phase 2: grafeo = { git = "...SuperNovae-st/grafeo", rev = "..." }  (if patches needed)
  Phase 3: nika-grafeo = "0.6.0-sn.1"                 (if heavy divergence)

Git branches:
  main      — sync with upstream GrafeoDB/grafeo
  sn/nika   — our patches, fixes, Nika-specific improvements

Contributions: PR from sn/nika → GrafeoDB/grafeo upstream
```

---

## 4. Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                      NIKA CORTEX                              │
│              "The memory that thinks about itself"            │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐    │
│  │               GRAFEO (graph engine)                   │    │
│  │                                                       │    │
│  │  Nodes: CortexNode (6 memory levels)                 │    │
│  │  Edges: CortexEdge (9 types, 4 families, Hebbian)    │    │
│  │  Vectors: HNSW (Scalar u8 / PQ k-means)              │    │
│  │  Text: BM25 full-text on content + anticipations      │    │
│  │  Hybrid: native RRF (vector + text + graph)           │    │
│  │  Algos: PageRank, Louvain, SSSP, centrality           │    │
│  │  Queries: GQL default, Cypher opt-in, SPARQL opt-in   │    │
│  │  Persistence: ~/.nika/egghead.grafeo (single file)     │    │
│  │  Concurrent: exclusive writer + shared readers         │    │
│  └──────────────────────────────────────────────────────┘    │
│                           +                                   │
│  ┌──────────────────────────────────────────────────────┐    │
│  │            RUSQLITE (metadata sidecar)                │    │
│  │                                                       │    │
│  │  Tables:                                              │    │
│  │  ├── fsrs_state (per-node FSRS-6 scheduler)          │    │
│  │  ├── access_log (timestamps for ACT-R activation)     │    │
│  │  ├── trigger_rules (conditional auto-recall patterns) │    │
│  │  ├── memory_changelog (mutations for rollback)        │    │
│  │  ├── schema_versions (ontology evolution history)     │    │
│  │  └── daemon jobs (existing)                           │    │
│  │  File: ~/.nika/egghead-meta.db                         │    │
│  └──────────────────────────────────────────────────────┘    │
│                           +                                   │
│  ┌──────────────────────────────────────────────────────┐    │
│  │          FASTEMBED (opt-in, feature-gated)            │    │
│  │                                                       │    │
│  │  Model: BGE-small-en-v1.5 (384d, 33MB, static ONNX) │    │
│  │  Or: multilingual-e5-small (384d, multilingual)       │    │
│  │  Reranking built-in                                   │    │
│  │  Feature flag: cortex-embed                           │    │
│  └──────────────────────────────────────────────────────┘    │
│                                                               │
│  12 cognitive mechanisms                                      │
│  6 memory levels                                              │
│  8-signal hybrid retrieval                                    │
│  9 builtin tools + MCP server + import/export                │
└──────────────────────────────────────────────────────────────┘
```

### File layout on disk

```
~/.nika/
├── egghead.grafeo         # Grafeo graph DB (nodes, edges, vectors, text index)
├── egghead-meta.db        # SQLite metadata (FSRS, logs, triggers, changelog)
├── daemon/
│   ├── nika.sock         # Daemon socket (existing)
│   ├── daemon.db         # Daemon jobs DB (existing)
│   └── ...
└── ...
```

---

## 5. Data Model

### 6 Memory Levels (GAAMA hierarchy)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryKind {
    Working,      // L0: Evidence packets, session-scoped, in-memory only
    Episodic,     // L1: Events, sessions, timeline. Fast FSRS-6 decay.
    Semantic,     // L2: Facts, entities, relations. Slow ACT-R decay.
    Procedural,   // L3: Skills, patterns, Bayesian reliability tracking.
    Reflective,   // L4: Meta-observations about patterns. Auto-generated.
    Conceptual,   // L5: Cross-cutting themes. PageRank hubs. Very slow decay.
}
```

### CortexNode (stored as Grafeo graph node)

```rust
pub struct CortexNode {
    // --- Identity ---
    pub id: NodeId,               // blake3 content hash
    pub kind: MemoryKind,         // L0-L5
    pub node_type: String,        // "fact" | "entity" | "event" | "skill" | custom
    pub realm: Realm,             // System | User | Discovered
    pub content: String,          // Memory content
    pub properties: Value,        // Type-specific structured data

    // --- Provenance (NovaNet ADR-042) ---
    pub source: Source,           // Workflow{id} | User | Inferred | Consolidated
    pub confidence: f64,          // 0.0-1.0
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub superseded_by: Option<NodeId>,

    // --- Gating (D-MEM) ---
    pub surprise: f64,            // 0.0 = routine, 1.0 = very surprising
    pub utility: f64,             // 0.0 = never used, 1.0 = critical

    // --- Salience (Pensyve 4-factor) ---
    pub salience: f64,            // 0.4×novelty + 0.3×importance + 0.1×ext + 0.2×spec

    // --- Kumiho Prospective Indexing ---
    pub anticipations: Vec<String>, // Future scenarios (searchable via BM25)

    // --- Procedural only: Bayesian reliability (Pensyve) ---
    pub success_count: Option<u32>,  // Times this skill succeeded
    pub failure_count: Option<u32>,  // Times this skill failed
    pub reliability: Option<f64>,    // success / (success + failure)

    // --- Vector embedding ---
    pub embedding: Option<Vec<f32>>, // Generated by fastembed, stored in Grafeo HNSW
}

// Cognitive state stored SEPARATELY in SQLite (frequent updates):
pub struct CognitiveState {  // In egghead-meta.db
    pub node_id: NodeId,
    pub activation: f64,              // ACT-R: B_i = ln(Σ t_j^(-0.5))
    pub storage_strength: f64,        // Bjork: encoding quality (monotone ↑)
    pub retrieval_strength: f64,      // Bjork: accessibility (decays)
    pub fsrs: FsrsState,             // FSRS-6 scheduler
    pub access_log: Vec<DateTime<Utc>>, // For ACT-R calculation
}
```

**Why split?** Cognitive state changes on EVERY access (access_log grows, activation recalculates, FSRS updates). Grafeo nodes are optimized for graph traversal, not frequent property updates. SQLite handles frequent small writes better via WAL.

### CortexEdge (stored as Grafeo graph edge)

```rust
pub struct CortexEdge {
    pub source: NodeId,
    pub target: NodeId,
    pub edge_type: EdgeType,
    pub family: EdgeFamily,
    pub weight: f64,              // Hebbian: +2.5%/-10%, floor 0.05, half-life 24h
    pub created_at: DateTime<Utc>,
}

pub enum EdgeType {
    Supports, Contradicts, Causes, DerivedFrom, SupersededBy,
    Refines, RelatedTo, PartOf, InstanceOf,
}

pub enum EdgeFamily {
    Causal,      // causes, derived_from
    Semantic,    // supports, contradicts, refines, related
    Temporal,    // superseded_by
    Structural,  // part_of, instance_of
}
```

### NodeType (auto-evolving ontology)

```rust
pub struct NodeType {
    pub name: String,             // "fact", "entity", "seo_keyword", custom
    pub realm: Realm,             // System | User | Discovered
    pub parent: Option<String>,   // Inheritance
    pub schema: Value,            // JSON Schema for properties validation
    pub source: String,           // "builtin" | "workflow:podcast-gen"
    pub instance_count: u64,      // For auto-graduation: Discovered→User at 10+
    pub created_at: DateTime<Utc>,
}
```

### FSRS-6 State (in SQLite)

```rust
pub struct FsrsState {
    pub difficulty: f64,   // 0.0-1.0
    pub stability: f64,    // Half-life in hours
    pub elapsed: f64,      // Hours since last access
    pub reps: u32,         // Successful recalls
    pub lapses: u32,       // Failed recalls
}

impl FsrsState {
    pub fn retrievability(&self) -> f64 {
        (1.0 + self.elapsed / (9.0 * self.stability)).powi(-1)
    }
}
```

### Evidence Packet (Working Memory, in-memory only)

```rust
pub struct EvidencePacket {
    pub node_id: NodeId,
    pub content: String,
    pub relevance: f64,           // Combined score from all signals
    pub distance: usize,          // Graph hops from query
    pub tokens: usize,            // Pre-calculated for budget
    pub signal_scores: SignalScores,
}

pub struct SignalScores {
    pub bm25: f64,                // Grafeo BM25
    pub cosine: f64,              // Grafeo HNSW
    pub pagerank: f64,            // Grafeo PageRank
    pub activation: f64,          // ACT-R (from SQLite)
    pub intent: f64,              // Query intent classification (Rust)
    pub confidence: f64,          // Node confidence × FSRS retrievability
    pub interference: f64,        // Interference penalty (Rust)
    pub salience: f64,            // Salience boost (Rust)
}
```

---

## 6. 12 Cognitive Mechanisms

| # | Mechanism | Source | Constants / Formula |
|---|-----------|--------|-------------------|
| ① | **Hebbian strengthening** | Shodh (Bi&Poo 1998) | +2.5% helpful, -10% misleading, floor 0.05, half-life 24h, max degree 500 |
| ② | **Dual decay** | FSRS-6 + ACT-R + Bjork | R(t,S)=(1+t/9S)^(-1), B=ln(Σt^(-0.5)), storage↑monotone, retrieval↓ |
| ③ | **Dopamine gate** | D-MEM (2603.14597) | surprise×utility>0.3→full, <0.1→routine, saves ~80% tokens |
| ④ | **Prospective indexing** | Kumiho (93.3% LoCoMo) | Write-time: LLM generates future scenarios, indexed in BM25 |
| ⑤ | **Narrative consolidation** | TraceMem + Vestige | Background: cluster episodes→threads, extract→semantic, SWR 70/30 replay |
| ⑥ | **Contradiction detection** | Kumiho AGM | Contraction before expansion, supersedes chain, never delete |
| ⑦ | **Salience encoding** | Pensyve | 0.4×novelty + 0.3×importance + 0.1×extremity + 0.2×specificity |
| ⑧ | **Feedback correction** | ICM | Wrong recall → Contradicts edge + Hebbian penalty -10%, closed-loop |
| ⑨ | **Synaptic tagging** | Vestige (Frey&Morris 1997) | 6h window, retroactive salience boost on related recent memories |
| ⑩ | **Interference detection** | Shodh | cosine>0.9 = interference candidate, flag for consolidation ⑤ |
| ⑪ | **Auto-linking** | A-Mem Zettelkasten | cosine>0.6 → RelatedTo edge, check Refines/Contradicts, max 500 edges |
| ⑫ | **Conditional triggers** | Nocturne | Pattern rules in SQLite, check on every recall, auto-inject critical facts |

---

## 7. 8-Signal Retrieval Pipeline

```
Query
  │
  ├─→ ⓪ TRIGGER CHECK (SQLite → auto-inject critical memories)
  │
  ├─→ GRAFEO HYBRID QUERY (signals ①②③ in ONE call):
  │     ① BM25 on content + anticipations (full-text)
  │     ② HNSW cosine on embedding (vector similarity)
  │     ③ PageRank from entity seed nodes (graph multi-hop)
  │     → Grafeo native RRF merge
  │
  ├─→ RUST POST-PROCESSING (signals ④⑤⑥⑦⑧):
  │     ④ ACT-R spreading activation (from SQLite access_log)
  │     ⑤ Query intent classification (5 intents: Question|Action|Recall|Code|Visual)
  │     ⑥ Node confidence × FSRS retrievability (from SQLite)
  │     ⑦ Interference penalty (cosine>0.9 between results)
  │     ⑧ Salience boost (high-salience memories promoted)
  │
  ├─→ FINAL RRF MERGE (adaptive k = max(1, count/10))
  │     Combine Grafeo scores with Rust post-processing scores
  │
  ├─→ TOKEN BUDGET FILTER
  │     Sort by relevance, accumulate tokens, truncate at budget
  │     Each result = EvidencePacket with pre-calculated token count
  │
  └─→ RecallResult { packets, total_tokens, budget_used, truncated }

  RECURSIVE RECALL (RLM-inspired, max depth 3):
    If top results have low relevance (<0.3):
    → Extract entities from top results
    → Re-query Grafeo with extracted entities as seeds
    → Merge across recursion levels
    → Deduplicate by node_id
```

### Context Assembly Modes

| Mode | Grafeo Query | Use Case |
|------|-------------|----------|
| `workflow` | `MATCH (n) WHERE n.source STARTS WITH 'workflow:{id}'` | All memory from current workflow |
| `task` | `MATCH (n)-[*1..2]-(m) WHERE n.id = $task_node_id` | 2-hop neighborhood of task |
| `knowledge` | `MATCH (n:Semantic) WHERE ...` | Facts only, no episodes |
| `targeted` | `MATCH (n {id: $entity})-[*1..N]-(m) RETURN m` | Entity + N-hop expansion |

---

## 8. Write Pipeline (nika:remember)

```
Input
  │
  ├─1. DEDUP
  │     blake3 hash → exact match? → skip
  │     Grafeo HNSW cosine > 0.85 → merge with existing
  │
  ├─2. DOPAMINE GATE ③
  │     surprise = 1.0 - max_cosine_to_existing (novelty proxy)
  │     utility = source.confidence × workflow.importance
  │     surprise × utility < 0.1 → ROUTINE (store only, skip 3-6)
  │     surprise × utility > 0.3 → FULL PROCESSING
  │
  ├─3. SALIENCE ENCODING ⑦
  │     0.4×novelty + 0.3×importance + 0.1×extremity + 0.2×specificity
  │
  ├─4. CONTRADICTION CHECK ⑥
  │     Grafeo: find facts with high cosine AND different content
  │     AGM: contraction before expansion → supersede old fact
  │     Emit ContradictionDetected event
  │
  ├─5. AUTO-LINKING ⑪
  │     Grafeo HNSW: find cosine > 0.6 neighbors
  │     Create edges: RelatedTo (weight=cosine), check Refines/Contradicts
  │     Respect MAX_ENTITY_DEGREE = 500
  │
  ├─6. PROSPECTIVE INDEXING ④ (only if FULL PROCESSING)
  │     LLM call: "In what future scenarios would this fact be useful?"
  │     Store anticipations as node property → indexed in BM25
  │
  ├─7. SYNAPTIC TAGGING ⑨
  │     If this fact is important (salience > 0.7):
  │       Find related facts created in last 6h with low salience
  │       Retroactively boost their salience
  │
  └─8. PERSIST
        Grafeo: CREATE node + edges + vector embedding
        SQLite: INSERT cognitive state (FSRS initial, access_log)
        Emit MemoryStored event with pipeline stats
```

---

## 9. Crate Structure (FINAL)

```
tools/nika-cortex/
├── Cargo.toml
└── src/
    ├── lib.rs                      # Cortex facade (open, close, health)
    │
    ├── store/
    │   ├── mod.rs                  # trait CortexStore (escape hatch if Grafeo dies)
    │   ├── grafeo.rs               # Grafeo graph engine wrapper
    │   ├── meta.rs                 # SQLite metadata (FSRS, logs, triggers, changelog)
    │   └── dedup.rs                # blake3 exact + cosine 85% dedup
    │
    ├── memory/
    │   ├── mod.rs                  # MemoryKind enum (6 levels)
    │   ├── episodic.rs             # Events, sessions, narrative threads
    │   ├── semantic.rs             # Facts, entities, typed relations
    │   ├── procedural.rs           # Skills, Bayesian reliability tracking
    │   ├── working.rs              # Evidence packets, token budgeting
    │   ├── reflective.rs           # Meta-observations (auto-generated by consolidation)
    │   └── conceptual.rs           # Cross-cutting themes (Louvain clusters, PageRank hubs)
    │
    ├── cognitive/
    │   ├── mod.rs                  # Constants module
    │   ├── hebbian.rs              # ① Edge strengthening
    │   ├── decay.rs                # ② FSRS-6 + ACT-R + Bjork dual-strength
    │   ├── gate.rs                 # ③ Dopamine gate
    │   ├── anticipation.rs         # ④ Prospective indexing
    │   ├── consolidation.rs        # ⑤ Narrative threads + sleep replay
    │   ├── contradiction.rs        # ⑥ AGM belief revision
    │   ├── salience.rs             # ⑦ Encoding importance (4-factor)
    │   ├── feedback.rs             # ⑧ Correction loop
    │   ├── tagging.rs              # ⑨ Synaptic tagging (6h window)
    │   ├── interference.rs         # ⑩ Proactive/retroactive interference
    │   ├── autolink.rs             # ⑪ Zettelkasten auto-linking
    │   └── triggers.rs             # ⑫ Conditional auto-recall
    │
    ├── retrieval/
    │   ├── mod.rs                  # HybridRetriever
    │   ├── grafeo_query.rs         # Grafeo hybrid (BM25+HNSW+PageRank in 1 call)
    │   ├── postprocess.rs          # 5 Rust signals (ACT-R, intent, confidence, interference, salience)
    │   ├── rrf.rs                  # Final RRF merge (Grafeo scores + Rust scores)
    │   ├── recursive.rs            # RLM-style recursive recall (max depth 3)
    │   └── assembly.rs             # 4 context assembly modes
    │
    ├── tools/                      # 9 builtin nika:* tools
    │   ├── remember.rs             # 8-step write pipeline
    │   ├── recall.rs               # 8-signal retrieval + recursive + assembly
    │   ├── revise.rs               # Supersedes chain (never delete)
    │   ├── correct.rs              # Feedback correction loop
    │   ├── consolidate.rs          # Merge + contradiction resolve + replay
    │   ├── schema.rs               # Auto-evolving ontology (GQL/SPARQL)
    │   ├── audit.rs                # CSR score, orphans, stale, integrity
    │   ├── export.rs               # YAML/JSON/NDJSON export
    │   └── history.rs              # Changelog, diff, rollback
    │
    ├── mcp/                        # Cortex as MCP server (innovation #9)
    │   ├── mod.rs                  # MCP server exposing Cortex to external tools
    │   └── tools.rs                # MCP tool definitions
    │
    └── import/                     # Memory import from other systems
        ├── mod.rs                  # Import trait
        ├── hermes.rs               # Import from Hermes SKILL.md
        ├── claude.rs               # Import from Claude MEMORY.md
        └── ndjson.rs               # Import from Nika NDJSON records (migration)

~50 files, ~18-22K LOC estimated
```

---

## 10. Innovations (what nobody else has)

### Grafeo-enabled (impossible without a graph DB)

| # | Innovation | What |
|---|-----------|------|
| 1 | **Cypher-native retrieval** | `MATCH (n:Fact)-[:CAUSES]->(m) RETURN m` — readable, debuggable |
| 2 | **SPARQL ontology** | Self-describing schema in W3C standard RDF |
| 3 | **Graph-vector hybrid queries** | Vector search WITHIN graph traversal (native) |
| 4 | **Louvain concept generation** | Community detection → auto Conceptual nodes (L5) |
| 5 | **Causal chain discovery** | SSSP between any two facts → causal explanation |
| 6 | **TUI graph panel** | Grafeo data → ASCII knowledge graph in terminal |

### Nika-specific (unique to our system)

| # | Innovation | What |
|---|-----------|------|
| 7 | **Workflow-as-procedural-memory** | Successful .nika.yaml auto-stored as L3 with Bayesian reliability |
| 8 | **Memory-guided orchestration** | P-ORCHESTRATE queries Cortex for past results → smarter planning |
| 9 | **Cortex MCP server** | Other tools (Claude Code, Cursor) can query Nika's memory |
| 10 | **Memory import** | Import from Hermes SKILL.md, Claude MEMORY.md, ICM, NDJSON |
| 11 | **Cross-workflow memory** | Workflow A learnings → available in workflow B automatically |
| 12 | **Embedding cache** | Embeddings cached as Grafeo node properties → no re-computation |
| 13 | **Memory federation** | Multiple Nika instances share memory via Cortex MCP |

---

## 11. Comparison (FINAL — honest)

| Dimension | mem0 (51K) | Graphiti (24K) | Hermes (19K) | Nika Cortex |
|-----------|-----------|---------------|-------------|-------------|
| Graph engine | Neo4j (external) | Neo4j (external) | None | **Grafeo (embedded, pure Rust)** |
| Hebbian learning | No | No | No | **Yes** |
| FSRS-6 + ACT-R | No | No | No | **Yes** |
| Dopamine gate | No | No | No | **Yes** |
| Prospective indexing | No | No | No | **Yes** |
| Causal edges | No | No | No | **Yes** |
| Auto-evolving ontology | No | No | No | **Yes (SPARQL)** |
| Feedback correction | No | No | No | **Yes** |
| Synaptic tagging | No | No | No | **Yes** |
| Interference detection | No | No | No | **Yes** |
| Auto-linking | No | No | No | **Yes** |
| Conditional triggers | No | No | No | **Yes** |
| 8-signal retrieval | No | No | No | **Yes** |
| Recursive recall | No | No | No | **Yes** |
| Bayesian procedural | No | No | No | **Yes** |
| MCP server | No | Yes | Yes | **Yes** |
| Single binary | No | No | No | **Yes** |
| Zero external service | No | No | No | **Yes** |
| Rust native | No | No | No | **Yes** |
| Import from competitors | No | No | No | **Yes** |

---

## 12. Migration from Current Records

### What stays unchanged
- `RecordSpec` AST field (`record:` in YAML)
- `RecordCompressor` (reused for anticipation extraction in mechanism ④)
- `ExecutorCompressorLlm` (bridge pattern for LLM calls)
- `OutputScanner` (security layer for `nika:remember`)
- Token budget estimation (evidence packet filtering)
- EventLog (extended with Cortex events)
- Daemon SQLite (extended with cortex-meta tables)

### What changes
| Current | Cortex |
|---------|--------|
| NDJSON files | Grafeo `.grafeo` single-file |
| `Record` struct (flat) | `CortexNode` (6 levels, cognitive state) |
| `nika:records` (1 tool) | 9 Cortex tools |
| No graph | Grafeo graph with typed edges |
| No vector search | Grafeo HNSW |
| No consolidation | Background daemon service |
| NovaNet COLD tier | Removed (Cortex is self-contained) |

### What's deprecated
- NDJSON `RecordWriter` → `import/ndjson.rs` migration path
- HOT/WARM/COLD naming → 6-level hierarchy (L0-L5)
- `nika:records` → alias for `nika:recall --mode=workflow`

---

## 13. Technologies Referenced but NOT in Core Stack

| Technology | Relevance to Nika | Status |
|-----------|-------------------|--------|
| **rig-rlm** (66 stars) | RLM recursive pattern using rig-core (already in Nika) | STUDY for recursive recall implementation |
| **tq-kv** (62 DL) | TurboQuant KV cache compression for GGUF | FUTURE: `provider: native` optimization, not Cortex |
| **RaBitQ** (SIGMOD) | SOTA vector quantization for 10M+ scale | FUTURE: if Cortex grows beyond 10M nodes |
| **Memvid** (13.7K stars) | Single-file memory format with Tantivy+HNSW | STUDY: append-only smart frames pattern |
| **Oxigraph** | SPARQL/RDF (Grafeo has this built-in) | REPLACED by Grafeo |
| **petgraph** | Graph algorithms | STAYS for DAG engine, NOT used in Cortex |

---

## 14. Research Sources

### Papers (6 key + 31 surveyed)
- Kumiho (2603.17244) — Prospective indexing, AGM, 93.3% LoCoMo
- D-MEM (2603.14597) — Dopamine gate, -80% tokens
- GAAMA (2603.27910) — 4-node hierarchy + PageRank
- TraceMem (2602.09712) — Narrative consolidation
- AMA-Agent (2602.22769) — Causal graphs, +11%
- RLM (2512.24601) — Recursive self-invocation, 100x context

### Rust Projects (7 studied)
- Grafeo (463 stars) — Graph DB, ADOPTED as core engine
- Shodh (182 stars) — Hebbian constants
- Vestige (456 stars) — FSRS-6, prediction gating, synaptic tagging
- ICM (129 stars) — Dual model, typed relations, feedback
- Pensyve (1 star) — 6-signal RRF, Bayesian procedural
- Nocturne (867 stars) — First-person sovereignty, triggers
- Memvid (13.7K stars) — Single-file format patterns

### NovaNet Concepts Ported
- Evidence packets + token budgeting
- Arc families → EdgeFamily enum
- Self-describing schema → SPARQL on Grafeo
- Provenance tracking (ADR-042) → Source enum
- Quality audit (CSR) → nika:egghead_audit
- Context assembly modes (4 adapted)

### Total Research
- 19 research agents deployed
- 37+ papers analyzed
- 80+ Rust crates examined
- 7 Rust projects deep-dived
- Grafeo verified from source code (15 files, 4 crates)
