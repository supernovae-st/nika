---
id: ADR-039
title: "Streaming MemoryRecall for lazy fusion · recall_stream()"
status: proposed
date: "2026-05-12"
phase: "Phase 1.5 (W4 admission target)"
deciders: ["@ThibautMelen"]
tags: ["memory", "trait-surface", "streaming", "zero-alloc", "fusion"]
affects_crates: ["nika-kernel", "nika-memory", "nika-rrf"]
affects_layers: ["L0.5", "L1", "L2"]
supersedes: []
superseded_by: []
related: ["ADR-006", "ADR-007", "ADR-014", "ADR-038", "ADR-040", "ADR-078"]
requires: ["ADR-006", "ADR-014", "ADR-078"]
enables: ["ADR-040"]
fci: ["FCI-006"]
inv: []
shadow_zones: []
nika_codes: []
timeline: "W4 (nika-rrf admission · 2026-06/07)"
follow_ups:
  - "Gate 5 mutation on nika-rrf · proptest set-equivalence top-K (NOT exact float equality) · per Agent 1 §4 mitigation"
---

# ADR-039 — Streaming `MemoryRecall` for lazy RRF fusion

## Context

Today `MemoryRecall::recall() -> Result<Vec<MemoryHit>, MemoryError>` materializes
the full candidate set per ranker (per `crates/nika-kernel-ai/src/memory.rs:248-250`).
For Reciprocal Rank Fusion (W4 · `nika-rrf` admission) the L2 orchestrator wants to
fuse top-K across N rankers without realizing all candidates from each.

Per Agent 1 (rust-architect) memo 2026-05-11 §5 · the materialized `Vec` blocks
lazy ranker streams. RRF mathematically only needs the top-K from each ranker to
compute `score(d) = Σ 1/(k + rank_i(d))` · materializing 10k candidates per ranker
when K=20 wastes the embedding decode + index lookup.

## Decision

Amend the `MemoryRecall` trait (sealed per ADR-014) with an additive
`recall_stream()` method · default impl over existing `recall()` for backward
compatibility (additive forward-compat per ADR-002 · amended D-2026-06-20-N1 · real semver toward 1.0).

```rust
// nika-kernel/src/ai/memory.rs (additive)
use core::pin::Pin;
use futures_core::Stream;

#[trait_variant::make(Send)]
pub trait MemoryRecall: private::Sealed + Send + Sync {
    async fn recall(&self, query: &RecallQuery) -> Result<Vec<MemoryHit>, MemoryError>;

    /// Lazy stream variant for L2 orchestrator fusion · default delegates to recall().
    /// L1 satellites with native streaming (e.g. nika-bm25 disk-iter · nika-hnsw
    /// k-NN walker) override for zero-alloc top-K extraction.
    fn recall_stream<'a>(
        &'a self,
        query: &'a RecallQuery,
    ) -> Pin<Box<dyn Stream<Item = Result<MemoryHit, MemoryError>> + Send + 'a>> {
        Box::pin(async_stream::stream! {
            match self.recall(query).await {
                Ok(hits) => for hit in hits { yield Ok(hit); },
                Err(e) => yield Err(e),
            }
        })
    }
}
```

`nika-rrf` consumes N `Pin<Box<dyn Stream<...>>>` · merges via tournament heap ·
emits top-K without realizing the full candidate set.

## Consequences

### Positive
- W4 RRF fusion lands without forcing `Vec` materialization per ranker.
- Default impl preserves backward compatibility · existing `recall()` impls unchanged.
- Native streaming impl for `nika-bm25` (W3) + `nika-hnsw` (W7 trigger-gated) reduces hot-path alloc.
- Additive change per ADR-007 · forward-compat invariant respected (sealed trait amendment OK).

### Negative
- `async_stream` proc-macro dep added to `nika-kernel` (MIT · binary impact measured at W4 admission via `cargo bloat --release`).
- `Pin<Box<dyn Stream>>` heap alloc per recall call. Mitigated · if Rust 1.85+ stabilizes
  `impl Stream + use<>` precise capture (RFC 3617), migrate to zero-alloc return at Phase 2.
- Mutation testing on stream merging requires proptest set-equivalence (not exact float
  equality) per Gate 5 ≥90% gotcha noted in Agent 1 memo.

### Neutral
- Sealed pattern (ADR-014) survives · default impl in trait body · external impls remain blocked.

## Evidence
- `crates/nika-kernel-ai/src/memory.rs:218-260` — current trait surface
- ADR-038 — W3 nika-bm25 (consumer of this trait)
- ADR-040 — Cargo feature matrix (rrf feature consumes stream)
- Agent 1 rust-architect memo 2026-05-11 §5 — Option D rationale

## Alternatives considered

### Alt A — Keep recall() vec-only
Rejected · forces N-way fusion to realize all candidates · violates zero-alloc-hot-path
discipline.

### Alt B — Replace recall() with recall_stream() (breaking)
Rejected per the forward-compat discipline (ADR-002 · amended D-2026-06-20-N1) · pre-1.0 breaking changes ship on MINOR but pure-additive is cheaper.

### Alt C — Tower::Service request/response wrapper
Rejected · adds tower dep · L2 orchestrator doesn't need middleware stack today ·
revisit if cross-cutting concerns (rate-limit · circuit-breaker) emerge.

## SOTA locks 2026-05-12 (post rust-async-expert + rust-ml + web-researcher audit)

Per parallel 3-agent dispatch 2026-05-12 PM · convergent SOTA Rust 2026 locks ·

### L-1 · Stream type · `Pin<Box<dyn Stream + Send + '_>>` canonical

LOCK · per rust-async-expert §3 · the canonical 2026 trait surface for
streaming recall is ·

```rust
fn recall_stream<'a>(
    &'a self,
    query: &'a RecallQuery,
) -> Pin<Box<dyn Stream<Item = Result<MemoryHit, MemoryError>> + Send + 'a>>;
```

**NOT** RPIT `impl Stream` (not yet dyn-safe at trait surface · kills `Arc<dyn>`).
**NOT** `async fn -> impl Stream` (forces 2 await points · breaks cancel-on-first-await).

### L-2 · Stream impl · `async_stream::stream!` macro

LOCK · canonical 2026 form for yielding from algorithm loops ·

```rust
fn recall_stream(&self, q: &RecallQuery) -> Pin<Box<dyn Stream<...> + Send + '_>> {
    let qtokens = tokenize(&q.text);
    Box::pin(async_stream::stream! {
        for (id, tf, doc_len) in self.docs_iter() {
            let score = compute(qtokens, tf, doc_len, ...);
            yield Ok(self.to_hit(id, score));
            if id.is_multiple_of(64) {
                tokio::task::yield_now().await; // §L-5 cooperation
            }
        }
    })
}
```

ROI · `async-stream 0.3` macro_rules! dep (zero runtime cost · Tower/hyper ecosystem standard 2026).

### L-3 · `MemoryRecallStream` separate trait (Pattern A)

LOCK · per rust-async-expert §9 Q-B · ship `MemoryRecallStream` as a
**separate sealed trait** rather than overloading `MemoryRecall` with a
default-impl on top of `recall`. Rationale · matches publishable-standalone
story (simple satellite ships `MemoryRecall` only without `async-stream` dep ·
streaming satellite opts in to `MemoryRecallStream`). ISP discipline preserved.

```rust
#[trait_variant::make(MemoryRecallStreamDyn: Send)]
pub trait MemoryRecallStream: Send + Sync + crate::sealed::Sealed {
    /// CANCEL SAFETY · cancel-safe · drop-cancel propagates.
    fn recall_stream<'a>(
        &'a self,
        query: &'a RecallQuery,
    ) -> Pin<Box<dyn Stream<Item = Result<MemoryHit, MemoryError>> + Send + 'a>>;
}
```

### L-4 · `JoinSet` (NOT FuturesUnordered, NOT try_join_all) for 9-way fan-out

LOCK · per rust-async-expert §4 · `RecallPool` (W10) uses `tokio::task::JoinSet`
for parallel satellite fan-out. Rationale ·
- True multi-core parallelism (BM25 CPU-bound · HNSW SIMD · must spread across cores)
- `JoinSet::Drop → abort_all()` cascades cancellation cleanly · zero explicit plumbing
- Early-terminate at k results · drop loop · drop JoinSet · stragglers aborted

### L-5 · `tokio::task::yield_now()` for cooperation (NOT `consume_budget`)

LOCK · per rust-async-expert §6 · use stable `tokio::task::yield_now()` ·
DO NOT use `tokio::task::consume_budget()` (still unstable · semantics change
1.40→1.42). Yield at the STREAM BOUNDARY (per-yielded-item via `Stream::poll_next`
return Pending IS the cooperation point) PLUS explicit `yield_now()` every
N=64 scored items on tight CPU-bound loops.

### L-6 · CancelCtx layering · L0 NO `&CancelCtx` · L2 TAKES `&CancelCtx`

LOCK · per rust-async-expert §7 · `MemoryRecall` / `MemoryRecallStream` traits
at L0.5 **stay free of `CancelCtx`** (cancellation is implicit via drop-cancel ·
adding CancelCtx couples L0.5 to ADR-016 runtime concern · breaks layering).
The L2 `RecallPool` **DOES take `&CancelCtx`** · uses `ctx.child("sat:bm25")`
per ADR-070 child-token tree.

### L-7 · `tokio_util::task::TaskTracker` for L2 pool graceful drain

LOCK · per rust-async-expert §7 · `RecallPool` uses `TaskTracker` for graceful
drain on shutdown · 2 cancel layers ·
- `req_cancel.child()` per-recall-call (request-scoped)
- `sat_cancel = pool.cancel.child_token()` per-satellite (pool-scoped shutdown)

### L-8 · v-table dispatch cost is acceptable · DROP ADR-078 risk #2

LOCK · per rust-async-expert §5 · `Arc<dyn MemoryRecallStreamDyn>` v-table
dispatch is ~2-4ns/call · negligible vs 100-500ms BGE-M3 embed call. Stay
with `Arc<dyn>` · keeps publishable-standalone story intact + cargo-public-api
+ cargo-semver-checks happy with opaque trait objects. The ADR-078 risk #2
(typed enum escape hatch) stays as deferred escape if Gate 7 benches at W7
show >5% regression vs enum baseline · but DO NOT pre-optimize.

### L-9 · RRF k=60 industry standard 2026 confirmed

LOCK · per rust-ml §4 + web-researcher §2 · Cormack 2009 `k=60` smoothing
constant remains 2026 industry standard (Microsoft Azure AI Search · MongoDB ·
ParadeDB · Lucene 10 all converge on k=60 default). `RrfParams::default()` at
W4 admission MUST ship `k: 60` per Cormack 2009 anchor.

`nika-rrf::RrfParams` canonical shape ·
```rust
pub struct RrfParams {
    /// Smoothing constant · canonical 60 (Cormack 2009).
    pub k: u32,
    /// Optional per-retriever weights · None = unweighted (Cormack pure) ·
    /// Some(_) = Mem0-style weighted RRF (forward-compat reserved).
    pub weights: Option<Vec<f64>>,
}
```

### L-10 · Score normalization · « don't normalize · fuse RANKS »

LOCK · per rust-ml §3 + web-researcher §1 · the 2026 SOTA consensus is
**rank-fusion via RRF beats score-normalization**. Raw `f64` scores from
`top_k` are correct · L2 `RecallPool` derives ranks from sorted output ·
`nika-rrf` consumes ranks. NO normalization at satellite layer.

Documented inline in `nika-bm25::BmParams` rustdoc 2026-05-12.

## Related
- ADR-006 — ISP atomic traits
- ADR-014 — sealed kernel traits
- ADR-038 — nika-bm25 admission (W3 trait consumer)
- ADR-040 — Cargo feature matrix (rrf gates streaming)

## Notes
Migration · ADR-039 lands AT W4 (rrf admission) NOT at W3 · bm25 ships with default-impl-only · rrf admission ships the native stream + first consumer. Two-phase landing prevents
shadow-zone shadow (trait amendment without consumer = ADR-007 violation).
