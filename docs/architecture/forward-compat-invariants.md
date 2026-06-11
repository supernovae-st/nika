# Forward-Compatibility Invariants

**Status**: LOCKED at v0.80.0. Every crate admitted to the Diamond workspace (`main` · ex `nika-diamond` renamed 2026-05) must
comply with these invariants before passing Gate 12.

## Why this document exists

Nika is forever-v0.x. Features ship incrementally over many years. The
architecture shipped in v0.90 must accommodate v0.95 (Cortex + agent-v2),
v0.100 (WASM plugins + observability), and features we haven't imagined yet
**without breaking changes to the public API**.

This document codifies the patterns that make additive change safe and
catches breaking change before it merges.

Distilled from: Rust (2015-present), Tokio (0.1 → 1.x), Deno (2018-present),
Cargo, Serde (9+ years stable), Axum/Tower, Bevy.

## The 8 patterns (ranked by ROI)

### 1. Kernel traits upfront, implementations deferred <!-- FCI-001 -->

**Pattern**: Define traits in `nika-kernel` at v0.90 for subsystems that
ship later. Provide default method implementations that return
`Err(Unsupported)` until real implementations land.

**Example**:
```rust
// nika-kernel/src/memory.rs (ships v0.90, impl lands v0.95)
#[trait_variant::make(MemoryStoreDyn: Send)]
pub trait MemoryStore: Send + Sync + 'static {
    async fn put(&self, frame: MemoryFrame) -> Result<MemoryFrameRef>;
    async fn query(&self, q: &MemoryQuery) -> Result<Vec<MemoryFrame>>;
    async fn forget(&self, _r: &MemoryFrameRef) -> Result<()> {
        Err(NikaError::Unsupported("forget"))
    }
}
```

**Decision A1**: `trait_variant` (not `async_trait`). Zero boxing on static dispatch;
`MemoryStoreDyn` companion gives object-safety for dynamic dispatch when needed.

**Impact**: v0.95 ships Cortex by adding `nika-memory-oxigraph` crate that
implements `MemoryStore`. Zero v0.90 code modification.

**Locked traits (v0.90)**:
- `MemoryStore` — Cortex impl v0.95
- `EmbeddingProvider` — Cortex impl v0.95
- `ToolExecutor` — real impl v0.90 (used by natives)
- `WasmPluginHost` — impl v0.100+
- `MetricsExporter` + `TracerProvider` — impl v0.100+ (`nika-observability-otel`)
- `Sandbox` — impl v0.100+ (Landlock/seccomp)

**Stub traits land in kernel v0.81.** `WasmPluginHost` and `Sandbox` are
defined as kernel traits — with their DTOs marked `#[non_exhaustive]` — in
the v0.81 kernel files `src/plugin/wasm.rs` and `src/plugin/sandbox.rs`.
Telemetry is split into the sibling traits `MetricsExporter` + `TracerProvider`
+ `AuditSink` + `EventSink` + `BillingSink` (the unified `ObservabilitySink`
stub was dropped per Q12 rev.3 2026-04-16; the 4-channel split is already
in-tree at `src/infra/{audit,metrics,trace,event_sink,billing}.rs`). Real
implementations land in v0.95+ / v0.100+ crates (`nika-wasm-host`,
`nika-sandbox-*`, `nika-observability-otel`). Reserving the trait shape and
field layout **now** keeps v0.95/v0.100 strictly additive — no consumer of
v0.8x breaks when the impls arrive. See §9 below.

### 2. `#[non_exhaustive]` on every public struct, enum, error variant <!-- FCI-002 -->

**Pattern**: Every public type is marked `#[non_exhaustive]`. External code
can never construct via `MyStruct { a, b }` literal — must use
`MyStruct::new(...)` or builder.

**Impact**: Adding a field (e.g., `pub memory: Option<MemoryDirective>` to
`InferRequest` in v0.95) is NOT a breaking change.

**Rule**: If you forget `#[non_exhaustive]` on a public type, CI fails via
`cargo public-api --diff-git-checkouts`.

### 3. Versioned file formats with explicit `schema:` field <!-- FCI-003 -->

**Pattern**: Every file format (workflows, pck manifests, event logs,
memory frames) carries a `schema:` version field. Parser dispatches by
version.

**Example**:
```rust
#[derive(Deserialize)]
#[serde(tag = "nika")]               // single version marker · envelope `nika: v1`
pub enum WorkflowDoc {
    #[serde(rename = "v1")]
    V1(WorkflowV1),
    // v2 reserved · forever-v0.x makes a contract break effectively never
}
```

**Locked schemas (v0.80)**:
- `nika: v1` — current workflow envelope (single version marker · per nika-spec · supersedes the K8s `apiVersion:` + `schema: nika/workflow@X` forms)
- `nika/pck@1` — pck manifest TOML
- `nika/event@1` — event log JSON
- `nika/memory-frame@1` — memory frame JSON (v0.95)

**Migration tooling** ships parallel to schema bumps. No silent migrations.

### 4. Extension namespaces (`nika.*` core + `x-*` community) <!-- FCI-004 -->

**Pattern**: Every registry (pck types, EventKind variants, MCP aliases,
error codes, builtin names) uses explicit namespaces. Core Nika reserves
`nika.*`. The `x-*` community prefix applies to the **pck registry only** —
the workflow-language TOOL namespace set is CLOSED at v1 (`nika:` + `mcp:`
per spec 02-verbs · engine-specific tools route through `mcp:` · the
spec reserves `x-` as a possible future additive minor).

**Locked namespaces**:
- **pck types**: `workflow` / `skill` / `agent` / `provider` / `mcp` /
  `eval` / `recipe` / `shield` / `lints` (9 core) + `x-*` community
- **builtins**: `nika:tool` core + `mcp:server/tool` MCP-namespaced
- **EventKind**: core variants + `Extension { ns, name, payload }` escape hatch
- **Skill targets**: `claude-code`, `cursor`, `windsurf`, `zed` (4 IDEs) +
  future additions

### 5. Reserved error code ranges <!-- FCI-005 -->

**Pattern**: Error codes are assigned in ranges by subsystem. Ranges are
reserved even if unused, preventing collision when future subsystems ship.

**Locked ranges**:

| Range | Subsystem | Status |
|---|---|---|
| NIKA-000..099 | Core engine | active |
| NIKA-100..199 | MCP / catalog | active (100, 101, 107) |
| NIKA-1600..1699 | audio L1 (stt · tts · vad · `ai::audio` seam R6) | reserved (2026-06-10 · impls stdlib v0.x) |
| NIKA-200..299 | pck / validation | reserved v0.80, active v0.90 |
| NIKA-300..399 | Providers + Shield | active (380-389 shield) |
| NIKA-400..499 | Cortex / memory queries | reserved v0.80, active v0.95 |
| NIKA-500..599 | agent-v2 | reserved v0.80, active v0.95 |
| NIKA-600..649 | Memory subsystem | reserved v0.80 (Category::Memory) |
| NIKA-650..699 | (unallocated) | reserved |
| NIKA-700..749 | WASM plugin host | reserved v0.80 (Category::WasmPlugin) |
| NIKA-750..799 | Sandbox / capabilities | reserved v0.80 (Category::Sandbox) |
| NIKA-800..819 | Observability / telemetry | reserved v0.80 (Category::Observability) |
| NIKA-820..899 | (unallocated) | reserved |
| NIKA-900..999 | Community / x-* extensions | reserved v0.80 |

### 6. Sealed traits for core, open traits for extension <!-- FCI-006 -->

**Pattern**: Traits the Nika team wants to evolve privately (adding methods,
changing internals) are sealed via private supertrait pattern. Traits that
community implements are open.

**Example**:
```rust
mod sealed { pub trait Sealed {} }

// Sealed: only Nika crates implement this
pub trait Verb: sealed::Sealed + Send + Sync {
    fn execute(&self, ctx: &Ctx) -> BoxFuture<Result<Output>>;
    fn dry_run(&self, _ctx: &Ctx) -> BoxFuture<Result<Plan>> {
        Box::pin(async { Err(NikaError::Unsupported("dry_run")) })
    }
}

// Open: community implements this
pub trait MemoryStore: Send + Sync { ... }
```

**Locked sealing policy**:
- **Sealed**: `Verb`, `Provider`, `Runtime`, `EventSink`, `BillingSink`,
  `SecretResolver` — core engine only
- **Open**: `MemoryStore`, `EmbeddingProvider`, `ToolExecutor`, `Sandbox`,
  `MetricsExporter`, `TracerProvider`, `WasmPluginHost` — community may implement

### 7. Feature flags with stable defaults <!-- FCI-007 -->

**Pattern**: Experimental subsystems live behind `unstable-*` feature
flags. Default features are stable. CI runs both matrices.

**Locked features**:
```toml
[features]
default = ["std"]
unstable-cortex     = ["dep:nika-memory-oxigraph"]
unstable-agent-v2   = ["dep:nika-agent-v2", "unstable-cortex"]
unstable-wasm       = ["dep:nika-wasm-host"]
unstable-observ     = ["dep:nika-observability"]
unstable-sandbox    = ["dep:nika-sandbox"]
```

**Rule**: an `unstable-*` feature can change API in minor versions. Stable
features follow strict semver.

### 8. Public API discipline via CI <!-- FCI-008 -->

**Pattern**: Every PR runs `cargo public-api --diff-git-checkouts` and
`cargo semver-checks check-release` to catch accidental breaking changes
before merge.

**CI gates**:
- `cargo public-api` — alerts on any public API surface change
- `cargo semver-checks` — detects breaking changes (SemVer-checked)
- `cargo deny check` — license + advisories + workspace layer rules
- `cargo machete` — unused dependencies

## The 5 architectural decisions locked at v0.80

If these are wrong, we pay the breaking-change tax for the life of the
project.

### Decision 1 — EventKind extensibility model <!-- FCI-009 -->

```rust
#[non_exhaustive]
pub enum EventKind {
    // Core (v0.80+)
    Started, Completed, Failed,
    TaskStarted, TaskCompleted, TaskFailed,
    // Reserved v0.95 Cortex
    MemoryHit, MemoryWrite, MemoryForget,
    // Reserved v0.95 agent-v2
    AgentPlanned, AgentReflected, AgentResumed,
    // Reserved v0.100 observability
    Trace { span_id: SpanId, kind: &'static str },
    // Extension escape hatch (community + future)
    Extension { ns: String, name: String, payload: serde_json::Value },
}
```

**Rationale**: EventLog is consumed by observability, replay, audit, cost
tracking. A closed enum would force every new subsystem into a breaking
change.

### Decision 2 — `InferRequest` / `InferResponse` field reservation <!-- FCI-010 -->

```rust
#[non_exhaustive]
pub struct InferRequest {
    pub model: ModelId,
    pub messages: Vec<Message>,
    pub memory: Option<MemoryDirective>,   // v0.95
    pub budget: Option<BudgetDirective>,   // v0.100
    pub replay_seed: Option<u64>,          // v0.100
    pub agent_state: Option<AgentStateRef>,// v0.95
}

impl InferRequest {
    pub fn new(model: ModelId, messages: Vec<Message>) -> Self { ... }
    pub fn builder() -> InferRequestBuilder { ... }
}
```

**Rationale**: `InferRequest` flows through every provider, every verb,
every memory call. Adding fields in v0.95 requires them to be `Option` with
`None` defaults today.

### Decision 3 — `schema:` field mandatory day-1 <!-- FCI-011 -->

Every file format ships with `schema:` from v0.80. Fail-fast on missing
schema. Never parse unversioned files.

**Rationale**: Cargo's `edition = "2018"` pattern. Without this, v1.0 can't
introduce a schema version without parsing every existing file as legacy.

### Decision 4 — Provider trait shape <!-- FCI-012 -->

```rust
#[trait_variant::make(Send)]
pub trait Provider {
    async fn infer(
        &self,
        request: InferRequest,
    ) -> Result<InferResponse, ProviderError>;
}
```

- Owned request + response (not borrowed) — caller doesn't need lifetime
  management
- Never expose `rig::Message` or other third-party types in public API
- `trait_variant` generates sync + Send + !Send variants
- `ProviderError` is `#[non_exhaustive]` + `#[from]` chains

**Rationale**: the provider set grows release over release; the trait
shape locks all downstream.

### Decision 5 — `CatalogEntry` shape with Cortex-ready fields <!-- FCI-013 -->

```rust
#[non_exhaustive]
pub struct CatalogEntry {
    pub id: EntryId,
    pub kind: EntryKind,
    pub embedding: Option<Vec<f32>>,   // v0.95 Cortex semantic search
    pub cost: Option<CostEstimate>,    // v0.100 budget enforcement
    pub perms: Vec<Permission>,        // v0.100 capability model
    pub last_verified: Option<&'static str>,  // xtask verifier
}
```

**Rationale**: Catalog is queried by Cortex (semantic search), agent-v2
(tool selection), CLI (autocomplete), observability (cost tracking). If
these fields aren't reserved, every consumer rewrites in v0.95/v0.100.

## 10 concrete API design rules <!-- FCI-014..023 -->

1. **Never `Vec<T>` returns** <!-- FCI-014 --> → `impl Iterator<Item = T>` or `impl
   IntoIterator<Item = T>`. Locks allocation strategy.
2. **Never expose third-party types in public API**. <!-- FCI-015 --> Wrap:
   `pub struct Message { inner: rig::Message }`. Tokio learned this
   with Mio in 0.1 → 0.2.
3. **Public fields require `#[non_exhaustive]`** <!-- FCI-016 --> — a pub field on a
   struct that lacks `#[non_exhaustive]` lets external code literal-construct it,
   locking the field set forever (adding a field becomes breaking). Prefer
   accessors/builder for fields whose *type* may change; pub fields are acceptable
   on `#[non_exhaustive]` DTOs, which is what the locked Decisions FCI-010
   (`InferRequest`) and FCI-013 (`CatalogEntry`) do.
   *Architecture review 2026-06-10*: the original wording ("never public fields,
   even on `#[non_exhaustive]`") contradicted those higher-authority Decisions and
   the ~579 pub fields in tree, so it is reconciled to the Decisions. Enforcement is
   structural, not social: `cargo public-api` (FCI-008) emits the `#[non_exhaustive]`
   marker and every pub field+type, so adding a field or dropping the marker shows as
   a surface diff. That diff now covers 15 admitted lib crates (the coverage floor
   `scripts/ci/public-api-coverage-baseline.txt`, projected into public-api.yml +
   semver-checks.yml + hygiene vector 38, ADR-090). Known non-`#[non_exhaustive]`
   value types carried as a deliberate exception: `NikaCode` (a `Copy` registry value
   const-constructed workspace-wide — adding `#[non_exhaustive]` would forbid the
   cross-crate `const NIKA_xxx = NikaCode { … }` literals) and `KeyMods`. Their
   surfaces are locked by the floor, so a field addition is a *tracked* break.
4. **Never closed numeric enums** <!-- FCI-017 --> — always `#[non_exhaustive]` +
   `#[serde(other)]` for the catch-all variant.
5. **Newtype every ID** <!-- FCI-018 --> — `ProviderId(SmolStr)`, `WorkflowName(String)`,
   `SkillName(String)`. Validation, normalization, interning become
   additive.
6. **All errors `#[non_exhaustive]` + `thiserror` + `#[from]` chains**. <!-- FCI-019 -->
   Never `String` error types in public API.
7. **Sealed traits + default methods** <!-- FCI-020 --> — `fn dry_run(&self) -> Result<...>
   { Err(Unsupported("dry_run")) }`. Adding methods later is additive.
8. **Opaque handles** <!-- FCI-021 --> — `pub struct Runtime { _inner: Arc<dyn RuntimeInner> }`.
   Scheduler rewrites don't break users.
9. **`schema:` field mandatory on every file format**. <!-- FCI-022 --> Fail-fast on missing.
10. **`#[must_use]` on every Result-returning API**. <!-- FCI-023 --> Silently dropped
    errors are forward-compat tax when tightening error reporting.

### Kernel effect-trait error-typing convention — ONE rule, no exceptions <!-- FCI-023bis -->

**DECIDED + LOCKED (2026-06-10 · scope widened to `ai/` 2026-06-11).** Every L0.5
kernel effect-trait — in BOTH `nika-kernel-core/src/io/*` AND
`nika-kernel-ai/src/*` — returns a **typed** error: `Result<T, XError>` where
`XError` is a `#[non_exhaustive]` thiserror enum living IN the trait's module, with
its `NikaErrorCode` impl + reserved NIKA range in the crate's `errors.rs`.
**Never `std::io::Result` at a kernel trait boundary.** A typed error carries the
NIKA taxonomy across the contract boundary so a caller (L2/L3) can match the code;
`io::Result` erases it — `io::Error` cannot carry a NIKA code, the "stringly-typed
of errors". There is no "which pattern?" question — there is one pattern.

This is **structurally enforced** (ADR-090): hygiene vector 40
(`check-kernel-io-typed-errors.sh`) reads each effect module in `io/` + `ai/` and
goes RED if a NEW trait returns `std::io::Result`. A type-state-generic trait (e.g.
`input`'s `InputDevice<S: ConsentState>`) is no exception — the error type is
orthogonal to the type-state, so it returns `Result<T, InputError>` just the same.

**Compliant today — the convention is UNIFORM (vector 40 GREEN · 12 typed
modules):** the WHOLE `io/` family — `blob`/`http`/`process`/`fs` (pure-effect)
+ `screen`/`ocr`/`a11y`/`input`/`browser` (computer-use, migrated 2026-06-10/11)
— and the WHOLE `ai/` family — `memory` + `provider` + `vision` (`VisionError` ·
NIKA-1501..1505) + `audio` (`AudioError` · NIKA-1601..1605 · one enum for the
stt/tts/vad capability family), typed 2026-06-11 while both still had ZERO
implementors (purely additive). `clock` is infallible (no error surface).
Vector 40's baseline (`scripts/ci/kernel-io-typed-error-baseline.txt`) holds
only comments; a NEW effect trait returning `std::io::Result` goes RED.

## 10 anti-patterns to avoid <!-- FCI-024..033 -->

1. Returning `Vec<T>` from public APIs. <!-- FCI-024 -->
2. Exposing third-party types (`pub fn foo() -> rig::Message`). <!-- FCI-025 -->
3. Public fields on a struct that LACKS `#[non_exhaustive]` (locks the field set —
   see FCI-016; pub fields on `#[non_exhaustive]` DTOs are fine). <!-- FCI-026 -->
4. Closed enums with literal numeric discriminants. <!-- FCI-027 -->
5. `pub use` of internal modules — use a curated `prelude` instead. <!-- FCI-028 -->
6. Hard-coded paths in error messages — use `Path::display()` + structured
   fields. <!-- FCI-029 -->
7. `String` types everywhere — always newtype IDs. <!-- FCI-030 -->
8. Trait methods without default impls — adding a method = breaking change. <!-- FCI-031 -->
9. `async fn` in public traits without `Send` bound — locks runtime choice. <!-- FCI-032 -->
10. Forgetting `#[must_use]` on Result-returning APIs. <!-- FCI-033 -->

## Per-subsystem forward-compat matrix

| Subsystem | Stable v0.90 surface | Extension points | Risk | Mitigation |
|---|---|---|---|---|
| Core engine | `Runtime::execute`, EventStream | EventKind Extension variant, new directives in InferRequest | LOW | EventKind `#[non_exhaustive]` |
| pck | `schema:` + 9 types | `x-*` namespace, new schema versions | MED | `deny_unknown_fields` top-level only |
| Natives | 4 verbs sealed · 22 spec-curated builtins | `ExternalVerb` registry for community | LOW | Sealed `Verb` + open `ExternalVerb` |
| Providers | 14 canonical (spec stdlib) via wire dialects + mock | New providers as crates, `ProviderId` as newtype with `Custom(String)` | HIGH | Crate split already locked |
| MCP | Catalog + aliases | New transports, MCP versions | MED | rmcp behind facade trait |
| Cortex | Trait stubs only | Real impl v0.95 | LOW | All hooks in kernel v0.90 |
| agent-v2 | Reserved EventKinds, error codes | New crate v0.95 | LOW | InferRequest fields + default methods |
| Shield | Permission enum stub | Landlock/seccomp impl v0.100 | MED | Define `Permission` enum now |
| Lints | nika-lints crate | New lints additive | LOW | Per-lint allow attribute namespace |
| Observability | EventLog only | Tracing, OTel, replay v0.100 | MED | `EventKind::Trace` reserved |

## 9. v0.95 / v0.100 reservation policy <!-- FCI-034 -->

Reserving trait shapes and DTO field layout **before** implementations exist
is the explicit mechanism that makes v0.95 and v0.100 additive-only releases.

**Policy**:

1. **Trait stubs land in kernel v0.81.** Every subsystem that v0.95 or v0.100
   will ship (Cortex, WASM plugins, observability, sandbox) has its trait
   signature, companion `*Dyn: Send` object-safe twin, and DTO types locked
   in `nika-kernel` at v0.81. Impls arrive later in their own crates.
2. **All DTOs are `#[non_exhaustive]`.** New fields become additive in later
   releases. Callers never construct via literal — builders or `::new` only.
3. **Reserved optional fields on load-bearing structs.** Types that flow
   through every verb and every provider (`InferRequest`, `MemoryFrame`,
   `CatalogEntry`) carry `Option<_>` fields for v0.95/v0.100 directives
   today. Default is `None`; v0.95 populates without breaking v0.8x.
4. **Cancellation is passed explicitly where future-drop is insufficient.**
   A `CancelCtx` module in the kernel re-exports `tokio_util::sync::CancellationToken`
   so trait methods whose futures are outlived by their outputs (streams,
   spawned compressors) take `cancel: CancelCtx` as a reserved parameter.
5. **Capability-scoped subsets accompany broad traits.** `WasmPluginHost` ships
   alongside narrower companion traits (`PluginFs`, `PluginHttp`, …) so
   v0.100 impls can wire capability-gated subsets without the community
   re-implementing the main trait.

**Planned kernel files (v0.81)**:

| File | Purpose | Impl crate (v0.95+ / v0.100+) |
|---|---|---|
| `src/cancel.rs` | `CancelCtx` re-export + `CancelDropGuard` | in-tree, zero-impl |
| `src/plugin/wasm.rs` | `WasmPluginHost` + `PluginCallContext` + `WasmPluginError` + capability-scoped subsets | `nika-wasm-host` (v0.100) |
| `src/plugin/sandbox.rs` | `Sandbox` trait + `SandboxPolicy` + `PluginCapabilities` | `nika-sandbox-linux`/`-macos`/`-windows` (v0.100) |
| `src/infra/{metrics,trace}.rs` | `MetricsExporter` + `TracerProvider` (the unified `ObservabilitySink` stub was dropped per Q12 rev.3) | `nika-observability-otel` (v0.100) |
| `src/infra/{audit,event_sink,billing}.rs` | `AuditSink` + `EventSink` + `BillingSink` | in-tree (v0.81+, sealed where contract-bearing) |

**Modifications to existing kernel files (v0.81)**:
- `provider.rs` — `ProviderStream::infer_stream` gains `cancel: CancelCtx`
- `context.rs` — `ContextCompressor::compress` gains `cancel: CancelCtx`
- `memory.rs` — `MemoryFrame` gains reserved `Option<_>` fields
  (`cipher`, `provenance`, `retention`, `redactions`), all `#[non_exhaustive]`,
  default `None` in v0.80
- `lib.rs` — `pub mod cancel; pub mod plugin; pub mod sandbox; pub mod observability;`

**Verification**: CI runs `cargo public-api --diff-git-checkouts` plus
`cargo semver-checks check-release` on every PR. Any change to a reserved
trait or DTO that is not strictly additive fails the gate.

**Rationale**: once v0.95 Cortex and v0.100 WASM plugins start landing,
shipping the trait surface is a mechanical crate addition rather than a
breaking-change negotiation across every consumer of v0.8x.

### Wave 4A / 4B reservations (Batch I.b, 2026-04-17) <!-- FCI-035 -->

Seven additive reservations shipped during Batch I.b lock v0.95 Cortex and
v0.100 observability shapes without breaking v0.8x consumers. Each landed as
a single atomic commit and is covered by `cargo public-api` +
`cargo semver-checks` at Gate 12.

| # | Surface | Crate / path | Target release | Commit |
|---|---|---|---|---|
| R1 | `EmbeddingSpec` value type (dim + provider + model + dtype + schema) | `nika-types/src/embedding.rs` | v0.95 Cortex | `001ae0b6f` |
| R2 | `MemoryFrameRef.trust: TrustLevel` reserved field | `nika-kernel` memory re-export (`nika-types`) | v0.95 Cortex | `41e8a1467` |
| R3 | `RecallQuery.tenant: Option<TenantId>` reserved field | `nika-kernel` memory re-export (`nika-types`) | v0.95 multi-tenant | `41e8a1467` |
| R4 | `WasmPluginError::{OutOfFuel, Trap{kind}}` variants + `PluginCallContext{cancel, caller_trust, plugin_trust}` | `nika-kernel/src/plugin/wasm.rs` | v0.100 WASM plugins | `368820e42` |
| R5 | `MemoryLifecycle` trait (`consolidate`, `prune` with `Unsupported` defaults) | `nika-kernel/src/ai/memory.rs` | v0.95 Cortex | `ac46b9ca5` |
| T1 | `SpanGuard.parent_span_id: Option<SpanId>` + `SpanRef{trace_id, span_id}` | `nika-kernel/src/infra/trace.rs` | v0.100 observability | `861f09bc9` |
| T2 | `Timestamp` + `WallDuration` value types (`_ms: u64` replacement) | `nika-types/src/timestamp.rs` | v0.95+ retrofit across 6 sites | `c5d292b6e` |

**Contract**:
- All 7 surfaces are `#[non_exhaustive]` or behind sealed defaults, so
  additional variants/fields remain non-breaking in later minor releases.
- T2 ships as *seed* (types exist); field retrofit across `budget.rs`,
  `checkpoint.rs`, `retry.rs`, `event_sink.rs`, `http.rs`, `process.rs` runs
  with `#[deprecated]` accessors for one minor before `_ms: u64` is removed.
- ADR stubs 029 (EmbeddingSpec), 030 (memory-frame-trust), 031 (tenant-
  keyspace), 032 (wasm-plugin-context), 035 (telemetry-seams) remain pending
  prose; the reservations are load-bearing ahead of the ADRs landing.

## Enforcement

Every crate admitted to the Diamond workspace passes Gate 12 verification:

- [ ] All public types `#[non_exhaustive]`
- [ ] All public errors `thiserror` + `#[from]` chains
- [ ] Error enums implement `NikaErrorCode` (`nika_code() -> NikaCode` ·
      the ONE canonical helper · typed num+category+slug). UNIFIED
      2026-06-10: the transitional inherent string `code() -> &'static str`
      helpers on screen · ocr · a11y are removed; all three now implement the
      trait with their ranges (NIKA-1000..1299) registered in
      `nika-error::codes` and pinned by per-crate `nika_code()` tests. The
      4 documented exemptions live in `scripts/ci/error-one-voice-allowlist.tsv`
      (enforced by hygiene vector 37 `check-error-one-voice`).
- [ ] All public traits sealed OR open-with-defaults
- [ ] No `Vec<T>` returns in public APIs (Iterator/IntoIterator instead)
- [ ] No third-party types in public API
- [ ] Public fields only on `#[non_exhaustive]` structs (FCI-016) — surface-locked
      by `cargo public-api` for the covered crates
- [ ] `schema:` on all file formats
- [ ] `cargo public-api --diff-git-checkouts main HEAD` green
- [ ] `cargo semver-checks check-release` green
- [ ] Reserved error code ranges respected

Violating any invariant blocks admission. No exceptions.

## See also

- `docs/architecture/crate-layer-registry.md` — L0 through L4 layering discipline
- `docs/adr/adr-006-layered-kernel-isp-traits.md` — Accepted
- `docs/adr/adr-007-forward-compat-invariants.md` — Accepted
- `docs/adr/adr-014-sealed-kernel-traits.md` — Accepted
- ADR-016 — Cancellation model: future-drop + `CancelCtx` module (Accepted)
- ADR-017 — Streaming policy: bounded `mpsc` + `futures_core::Stream` (Accepted)
- ADR-018 — Runtime + sync primitives (Accepted)
- ADR-019 — Retry + timeout ownership by layer (Accepted)
- ADR-020 — WASM plugin boundary + Sandbox capability model (Accepted)
- ADR-021 — YAML envelope convention: apiVersion + kind + metadata + spec (Accepted)
- ADR-022 — Foundation crate layout v0.81: 14 crates, publish = false (Accepted)
- ADR-023 — File modularity discipline: 800 warn / 1500 fail / 3000 allowlist (Accepted)
- ADR-024 — Adopt SOTA Rust patterns: bon Builder, Arc<str>, camino, sealed, strum (Accepted)
- ADR-025 — Per-crate semver via release-plz (Accepted)
- ADR-026 — workspace.toml single source of truth + auto-generated crate.md (Accepted)
- ADR-027 — Strict L0 sublayer tiers + max 3 sibling deps + cargo timings (Accepted)
- ADR-028 — Forward-compat reservation policy: seams now, crates later (Accepted)

🦋
