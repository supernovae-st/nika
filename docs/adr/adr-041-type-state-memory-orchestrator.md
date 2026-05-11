---
id: ADR-041
title: "Type-state lifecycle for nika-memory orchestrator"
status: proposed
date: "2026-05-12"
phase: "Phase 1.5 (W10 orchestrator admission)"
deciders: ["@ThibautMelen"]
tags: ["memory", "type-state", "orchestrator", "compile-time", "zero-cost"]
affects_crates: ["nika-memory"]
affects_layers: ["L2"]
supersedes: []
superseded_by: []
related: ["ADR-006", "ADR-012", "ADR-014", "ADR-040", "ADR-078", "ADR-079"]
requires: ["ADR-040"]
enables: []
fci: ["FCI-006"]
inv: ["INV-016"]
shadow_zones: []
nika_codes: []
timeline: "W10 orchestrator admission · Phase 1.5 close"
follow_ups: []
---

# ADR-041 — Type-state lifecycle for `nika-memory` orchestrator

## Context

The L2 orchestrator `nika-memory` (W10) composes N L1 satellites via runtime
dispatch (`Arc<dyn MemoryRecall>` etc. per ADR-006). Without compile-time
state-tracking, methods that should only run post-binding (`recall()` after
backends bound) can be called pre-binding · runtime panic risk · violates
zero-unwrap discipline.

Per Agent 1 (rust-architect) §1 + Perplexity 2026-05-12 synthesis (MindPalace
7-layer pattern · zero-cost newtype + const-generic invariants) · the 2026
Rust idiom for actor-style orchestrators IS type-state.

## Decision

`NikaStore` carries a phantom-type state parameter · methods gated per state ·
zero-cost (phantom is monomorphized away).

```rust
// nika-memory/src/lib.rs
use core::marker::PhantomData;

pub struct NikaStore<S = Building> {
    satellites: Satellites,                       // private · Option<Arc<dyn MemoryRecall>> · etc.
    backend: Option<Arc<dyn MemoryBackend>>,      // private · construction only via build()
    _state: PhantomData<S>,                       // private · sealed by visibility
}

// Type-state markers (zero-size · zero-cost · pub for consumer type-bounds · NO ctor)
pub struct Building;
pub struct Ready;
// `Querying` marker DELETED per review (was dead code · no impl block referenced it).
// Reintroduce as `Closing` / `Closed` only if explicit teardown ceremony crystallizes
// (per no-legacy-no-back-compat.md doctrine · no speculative abstraction).

// Methods on Building only
impl NikaStore<Building> {
    pub fn new() -> Self { /* ... */ }
    pub fn with_recall(self, recall: Arc<dyn MemoryRecall>) -> Self { /* ... */ }
    pub fn with_backend(self, backend: Arc<dyn MemoryBackend>) -> Self { /* ... */ }

    pub fn build(self) -> Result<NikaStore<Ready>, BuildError> {
        // Validate: backend bound · at least 1 recall satellite · etc.
        // Compile-time invariant: build() consumes self · no re-build possible
    }
}

// Query methods on Ready only · compile error if called on Building
impl NikaStore<Ready> {
    pub async fn recall(&self, query: &RecallQuery) -> Result<Vec<MemoryHit>, MemoryError> { /* ... */ }
    pub async fn remember(&self, frame: MemoryFrame) -> Result<MemoryId, MemoryError> { /* ... */ }
}
```

Consumer pattern :

```rust
let store = NikaStore::new()
    .with_recall(Arc::new(BmRecall::new()))
    .with_backend(Arc::new(OxigraphBackend::open(path)?))
    .build()?;                                      // Building → Ready

let hits = store.recall(&query).await?;            // OK · Ready impl

// store.with_recall(...)  // COMPILE ERROR · no method on Ready
// NikaStore::<Ready>::recall before build()       // COMPILE ERROR · no path
```

## Consequences

### Positive
- **Compile-time invariant** · `recall()` cannot be called pre-binding · zero runtime check.
- **Zero-cost** · `PhantomData<Building>` monomorphizes to nothing · `NikaStore<Ready>` and `NikaStore<Building>` are distinct types at compile · runtime layout identical.
- **Self-documenting API** · the type signature tells the consumer the lifecycle.
- **Eliminates a class of zero-unwrap bugs** · « called recall() before binding » becomes UNREPRESENTABLE.

### Negative
- **API surface adds one type parameter** · `NikaStore<S>` · mitigated by the default
  parameter at line 47 (`S = Building`) which lets consumers write `NikaStore::new()` and
  let inference fill in `<Building>` · then `build()` returns `NikaStore<Ready>` explicit.
  No separate `pub type` alias needed (would be redundant).
- **Test fixtures need explicit `<Ready>` annotation** in some cases. Acceptable ceremony.
- **`build()` is one-shot** · on `Err(_)` the `NikaStore<Building>` is consumed and
  dropped · consumer cannot retry binding mid-failure · chain all `with_*()` calls
  BEFORE `build()?`. Clean failure mode per NUKE-LEGACY doctrine.

### Neutral
- Pattern proven in 2026 Rust ecosystem (axum routers · tower stacks · per Perplexity citations) · not novel · battle-tested idiom.

## Evidence
- `crates/nika-kernel/src/ai/memory.rs:374-407` — current trait composition (untyped state)
- ADR-006 — atomic ISP traits (orchestrator composes)
- ADR-012 — typestate runtime (precedent · same pattern at runtime level)
- ADR-040 — Cargo feature matrix (orchestrator features compose into Ready state)
- Perplexity 2026-05-12 synthesis · zero-cost newtype + type-state idioms

## Alternatives considered

### Alt A — Runtime enum state
Rejected · runtime check per method call · panic risk if API misused · violates
zero-unwrap discipline + zero-cost-abstraction principle.

### Alt B — Builder pattern returning `Result<NikaStore>`
Considered · less ergonomic than type-state · builder pattern is FOR construction not
LIFECYCLE. Type-state subsumes builder (build() = Building → Ready transition).

### Alt C — Newtype `ReadyNikaStore` separate struct
Rejected · code duplication · type-state via PhantomData is the canonical Rust idiom.

## Related
- ADR-006 — kernel ISP traits
- ADR-012 — typestate runtime (analogue at runtime level)
- ADR-014 — sealed kernel traits (state types are not sealed · per design · sub-typing not the concern)
- ADR-040 — Cargo features (compose into Ready state)

## Notes
Ships at W10 orchestrator admission · not earlier. Satellites (W3-W9) use plain
trait impls · type-state is orchestrator-only concern. Migration is greenfield ·
no existing code to refactor.
