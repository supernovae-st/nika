---
id: ADR-033
title: "L0 foundational types: Cost, UUIDv7, TrustLevel, TokenUsage evolution"
status: accepted
date: "2026-04-16"
phase: "Phase D -- Session S1-E (Wave 2)"
deciders: ["@ThibautMelen"]
tags: ["l0", "types", "foundational", "cost", "uuid", "trust-level", "token-usage"]
affects_crates: ["nika-error", "nika-kernel"]
affects_layers: ["L0", "L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-005", "ADR-007", "ADR-019", "ADR-034"]
requires: ["ADR-005", "ADR-007"]
enables: ["ADR-034"]
amends: []
fci: ["pattern-2-non-exhaustive"]
inv: ["INV-019", "INV-025"]
shadow_zones: []
nika_codes: ["NIKA-380", "NIKA-381", "NIKA-389"]
timeline: "v0.81.0-alpha.1, Phase C / Session S1-C"
follow_ups:
  - "Migrate MemoryId from u128 to UUIDv7-backed newtype (DONE Wave 3)"
  - "Migrate TaskId from String to UUIDv7-backed newtype (DEFERRED to v0.82+ — user-named YAML workflow IDs need a design pass on parse validation vs. human readability)"
  - "Replace InferResponse.cost_usd: Option<f64> with Cost(i128) (DONE Wave 3 — cost: Option<Cost> added, #[deprecated] on cost_usd, removal in v0.85)"
  - "Extend cost deprecation to AgentOutcome + AgentCheckpoint (DONE Wave 3)"
  - "Wire TrustLevel into Shield (NIKA-380..389) when verb-* crates ship"
---

# ADR-033: L0 foundational types: Cost, UUIDv7, TrustLevel, TokenUsage evolution

## Context

L0 today contains exactly two crates: `nika-error` (error codes,
NIKA-XXX ranges) and `nika-catalog` (provider/model catalog data).
Neither holds the foundational *value* types — Cost, IDs, TrustLevel —
that L1 effect crates and L2 verb crates will need.

The kernel currently leaks several types that should live one layer
below it.

**TokenUsage** (`crates/nika-kernel/src/provider.rs:247-256`) has 4 fields:

```rust
#[non_exhaustive]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}
```

This is missing several axes that real providers already bill for:

- `reasoning_tokens` — Anthropic extended thinking, OpenAI o3.
- `thinking_tokens` — separate from reasoning; visible thinking trace.
- `cache_creation_tokens` — Anthropic prompt caching write side.
- `image_tokens_in` / `image_tokens_out` — vision input + image output.
- `audio_tokens_in` / `audio_tokens_out` — Gemini Live, GPT-4o audio.
- `video_tokens` — Gemini 1.5 Pro video.
- `tool_use_tokens` — some providers bill tool definitions separately.

**MemoryId** (`crates/nika-kernel/src/memory.rs:21-22`) is `MemoryId(u128)`
with `Display` as `mem-{hex32}`. The `u128` is opaque random; not
time-sortable. Memory queries that want chronological ordering need
a separate timestamp.

**TaskId** (`crates/nika-kernel/src/types.rs`) is `TaskId(String)` —
a bare string wrapper, no validation, no time ordering, no parse
guarantee.

**Cost** is currently `cost_usd: Option<f64>` on `InferResponse`
(`crates/nika-kernel/src/provider.rs:307`). Floating-point accumulation
across thousands of API calls drifts; we cannot ship billing on f64.

**TrustLevel** is referenced by Shield error codes
(NIKA-380..389: `nika-error/src/codes.rs`) but currently passed as
hardcoded strings in workflow engine code. No enum exists in any
admitted crate.

L1 effect crates and L2 verb crates need a shared vocabulary for these
values. Putting them in `nika-kernel` (L0.5) creates an upward
dependency from L0 catalog → L0.5 kernel just to share an `IdGenerator`
target type. Putting them in L0 `nika-error` is the only stratum that
all higher layers already depend on.

## Decision

**Add 23 foundational value types to `nika-error` (L0). All higher
layers re-export or import directly. `nika-error` keeps its dependency
budget: std + serde + thiserror + miette only.**

The 23 types group into 9 modules:

| Module | Types | Why |
|---|---|---|
| `cost` | `Cost(i128)` | nano-USD; arithmetic; Display `$X.XXXXXX`; no float drift |
| `id` | 9 newtyped UUIDv7 IDs: `RunId`, `EventId`, `TraceId`, `SpanId`, `CorrelationId`, `WorkflowId`, `TenantId`, `ProviderId`, `ModelId` | type-safe ID separation; chronological ordering |
| `trust` | `TrustLevel(u8)` lattice + `meet` / `join` / `is_at_least` | Shield gating becomes a compile-time invariant |
| `retry` | `RetryConfig`, `ErrorCategory` | shared vocabulary for ADR-019's stratified retry |
| `budget` | `BudgetDirective` enum | per-task budget enforcement (cost, tokens, time) |
| `schema` | `EventSchemaVersion`, `TraceFormatVersion` | wire-compat versioning for events + traces |
| `hash` | `Blake3Hash`, `ContentDigest`, `BlobRef` | content-addressed storage primitive |
| `baggage` | W3C Baggage (max 64 entries, 8 KB cap) | OTel propagation across spans |
| `resource` | OTel `Resource { service_name, attrs }` | OTel resource attributes, span enrichment |

**Migration of existing kernel types** (lands in S1-C):

- `MemoryId(u128)` → `MemoryId(Uuid)` UUIDv7-backed. `Display` becomes
  `mem-{uuid}`. Existing serde format moves to UUIDv7 string. **Pre-S1-C
  callers will need to be re-codegened** (mock IDs in tests included).
- `TaskId(String)` → `TaskId(Uuid)` UUIDv7-backed. Validation on parse.
- `InferResponse.cost_usd: Option<f64>` → `cost: Option<Cost>`.
- `TokenUsage` 4-field → 18-field expansion (reasoning, thinking,
  cache_creation, image*, audio*, video, tool_use, ...). All new fields
  `Option<u64>` so producers opt in. `#[non_exhaustive]` makes the
  expansion non-breaking.

**Cost arithmetic** (concrete shape):

```rust
#[non_exhaustive]
pub struct Cost(i128); // nano-USD: 1 USD = 1_000_000_000

impl Cost {
    pub const ZERO: Cost;
    pub fn from_usd(usd: f64) -> Cost; // saturating
    pub fn from_nano_usd(n: i128) -> Cost;
    pub fn as_nano_usd(&self) -> i128;
    pub fn as_usd(&self) -> f64; // lossy display
}

impl Add for Cost { ... }
impl Sub for Cost { ... }
impl Mul<u64> for Cost { ... }
impl Display for Cost { /* "$X.XXXXXX" */ }
```

i128 gives a range of ±1.7e29 nano-USD = ±170 quintillion USD. Enough
for any conceivable aggregation horizon.

**TrustLevel lattice** (concrete shape):

```rust
#[non_exhaustive]
pub struct TrustLevel(u8); // Untrusted=0, LocalTrusted=128, Trusted=255

impl TrustLevel {
    pub const UNTRUSTED: Self;
    pub const LOCAL_TRUSTED: Self;
    pub const TRUSTED: Self;
    pub fn meet(self, other: Self) -> Self; // min
    pub fn join(self, other: Self) -> Self; // max
    pub fn is_at_least(self, threshold: Self) -> bool;
}

impl Display for TrustLevel { ... }
impl FromStr for TrustLevel { ... }
impl Serialize / Deserialize for TrustLevel { ... }
```

Lattice ops let Shield gate "this tool requires `Trusted` data" with a
compile-time-checkable contract.

**UUIDv7** is chosen over UUIDv4 because:

1. Time-sortable: natural chronological ordering for memory queries,
   event logs, task lookup.
2. Random suffix preserves uniqueness at high creation rates.
3. Wire-compatible with `uuid::Uuid` ecosystem.

`nika-error` adds `uuid = { version = "1", features = ["v7", "serde"] }`
to its dep list (S1-C lands the dep).

## Consequences

### Positive

- Single L0 vocabulary across all higher layers — no more "what's a
  TrustLevel" ambiguity.
- Cost arithmetic moves from f64 (drift) to i128 (exact) — billing
  becomes auditable.
- UUIDv7 IDs give natural chronological ordering for memory queries
  and event logs without a parallel timestamp field.
- TokenUsage expansion (4 → 18 fields) covers every billable axis
  current providers expose. New axes land additively under
  `#[non_exhaustive]`.
- TrustLevel becomes a typed value with lattice ops; Shield can express
  "tool X requires `at_least(LocalTrusted)`" as code, not docs.
- `RetryConfig` + `ErrorCategory` give ADR-019's three retry layers a
  shared vocabulary.

### Negative

- 23 new types is a bigger admit than typical for L0. Mitigated by
  topological ordering in S1-C (hash → schema → id → baggage → trust →
  retry → budget → cost → resource).
- Migrating `MemoryId` and `TaskId` to UUIDv7 changes their wire
  format (existing snapshots in tests must be refreshed). Pre-emption:
  no production data exists yet (Diamond is orphan-branch, no users).
- `nika-error` gains a `uuid` dependency. Acceptable — `uuid` is
  std-only with `serde` + `v7` features; no transitive runtime.
- TokenUsage 18-field struct is wider; serialization size grows.
  Mitigated: all new fields are `Option<u64>` and producers omit them
  by default.

### Neutral

- `nika-error` already crosses 184 tests (largest crate by test
  count); adding ~70-90 more keeps it under reasonable test budget.
- Some types (RetryConfig, BudgetDirective) live in `nika-error`
  even though "error" is not literally what they describe. This is
  consistent with `nika-error` being the de-facto "L0 foundation"
  crate. We may rename it to `nika-foundation` at v0.82+ if friction
  appears.

## Evidence / Affected code

- `crates/nika-kernel/src/provider.rs:247-256` — current 4-field
  TokenUsage.
- `crates/nika-kernel/src/provider.rs:307` — current
  `cost_usd: Option<f64>`.
- `crates/nika-kernel/src/memory.rs:21-22` — current `MemoryId(u128)`.
- `crates/nika-kernel/src/types.rs` — current `TaskId(String)`.
- `crates/nika-error/src/codes.rs` — Shield NIKA-380..389 (TrustLevel
  consumers).
- `docs/architecture/forward-compat-invariants.md` — Pattern 2
  (`#[non_exhaustive]`), INV-019 (per-crate `new()` ctor), INV-025
  (verb error enums `#[non_exhaustive]`).
- ADR-019 — names `RetryConfig` + `ErrorCategory` as the shared
  retry vocabulary that lands here.
- ADR-034 — L0.5 traits expansion that depends on these L0 types
  (IdGenerator returns `Uuid`, BillingSink takes `Cost`).

## Alternatives considered

### Alt A -- Keep `cost_usd` as `f64`
"Floats are simpler."
Rejected: f64 accumulation across thousands of provider calls drifts
in the 7th decimal. Auditable billing is not negotiable. i128
nano-USD is exact and trivially comparable.

### Alt B -- UUIDv4 (random only)
"Standard, well-supported."
Rejected: not time-sortable. Memory queries that want
"recent first" need a separate timestamp; UUIDv7 collapses both into
one column. UUIDv7 is also already an RFC (9562, 2024-05).

### Alt C -- Types stay in `nika-kernel`
"Don't add a new home."
Rejected: L1 crates (nika-http, nika-fs) need `Cost` and `TrustLevel`
but must not depend on L0.5 kernel (per ADR-006 layering and
`scripts/ci/check-layering.sh`). L0 `nika-error` is the only
common ancestor.

### Alt D -- Separate `nika-types` or `nika-foundation` crate
"Logically distinct from errors."
Rejected: would proliferate L0 crates without functional gain.
`nika-error` already has zero deps beyond std + serde + thiserror;
adding types keeps the dep graph flat. The name is suboptimal but
not load-bearing — we may rename later.

### Alt E -- Decimal floats (e.g., `decimal_rs::Decimal`)
"Exact arithmetic without going to integer."
Rejected: brings a non-trivial third-party dep into L0 (the strictest
layer). i128 gives the same exactness with zero deps.

## Related

- ADR-005 — error hierarchy. ADR-033 lives in the same crate
  (`nika-error`) but adds value types alongside the existing error
  codes.
- ADR-007 — forward-compat invariants. Pattern 2
  (`#[non_exhaustive]`) is applied to every new struct/enum.
- ADR-019 — retry + timeout ownership. ADR-019 named
  `RetryConfig` + `ErrorCategory` as the shared vocabulary; ADR-033
  is where they actually live.
- ADR-034 — L0.5 kernel traits expansion. ADR-034's traits
  (IdGenerator, BillingSink, ...) consume ADR-033's types.

## Notes

Follow-ups (S1-C and beyond):

- Migrate `MemoryId(u128)` → `MemoryId(Uuid)` UUIDv7-backed (S1-C).
- Migrate `TaskId(String)` → `TaskId(Uuid)` UUIDv7-backed (S1-C).
- Replace `InferResponse.cost_usd: Option<f64>` with
  `cost: Option<Cost>` (S1-C).
- Wire `TrustLevel` into Shield gating when verb-* crates ship.
- Consider renaming `nika-error` → `nika-foundation` at v0.82+ if
  the "errors crate that holds value types" framing causes friction.
