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
the full candidate set per ranker (per `crates/nika-kernel/src/ai/memory.rs:248-250`).
For Reciprocal Rank Fusion (W4 · `nika-rrf` admission) the L2 orchestrator wants to
fuse top-K across N rankers without realizing all candidates from each.

Per Agent 1 (rust-architect) memo 2026-05-11 §5 · the materialized `Vec` blocks
lazy ranker streams. RRF mathematically only needs the top-K from each ranker to
compute `score(d) = Σ 1/(k + rank_i(d))` · materializing 10k candidates per ranker
when K=20 wastes the embedding decode + index lookup.

## Decision

Amend the `MemoryRecall` trait (sealed per ADR-014) with an additive
`recall_stream()` method · default impl over existing `recall()` for backward
compatibility within v0.x (forever-v0.x · ADR-002).

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
- `crates/nika-kernel/src/ai/memory.rs:218-260` — current trait surface
- ADR-038 — W3 nika-bm25 (consumer of this trait)
- ADR-040 — Cargo feature matrix (rrf feature consumes stream)
- Agent 1 rust-architect memo 2026-05-11 §5 — Option D rationale

## Alternatives considered

### Alt A — Keep recall() vec-only
Rejected · forces N-way fusion to realize all candidates · violates zero-alloc-hot-path
discipline.

### Alt B — Replace recall() with recall_stream() (breaking)
Rejected per forever-v0.x · breaking changes ship on MINOR but pure-additive is cheaper.

### Alt C — Tower::Service request/response wrapper
Rejected · adds tower dep · L2 orchestrator doesn't need middleware stack today ·
revisit if cross-cutting concerns (rate-limit · circuit-breaker) emerge.

## Related
- ADR-006 — ISP atomic traits
- ADR-014 — sealed kernel traits
- ADR-038 — nika-bm25 admission (W3 trait consumer)
- ADR-040 — Cargo feature matrix (rrf gates streaming)

## Notes
Migration · ADR-039 lands AT W4 (rrf admission) NOT at W3 · bm25 ships with default-impl-only · rrf admission ships the native stream + first consumer. Two-phase landing prevents
shadow-zone shadow (trait amendment without consumer = ADR-007 violation).
