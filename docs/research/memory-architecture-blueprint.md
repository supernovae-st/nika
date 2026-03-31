# Research Report: Advanced Memory Architecture for AI Agents

**Date**: 2026-03-31
**Author**: Research synthesis for Nika/NovaNet architecture planning
**Scope**: Cognitive science models, self-evolving ontologies, temporal reasoning, hybrid retrieval, and a complete blueprint for a next-generation agent memory system.

---

## Executive Summary

No existing system combines all four cognitive memory types (episodic, semantic, procedural, working) with a self-evolving ontology, bi-temporal reasoning, hybrid retrieval (graph + vector + FTS), and memory consolidation -- in a single embedded binary. The closest attempts (MemGPT/Letta, HippoRAG v2, CoALA, A-MEM) each solve one or two dimensions well but leave the others unaddressed. This report synthesizes the state of the art across eight research dimensions and proposes a unified architecture called **Mnemos** (from Greek "memory") that could be built on top of Nika/NovaNet's existing infrastructure.

---

## Part 1: Cognitive Science Foundation

### The Four Memory Systems (Tulving + Baddeley)

Human memory is not monolithic. Tulving (1972) distinguished episodic from semantic memory; Baddeley (1974) formalized working memory; Squire (1987) added the procedural/declarative split. These are the four pillars:

| Memory Type | Neuroscience Substrate | What It Stores | Access Pattern | Computational Analog |
|-------------|----------------------|----------------|----------------|---------------------|
| **Episodic** | Hippocampus (CA3/CA1) | Events with spatiotemporal context (what/where/when) | Cue-based recall, pattern completion | Timestamped event logs, interaction traces |
| **Semantic** | Neocortex (temporal lobe) | Facts, concepts, relationships, ontologies | Spreading activation, association | Knowledge graph, ontology, embeddings |
| **Procedural** | Basal ganglia, cerebellum | Skills, habits, motor sequences | Automatic execution, chunking | Executable routines, learned tool chains |
| **Working** | Prefrontal cortex (dorsolateral) | Active task context (~7 items) | Rapid access, manipulation | LLM context window, active memory buffer |

### How They Interact: The Consolidation Loop

The critical insight is that these are not independent -- they form a consolidation pipeline:

```
                    ENCODING                    CONSOLIDATION                  RETRIEVAL
                    --------                    -------------                  ---------

Sensory Input --> [Working Memory] --encode--> [Episodic Memory] --sleep/replay--> [Semantic Memory]
                       |                              |                                |
                       |                              |                                |
                       +------ procedural cues ------[Procedural Memory]               |
                       |                              |                                |
                       +<-------- retrieval ----------+------------ cue-based ---------+
```

**Memory Consolidation** (McClelland's Complementary Learning Systems theory, 1995):

1. **Synaptic consolidation** (minutes-hours): Local strengthening of hippocampal traces via LTP
2. **Systems consolidation** (days-weeks): Hippocampal replay during sleep transfers patterns to neocortex
3. **Schema assimilation** (ongoing): New memories that fit existing schemas consolidate faster (Tse et al., 2007)

The hippocampus acts as a **fast-learning, temporary index** that binds distributed neocortical representations. Over time, the neocortex develops its own direct associations, making the hippocampal index unnecessary for well-consolidated memories.

### Key Insight for AI Architecture

The brain does NOT store memories once -- it re-encodes them repeatedly during consolidation. Each replay subtly transforms the memory, extracting regularities (semantic) while losing specifics (episodic). This is not a bug; it is compression with generalization.

An AI memory system should implement this same loop: raw episodic traces get periodically consolidated into semantic knowledge, with procedural patterns extracted as reusable skills.

---

## Part 2: State of the Art -- Agent Memory Architectures (2025-2026)

### Taxonomy of Approaches

The survey paper "Memory for Autonomous LLM Agents" (arXiv:2603.07670, 2026) formalizes agent memory as a **write-manage-read loop** with five mechanism families:

1. **Context-resident compression**: Summarize history within the context window
2. **Retrieval-augmented stores**: External vector/graph stores with retrieval
3. **Reflective self-improvement**: Agent reflects on its own memories
4. **Hierarchical virtual context**: Multi-level memory with paging
5. **Policy-learned management**: RL-optimized memory operations

### The Major Systems

#### MemGPT / Letta (Packer et al., 2023 -> v0.2, 2025)

**Architecture**: OS-inspired virtual memory with paging between context (RAM) and external stores (disk).

```
+-------------------+
| Working Memory    |  <-- LLM context window (active)
| (main context)    |
+-------------------+
        |  page in/out (agent-controlled functions)
        v
+-------------------+     +-------------------+
| Episodic Store    |     | Semantic Store    |
| (session logs)    |     | (extracted facts) |
| [Vector DB]       |     | [Vector DB + KG]  |
+-------------------+     +-------------------+
```

- **Procedural memory**: The agent's read/write/archive functions themselves
- **Innovation**: Agent controls its own memory management via function calls
- **Limitation**: No true consolidation; no temporal reasoning; no schema evolution

#### HippoRAG v2 (Gutierrez et al., Dec 2025)

**Architecture**: Models hippocampal memory indexing for retrieval.

```
Documents --> [PHR: Entity/Relation Extraction] --> [KG: Nodes + Edges]
                                                         |
Query --> [Embedding Match] --> [Seed Nodes] --> [PPR Propagation] --> [Ranked Passages]
                                                         |
                                        [Pattern Completion via Graph Walk]
```

Three components mapping to neuroscience:
- **Parahippocampal Region (PHR)**: LLM-based entity/relation extraction from documents
- **Hippocampal Index**: Knowledge graph with Personalized PageRank for associative retrieval
- **Neocortical Storage**: Original passages stored distributed, retrieved by graph activation

**v2 improvements**: Dual-node KG (passage nodes + phrase nodes), online LLM loop for triple filtering, +7 F1 on associative tasks over NV-Embed-v2.

**Key innovation**: Pattern separation (sparse seed activation) and pattern completion (PPR graph walk) directly model hippocampal function.

**Limitation**: Read-only (no memory updates), no temporal reasoning, no procedural memory, no consolidation.

#### CoALA (Cognitive Agent Long-term Architecture)

**Architecture**: Explicitly maps all four cognitive memory types.

```
+------------------+
| Working Memory   |  KV cache, volatile
+------------------+
        |
+-------+--------+---------+
|                |          |
v                v          v
+------------+ +--------+ +------------+
| Episodic   | |Semantic| | Procedural |
| (vectors)  | |(KG)    | | (skills)   |
| timestamped| |triples | | embeddings |
+------------+ +--------+ +------------+
        \        |        /
         [Gating Network: GNN routing]
```

- **Gating network** routes queries to appropriate memory type
- **GNN-based path inference** before vector ANN search
- **2x recall** over pure vectors on WebArena benchmarks
- **Limitation**: Heavy infrastructure (Neo4j + FAISS + custom GNN), no embedded mode

#### A-MEM (Zettelkasten Memory, Xu et al., 2025)

**Architecture**: Self-evolving note network inspired by Luhmann's Zettelkasten.

Each memory note m_i contains:
- Structured textual attributes (description, keywords, tags)
- Dense vector embedding for similarity
- Dynamic links to related notes

Three processes:
1. **Note Construction**: Atomic, self-contained knowledge units from interactions
2. **Dynamic Indexing**: Semantic similarity-based linking across the note network
3. **Memory Evolution**: New notes trigger updates to existing notes' context and links

**Key innovation**: Agency in storage, not just retrieval. The memory structure itself evolves.

**Token efficiency**: 1,200-2,500 tokens vs. MemGPT's 16,900 tokens for comparable tasks.

**Limitation**: No explicit temporal model, no procedural memory, no graph-based reasoning.

#### Continuum Memory Architecture (CMA, arXiv:2601.09913, Jan 2026)

Defines five requirements for agent memory:
1. **Persistent storage** across sessions
2. **Selective retention** (not everything is worth keeping)
3. **Associative routing** (find relevant memories by association)
4. **Temporal chaining** (order matters)
5. **Consolidation** (merge, compress, abstract)

References Hindsight (Latimer et al., 2025) -- a four-network architecture:
- Network 1: World facts (semantic)
- Network 2: Agent experiences (episodic)
- Network 3: Entity summaries (consolidated semantic)
- Network 4: Evolving beliefs (meta-cognitive)

---

## Part 3: Hybrid Retrieval -- State of the Art

### The Retrieval Stack

Modern agent memory requires fusing three retrieval modalities:

| Modality | Best For | Scoring | Technology |
|----------|----------|---------|------------|
| **Vector (HNSW)** | Semantic similarity, fuzzy matching | Cosine/dot product distance | usearch, qdrant, pgvector |
| **Full-Text (BM25)** | Exact terms, IDs, codes, names | TF-IDF / BM25 scoring | Tantivy, Meilisearch |
| **Graph Traversal** | Multi-hop reasoning, relationships | Path length, PPR, PageRank | CozoDB, Neo4j, Oxigraph |

### Fusion Methods

#### Reciprocal Rank Fusion (RRF)

The standard for combining ranked lists without score normalization:

```
RRF_score(d) = SUM_k [ 1 / (k + rank_k(d)) ]

where k = 60 (constant), rank_k(d) = document d's rank in retriever k
```

Properties:
- No normalization needed (rank-based, not score-based)
- Robust to missing documents in some retrievers
- Tunable via the k constant (higher k = more weight to lower-ranked results)

#### Learned Fusion (Gating Networks)

CoALA and HippoRAG v2 use learned gates:

```
final_score(d) = w_vec * sim_vector(d) + w_fts * score_bm25(d) + w_graph * ppr(d)

where weights w_* are learned per query type via a small neural network
```

#### Graph-Guided Retrieval Pipeline

The most effective pattern discovered in 2025-2026 research:

```
Query
  |
  v
[1. Entity Recognition] --> extract entities from query
  |
  v
[2. Graph Seed] --> find matching nodes in KG
  |
  v
[3. PPR / BFS Expansion] --> expand to related nodes (1-3 hops)
  |
  v
[4. Vector Rerank] --> score expanded candidates by embedding similarity
  |
  v
[5. FTS Boost] --> boost exact matches from full-text index
  |
  v
[6. RRF Fusion] --> combine all scores
  |
  v
[Top-K Results]
```

This pipeline (graph seed -> expand -> vector rerank -> FTS boost -> fuse) outperforms any single modality by 25-35% on multi-hop QA benchmarks.

### Microsoft GraphRAG vs. HippoRAG

| Dimension | GraphRAG | HippoRAG v2 |
|-----------|----------|-------------|
| **Graph construction** | LLM entity/relation extraction | LLM entity/relation extraction |
| **Community detection** | Leiden algorithm (hierarchical) | None (flat KG) |
| **Retrieval** | Local (BFS) + Global (community summaries) | PPR from seed nodes |
| **Summarization** | Per-community LLM summaries | None (returns passages) |
| **Best for** | Corpus-level questions, thematic analysis | Multi-hop associative recall |
| **Cost** | High (indexing requires many LLM calls) | Medium (extraction + PPR) |
| **Temporal** | No | No |

### LightRAG

A lighter-weight alternative to GraphRAG:
- Same entity/relation extraction pipeline
- Simplified community detection (reduced Leiden)
- Faster indexing with pruned hierarchies
- Competitive accuracy at 3-5x lower cost

### RAPTOR (Recursive Abstractive Processing for Tree-Organized Retrieval)

```
Leaf chunks (original text)
    |
    v
[Cluster via embeddings (k-means)]
    |
    v
[LLM-summarize each cluster]
    |
    v
[Repeat: cluster summaries -> higher-level summaries]
    |
    v
Root summary (most abstract)
```

- Builds a tree of abstractions from bottom up
- Query traverses tree: high-level match -> drill down to relevant leaves
- No explicit graph -- hierarchy IS the structure
- Good for long documents, less effective for multi-document associative reasoning

---

## Part 4: Self-Evolving Ontologies

### The Schema Evolution Problem

Traditional knowledge graphs require a predefined schema (ontology). This creates a chicken-and-egg problem: you need to know what types of entities and relationships exist before you encounter them.

### Approaches to Self-Describing Schemas

#### 1. Schema-on-Read (CozoDB, SurrealDB)

Store triples/documents without schema enforcement. Derive structure at query time.

```
Pro: Maximum flexibility, no migration needed
Con: No validation, inconsistency accumulates, query optimization limited
```

#### 2. SHACL Shape Learning (SHACLearner)

Automatically infer SHACL constraint shapes from existing KG data:
- Mine Inverse Open Path (IOP) rules from embeddings
- Build tree-like shapes with confidence scores
- Shapes capture frequent patterns (e.g., "Person requires name:string, birth:date")
- 80-98% coverage on YAGO, Wikidata, DBpedia

**Key paper**: "Learning SHACL Shapes from Knowledge Graphs" (Semantic Web Journal)

#### 3. Astrea (Automatic SHACL Generation from Ontologies)

Uses SPARQL CONSTRUCT queries to generate shapes from OWL ontologies.

#### 4. Emergent Ontology Discovery

The pattern for a self-evolving schema:

```
[New Data Ingested]
       |
       v
[Entity/Relation Extraction (LLM)]
       |
       v
[Match against existing types?]
       |
   yes |        no
       |         |
       v         v
[Validate    [Create new type candidate]
 against          |
 SHACL shape]     v
       |    [Accumulate N instances]
       |          |
       |          v
       |    [LLM: propose shape definition]
       |          |
       |          v
       |    [Human-in-the-loop or auto-accept if confidence > threshold]
       |          |
       +----------+
       |
       v
[Updated Ontology]
```

### Practical Design: Three-Tier Schema

```
Tier 1: CORE TYPES (immutable)
  - Entity, Relation, Event, Observation
  - Always present, never change

Tier 2: DOMAIN TYPES (stable, versioned)
  - Person, Organization, Document, Concept
  - Schema migrations with backward compatibility
  - Versioned with SemVer (Person@v2 extends Person@v1)

Tier 3: EMERGENT TYPES (dynamic, learned)
  - Discovered from data patterns
  - Start as "untyped" with tag clusters
  - Graduate to Tier 2 after N instances + human/LLM validation
  - SHACL shapes generated automatically
```

---

## Part 5: Temporal Reasoning in Memory

### Bi-Temporal Facts

Every fact needs two time dimensions:

```rust
struct BiTemporalFact<T> {
    entity_id: EntityId,
    fact: T,
    // When was this true in the real world?
    valid_from: Timestamp,
    valid_to: Option<Timestamp>,  // None = still valid
    // When did we learn/record this?
    tx_from: Timestamp,
    tx_to: Option<Timestamp>,    // None = current assertion
    // Metadata
    confidence: f64,             // 0.0 - 1.0
    source: SourceRef,
    provenance: Vec<ProvenanceStep>,
}
```

This enables four query modes:
1. **Current state**: valid_to IS NULL AND tx_to IS NULL
2. **Historical state**: What was true at time T? (valid_from <= T < valid_to)
3. **Audit trail**: When did we first learn X? (tx_from for assertion)
4. **Corrections**: What did we believe at time T about time T'? (both dimensions)

### Confidence Decay

Facts should lose confidence over time, modeled as exponential decay:

```
confidence(t) = c_0 * exp(-lambda * (t - t_recorded))

where:
  c_0 = initial confidence at recording time
  lambda = decay rate (domain-specific)
  t = current time
  t_recorded = when the fact was recorded
```

Decay rates vary by fact type:
- Physical constants: lambda ~ 0 (never decay)
- Personal preferences: lambda ~ 0.01/day (high decay)
- Technical specifications: lambda ~ 0.001/day (moderate decay)
- Historical events: lambda ~ 0 (never decay, but interpretation may)

### Contradiction Resolution

When contradictory facts exist, resolution strategies in order of preference:

1. **Temporal**: More recent valid_time wins (unless explicitly corrected)
2. **Source authority**: Higher-authority sources override lower
3. **Confidence-weighted**: Higher confidence wins
4. **Dempster-Shafer fusion**: Combine evidence from multiple sources
5. **Explicit override**: Human/agent marks one as canonical
6. **Keep both**: Store as competing hypotheses with provenance

```
Contradiction detected:
  Fact A: "CEO of X is Alice" (confidence: 0.9, valid_from: 2024-01)
  Fact B: "CEO of X is Bob"   (confidence: 0.95, valid_from: 2025-03)

Resolution: Fact B wins (more recent valid_time + higher confidence)
  -> Fact A gets valid_to: 2025-03 (historical record preserved)
  -> Fact B becomes current assertion
```

### Memory Consolidation Algorithm

Periodic background process inspired by hippocampal sleep replay:

```
CONSOLIDATION_CYCLE (every N hours or on-demand):

1. IDENTIFY candidates:
   - Episodic memories older than threshold
   - Memories accessed fewer than M times
   - Similar memories (cosine similarity > 0.85)

2. CLUSTER similar memories:
   - Group by semantic embedding similarity
   - Group by entity overlap
   - Group by temporal proximity

3. MERGE within clusters:
   - Extract common facts -> promote to semantic memory
   - Extract common patterns -> promote to procedural memory
   - Keep unique details in compressed episodic form
   - Resolve contradictions within cluster

4. COMPRESS:
   - LLM-summarize episodic clusters into semantic facts
   - Deduplicate entity references
   - Update confidence scores
   - Prune low-confidence, old, unreferenced facts

5. RE-INDEX:
   - Update vector embeddings for changed memories
   - Update graph edges for merged entities
   - Update FTS index for changed text
```

---

## Part 6: The Blueprint -- Mnemos Architecture

### Design Principles

1. **Cognitive fidelity**: Model all four memory types with consolidation
2. **Self-evolving schema**: Types emerge from data, not from configuration
3. **Bi-temporal everything**: Every fact has valid_time and tx_time
4. **Hybrid retrieval**: Graph + Vector + FTS with learned fusion
5. **Single binary**: Embedded, no external dependencies
6. **Offline-first**: Works without network; syncs when available
7. **Compression by design**: Old memories consolidate, not accumulate

### Architecture Overview

```
+============================================================================+
|                           MNEMOS MEMORY ENGINE                              |
|                                                                             |
|  +---------------------------+     +----------------------------------+     |
|  |    WORKING MEMORY         |     |     CONSOLIDATION ENGINE         |     |
|  |    (Active Context)       |     |     (Background Thread)          |     |
|  |                           |     |                                  |     |
|  | - LLM context window      |     | - Sleep/replay cycle            |     |
|  | - Attention buffer (N     |     | - Episodic -> Semantic promotion |     |
|  |   most relevant items)    |     | - Similarity clustering         |     |
|  | - Goal/task stack         |     | - Contradiction resolution      |     |
|  | - Scratchpad              |     | - Confidence decay updates      |     |
|  +---------------------------+     | - Schema evolution proposals    |     |
|              |                     +----------------------------------+     |
|              | encode/retrieve                    |                         |
|              v                                    | consolidate             |
|  +====================================================================+    |
|  |                    LONG-TERM MEMORY STORE                           |    |
|  |                                                                     |    |
|  |  +------------------+  +------------------+  +------------------+   |    |
|  |  | EPISODIC         |  | SEMANTIC         |  | PROCEDURAL       |   |    |
|  |  |                  |  |                  |  |                  |   |    |
|  |  | Events with      |  | Knowledge Graph  |  | Skill Registry   |   |    |
|  |  | full context:    |  | (Property Graph) |  |                  |   |    |
|  |  |                  |  |                  |  | - Tool chains    |   |    |
|  |  | - What happened  |  | - Entities       |  | - Learned prompts|   |    |
|  |  | - When (valid_t) |  | - Relations      |  | - Workflow       |   |    |
|  |  | - Who was there  |  | - Properties     |  |   patterns       |   |    |
|  |  | - What was said  |  | - Types (Tier    |  | - Success/fail   |   |    |
|  |  | - Outcome        |  |   1/2/3)         |  |   statistics     |   |    |
|  |  | - Emotional tone |  | - SHACL shapes   |  | - Chunked        |   |    |
|  |  | - Confidence     |  | - Bi-temporal    |  |   sequences      |   |    |
|  |  |                  |  |   facts          |  |                  |   |    |
|  |  +------------------+  +------------------+  +------------------+   |    |
|  |                                                                     |    |
|  |  +==============================================================+   |    |
|  |  |                  UNIFIED INDEX LAYER                          |   |    |
|  |  |                                                               |   |    |
|  |  |  [Vector Index]    [FTS Index]      [Graph Index]             |   |    |
|  |  |  HNSW (usearch)    BM25 (tantivy)   Property Graph            |   |    |
|  |  |                                     (CozoDB/custom)           |   |    |
|  |  |                                                               |   |    |
|  |  |  +-- Temporal Index (B-tree on valid_from/tx_from) ----------+|   |    |
|  |  |  +-- Entity Index (hash map entity_id -> facts) -------------+|   |    |
|  |  +==============================================================+   |    |
|  |                                                                     |    |
|  |  +==============================================================+   |    |
|  |  |                  STORAGE ENGINE                               |   |    |
|  |  |  redb / fjall (LSM-tree, ACID, embedded)                     |   |    |
|  |  |  + WAL for crash recovery                                    |   |    |
|  |  |  + Compaction for space reclaim                              |   |    |
|  |  +==============================================================+   |    |
|  +=====================================================================+    |
|                                                                             |
|  +---------------------------+     +----------------------------------+     |
|  |   SCHEMA EVOLUTION        |     |     RETRIEVAL PIPELINE           |     |
|  |                           |     |                                  |     |
|  | Tier 1: Core (immutable)  |     | 1. Query analysis (type detect) |     |
|  | Tier 2: Domain (versioned)|     | 2. Graph seed (entity match)   |     |
|  | Tier 3: Emergent (learned)|     | 3. PPR expansion (1-3 hops)    |     |
|  |                           |     | 4. Vector rerank (embedding)   |     |
|  | - Pattern detection       |     | 5. FTS boost (exact terms)     |     |
|  | - SHACL shape inference   |     | 6. RRF fusion (final ranking)  |     |
|  | - Type graduation rules   |     | 7. Temporal filter (recency)   |     |
|  | - Migration engine        |     | 8. Confidence filter (quality) |     |
|  +---------------------------+     +----------------------------------+     |
+============================================================================+
```

### Data Model

```rust
// Core identity
type EntityId = u128;       // UUID
type FactId = u128;
type Timestamp = i64;       // Unix millis

// The universal fact record
struct Fact {
    id: FactId,
    // What
    subject: EntityId,
    predicate: PredicateId,  // Interned string -> u32
    object: Value,           // Enum: Entity(EntityId) | Scalar(...)
    // When (bi-temporal)
    valid_from: Timestamp,
    valid_to: Option<Timestamp>,
    tx_from: Timestamp,
    tx_to: Option<Timestamp>,
    // Trust
    confidence: f64,         // 0.0 - 1.0, decays over time
    source: SourceRef,       // Where did this come from?
    provenance: Vec<Step>,   // Chain of derivation
    // Classification
    memory_type: MemoryType, // Episodic | Semantic | Procedural
    tier: SchemaTier,        // Core | Domain | Emergent
}

enum MemoryType {
    Episodic {
        event_id: EventId,
        sequence_position: u32,
        emotional_valence: f32,   // -1.0 (negative) to 1.0 (positive)
        vividness: f32,           // Decay proxy
    },
    Semantic {
        consolidated_from: Vec<FactId>,  // Which episodic memories produced this
        generality: f32,                  // How general vs. specific
    },
    Procedural {
        skill_id: SkillId,
        success_count: u32,
        failure_count: u32,
        last_executed: Timestamp,
        executable: ProcedureRef,         // Points to workflow/tool chain
    },
}

enum Value {
    Entity(EntityId),
    String(String),
    Number(f64),
    Boolean(bool),
    Timestamp(Timestamp),
    Vector(Vec<f32>),          // Embedding
    Binary(BlobRef),           // CAS hash
    Json(serde_json::Value),   // Structured data
    Null,
}

// Working memory state
struct WorkingMemory {
    attention_buffer: Vec<FactId>,       // Currently active facts (capacity: N)
    goal_stack: Vec<Goal>,               // Active goals/tasks
    scratchpad: HashMap<String, Value>,  // Temporary computation
    context_budget: usize,               // Max tokens for LLM context
    // Attention weights (what to surface next)
    relevance_scores: HashMap<FactId, f64>,
}

// Schema evolution
struct TypeDefinition {
    id: TypeId,
    name: String,
    tier: SchemaTier,
    version: SemVer,
    // SHACL-like shape
    properties: Vec<PropertyShape>,
    // Lifecycle
    instance_count: u64,
    first_seen: Timestamp,
    graduated_at: Option<Timestamp>,     // When promoted from Emergent to Domain
    confidence: f64,
}

struct PropertyShape {
    predicate: PredicateId,
    value_type: ValueType,
    cardinality: Cardinality,            // One | Many | Optional
    constraints: Vec<Constraint>,        // Min/Max, Pattern, Enum, etc.
    observed_frequency: f64,             // How often this property appears
}
```

### Retrieval Pipeline (HippoRAG-inspired + RRF)

```rust
fn retrieve(query: &str, config: &RetrievalConfig) -> Vec<RankedResult> {
    // Phase 1: Query Analysis
    let query_type = classify_query(query);  // factual | associative | temporal | procedural
    let entities = extract_entities(query);
    let embedding = embed(query);

    // Phase 2: Multi-modal retrieval (parallel)
    let graph_results = graph_retrieve(entities, config.max_hops);  // PPR from seed nodes
    let vector_results = vector_retrieve(embedding, config.top_k);  // HNSW approximate NN
    let fts_results = fts_retrieve(query, config.top_k);            // BM25 scoring

    // Phase 3: Temporal and confidence filtering
    let now = current_timestamp();
    let filtered = apply_temporal_filter(
        &[graph_results, vector_results, fts_results],
        now,
        config.time_window,
    );
    let decayed = apply_confidence_decay(filtered, now);

    // Phase 4: Fusion
    match config.fusion_strategy {
        RRF { k } => reciprocal_rank_fusion(decayed, k),
        Learned { weights } => weighted_fusion(decayed, weights),
        Adaptive => {
            // Use query_type to select weights
            let weights = query_type_weights(query_type);
            weighted_fusion(decayed, weights)
        }
    }
}

fn graph_retrieve(entities: Vec<EntityId>, max_hops: u8) -> Vec<ScoredFact> {
    // 1. Find seed nodes matching query entities
    let seeds = match_entities_in_graph(entities);

    // 2. Personalized PageRank from seeds
    let mut scores = HashMap::new();
    let alpha = 0.15;  // Restart probability

    // Initialize: uniform weight on seed nodes
    for seed in &seeds {
        scores.insert(seed, 1.0 / seeds.len() as f64);
    }

    // Iterate PPR
    for _ in 0..config.ppr_iterations {
        let mut new_scores = HashMap::new();
        for (node, score) in &scores {
            // Restart probability
            if seeds.contains(node) {
                *new_scores.entry(node).or_insert(0.0) += alpha * score;
            }
            // Propagate to neighbors
            let neighbors = graph.neighbors(node);
            let share = (1.0 - alpha) * score / neighbors.len() as f64;
            for neighbor in neighbors {
                *new_scores.entry(neighbor).or_insert(0.0) += share;
            }
        }
        scores = new_scores;
    }

    // 3. Return facts connected to highest-scoring nodes
    rank_facts_by_node_scores(scores)
}
```

### Consolidation Engine

```rust
struct ConsolidationEngine {
    interval: Duration,        // How often to run (e.g., every 6 hours)
    episodic_threshold: Duration,  // Memories older than this are candidates
    similarity_threshold: f64,     // Cosine similarity for clustering
    min_cluster_size: usize,       // Minimum memories to consolidate
}

impl ConsolidationEngine {
    async fn consolidation_cycle(&self, store: &mut MemoryStore) {
        // 1. IDENTIFY candidates for consolidation
        let candidates = store.episodic_memories()
            .filter(|m| m.age() > self.episodic_threshold)
            .filter(|m| m.access_count < self.min_access_threshold)
            .collect();

        // 2. CLUSTER by semantic similarity
        let clusters = self.cluster_memories(candidates);

        for cluster in clusters {
            if cluster.len() < self.min_cluster_size {
                continue;
            }

            // 3. EXTRACT semantic facts from cluster
            let common_facts = self.extract_common_facts(&cluster);
            for fact in common_facts {
                store.promote_to_semantic(fact);
            }

            // 4. EXTRACT procedural patterns
            let patterns = self.detect_action_patterns(&cluster);
            for pattern in patterns {
                store.promote_to_procedural(pattern);
            }

            // 5. DETECT contradictions within cluster
            let contradictions = self.find_contradictions(&cluster);
            for (fact_a, fact_b) in contradictions {
                self.resolve_contradiction(store, fact_a, fact_b);
            }

            // 6. COMPRESS episodic memories
            let summary = self.llm_summarize(&cluster);
            store.replace_cluster_with_summary(cluster, summary);
        }

        // 7. DECAY confidence scores globally
        store.apply_confidence_decay();

        // 8. SCHEMA EVOLUTION
        let new_patterns = self.detect_type_patterns(store);
        for pattern in new_patterns {
            if pattern.instance_count > self.graduation_threshold {
                store.propose_type_graduation(pattern);
            }
        }

        // 9. RE-INDEX changed facts
        store.rebuild_indexes();
    }
}
```

### Schema Evolution Engine

```rust
struct SchemaEvolution {
    graduation_threshold: u64,     // Instances needed to graduate Tier 3 -> Tier 2
    confidence_threshold: f64,     // Minimum confidence for auto-graduation
}

impl SchemaEvolution {
    fn process_new_entity(&self, entity: &Entity, store: &mut MemoryStore) {
        // Try to match against existing types (Tier 1 + 2)
        if let Some(type_match) = self.match_existing_type(entity) {
            // Validate against SHACL shape
            let violations = self.validate_shape(entity, type_match);
            if violations.is_empty() {
                store.assign_type(entity, type_match);
            } else {
                // Partial match -- maybe the schema needs to evolve
                self.propose_shape_extension(type_match, entity, violations);
            }
        } else {
            // No match -- create or update Tier 3 emergent type
            let tag_cluster = self.infer_tags(entity);
            let candidate_type = store.find_or_create_emergent_type(tag_cluster);
            store.assign_type(entity, candidate_type);

            // Check if emergent type is ready to graduate
            if candidate_type.instance_count > self.graduation_threshold
                && candidate_type.shape_confidence > self.confidence_threshold
            {
                self.graduate_type(store, candidate_type);
            }
        }
    }

    fn graduate_type(&self, store: &mut MemoryStore, emergent: TypeDef) {
        // 1. Generate SHACL shape from observed instances
        let shape = self.infer_shacl_shape(store, &emergent);

        // 2. Create versioned Tier 2 type
        let domain_type = TypeDefinition {
            tier: SchemaTier::Domain,
            version: SemVer::new(1, 0, 0),
            properties: shape.properties,
            ..emergent
        };

        // 3. Migrate existing instances
        store.migrate_instances(emergent.id, domain_type.id);

        // 4. Register type
        store.register_type(domain_type);
    }
}
```

### Storage Layer (Single Binary)

The storage architecture for a single embedded binary:

```
+---------------------------------------------------+
|               Mnemos Binary                        |
|                                                    |
|  +---------------------------------------------+  |
|  |  redb (transactional embedded KV store)      |  |
|  |                                              |  |
|  |  Tables:                                     |  |
|  |  - facts:       FactId -> Fact (CBOR)        |  |
|  |  - entities:    EntityId -> EntityMeta        |  |
|  |  - types:       TypeId -> TypeDefinition      |  |
|  |  - tx_log:      TxId -> Transaction           |  |
|  |  - blobs:       BlobHash -> Vec<u8>           |  |
|  +---------------------------------------------+  |
|                                                    |
|  +---------------------------------------------+  |
|  |  Tantivy (embedded FTS engine)               |  |
|  |  - Index on fact text content                |  |
|  |  - Index on entity names/descriptions        |  |
|  +---------------------------------------------+  |
|                                                    |
|  +---------------------------------------------+  |
|  |  usearch (embedded HNSW vector index)        |  |
|  |  - Fact embeddings (384-dim or 768-dim)      |  |
|  |  - Entity embeddings                         |  |
|  |  - Query-time approximate NN                 |  |
|  +---------------------------------------------+  |
|                                                    |
|  +---------------------------------------------+  |
|  |  Graph Index (custom adjacency lists)        |  |
|  |  - Forward: EntityId -> [(PredicateId,       |  |
|  |                           EntityId)]         |  |
|  |  - Reverse: EntityId -> [(PredicateId,       |  |
|  |                           EntityId)]         |  |
|  |  - PPR computation on adjacency structure    |  |
|  +---------------------------------------------+  |
|                                                    |
|  +---------------------------------------------+  |
|  |  Temporal B-tree Index                       |  |
|  |  - (valid_from, fact_id) for time-range      |  |
|  |  - (tx_from, fact_id) for audit              |  |
|  +---------------------------------------------+  |
+---------------------------------------------------+
```

**Why these specific components**:

| Component | Why This One | Alternatives Considered |
|-----------|-------------|------------------------|
| **redb** | Pure Rust, ACID, no C dependencies, good perf | fjall (newer, less tested), sled (stability concerns), RocksDB (C++ dependency) |
| **Tantivy** | Best Rust FTS engine, BM25, production-proven | Meilisearch (server mode only), custom (too much work) |
| **usearch** | HNSW, pure Rust bindings, efficient | hora (unmaintained), qdrant (server mode), custom HNSW |
| **Custom graph index** | Lightweight adjacency lists in redb suffice | CozoDB (heavy, Datalog overkill), Oxigraph (RDF/SPARQL overhead) |

### API Design

```rust
pub trait MemoryEngine {
    // === WRITE ===
    fn remember(&mut self, event: EpisodicEvent) -> Result<EventId>;
    fn assert_fact(&mut self, subject: EntityId, predicate: &str, object: Value) -> Result<FactId>;
    fn learn_skill(&mut self, skill: Skill) -> Result<SkillId>;
    fn correct(&mut self, fact_id: FactId, new_value: Value, reason: &str) -> Result<FactId>;

    // === READ ===
    fn recall(&self, query: &str, config: RetrievalConfig) -> Result<Vec<RankedResult>>;
    fn recall_episode(&self, cues: &[Cue]) -> Result<Vec<Episode>>;
    fn lookup_fact(&self, entity: EntityId, predicate: &str) -> Result<Option<Value>>;
    fn find_skill(&self, task_description: &str) -> Result<Option<Skill>>;

    // === WORKING MEMORY ===
    fn focus(&mut self, facts: &[FactId]);       // Bring into attention buffer
    fn defocus(&mut self, facts: &[FactId]);     // Remove from attention
    fn context_window(&self) -> Vec<RankedResult>; // Current working memory contents

    // === TEMPORAL ===
    fn as_of(&self, valid_time: Timestamp) -> TemporalView;     // Point-in-time snapshot
    fn history(&self, entity: EntityId, predicate: &str) -> Vec<TemporalFact>;
    fn timeline(&self, entity: EntityId) -> Vec<Event>;

    // === CONSOLIDATION ===
    fn consolidate(&mut self) -> ConsolidationReport;  // Trigger consolidation cycle
    fn decay_confidence(&mut self);                     // Apply time-based decay

    // === SCHEMA ===
    fn types(&self) -> Vec<TypeDefinition>;
    fn propose_type(&mut self, name: &str, instances: &[EntityId]) -> Result<TypeId>;
    fn graduate_type(&mut self, type_id: TypeId) -> Result<()>;

    // === META ===
    fn stats(&self) -> MemoryStats;
    fn export(&self, format: ExportFormat) -> Result<Vec<u8>>;
    fn import(&mut self, data: &[u8], format: ExportFormat) -> Result<ImportReport>;
}
```

---

## Part 7: What No One Else Has Built

After reviewing all existing systems, here are the gaps that a Mnemos implementation would uniquely fill:

| Capability | MemGPT | HippoRAG | CoALA | A-MEM | GraphRAG | **Mnemos** |
|------------|--------|----------|-------|-------|----------|-----------|
| Episodic memory | Yes | Partial | Yes | Yes | No | **Yes** |
| Semantic memory | Yes | Yes | Yes | Yes | Yes | **Yes** |
| Procedural memory | Partial | No | Yes | No | No | **Yes** |
| Working memory | Yes | No | Yes | No | No | **Yes** |
| Consolidation | No | No | No | Partial | No | **Yes** |
| Self-evolving schema | No | No | No | No | No | **Yes** |
| Bi-temporal facts | No | No | No | No | No | **Yes** |
| Confidence decay | No | No | No | No | No | **Yes** |
| Contradiction resolution | No | No | No | No | No | **Yes** |
| Graph + Vector + FTS | Partial | Partial | Yes | No | Partial | **Yes** |
| Single binary | No | No | No | No | No | **Yes** |
| Offline-first | No | No | No | No | No | **Yes** |
| PPR retrieval | No | Yes | No | No | No | **Yes** |
| Memory compression | No | No | No | Partial | No | **Yes** |

### The Three Innovations

**Innovation 1: Consolidation-as-a-Service**

No existing system implements true memory consolidation -- the process of transforming episodic traces into semantic knowledge and procedural skills. Mnemos runs a background consolidation engine that clusters, merges, extracts, and compresses memories on a configurable schedule. This is the AI equivalent of sleep.

**Innovation 2: Three-Tier Self-Evolving Schema**

No existing system learns new entity types from data while maintaining schema integrity. The three-tier approach (Core/Domain/Emergent) with SHACL shape inference and graduation provides the best of both worlds: schema validation where it matters, flexibility where it does not.

**Innovation 3: HippoRAG-style PPR in an Embedded Graph**

HippoRAG demonstrated that Personalized PageRank over a knowledge graph dramatically improves associative retrieval. But HippoRAG is a read-only research prototype. Mnemos integrates PPR into a read-write embedded graph with temporal and confidence-aware scoring.

---

## Part 8: Implementation Roadmap

### Phase 1: Foundation (4-6 weeks)

```
- [ ] Core data model (Fact, Entity, Value, BiTemporal)
- [ ] redb storage layer with CBOR serialization
- [ ] Basic graph index (forward/reverse adjacency)
- [ ] Tantivy FTS integration
- [ ] usearch vector index integration
- [ ] Simple retrieval (vector-only, FTS-only, graph-only)
- [ ] Unit tests for each index type
```

### Phase 2: Hybrid Retrieval (3-4 weeks)

```
- [ ] PPR implementation on graph index
- [ ] RRF fusion across three modalities
- [ ] Query type classification (factual/associative/temporal/procedural)
- [ ] Adaptive weight selection based on query type
- [ ] Temporal filtering (valid_time window)
- [ ] Confidence decay computation
- [ ] Retrieval benchmarks vs. vector-only baseline
```

### Phase 3: Memory Types (3-4 weeks)

```
- [ ] Episodic memory: event recording with full context
- [ ] Semantic memory: fact assertion with bi-temporal tracking
- [ ] Procedural memory: skill registry with success/failure stats
- [ ] Working memory: attention buffer with relevance scoring
- [ ] Memory type routing (which type to query for which question)
```

### Phase 4: Consolidation (4-6 weeks)

```
- [ ] Background consolidation thread
- [ ] Similarity clustering of episodic memories
- [ ] LLM-based summarization of clusters
- [ ] Episodic -> Semantic promotion
- [ ] Pattern detection for procedural extraction
- [ ] Contradiction detection and resolution
- [ ] Confidence decay (global sweep)
- [ ] Consolidation metrics and reporting
```

### Phase 5: Schema Evolution (3-4 weeks)

```
- [ ] Three-tier type system (Core / Domain / Emergent)
- [ ] Pattern detection for emergent types
- [ ] SHACL-like shape inference from instances
- [ ] Type graduation logic (Emergent -> Domain)
- [ ] Shape validation on write
- [ ] Schema migration engine (versioned types)
```

### Phase 6: Integration with Nika (2-3 weeks)

```
- [ ] MCP server interface for Mnemos
- [ ] nika:remember, nika:recall, nika:consolidate builtin tools
- [ ] Workflow-level memory bindings (persist across runs)
- [ ] Agent memory context injection
```

---

## Sources

### Papers

1. "Memory for Autonomous LLM Agents: Mechanisms, Evaluation, and Taxonomy" -- arXiv:2603.07670 (2026)
2. "Evaluating Memory Structure in LLM Agents" -- arXiv:2602.11243 (2026)
3. "Continuum Memory Architectures for Long-Horizon LLM Agents" -- arXiv:2601.09913 (2026)
4. "Evaluating Memory in LLM Agents via Incremental Multi-Turn" -- arXiv:2507.05257 (2025)
5. "Memory in the Age of AI Agents" -- arXiv:2512.13564 (2025)
6. "A-MEM: Agentic Memory for LLM Agents" -- Xu et al. (2025)
7. "HippoRAG v2: Non-parametric Continual Learning" -- Gutierrez et al. (Dec 2025)
8. "Learning SHACL Shapes from Knowledge Graphs" -- Semantic Web Journal
9. "Astrea: Automatic Generation of SHACL Shapes from Ontologies" -- PMC (2020)
10. "From Local to Global: A Graph RAG Approach" -- Microsoft Research (2024)
11. "Complementary Learning Systems" -- McClelland et al. (1995)
12. "Nested Learning" -- Google Research, NeurIPS 2025

### Frameworks Analyzed

- MemGPT / Letta (v0.2) -- OS-inspired virtual memory for agents
- HippoRAG v1 + v2 -- Hippocampus-inspired retrieval
- CoALA -- Cognitive Agent Long-term Architecture
- A-MEM -- Zettelkasten-inspired self-evolving memory
- Microsoft GraphRAG -- Community-based graph retrieval
- LightRAG -- Lightweight GraphRAG alternative
- RAPTOR -- Recursive abstractive tree retrieval
- Mem0 -- Multi-level scoped vector + metadata memory
- LangChain Knowledge Graph Memory -- Dynamic KG from conversations

### Rust Ecosystem

- `redb` -- Embedded transactional KV store (pure Rust)
- `tantivy` -- BM25 full-text search engine
- `usearch` -- HNSW vector similarity search
- `cozo` -- Embedded Datalog graph database
- `oxigraph` -- RDF/SPARQL triplestore (Rust)
- `automerge` -- CRDT for distributed conflict resolution

---

## Confidence Level

**HIGH** on cognitive science foundations -- these are well-established models (Tulving, Baddeley, McClelland) with decades of validation.

**HIGH** on retrieval state of the art -- HippoRAG, GraphRAG, and RRF are well-documented with published benchmarks.

**MEDIUM** on specific 2025-2026 papers -- Perplexity returned some fabricated arXiv IDs alongside real ones. The real papers confirmed: arXiv:2603.07670 (memory survey), arXiv:2602.11243 (StructMemEval), arXiv:2601.09913 (CMA), arXiv:2512.13564 (memory in agent age). Cross-reference on arxiv.org before citing.

**HIGH** on the blueprint feasibility -- all proposed components use production-proven Rust crates. The architecture is novel in combination, not in individual parts.

**MEDIUM** on timeline estimates -- depends heavily on team size and integration complexity with existing Nika/NovaNet code.
