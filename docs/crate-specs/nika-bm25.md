# Crate spec — `nika-bm25`

| | |
|---|---|
| Status | **ADMITTED 2026-05-12** (`36ca8c3ee`) · was **Phase 1.5 W3 admission target** (first L1 memory satellite · ADR-004 1+9 architecture · ADR-038 admission prep shipped) |
| Layer | L1 — memory satellite · pure-algo · zero I/O at trait boundary · `Send + Sync` |
| Sub-tier | L1-deterministic — no ML deps · no async · pure-sync CPU-bound scoring (cooperation delegated to L2 `RecallPool` fan-out boundary per ADR-078 step 6) |
| Design | **Pure BM25 (Okapi)** — Robertson 1994 canonical formula · k1 saturation · b length-norm · IDF smoothing |
| LOC budget | ≤800 src (target ~600 · scoring kernel ~150 + index ~200 + query ~100 + impls ~150) |
| File cap | ≤1,500 LOC each (none expected near cap) |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace (`0.90.0`) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `true` — publishable on crates.io per ADR-004 « 10 satellites publishable standalone » external open-source value |
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
beyond what the L2 `RecallPool` fan-out arranges (cooperative yield is the
pool's responsibility per ADR-078 step 6 · satellite stays pure-sync). Anything that
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
pub struct BmIndex { /* per-doc tf-tables · df (document-frequency) map · IDF · avgdl */ }

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
| `index.rs` | ~200 | `BmIndex` · per-doc tf-tables · df (document-frequency) map · doc-length stats · IDF computation. **No postings list yet** (term→doc-ids) · `top_k` scores the full corpus · see P-4 below |
| `scorer.rs` | ~118 (shipped) | Pure-sync `idf_robertson` + `term_score` Robertson 1994 canonical · no async · cooperation delegated to L2 pool per ADR-078 step 6 |
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

# Tokio · gated behind `kernel` feature · used only by the deferred
# `MemoryRecall` adapter (W4-W10) · NOT by the pure-algo core.
tokio = { workspace = true, features = ["rt"], optional = true }

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
| 9 CANARY | **EXEMPT** | Pure-algo · no I/O · no provider call · no MCP server · workflow harness N/A. Justified per `nika-bm25.md` §1 « pure-sync · cooperation delegated to L2 RecallPool ». |
| 10 PARITY | **EXEMPT** | CRAFT not extraction · `git show brouillon:tools/` had no BM25 impl (verified · brouillon predates Diamond memory subsystem) · greenfield crate per ADR-001. |
| 11 REVIEW | ✅ | Gate 11 self-review 2026-05-12 (spn-nika agent context-thrashed · fallback direct grep+verify) · 11/11 commits Nika 🦋 trailer · 0 Claude leak · 0 unwrap src · sealed pattern OK · total_cmp OK · BmIndex `#[non_exhaustive]` P1 fix shipped this commit · L1 layer registry OK · ADR cross-links OK · spec coherent |
| 12 ATOMIC | ✅ | This commit · adds INV-019 `#[non_exhaustive]` on BmIndex · flips Gate 11 + 12 ✅ · workspace member already at line 3 + diamond layer at line 47 |

---

## 6. Performance roadmap (post rust-pro + rust-perf SOTA audit 2026-05-12)

W3 admission ships **correct + clean + lint-pure** · **all 3 Gate 7 bench
targets met with massive headroom** (per rust-perf audit 2026-05-12 PM
on engine main `36ca8c3ee`) ·

| Bench | Measured | Target | Headroom |
|---|---|---|---|
| `manning_top_k_3` | 454 ns | sub-µs | 2.2× under |
| `synthetic_100_top_k_10` | 14.05 µs | <1 ms | 71× headroom (trivial scale) |
| `finalize_100` | 331 µs | <5 ms | 15× headroom |
| `synthetic_10k_top_k_10` | **1.49 ms** | <5 ms (10k scale) | **3.3× headroom** |
| `finalize_10k` | (measured next iter) | <500 ms (10k scale) | (pending) |

Per-doc scoring cost ~140 ns at 3-token query × ~30-token avg doc · right
on cache-line latency for non-pathological BTreeMap walk. SIMD opportunity
zero at this scale (`idf_robertson` pre-computed once at finalize ·
14 unique vocab terms × 50 ns total).

**Production-scale validation 2026-05-12** (post socratic critique Q6) ·
linear-scale assumption HOLDS at 10k docs · 1.49 ms vs 1.4 ms linear
extrapolation = within 6% (cache effects minimal · BTreeMap log-N
behaviour empirical). **At 100k+ docs** the P-2 BTreeMap → FxHashMap
migration becomes meaningful (predicted ~15 ms linear vs target 50 ms ·
3× headroom · marginal · ADR-082 trigger).

3 named perf trade-offs · all empirically deferred · all spec'd to prevent W4-W10 cascade.

- **P-1 · `Vec<String>` tokenize allocation** (`tokenize.rs:14`) · per-call heap alloc per token. Acceptable for W3 admission (small corpora). Migrate to iterator API (`fn tokens(&str) -> impl Iterator<Item = &str>` lifetime-tied · zero-alloc) at W4 if `synthetic_100_top_k_10` criterion bench > 1ms. Cross-link · BLUEPRINT v1.5 Gallant ripgrep 0-alloc CLI craft ratchet · tantivy `TextAnalyzer::token_stream` precedent.
- **P-2 · `BTreeMap<String, u32>` tf-table** (`index.rs:24`) · O(log n) lookup · poor cache locality. Acceptable for W3 (deterministic iteration · zero unsafe · zero new dep). Migrate to `rustc_hash::FxHashMap` (used by rust-analyzer + cargo + tantivy stacker) at W4 if hot-path benchmark > 1ms. Pre-empt 8-sister cascade by documenting the trade-off now.
- **P-3 · `BmIndex` type-state (Building → Finalized)** (`index.rs:36`) · today runtime `finalized: bool` flag · score-before-finalize is silent stale-IDF math. Migrate to `BmIndex<Building>` / `BmIndex<Finalized>` typestate (ADR-079 family · `PhantomData<S>` zero-cost) at W4 once sealed-pattern stabilizes across 10 satellites. Compile-time enforcement of state machine.
- **P-4 · no postings list · `top_k` full-corpus scan** (`index.rs` `df` map + `query.rs` `top_k`) · the index keeps `df` (term→doc-COUNT) but no postings (term→doc-IDS), so `top_k` scores EVERY document, including those sharing no query term · O(N_docs · |query|) where a postings-driven scan + bounded top-k heap is O(matching_docs · log k). Empirically deferred · `synthetic_10k_top_k_10` measures 1.49 ms (3.3× under the <5 ms budget), so it is NOT a W3 blocker. Add `postings: BTreeMap<String, Vec<u32>>` (built in `add_document`, consumed by `top_k`) at W4 if a larger-corpus bench crosses budget. Correctness-preserving (identical BM25 scores · only skips guaranteed-zero docs).

P-1/P-2/P-3/P-4 are tracked as **ADR-082 candidates** (post-W3 perf hardening cluster). NOT Gate 12 blockers · gate via Gate 7 criterion bench numbers (target `synthetic_100_top_k_10` < 1ms · `finalize_100` < 5ms per spec §5).

---

## 7. Forward-compat (FCI · per ADR-007)

- **FCI-003** · on-disk format · `BmIndex` postings serialization · DEFERRED to W3 GREEN phase · candidate format = bincode-compressed postings OR `xsd:base64Binary` literals in Oxigraph (per ADR-029 reservation · placeholder ADR-043) · NOT locked at admission · re-evaluate when `nika-memory` orchestrator (W10) needs persistence
- **FCI-006** · `MemoryRecall` sealed trait per ADR-014 · `BmIndex: MemoryRecall` impl crosses sealed boundary via `nika-kernel` workspace dep (not external impl)
- **`#[non_exhaustive]`** on `BmParams` + `BmIndex` per INV-019 · `pub fn new()` constructors mandatory

---

## 8. References

- Robertson, S. E., & Walker, S. (1994). « Some simple effective approximations to the 2-Poisson model for probabilistic weighted retrieval. »
- Manning, C. D., Raghavan, P., & Schütze, H. (2008). *Introduction to Information Retrieval* · ch. 11 « Probabilistic information retrieval ».
- `furkantoprak/okapibm25` · MIT · reference impl for fixture parity (Gate 2)
- `tantivy` · `bm25` Rust crate · `lib.rs` (read for inspiration only · NOT copy-paste · per ADR-001 CRAFT discipline)
- `nika/engine/docs/adr/adr-038-nika-bm25-admission.md` · binding admission prep
- `nika/engine/docs/architecture/BLUEPRINT_2036.md` v1.3 §2.5 nika-bm25 row

## Audit trail (continued)

| Date | What | Detail |
|---|---|---|
| 2026-06-12 | `EagerIndex` — the BM25S architecture | arXiv-grounded (Lù 2024 *BM25S: Orders of magnitude faster lexical search via eager sparse scoring* · arxiv.org/abs/2407.03618): once the corpus is frozen, every `(term, doc)` BM25 contribution is fully determined — precompute at build time, make a query a SPARSE accumulation over its terms' postings (docs sharing no vocabulary are never visited; the lazy path is `O(N·|Q|)` dense). Additive new type built FROM a finalized `BmIndex` (the separate type IS the freshness contract: cannot exist unfinalized · cannot be silently staled — a re-opened source refuses `build` until re-finalize). **Byte-identical to the lazy path** — the accumulator adds in QUERY-TOKEN order (the lazy per-doc sum's order; IEEE addition is non-associative, matching the order is what buys exactness) — pinned by 5 unit tests + a proptest equivalence over the corpus/query space (property #6 in the Gate-6 suite). Degenerate-case guards deleted ON PURPOSE (empty corpus/query/k=0 fall through the same path — the round-19 lesson: a fast-path guard is an equivalent-mutant factory). Gate 5 on `eager.rs`: 14 mutants → 13 caught · 1 unviable · **0 missed**. API baseline regenerated: 74→105 lines, **0 removals** (additive proof). Citations shipped repo-wide: `CITATIONS.md` (15 works · per-module mapping · correction policy). |
| 2026-06-12 | BM25+ active · `MaxScore` pruning · `rank.rs` selection | (1) the reserved `delta` WIRED through scorer + both query paths (Lv & Zhai 2011 · matching terms bounded below by δ·idf · `None ≡ Some(0.0) ≡` Okapi byte-identical). (2) `EagerIndex::top_k_pruned{,_stats}` — `MaxScore` dynamic pruning (Turtle & Flood 1995 · arXiv anchor Qiao et al. 2023 · arxiv.org/abs/2305.01203): per-term max scores at build · essential/non-essential split · strict-`<` prune (ties always evaluated) · `PruneStats` makes pruning MEASURABLE · contract is RANK-exact with ≤1-ULP scores (term reorder ≡ the algorithm · IEEE non-associativity · pinned at 1e-9 relative). (3) `rank.rs::select_top_k` — the ONE ranking order (score desc · doc asc · `total_cmp`) + bounded-heap selection (IIR ch.7 §7.1 · O(N log k) vs the previous 3-site full sort) consumed by both paths; the tie unit test caught an inverted doc-half in the heap `Ord` BEFORE wiring; `term_idf` hoisted out of the per-(doc,term) hot loop. (4) `tests/research_conformance.rs` — one executable property per arXiv claim (BM25S byte-equality at scale · `MaxScore` rank-exact + measured pruning · BM25+ lower bound at scale · BM25+ preserves Okapi single-term order). Mutation: scorer 47/47 with integration suites · rank 14 caught + 1 false-missed hand-verified killed. API baseline 105→109, 0 removals. |
