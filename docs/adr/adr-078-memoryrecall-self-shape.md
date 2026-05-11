---
id: ADR-078
title: "MemoryRecall &self satellite contract + &mut self orchestrator pool (Option D)"
status: proposed
date: "2026-05-12"
phase: "Phase 2 (pre-W3 nika-bm25 admission blocker)"
deciders: ["@ThibautMelen"]
tags: ["kernel", "memory", "trait-shape", "send-sync", "interior-mutability", "satellite", "pool", "restate-pattern"]
affects_crates: ["nika-kernel", "nika-bm25", "nika-hnsw", "nika-rrf", "nika-fsrs", "nika-graph-algos", "nika-rdfs-reasoner", "nika-temporal", "nika-autodesc-minimal", "nika-autodesc-full", "nika-memory"]
affects_layers: ["L0.5", "L1", "L2"]
supersedes: []
superseded_by: []
related: ["ADR-006", "ADR-014", "ADR-016", "ADR-038", "ADR-039", "ADR-040", "ADR-041", "ADR-042", "ADR-079"]
requires: ["ADR-006", "ADR-014"]
enables: ["ADR-038", "ADR-039"]
fci: ["FCI-006"]
inv: ["INV-016", "INV-017", "INV-029"]
shadow_zones: []
nika_codes: ["NIKA-605 (proposed · SatellitePoisoned · W10 activation)"]
timeline: "ship pre-W3 nika-bm25 Gate 12 atomic commit · same wave as sealed-pattern lock pass"
follow_ups: ["nika-memory W10 RecallPool concrete struct", "potential MemoryRecallPool trait promotion if consumer signal emerges"]
---

# ADR-078: `MemoryRecall::recall` trait shape · Option D · split satellite vs pool

## Context

`crates/nika-kernel/src/ai/memory.rs:244-250` defines the canonical L0.5 trait:

```rust
#[trait_variant::make(MemoryRecallDyn: Send)]
pub trait MemoryRecall: Send + Sync {
    async fn recall(&self, query: RecallQuery) -> Result<Vec<MemoryHit>, MemoryError>;
}
```

Per Ryhl/Terekhin 2026 (« the Sync bound nobody asked for ») the `async fn recall(&self, ...)` desugar captures `&self` across `.await`, forcing `Self: Sync`. Every L1 satellite then wraps mutable state (LRU caches, stats counters, IDF tables) in `Mutex/RwLock` — measurable interior-mutability tax at 1M+ BGE-M3 cosine calls/sec on the hot path.

The 9 memory satellites are about to admit through 12-gate ceremony — `nika-bm25` is W3 (immediate next). Changing the trait shape **post-admission** cascades through all 9 satellites + orchestrator + spec (`nika-bm25.md:66-68`). MUST resolve before W3 commit.

Anchor: BLUEPRINT_2036.md v1.5 §ADR-078 entry (locked 2026-05-12).

## Decision

**Adopt Option D — separate concerns by layer.**

### L0.5 kernel contract (UNCHANGED)

The kernel trait keeps `&self + Send + Sync`. Ships sealed (ADR-014) in the same commit:

```rust
mod private { pub trait Sealed {} }

#[trait_variant::make(MemoryRecallDyn: Send)]
pub trait MemoryRecall: private::Sealed + Send + Sync {
    /// CANCEL SAFETY: cancel-safe (read-only similarity search).
    /// SYNC BOUND: explicit per ADR-078. Satellites SHOULD use parking_lot::Mutex,
    /// DashMap, or AtomicU64 for stats/cache state. Hot-path contention is bounded
    /// by per-satellite sharding at the L2 RecallPool boundary.
    async fn recall(&self, query: RecallQuery) -> Result<Vec<MemoryHit>, MemoryError>;
}

impl<T: MemoryRecall + ?Sized> private::Sealed for T {}
```

Sister traits `MemoryRemember::remember(&self, frame)` (line 240) and `MemoryForget::forget(&self, id)` (line 259) keep symmetric `&self + Send + Sync` shape — same rationale, ISP discipline preserved (ADR-006).

### L2 orchestrator pool contract (NEW at W10 · `nika-memory`)

When `nika-memory` admits at W10, it ships a concrete (non-trait) `RecallPool` struct that owns exclusive `&mut self` access to dispatch tables + stats + RRF state — wrapped in `tokio::sync::RwLock` at the `NikaStore` actor boundary (per ADR-041 type-state):

```rust
// crates/nika-memory/src/pool.rs (NEW at W10)
pub struct RecallPool {
    satellites: Vec<Arc<dyn MemoryRecallDyn>>,  // 9 satellites · &self contract preserved
    stats: PoolStats,
    rrf_k: f64,
    budget: BudgetTracker,
}

impl RecallPool {
    pub async fn recall_fused(
        &mut self,
        query: RecallQuery,
        k: usize,
    ) -> Result<Vec<MemoryHit>, MemoryError> { /* ... */ }
}
```

Pattern matches Restate stateful-vs-stateless service split: satellites = stateless handlers (per-call), pool = stateful keyed Virtual Object (cross-call mutation). Tower `&mut self` semantics at the pool layer without forcing satellite `Clone`.

Trait promotion (`pub trait MemoryRecallPool`) deferred until Step 0 consumer signal — second pool implementation surfaces (LOCK-031 spirit).

## Consequences

**Positive:**
- Zero W3 cascade — `nika-bm25` admits with spec-locked shape (`nika-bm25.md:66-68`)
- 9 ADRs collectively consistent: ADR-006/014/016/038/039/041 all compatible
- `Arc<dyn MemoryRecallDyn>` fan-out preserved (BLUEPRINT v1.5 line 130 canon)
- ADR-039 streaming `recall_stream` default impl ships at W4 without prerequisite refactor
- Restate canonical pattern adopted at L2 — best-architects 2030 ratchet

**Negative (acknowledged · measured):**
- Interior-mutability tax at L1 satellites (Ryhl-flagged) — mitigated by per-satellite sharding + criterion bench gate at admission (P99 recall < 2ms / 8-thread fan-out)
- Loom shadow ratchet adds Gate 9.5 cost per admission (Mara Bos canonical · BLUEPRINT v1.5 line 267)

## 4-option grade matrix (lenses: Maint · TypeSafety · ForwardCompat · Doctrine · Idioms2026 · Cascade)

| Option | Total | Verdict |
|---|---|---|
| A · `&mut self` Tower idiom | breaks `Arc<dyn>` per line 130 + ADR-039 prerequisite | **REJECT** |
| B · keep `&self` canonical (today) | viable but Ryhl-tax unmeasured | viable but fragile |
| C · split Shared/Exclusive sub-traits | premature abstraction · 2× `trait_variant` surface | **REJECT** |
| **D · L0.5 `&self` + L2 `&mut self` pool** | zero cascade · 9 ADRs consistent · Restate-canonical | **ACCEPT** |

## Migration plan

| Step | Wave | Action | Verify |
|---|---|---|---|
| 1 | W3 same commit | Apply sealed-trait pattern (`mod private { trait Sealed }` + blanket impl) | `cargo public-api` diff additive |
| 2 | W3 same commit | Lock SYNC BOUND rationale docstring at `memory.rs:245-248` citing ADR-078 + Ryhl context | `cargo doc --no-deps` 0 warn |
| 3 | W3 admission | `nika-bm25` admits with canonical `&self` shape · `parking_lot::Mutex<BmIdfCache>` justified inline | spec parity ✅ |
| 4 | W3 admission | `#[cfg(loom)]` 2-thread × 2-recall-fanout shadow test (Mara Bos ratchet · Gate 9.5) | `RUSTFLAGS="--cfg loom" cargo test --release` |
| 5 | W4 nika-rrf | `recall_stream` default impl ships (ADR-039 enabled) | `cargo semver-checks` non-breaking |
| 6 | W10 nika-memory | `RecallPool` concrete struct · `&mut self` · `Arc<dyn>` per-satellite | E2E canary |
| 7 | W11+ if signal | Promote `RecallPool` concrete → `MemoryRecallPool` trait (Step 0 gated · LOCK-031) | consumer signal log |

## NIKA error code reservation

**NIKA-605 · `SatellitePoisoned`** (Memory range 600-649) — activates at W10 when `RecallPool` integrates `Mutex/RwLock`-poisoned satellite detection. `is_transient() = false` → pool drops + rebuilds the satellite (Restate-style supervision per ADR-070 child-token tree). Code reserved · concrete activation deferred to W10.

## Risk audit (5 measurable assertions)

1. **Interior-mutability tax materializes (60% likelihood)** — escalation: sharded `DashMap` migration (ADR-082 candidate) · gate: P99 recall < 5ms at 8-thread fan-out
2. **`Arc<dyn>` v-table dispatch cost > 5% (40%)** — escalation: typed `enum SatelliteHandle { Bm25, Hnsw, ... }` · 1 day refactor L2-only
3. **NUKE-LEGACY sealed blocks external satellite (10%)** — escalation: adapter-macro pattern ADR-014 step 2 · consumer-signal gated
4. **`RecallPool` type-state clashes with `NikaStore::shutdown(self)` (15%)** — escalation: ADR-077 async-drop migration absorbs
5. **Loom shadow runtime > 2min wall-clock (25%)** — escalation: per-satellite Loom budget at W7 nika-hnsw

## Forward-compat invariants

- **FCI-078a** · `MemoryRecall` trait shape sealed via `private::Sealed` — external impls blocked structurally
- **FCI-078b** · `Send + Sync` bound explicit + documented · removing it = breaking change (semver-checks gate)
- **FCI-078c** · `RecallPool` concrete struct (not trait) at W10 — promotion to trait requires consumer-signal ADR amendment

## 5 raisons coherence

| Raison | Grade | Justification |
|---|:-:|---|
| ① Liberté cognitive | ✅ | Kernel surface unchanged · cognitive load on satellite implementor zero-delta |
| ② Souveraineté | ✅ | Zero vendor lock-in · SOTA references (Ryhl · Tower · Restate · Mara Bos) cited as inspiration |
| ③ Joy 🦋 | ✅ | NUKE-LEGACY clean · zero shim · additive sealed + streaming + pool |
| ④ Composable galaxy | ✅ | 9 publishable satellites preserved · pool is L2-only · adapter-macro path for external |
| ⑤ Studio signature | ✅ | Layer-correct decision · « kernel ISP · orchestrator pool · zero-cost dyn-dispatch » = Diamond signature |

🦋
