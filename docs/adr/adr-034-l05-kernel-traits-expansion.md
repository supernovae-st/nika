---
id: ADR-034
title: "L0.5 kernel traits expansion: 6 new traits scoped to v0.81+ additions"
status: accepted
date: "2026-04-16"
phase: "Phase D -- Session S1-E (Wave 2)"
deciders: ["@ThibautMelen"]
tags: ["l0.5", "kernel", "traits", "isp", "sealing", "id-generator", "secrets", "metrics", "tracing", "events", "billing"]
affects_crates: ["nika-kernel", "nika-kernel-mock", "nika-error"]
affects_layers: ["L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-006", "ADR-007", "ADR-014", "ADR-016", "ADR-020", "ADR-033"]
requires: ["ADR-006", "ADR-014", "ADR-033"]
enables: []
amends: ["ADR-006"]
fci: ["pattern-1-kernel-traits-upfront"]
inv: ["INV-024"]
shadow_zones: []
nika_codes: []
timeline: "v0.81.0-alpha.1, Phase D / Session S1-C"
follow_ups:
  - "Land Sealed supertrait + apply to Provider, EventSink, BillingSink, SecretResolver (DONE Wave 3 — cargo-expand verified)"
  - "Add MemoryUpdate sub-trait + augment MemoryStore blanket (deferred to v0.95 Cortex)"
  - "Implement nika-secret-vault (L1) when production secrets land"
  - "Implement nika-observability-otel (L1) when OTel export is wired"
---

# ADR-034: L0.5 kernel traits expansion: 6 new traits scoped to v0.81+ additions

## Boundary statement (read first)

**This ADR does NOT supersede ADR-006.** It is scoped to the **6 NEW v0.81+
traits** listed below (`IdGenerator`, `SecretResolver`, `MetricsExporter`,
`TracerProvider`, `EventSink`, `BillingSink`) plus the planned
`MemoryUpdate` sub-trait. ADR-006 remains authoritative for the **v0.80
traits** (`Clock`, `Fs*`, `Http*`, `Shell*`, `BlobStore`, `Provider*`,
`Memory*`, `ToolExecutor`, `ContextCompressor`).

| Scope | Authoritative ADR |
|---|---|
| v0.80 traits: Clock, Fs*, Http*, Shell*, BlobStore, Provider*, Memory*, ToolExecutor, ContextCompressor | **ADR-006** |
| v0.81 stub traits: WasmPluginHost, Sandbox, ObservabilitySink | **ADR-020** (WASM/Sandbox), **(unified ObservabilitySink lives in S1-B; split planned at v0.100, see below)** |
| v0.81 cancellation primitive: CancelCtx (struct, not a trait) | **ADR-016** |
| v0.81+ NEW traits: IdGenerator, SecretResolver, MetricsExporter, TracerProvider, EventSink, BillingSink, MemoryUpdate | **ADR-034 (this)** |

ADR-034 frontmatter lists `amends: ["ADR-006"]` because the L0.5 trait
inventory grows; ADR-006's trait list itself is not changed.

## Context

The kernel today (`crates/nika-kernel/src/lib.rs:48-66`) declares 13
modules and ~28 public traits. ADR-006 froze that v0.80 list. New
sessions (Phase D Session 4 — HTTP API, DataSource, MCP lifecycle;
Phase E2 — TOML pricing) need additional injection points that v0.80
did not anticipate.

Existing patterns to mirror (verified):

- All async kernel traits use `#[trait_variant::make(XxxDyn: Send)]`
  for object-safe `Dyn` companions (e.g.,
  `crates/nika-kernel/src/io/clock.rs:27`,
  `crates/nika-kernel/src/ai/context.rs:48`,
  `crates/nika-kernel/src/ai/provider.rs:444`,
  `crates/nika-kernel/src/ai/memory.rs:268`).
- All public types are `#[non_exhaustive]` per ADR-007.
- All public structs have a `pub fn new()` constructor per INV-019.
- All async traits require `Send + Sync`.
- Mocks live in `crates/nika-kernel-mock/src/`. Pattern:
  `#[derive(Clone, Debug, Default)]`, `pub fn new() -> Self`,
  trait impl with no-op or deterministic behavior, inline tests for
  Send+Sync + behavior. See `NullMemoryStore`
  (`crates/nika-kernel-mock/src/memory.rs:14-41`),
  `NullContextCompressor` (`crates/nika-kernel-mock/src/compressor.rs`).

ADR-014 sealing policy applies per-trait. The v0.81 expansion needs an
explicit sealing decision for each new trait.

INV-024 ("EventKind emission from exactly 1 site per verb path") makes
the case for a typed `EventSink`: events are scattered across stdout
writes today; one trait makes the single emission site machine-checkable.

## Decision

**Add 6 new L0.5 traits to `nika-kernel` at v0.81 (S1-C).** All follow
the existing patterns. Each has a paired mock in `nika-kernel-mock`.

### 1. `IdGenerator` (sync, sealed = NO / **open**)

```rust
pub trait IdGenerator: Send + Sync {
    fn next_id(&self) -> Uuid; // UUIDv7 in production, sequential in tests
}
```

**Why:** ID generation today uses `Uuid::now_v7()` scattered through
the codebase. Tests that compare against goldens cannot use exact
matching. `IdGenerator` lets tests inject a `SequentialIdGenerator`
with deterministic output. **Sync** because ID gen is never I/O.

**Open** so users can plug in a custom monotonic generator (e.g.,
Snowflake-style for cross-process ordering).

Mock: `SequentialIdGenerator { next: AtomicU64 }` increments a counter,
formats as a deterministic UUID.

### 2. `SecretResolver` (async, sealed = YES)

```rust
#[trait_variant::make(SecretResolverDyn: Send)]
pub trait SecretResolver: Send + Sync + sealed::Sealed {
    async fn resolve(&self, key: &str) -> Result<Secret, SecretError>;
}

pub struct Secret(zeroize::Zeroizing<String>); // zeros on drop
pub struct SecretRef(String);                  // public reference; safe to log
```

**Why:** Workflow YAML uses `${env.API_KEY}` references. Today the engine
calls `std::env::var` directly — no abstraction means no vault backend,
no test in-memory store. `SecretResolver` is async because vault
backends (AWS, GCP, HashiCorp) are network-bound.

**Sealed** because secret leakage via custom impls is a Shield
exfiltration risk — this is a security boundary; community impls go
through pck protocol with audit.

`Secret` wraps `zeroize::Zeroizing<String>` so credentials zero on
drop. `SecretRef` is a separate type that carries only the *reference*
(safe to log).

Mock: `NullSecretResolver` returns `SecretError::NotFound` for everything.
A `MapSecretResolver { entries: BTreeMap<String, String> }` mock for
deterministic tests.

### 3. `MetricsExporter` (async, sealed = NO / **open**)

```rust
#[trait_variant::make(MetricsExporterDyn: Send)]
pub trait MetricsExporter: Send + Sync {
    async fn counter(&self, name: &str, value: u64, labels: &[(String, String)]);
    async fn gauge(&self, name: &str, value: f64, labels: &[(String, String)]);
    async fn histogram(&self, name: &str, value: f64, labels: &[(String, String)]);
}
```

**Why:** Splits from the unified `ObservabilitySink` (S1-B,
`crates/nika-kernel/src/observability.rs`). v0.95 keeps the unified
sink for development; v0.100 introduces dedicated metrics + tracing
traits because backends (Prometheus vs OTel vs StatsD) want different
shapes.

**Open** so users can plug in alternative metrics backends.

Mock: `NullMetricsExporter` (no-op), `RecordingMetricsExporter` (captures
calls in `Vec` for assertions).

### 4. `TracerProvider` (async, sealed = NO / **open**)

```rust
#[trait_variant::make(TracerProviderDyn: Send)]
pub trait TracerProvider: Send + Sync {
    async fn start_span(&self, name: &str, parent: Option<SpanId>) -> SpanId;
    async fn end_span(&self, span: SpanId);
    fn inject(&self, span: SpanId, carrier: &mut dyn TraceCarrier);
    fn extract(&self, carrier: &dyn TraceCarrier) -> Option<SpanId>;
}
```

**Why:** Splits from `ObservabilitySink`. OTel TracerProvider shape.
Inject/extract enables W3C trace propagation across HTTP boundaries.

**Open** so users can ship to alternative trace backends (Jaeger,
Tempo, Honeycomb).

Mock: `NullTracerProvider` (no-op, returns synthetic SpanIds).

### 5. `EventSink` (async, sealed = YES)

```rust
#[trait_variant::make(EventSinkDyn: Send)]
pub trait EventSink: Send + Sync + sealed::Sealed {
    async fn emit(&self, event: WorkflowEvent) -> Result<(), EventSinkError>;
    async fn flush(&self) -> Result<(), EventSinkError>;
}
```

**Why:** INV-024 mandates "EventKind emission from exactly 1 site per
verb path". `EventSink` is that single site. Today events are written
to stdout/file via direct calls — `EventSink` lets tests assert on
emitted events without parsing stdout.

**Sealed** because workflow event taxonomy is engine-controlled; users
should not invent new variants. New event sinks (OTLP, Kafka) ship via
pck protocol.

Mock: `NullEventSink` (drops events), `RecordingEventSink` (captures
events in a `parking_lot::Mutex<Vec<WorkflowEvent>>` for assertions).

### 6. `BillingSink` (async, sealed = YES)

```rust
#[trait_variant::make(BillingSinkDyn: Send)]
pub trait BillingSink: Send + Sync + sealed::Sealed {
    async fn record_cost(
        &self,
        task_id: TaskId,
        cost: Cost,
        provider_id: ProviderId,
        model_id: ModelId,
    ) -> Result<(), BillingSinkError>;

    async fn flush(&self) -> Result<(), BillingSinkError>;
}
```

**Why:** Cost tracking has fundamentally different reliability
requirements from telemetry — billing must not be lossy. Separate
trait so backends can implement write-ahead logging, exactly-once
semantics, or batch-flush durability without entangling with the
metrics path.

**Sealed** because billing is a contract surface (we are accountable
for cost reporting accuracy); custom impls would risk audit gaps.

Mock: `NullBillingSink` (no-op for tests that don't care),
`RecordingBillingSink` (captures + sums for assertions).

### 7. `MemoryUpdate` sub-trait + `MemoryStore` evolution (deferred)

The existing `MemoryStore` blanket
(`crates/nika-kernel/src/ai/memory.rs:289-290`) is
`MemoryRemember + MemoryRecall + MemoryForget`. Cortex (v0.95) needs
an `update()` operation (patch a memory frame in place). Plan:

```rust
pub trait MemoryUpdate: Send + Sync {
    async fn update(&self, id: MemoryId, patch: MemoryFramePatch)
        -> Result<(), MemoryError>;
}

// Default blanket impl: returns Err(MemoryError::Unavailable)
impl<T: ?Sized + Send + Sync> MemoryUpdate for T {
    default async fn update(&self, _: MemoryId, _: MemoryFramePatch)
        -> Result<(), MemoryError>
    {
        Err(MemoryError::Unavailable { reason: "update not implemented".into() })
    }
}

// Augmented blanket:
pub trait MemoryStore:
    MemoryRemember + MemoryRecall + MemoryForget + MemoryUpdate {}
```

**Deferred to v0.95 Cortex.** Specialization-style default impls are
not stable; we will land this at v0.95 with concrete pattern
(probably an extension trait, not specialization). The decision
*to add it* is locked here so consumers know the shape.

### Sealing pattern (lands in S1-C)

```rust
// crates/nika-kernel/src/sealed.rs
mod sealed {
    pub trait Sealed {}
}
```

S1-C applies `: sealed::Sealed` supertrait to: `Provider` (existing),
`EventSink` (new), `BillingSink` (new), `SecretResolver` (new). Other
traits stay open.

## Consequences

### Positive

- INV-024 ("single emission site per verb path") becomes
  machine-checkable — only `EventSink::emit` writes events; everything
  else is a syntax error.
- Tests can inject deterministic `IdGenerator`, `SequentialClock`,
  `MapSecretResolver` — golden tests use exact comparison.
- Billing path is structurally separated from telemetry — no risk of
  sampling losing cost data.
- Sealing on `Provider`, `EventSink`, `BillingSink`, `SecretResolver`
  closes four sensitive contract surfaces; `MetricsExporter`,
  `TracerProvider`, `IdGenerator` stay open for community backends.
- Mock crate keeps 1:1 trait coverage — no "trait without mock" gaps.

### Negative

- 6 new traits + 6 new mocks = ~600 LOC added to L0.5. Mitigated
  because most are small (mocks are 30-50 LOC each, traits are
  10-30 LOC each).
- Sealing pattern adds a `sealed.rs` module; new contributors need to
  understand why they cannot impl `Provider` (answer: pck protocol).
  Documented in module-level comment.
- `Secret` requires `zeroize` dep on `nika-kernel`. Lightweight, but
  one more dep. Acceptable — `zeroize` has zero transitive deps.
- `MemoryUpdate` deferred to v0.95 means today's `MemoryStore`
  contract is **not** complete. Acceptable — `MemoryStore` is itself
  a kernel hook for v0.95, not exposed to users yet.

### Neutral

- ObservabilitySink (S1-B) stays as the v0.95 unified path.
  `MetricsExporter` + `TracerProvider` are the v0.100 split. Both
  coexist; users opt into the split when v0.100 ships.
- Sync vs async choice per trait follows the existing kernel rule
  (sync where I/O-free, async where backends are network-bound).

## Evidence / Affected code

- `crates/nika-kernel/src/lib.rs:48-66` — current module list (S1-B
  state: 17 modules).
- `crates/nika-kernel/src/io/clock.rs:27` — `trait_variant::make` pattern.
- `crates/nika-kernel/src/ai/context.rs:48` — `trait_variant::make`
  pattern.
- `crates/nika-kernel/src/ai/provider.rs:444-454` — `Provider` trait
  family (will gain `: sealed::Sealed` in S1-C).
- `crates/nika-kernel/src/ai/memory.rs:289-290` — current `MemoryStore`
  blanket (will gain `MemoryUpdate` at v0.95).
- `crates/nika-kernel/src/observability.rs` — unified
  `ObservabilitySink` (S1-B). v0.100 split target.
- `crates/nika-kernel-mock/src/lib.rs` — mock module list (mirrors
  every kernel trait).
- ADR-006 — v0.80 trait set, **not superseded**.
- ADR-014 — sealing policy applied per-trait above.
- ADR-033 — L0 types consumed by these traits (`Cost`, `Uuid`-backed
  IDs).
- INV-024 — "single emission site per verb path" (motivates `EventSink`).
- INV-019 — `pub fn new()` constructor on every public struct.

## Alternatives considered

### Alt A -- No `IdGenerator`, use `Uuid::now_v7()` directly
"Don't over-abstract."
Rejected: every golden-file test that includes IDs would need regex
matching instead of exact comparison. Determinism is worth the trait.

### Alt B -- Single `TelemetrySink` trait covering metrics, traces, billing
"Unify the observability surface."
Rejected: billing has different reliability contracts (durable, no
sampling). Traces and metrics tolerate sampling and best-effort
delivery. One trait would either over-promise on traces or
under-promise on billing. Three traits = three contracts, each met.

### Alt C -- `EventSink` as part of `ObservabilitySink`
"Events are observability."
Rejected: `EventSink` operates at the workflow level (task started,
task completed, tool invoked). `ObservabilitySink` operates at the
infrastructure level (spans, metrics). Different abstraction
layers; combining them blurs the contract.

### Alt D -- `MemoryUpdate` as separate trait, not added to `MemoryStore`
"Don't change the blanket."
Considered. Rejected: every `MemoryStore` consumer would need a
second trait bound (`T: MemoryStore + MemoryUpdate`). Adding it to the
blanket with a default `Err(Unavailable)` impl is backward-compatible
(existing `MemoryStore` impls continue to compile, get the default
behavior). We accept the small specialization complexity at v0.95.

### Alt E -- Open all 6 traits (no sealing)
"Trust the community."
Rejected: `Provider`, `EventSink`, `BillingSink`, `SecretResolver`
each carry security or contract risk. Sealing forces structured pck
plugin protocol with audit; open traits would let any crate impl
them with no review.

## Related

- ADR-006 — kernel ISP trait design. ADR-034 **amends** ADR-006 by
  adding 6 new traits to the L0.5 inventory; ADR-006's trait list is
  unchanged.
- ADR-007 — forward-compat invariants. Pattern 1 (kernel traits
  upfront) is the rationale for landing these at v0.81.
- ADR-014 — sealed kernel traits. Per-trait sealing decisions made
  here.
- ADR-016 — Cancellation. Future `Cancellable` + `CancelPropagator`
  v0.95 traits will live alongside these.
- ADR-020 — WASM plugin boundary. ObservabilitySink (S1-B) is the
  unified predecessor that splits into `MetricsExporter` +
  `TracerProvider` at v0.100.
- ADR-033 — L0 foundational types. `IdGenerator` returns `Uuid`,
  `BillingSink` takes `Cost` + `TaskId` + `ProviderId` + `ModelId`,
  all from `nika-error`.

## Notes

Follow-ups:

- S1-C lands the 6 traits + 6 mocks + `sealed.rs` + applies sealing
  to `Provider`, `EventSink`, `BillingSink`, `SecretResolver`.
- v0.95 Cortex lands `MemoryUpdate` sub-trait + augments `MemoryStore`
  blanket.
- v0.100 splits `ObservabilitySink` into `MetricsExporter` +
  `TracerProvider` (both already defined here).
- Implement `nika-secret-vault` (L1) when production secrets land.
- Implement `nika-observability-otel` (L1) when OTel export is wired
  end-to-end.
