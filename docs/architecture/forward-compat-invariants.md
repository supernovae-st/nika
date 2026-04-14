# Forward-Compatibility Invariants

**Status**: LOCKED at v0.80.0. Every crate admitted to nika-diamond must
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

### 1. Kernel traits upfront, implementations deferred

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
- `ObservabilitySink` — impl v0.100+
- `Sandbox` — impl v0.100+ (Landlock/seccomp)

### 2. `#[non_exhaustive]` on every public struct, enum, error variant

**Pattern**: Every public type is marked `#[non_exhaustive]`. External code
can never construct via `MyStruct { a, b }` literal — must use
`MyStruct::new(...)` or builder.

**Impact**: Adding a field (e.g., `pub memory: Option<MemoryDirective>` to
`InferRequest` in v0.95) is NOT a breaking change.

**Rule**: If you forget `#[non_exhaustive]` on a public type, CI fails via
`cargo public-api --diff-git-checkouts`.

### 3. Versioned file formats with explicit `schema:` field

**Pattern**: Every file format (workflows, pck manifests, event logs,
memory frames) carries a `schema:` version field. Parser dispatches by
version.

**Example**:
```rust
#[derive(Deserialize)]
#[serde(tag = "schema")]
pub enum WorkflowDoc {
    #[serde(rename = "nika/workflow@0.12")]
    V0_12(WorkflowV0_12),
    #[serde(rename = "nika/workflow@0.20")]
    V0_20(WorkflowV0_20),
}
```

**Locked schemas (v0.80)**:
- `nika/workflow@0.12` — current workflow YAML
- `nika/pck@1` — pck manifest TOML
- `nika/event@1` — event log JSON
- `nika/memory-frame@1` — memory frame JSON (v0.95)

**Migration tooling** ships parallel to schema bumps. No silent migrations.

### 4. Extension namespaces (`nika.*` core + `x-*` community)

**Pattern**: Every registry (pck types, EventKind variants, MCP aliases,
error codes, builtin names) uses explicit namespaces. Core Nika reserves
`nika.*`. Community uses `x-*` or reverse-DNS (`com.acme.type/v1`).

**Locked namespaces**:
- **pck types**: `workflow` / `skill` / `agent` / `provider` / `mcp` /
  `eval` / `recipe` / `shield` / `lints` (9 core) + `x-*` community
- **builtins**: `nika:tool` core + `server::tool` MCP-namespaced
- **EventKind**: core variants + `Extension { ns, name, payload }` escape hatch
- **Skill targets**: `claude-code`, `cursor`, `windsurf`, `zed` (4 IDEs) +
  future additions

### 5. Reserved error code ranges

**Pattern**: Error codes are assigned in ranges by subsystem. Ranges are
reserved even if unused, preventing collision when future subsystems ship.

**Locked ranges**:

| Range | Subsystem | Status |
|---|---|---|
| NIKA-000..099 | Core engine | active |
| NIKA-100..199 | MCP / catalog | active (100, 101, 107) |
| NIKA-200..299 | pck / validation | reserved v0.80, active v0.90 |
| NIKA-300..399 | Providers | active (DNS shown, codes TBD) |
| NIKA-400..499 | Cortex / memory | reserved v0.80, active v0.95 |
| NIKA-500..599 | agent-v2 | reserved v0.80, active v0.95 |
| NIKA-600..699 | Shield / sandbox | active (380-389) |
| NIKA-700..799 | Replay / observability | reserved v0.80, active v0.100 |
| NIKA-800..899 | WASM plugins | reserved v0.80, active v0.100+ |
| NIKA-900..999 | Community / x-* extensions | reserved v0.80 |

### 6. Sealed traits for core, open traits for extension

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
- **Sealed**: `Verb`, `Provider`, `Runtime`, `EventSink` — core engine only
- **Open**: `MemoryStore`, `EmbeddingProvider`, `ToolExecutor`, `Sandbox`,
  `ObservabilitySink`, `WasmPluginHost` — community may implement

### 7. Feature flags with stable defaults

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

### 8. Public API discipline via CI

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

### Decision 1 — EventKind extensibility model

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

### Decision 2 — `InferRequest` / `InferResponse` field reservation

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

### Decision 3 — `schema:` field mandatory day-1

Every file format ships with `schema:` from v0.80. Fail-fast on missing
schema. Never parse unversioned files.

**Rationale**: Cargo's `edition = "2018"` pattern. Without this, v1.0 can't
introduce a schema version without parsing every existing file as legacy.

### Decision 4 — Provider trait shape

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

**Rationale**: 9 providers in v0.90, 20+ in v0.100. Trait shape locks all
downstream.

### Decision 5 — `CatalogEntry` shape with Cortex-ready fields

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

## 10 concrete API design rules

1. **Never `Vec<T>` returns** → `impl Iterator<Item = T>` or `impl
   IntoIterator<Item = T>`. Locks allocation strategy.
2. **Never expose third-party types in public API**. Wrap:
   `pub struct Message { inner: rig::Message }`. Tokio learned this
   with Mio in 0.1 → 0.2.
3. **Never public fields** — even on `#[non_exhaustive]` structs. Use
   accessors or builder pattern.
4. **Never closed numeric enums** — always `#[non_exhaustive]` +
   `#[serde(other)]` for the catch-all variant.
5. **Newtype every ID** — `ProviderId(SmolStr)`, `WorkflowName(String)`,
   `SkillName(String)`. Validation, normalization, interning become
   additive.
6. **All errors `#[non_exhaustive]` + `thiserror` + `#[from]` chains**.
   Never `String` error types in public API.
7. **Sealed traits + default methods** — `fn dry_run(&self) -> Result<...>
   { Err(Unsupported("dry_run")) }`. Adding methods later is additive.
8. **Opaque handles** — `pub struct Runtime { _inner: Arc<dyn RuntimeInner> }`.
   Scheduler rewrites don't break users.
9. **`schema:` field mandatory on every file format**. Fail-fast on missing.
10. **`#[must_use]` on every Result-returning API**. Silently dropped
    errors are forward-compat tax when tightening error reporting.

## 10 anti-patterns to avoid

1. Returning `Vec<T>` from public APIs.
2. Exposing third-party types (`pub fn foo() -> rig::Message`).
3. Public fields on structs, even `#[non_exhaustive]`.
4. Closed enums with literal numeric discriminants.
5. `pub use` of internal modules — use a curated `prelude` instead.
6. Hard-coded paths in error messages — use `Path::display()` + structured
   fields.
7. `String` types everywhere — always newtype IDs.
8. Trait methods without default impls — adding a method = breaking change.
9. `async fn` in public traits without `Send` bound — locks runtime choice.
10. Forgetting `#[must_use]` on Result-returning APIs.

## Per-subsystem forward-compat matrix

| Subsystem | Stable v0.90 surface | Extension points | Risk | Mitigation |
|---|---|---|---|---|
| Core engine | `Runtime::execute`, EventStream | EventKind Extension variant, new directives in InferRequest | LOW | EventKind `#[non_exhaustive]` |
| pck | `schema:` + 9 types | `x-*` namespace, new schema versions | MED | `deny_unknown_fields` top-level only |
| Natives | 7 builtins sealed | `ExternalVerb` registry for community | LOW | Sealed `Verb` + open `ExternalVerb` |
| Providers | 21 via rig + native + mock | New providers as crates, `ProviderId` as newtype with `Custom(String)` | HIGH | Crate split already locked |
| MCP | Catalog + aliases | New transports, MCP versions | MED | rmcp behind facade trait |
| Cortex | Trait stubs only | Real impl v0.95 | LOW | All hooks in kernel v0.90 |
| agent-v2 | Reserved EventKinds, error codes | New crate v0.95 | LOW | InferRequest fields + default methods |
| Shield | Permission enum stub | Landlock/seccomp impl v0.100 | MED | Define `Permission` enum now |
| Lints | nika-lints crate | New lints additive | LOW | Per-lint allow attribute namespace |
| Observability | EventLog only | Tracing, OTel, replay v0.100 | MED | `EventKind::Trace` reserved |

## Enforcement

Every crate admitted to nika-diamond workspace passes Gate 12 verification:

- [ ] All public types `#[non_exhaustive]`
- [ ] All public errors `thiserror` + `#[from]` chains
- [ ] All public traits sealed OR open-with-defaults
- [ ] No `Vec<T>` returns in public APIs (Iterator/IntoIterator instead)
- [ ] No third-party types in public API
- [ ] No public fields (accessors only)
- [ ] `schema:` on all file formats
- [ ] `cargo public-api --diff-git-checkouts main HEAD` green
- [ ] `cargo semver-checks check-release` green
- [ ] Reserved error code ranges respected

Violating any invariant blocks admission. No exceptions.

🦋
