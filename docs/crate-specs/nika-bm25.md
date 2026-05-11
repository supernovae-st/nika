# Crate spec — `nika-bm25`

| | |
|---|---|
| Status | **Phase 1.5 W3 admission target** (first L1 memory satellite · ADR-004 1+9 architecture · ADR-038 admission prep shipped) |
| Layer | L1 — memory satellite · pure-algo · zero I/O at trait boundary · `Send + Sync` |
| Sub-tier | L1-deterministic — no ML deps · no async sleep · CPU-bound scoring + bounded `consume_budget()` cooperation |
| Design | **Pure BM25 (Okapi)** — Robertson 1994 canonical formula · k1 saturation · b length-norm · IDF smoothing |
| LOC budget | ≤800 src (target ~600 · scoring kernel ~150 + index ~200 + query ~100 + impls ~150) |
| File cap | ≤1,500 LOC each (none expected near cap) |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace (`0.80.0`) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `true` — publishable on crates.io per ADR-004 « 9 satellites publishable standalone » external open-source value |
| Reference | [Robertson & Walker 1994 · Okapi BM25](https://en.wikipedia.org/wiki/Okapi_BM25) · Manning Raghavan Schütze 2008 ch. 11 |

---

## 1. Purpose

`nika-bm25` is **the lexical scoring satellite** of the Diamond memory subsystem.
It implements the canonical BM25 (Best Match 25 · Okapi) ranking function for
sparse-term retrieval. It pairs with `nika-hnsw` (W7 · trigger-gated) for hybrid
dense + sparse retrieval · fused via `nika-rrf` (W4) Reciprocal Rank Fusion.

BM25 is the IR standard for 15+ years (Robertson 1994 · refined Sparck Jones 2000)
· empirically beats TF-IDF on most corpora · widely used by Elasticsearch ·
Solr · Lucene · tantivy · Meilisearch.

The crate is intentionally **pure-algo** — no embeddings · no LLM · no async I/O
beyond cooperative `tokio::task::consume_budget()` scheduling. Anything that
touches embeddings lives in `nika-hnsw` · anything that fuses rankers lives in
`nika-rrf` · anything that orchestrates lives in `nika-memory` (W10).

---

## 2. Public API

```rust
//! `nika-bm25` · lexical BM25 scoring satellite.

/// Tunable BM25 parameters per Robertson 1994.
#[non_exhaustive]
pub struct BmParams {
    pub k1: f64,   // term-frequency saturation (typical: 1.2)
    pub b: f64,    // length-normalization (typical: 0.75)
}

impl BmParams { pub const fn new(k1: f64, b: f64) -> Self; }
impl Default for BmParams { /* k1=1.2 b=0.75 canonical */ }

/// In-memory BM25 index over a document corpus.
#[non_exhaustive]
pub struct BmIndex { /* private fields · postings + length stats */ }

impl BmIndex {
    pub fn new(params: BmParams) -> Self;
    pub fn add_document(&mut self, id: DocId, text: &str);
    pub fn finalize(&mut self);   // computes IDF + avgdl
    pub fn score(&self, query: &str, doc_id: DocId) -> Option<f64>;
    pub fn top_k(&self, query: &str, k: usize) -> Vec<(DocId, f64)>;
}

/// Implements `nika-kernel::ai::memory::MemoryRecall` for lexical recall.
impl MemoryRecall for BmIndex {
    async fn recall(&self, query: RecallQuery) -> Result<Vec<MemoryHit>, MemoryError>;
}
```

---

## 3. Internal modules

| Module | LOC | Purpose |
|---|---|---|
| `lib.rs` | ~50 | re-exports + crate-level docs + feature flag |
| `params.rs` | ~50 | `BmParams` · canonical k1=1.2 · b=0.75 |
| `tokenize.rs` | ~80 | UTF-8 + Unicode-aware token splitting · whitespace + punctuation · lowercase |
| `index.rs` | ~200 | `BmIndex` · postings list · doc-length statistics · IDF computation |
| `scorer.rs` | ~150 | Score formula · `tokio::task::consume_budget()` cooperative yield every 128 iters per BLUEPRINT v1.3 §4 |
| `query.rs` | ~100 | Query parsing + execution · `top_k` heap-based · cancel-cooperative |
| `recall_impl.rs` | ~80 | `MemoryRecall` trait impl bridging `BmIndex` to `RecallQuery` |
| **Total** | **~710** | within ≤800 budget · ~90 LOC headroom |

---

## 4. Dependencies

```toml
[dependencies]
nika-types  = { path = "../nika-types", workspace = true }
nika-error  = { path = "../nika-error", workspace = true }
nika-kernel = { path = "../nika-kernel", workspace = true }
thiserror   = { workspace = true }

# Tokio for cooperative scheduling (consume_budget · per BLUEPRINT v1.3 §4)
tokio = { workspace = true, features = ["rt"] }

# Async trait variant (per ADR-006 + ADR-014 sealed traits)
trait-variant = { workspace = true }

[dev-dependencies]
proptest    = { workspace = true }   # Gate 6 property tests
criterion   = { workspace = true }   # Gate 7 benches
insta       = { workspace = true }   # Gate 2 snapshot fixtures
rstest      = { workspace = true }   # parametric fixtures
```

**Zero ML deps · zero embedding deps · zero RDF deps** — per ADR-004 sub-tier
discipline. The lexical-only stance is the W3 invariant.

---

## 5. Gate-by-gate readiness

| Gate | Status | Notes |
|---|---|---|
| 1 SPEC | ✅ | This doc |
| 2 TDD | ✅ | `tests/red_phase.rs` · 7 tests RED→GREEN transition shipped 2026-05-12 |
| 3 IMPL | ✅ | 580 LOC · 5 modules (tokenize · scorer · index · query · lib) · commit `92e5d39fb` |
| 4 CLIPPY | ✅ | 0 warnings pedantic-strict · workspace-inherited |
| 5 MUTATION ≥90% | ✅ | **96.9% kill rate** (95/98 viable · 3 survivors are defensive `\|\|→&&` guard-clause flips · acceptable per ADR-038 set-equivalence threshold) · added `tests/ranking_parity.rs` (5 pairwise ordering pins) + `tests/golden_values.rs` (5 exact-score pins within 1e-9) |
| 6 PROPERTY | ✅ | `tests/proptest_invariants.rs` · 4 proptest (256 cases each) + 2 deterministic (monotonicity + saturation) · commit `2da7c1e24` |
| 7 BENCHMARKS | ✅ | `benches/bm25_bench.rs` · 3 criterion benches · `manning` · `synthetic_100` · `finalize_100` · `[profile.bench]` LTO inherited |
| 8 DOCS | ✅ | `cargo doc --no-deps` 0 warnings · 1 doctest passes · `rustdoc::broken_intra_doc_links=deny` inherited |
| 9 CANARY | **EXEMPT** | Pure-algo · no I/O · no provider call · no MCP server · workflow harness N/A. Justified per `nika-bm25.md` §1 « pure-algo · no async I/O beyond `consume_budget()` ». |
| 10 PARITY | **EXEMPT** | CRAFT not extraction · `git show brouillon:tools/` had no BM25 impl (verified · brouillon predates Diamond memory subsystem) · greenfield crate per ADR-001. |
| 11 REVIEW | ⏸️ pending | 3-agent swarm (`spn-nika:code-reviewer` + `spn-rust:rust-pro` + `feature-dev:code-reviewer`) · deferred to next session (session-cap 3/3 reached) |
| 12 ATOMIC | ⏸️ blocked on 5+11 | Single commit adds to `[workspace.members]` + admission body per `commit-granularity.md` |

---

## 6. Forward-compat (FCI · per ADR-007)

- **FCI-003** · on-disk format · `BmIndex` postings serialization · DEFERRED to W3 GREEN phase · candidate format = bincode-compressed postings OR `xsd:base64Binary` literals in Oxigraph (per ADR-029 reservation · placeholder ADR-043) · NOT locked at admission · re-evaluate when `nika-memory` orchestrator (W10) needs persistence
- **FCI-006** · `MemoryRecall` sealed trait per ADR-014 · `BmIndex: MemoryRecall` impl crosses sealed boundary via `nika-kernel` workspace dep (not external impl)
- **`#[non_exhaustive]`** on `BmParams` + `BmIndex` per INV-019 · `pub fn new()` constructors mandatory

---

## 7. References

- Robertson, S. E., & Walker, S. (1994). « Some simple effective approximations to the 2-Poisson model for probabilistic weighted retrieval. »
- Manning, C. D., Raghavan, P., & Schütze, H. (2008). *Introduction to Information Retrieval* · ch. 11 « Probabilistic information retrieval ».
- `furkantoprak/okapibm25` · MIT · reference impl for fixture parity (Gate 2)
- `tantivy` · `bm25` Rust crate · `lib.rs` (read for inspiration only · NOT copy-paste · per ADR-001 CRAFT discipline)
- `nika/engine/docs/adr/adr-038-nika-bm25-admission.md` · binding admission prep
- `nika/engine/docs/architecture/BLUEPRINT_2036.md` v1.3 §2.5 nika-bm25 row
