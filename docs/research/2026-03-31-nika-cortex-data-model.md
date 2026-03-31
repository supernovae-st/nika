# Nika Cortex — Data Model (Rust Structs)

> Status: DRAFT — brainstorming in progress
> Date: 2026-03-31

## Core Enums

```rust
/// 4 memory types inspired by Tulving + Baddeley cognitive science
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemoryKind {
    /// WHAT happened, WHEN, in WHAT context — events, sessions, timeline
    Episodic,
    /// WHAT is true — facts, entities, relations, ontology
    Semantic,
    /// HOW to do things — skills, patterns, recipes, Bayesian reliability
    Procedural,
    /// Current context — evidence packets, active recall (session-scoped)
    Working,
}

/// Access control + lifecycle (inspired by NovaNet 2-realm architecture)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Realm {
    /// Built-in types: fact, entity, event, skill, preference (readonly)
    System,
    /// User-created types via nika:cortex_schema
    User,
    /// Auto-discovered types when data doesn't match existing types
    Discovered,
}

/// Edge relationship types (ICM + AMA-Agent inspired)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeType {
    Supports,       // A reinforces B
    Contradicts,    // A conflicts with B
    Causes,         // A caused B (causal reasoning, AMA-Agent paper)
    DerivedFrom,    // A was inferred from B
    SupersededBy,   // A replaced by B (temporal)
    Refines,        // A is more precise version of B
    RelatedTo,      // A is associated with B
    PartOf,         // A is component of B
    InstanceOf,     // A is instance of type B
}

/// Edge family (NovaNet arc families concept)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeFamily {
    Causal,      // causes, derived_from — WHY
    Semantic,    // supports, contradicts, refines, related — WHAT
    Temporal,    // superseded_by — WHEN
    Structural,  // part_of, instance_of — HOW organized
}

/// Provenance source (NovaNet ADR-042)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Source {
    Workflow { id: String },
    User,
    Inferred,
    Consolidated,
}
```

## Core Node

```rust
/// A node in the Cortex memory graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CortexNode {
    /// blake3 content hash (dedup, like Shodh SHA256 but faster)
    pub id: NodeId,
    /// Which memory system this belongs to
    pub kind: MemoryKind,
    /// Specific type within the kind ("fact", "entity", "event", "skill", custom)
    pub node_type: String,
    /// Access control realm
    pub realm: Realm,
    /// The actual memory content
    pub content: String,
    /// Type-specific structured data (validated against NodeType.schema)
    pub properties: serde_json::Value,

    // --- Provenance (NovaNet ADR-042) ---
    pub source: Source,
    pub confidence: f64,        // 0.0 - 1.0
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub superseded_by: Option<NodeId>,

    // --- Cognitive State ---
    /// ACT-R base-level activation: B_i = ln(Σ t_j^(-d))
    /// where d = 0.5 (standard decay parameter)
    pub activation: f64,
    /// Timestamps of each access (for ACT-R calculation)
    pub access_log: Vec<DateTime<Utc>>,
    /// Bjork dual-strength: encoding quality (increases with each access)
    pub storage_strength: f64,
    /// Bjork dual-strength: accessibility (decays, boosted by recall)
    pub retrieval_strength: f64,
    /// FSRS-6 scheduler state
    pub fsrs: FsrsState,

    // --- Gating (D-MEM paper) ---
    /// How unexpected was this fact (0.0 = routine, 1.0 = very surprising)
    pub surprise: f64,
    /// How useful has this fact been (0.0 = never used, 1.0 = critical)
    pub utility: f64,

    // --- Salience (Pensyve 4-factor) ---
    /// Composite: 0.4*novelty + 0.3*importance + 0.1*extremity + 0.2*specificity
    pub salience: f64,

    // --- Kumiho Prospective Indexing ---
    /// Future scenarios where this fact might be useful (generated at write-time)
    pub anticipations: Vec<String>,
}
```

## FSRS-6 State

```rust
/// Free Spaced Repetition Scheduler v6 (Vestige/Anki algorithm)
/// R(t,S) = (1 + t/(9*S))^(-1)
/// At t = 9*S: R = 50% (half-life definition)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsrsState {
    /// 0.0-1.0, how hard to remember (affects stability growth)
    pub difficulty: f64,
    /// Half-life in hours (at t=9*stability, retrievability = 50%)
    pub stability: f64,
    /// Hours since last access
    pub elapsed: f64,
    /// Number of successful recalls
    pub reps: u32,
    /// Number of failed recalls (decay triggers)
    pub lapses: u32,
}

impl FsrsState {
    /// Calculate current retrievability (0.0 - 1.0)
    pub fn retrievability(&self) -> f64 {
        (1.0 + self.elapsed / (9.0 * self.stability)).powi(-1)
    }

    /// Update after successful recall
    pub fn on_recall(&mut self) {
        self.reps += 1;
        self.elapsed = 0.0;
        // Stability grows: S' = S * (1 + e^(0.1) * (11 - D) * S^(-0.2))
        self.stability *= 1.0 + (0.1_f64).exp() * (11.0 - self.difficulty * 10.0) * self.stability.powf(-0.2);
    }

    /// Update after failed recall (forgetting)
    pub fn on_forget(&mut self) {
        self.lapses += 1;
        self.elapsed = 0.0;
        // Stability drops: S' = S * 0.3 * (11 - D) / 10
        self.stability *= 0.3 * (11.0 - self.difficulty * 10.0) / 10.0;
    }
}
```

## Edges

```rust
/// A typed, weighted edge in the memory graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CortexEdge {
    pub source: NodeId,
    pub target: NodeId,
    pub edge_type: EdgeType,
    pub family: EdgeFamily,
    /// Hebbian weight: strengthens with co-access
    /// +2.5% per helpful co-retrieval (Shodh HEBBIAN_BOOST_HELPFUL)
    /// -10% per misleading co-retrieval (Shodh HEBBIAN_DECAY_MISLEADING)
    /// Floor: 0.05 (never fully forget, Shodh IMPORTANCE_FLOOR)
    /// Half-life: 24h without reinforcement (Shodh EDGE_HALF_LIFE_HOURS)
    pub weight: f64,
    pub created_at: DateTime<Utc>,
}
```

## Auto-Evolving Ontology

```rust
/// A node type definition (self-describing schema)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeType {
    /// Type name: "fact", "entity", "seo_keyword", "audio_segment", ...
    pub name: String,
    /// Access realm
    pub realm: Realm,
    /// Inheritance parent (e.g., "seo_keyword" extends "fact")
    pub parent: Option<String>,
    /// JSON Schema for validating properties
    pub schema: serde_json::Value,
    /// How this type was created
    pub source: String, // "builtin" | "workflow:podcast-gen" | "inferred"
    /// How many nodes use this type (for graduation: Discovered → User)
    pub instance_count: u64,
    pub created_at: DateTime<Utc>,
}

// System types (readonly):
// - fact: base type for all knowledge
// - entity: named thing with identity
// - event: temporal occurrence
// - skill: procedural knowledge (workflow patterns)
// - preference: user preference
//
// Discovered types auto-graduate to User realm after:
// - 10+ instances with consistent schema
// - confidence > 0.8 across instances
```

## Evidence Packets (Working Memory)

```rust
/// A retrieved memory packaged for LLM consumption
/// Inspired by NovaNet's EvidencePacket with token budgeting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidencePacket {
    pub node_id: NodeId,
    /// Compressed content payload
    pub content: String,
    /// Combined relevance score from all signals (0.0 - 1.0)
    pub relevance: f64,
    /// Graph hops from query focus node
    pub distance: usize,
    /// Pre-calculated token count (for budget enforcement)
    pub tokens: usize,
    /// Breakdown per retrieval signal
    pub signal_scores: SignalScores,
}

/// Per-signal breakdown for retrieval transparency
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SignalScores {
    /// FTS5 BM25 keyword match score
    pub bm25: f64,
    /// usearch vector cosine similarity
    pub cosine: f64,
    /// Personalized PageRank (HippoRAG-inspired)
    pub pagerank: f64,
    /// ACT-R spreading activation (Collins & Loftus 1975)
    pub activation: f64,
    /// Query intent classification (5 intents)
    pub intent: f64,
    /// Node confidence score
    pub confidence: f64,
}

/// Token-budget-aware retrieval result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallResult {
    /// Evidence packets sorted by relevance DESC
    pub packets: Vec<EvidencePacket>,
    /// Total tokens across all packets
    pub total_tokens: usize,
    /// Tokens consumed within budget
    pub budget_used: usize,
    /// True if budget was exceeded and packets were truncated
    pub truncated: bool,
    /// Retrieval latency
    pub query_time_ms: u64,
}
```

## Hebbian Constants (from Shodh, neuroscience-backed)

```rust
/// Hebbian learning parameters (Bi & Poo 1998, Chechik 1998)
pub mod hebbian {
    /// Importance boost per successful co-retrieval (2.5%)
    pub const BOOST_HELPFUL: f64 = 0.025;
    /// Importance decay per misleading co-retrieval (10%, asymmetric)
    pub const DECAY_MISLEADING: f64 = 0.10;
    /// Minimum weight floor (never fully forget — "savings" effect)
    pub const IMPORTANCE_FLOOR: f64 = 0.05;
    /// Association half-life without reinforcement (hours)
    pub const EDGE_HALF_LIFE_HOURS: f64 = 24.0;
    /// Max edges per node (prevent graph explosion)
    pub const MAX_ENTITY_DEGREE: usize = 500;
    /// Long-term potentiation threshold (accesses before permanent)
    pub const POTENTIATION_THRESHOLD: u32 = 5;
}
```

## Dopamine Gate Constants (from D-MEM paper)

```rust
/// Dopamine-gated memory (D-MEM paper, 2603.14597)
/// Critic Router evaluates: Surprise × Utility > threshold → process
pub mod gate {
    /// Minimum surprise × utility product to trigger full processing
    pub const PROCESSING_THRESHOLD: f64 = 0.3;
    /// Routine inputs below this skip expensive memory evolution
    pub const ROUTINE_THRESHOLD: f64 = 0.1;
    /// Maximum token budget for routine writes (skip LLM, just store)
    pub const ROUTINE_MAX_TOKENS: usize = 0;
    /// Full processing budget (LLM anticipation, contradiction check)
    pub const FULL_PROCESSING_MAX_TOKENS: usize = 2000;
}
```

## Salience Constants (from Pensyve)

```rust
/// Salience encoding at write-time (Pensyve 4-factor model)
pub mod salience {
    pub const NOVELTY_WEIGHT: f64 = 0.4;
    pub const IMPORTANCE_WEIGHT: f64 = 0.3;
    pub const EXTREMITY_WEIGHT: f64 = 0.1;
    pub const SPECIFICITY_WEIGHT: f64 = 0.2;
}
```

## SQLite Schema

```sql
-- Core nodes
CREATE TABLE nodes (
    id              TEXT PRIMARY KEY,
    kind            TEXT NOT NULL CHECK(kind IN ('episodic','semantic','procedural')),
    node_type       TEXT NOT NULL,
    realm           TEXT NOT NULL CHECK(realm IN ('system','user','discovered')),
    content         TEXT NOT NULL,
    properties      TEXT DEFAULT '{}',
    source          TEXT NOT NULL,
    confidence      REAL DEFAULT 1.0,
    created_at      TEXT NOT NULL,
    updated_at      TEXT,
    superseded_by   TEXT REFERENCES nodes(id),
    activation      REAL DEFAULT 0.0,
    access_log      TEXT DEFAULT '[]',
    storage_strength REAL DEFAULT 1.0,
    retrieval_strength REAL DEFAULT 1.0,
    fsrs_difficulty REAL DEFAULT 0.3,
    fsrs_stability  REAL DEFAULT 24.0,
    fsrs_elapsed    REAL DEFAULT 0.0,
    fsrs_reps       INTEGER DEFAULT 0,
    fsrs_lapses     INTEGER DEFAULT 0,
    surprise        REAL DEFAULT 0.0,
    utility         REAL DEFAULT 0.0,
    salience        REAL DEFAULT 0.5,
    anticipations   TEXT DEFAULT '[]'
);

-- Full-text search index
CREATE VIRTUAL TABLE nodes_fts USING fts5(
    content, node_type, anticipations,
    content=nodes, content_rowid=rowid
);

-- Edges with Hebbian weights
CREATE TABLE edges (
    source_id   TEXT NOT NULL REFERENCES nodes(id),
    target_id   TEXT NOT NULL REFERENCES nodes(id),
    edge_type   TEXT NOT NULL,
    family      TEXT NOT NULL,
    weight      REAL DEFAULT 1.0,
    created_at  TEXT NOT NULL,
    PRIMARY KEY (source_id, target_id, edge_type)
);

-- Auto-evolving ontology
CREATE TABLE node_types (
    name        TEXT PRIMARY KEY,
    realm       TEXT NOT NULL,
    parent      TEXT REFERENCES node_types(name),
    schema      TEXT NOT NULL,
    source      TEXT NOT NULL,
    instance_count INTEGER DEFAULT 0,
    created_at  TEXT NOT NULL
);

-- Sessions (cross-run tracking)
CREATE TABLE sessions (
    id          TEXT PRIMARY KEY,
    workflow    TEXT,
    started_at  TEXT NOT NULL,
    ended_at    TEXT
);

-- Temporal indexes
CREATE INDEX idx_nodes_created ON nodes(created_at);
CREATE INDEX idx_nodes_kind ON nodes(kind);
CREATE INDEX idx_nodes_type ON nodes(node_type);
CREATE INDEX idx_nodes_superseded ON nodes(superseded_by) WHERE superseded_by IS NOT NULL;
CREATE INDEX idx_edges_source ON edges(source_id);
CREATE INDEX idx_edges_target ON edges(target_id);
CREATE INDEX idx_edges_family ON edges(family);
```

## usearch Vector Index

```rust
// Separate file: ~/.nika/cortex.usearch
// Managed by usearch crate, memory-mapped, SIMD-accelerated
use usearch::ffi::{IndexOptions, MetricKind, ScalarKind};

let options = IndexOptions {
    dimensions: 384,                    // bge-small-en-v1.5
    metric: MetricKind::Cos,            // cosine similarity
    quantization: ScalarKind::F16,      // f16 quantization (50% size)
    connectivity: 16,                   // HNSW M parameter
    expansion_add: 128,                 // ef_construction
    expansion_search: 64,               // ef_search
};
```
