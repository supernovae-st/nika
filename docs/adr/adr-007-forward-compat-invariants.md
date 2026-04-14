# ADR-007: Forward-compat invariants (`#[non_exhaustive]` + `::new()` + pre-planted kernel hooks)

**Status:** Accepted
**Date:** 2026-04-13 (invariant #19 FULL as of Session 2a, 2026-04-14)
**Phase:** Phase 0 → cross-cutting
**Deciders:** @ThibautMelen + 16-agent Rust Council

## Context

Nika is forever v0.x (ADR-002) with features shipping incrementally over years. Adding a field to a public struct after v0.90 would be a breaking change for any downstream crate using struct literal syntax. `InferRequest` flows through every provider, verb, and memory call — if Cortex memory fields aren't reserved at v0.90, every consumer must be touched at v0.95.

The 16-agent Rust Council (2026-04-13) quantified the ROI: **1.5 days of pre-planting now saves 10 days of migration later (ROI 6.7×)**. The decision was unanimous.

## Decision

**Four interlocking mechanisms** make Nika's public API forward-compatible by construction:

### 1. `#[non_exhaustive]` + `pub fn new()` (Invariant #19)

Every public struct, enum, and error variant carries `#[non_exhaustive]`. External code cannot use struct literal syntax (`Foo { x: 1 }`) — only the constructor (`Foo::new(1)`). Adding a field post-v0.90 is then non-breaking: existing callers keep calling `new()` with the old signature; new fields get default values; the struct literally grows without rippling.

Invariant #19 mandates `pub fn new()` on **every** `#[non_exhaustive]` public struct. 15 constructors confirmed across the workspace as of Session 2a. Verified by `scripts/ci/check-tests.sh` + explicit snapshot tests in `nika-catalog`.

### 2. Reserved `Option<T>` fields on `InferRequest` / `InferResponse`

Future-phase fields land **today** as `None`-initialized options:

- `memory: Option<MemoryDirective>` — Cortex, v0.95
- `budget: Option<BudgetDirective>` — v0.100
- `replay_seed: Option<u64>` — v0.100
- `memory_frames: Vec<MemoryFrameRef>` — Cortex, v0.95

When v0.95 lands, the provider impls start *populating* these fields. No struct shape changes. No downstream migrations.

### 3. Kernel hooks pre-planted at Phase 0

`nika-kernel` (L0.5) defines these traits **now**, even though no implementation ships until v0.95/v0.100:

- `MemoryStore`, `EmbeddingProvider` — Cortex (v0.95)
- `ToolExecutor` — agent-v2 parallel tool calling (v0.100)
- `WasmPluginHost` — WASM plugins (v0.100)
- `ObservabilitySink` — tracing + metrics (v0.100)

At v0.95, a new crate `nika-memory-oxigraph` implements `MemoryStore`. Zero modification to the already-admitted kernel.

### 4. `EventKind` reserved variants + `Extension` escape hatch

`EventKind` is `#[non_exhaustive]` and carries placeholder variants for every planned subsystem: `MemoryHit`, `AgentPlanned`, `Trace`. Plus an `Extension { ns: &'static str, name: &'static str, payload: serde_json::Value }` variant for community extensions that bypass core without conflicting.

## Consequences

### Positive
- v0.95 Cortex ships by adding `nika-memory-oxigraph` implementing kernel traits — zero modification to v0.90 code.
- `InferRequest` field additions are not breaking changes within v0.x.
- `Extension` variant gives ecosystem extensions a forward-safe escape hatch.
- Downstream users are protected from struct-shape churn during 11+ month Diamond build.

### Negative
- Every public type carries invariant overhead (`#[non_exhaustive]` + `new()` constructor). For internal crates this is ceremony — mitigated by workspace lint `unreachable_pub = warn` which catches over-visibility.
- Reserved `Option` fields in `InferRequest` serialize as `None` until v0.95; slight JSON bloat (~5 extra null fields per request). Acceptable.
- Future-phase fields require discipline to never accidentally use in pre-phase code. Compiler does not enforce "use only when phase ≥ X."

### Neutral
- `Extension` variant payload is `serde_json::Value` (unstructured). Intentional — extensions define their own schema; core stays neutral.

## Evidence

- `tools/nika-kernel/src/provider.rs` lines 196–239 — `InferRequest` with `memory: Option<MemoryDirective>`, `new()` constructor, full field list
- `tools/nika-kernel/src/provider.rs` lines 291–312 — `InferResponse` with `memory_frames: Vec<MemoryFrameRef>`
- `tools/nika-kernel/src/memory.rs` lines 272–305 — `MemoryStore` + `EmbeddingProvider` trait declarations (Cortex hooks)
- `tools/nika-kernel/src/tool_executor.rs` — `ToolExecutor` trait (agent-v2 hook)
- `docs/architecture/forward-compat-invariants.md` lines 1–100 — the 8 patterns, 5 locked decisions, 10 API design rules, 10 anti-patterns
- memory: `POST_AUDIT_REVISIONS.md` DÉCISION 3 — kernel hooks Phase 0 mandatory
- Invariant #19 FULL status in `memory: STATE.md` (15 constructors, all tested)
- Related crates: **`nika-error`** (error variants #[non_exhaustive]), **`nika-kernel`** (pre-planted hooks), **`nika-catalog`** (16 new constructors Session 2a)

## Alternatives considered

### Alt A — Add fields as they're needed, breaking-change bump
Rejected per ROI math (6.7× cost at v0.95).

### Alt B — Use builder pattern everywhere (typestate)
Considered. Rejected: overkill for request/response types that have reasonable defaults. Reserve for kernel-level ceremony like `WorkflowBuilder`.

### Alt C — Versioned struct types (`InferRequestV1` / `InferRequestV2`)
Rejected: API churn at version boundaries is exactly what forever-v0.x (ADR-002) is trying to avoid.

## Related

- ADR-002 — forever v0.x (the release model that demands forward-compat)
- ADR-005 — error hierarchy (error enums also `#[non_exhaustive]`)
- ADR-006 — kernel ISP traits (where the hooks land)
- `docs/architecture/forward-compat-invariants.md` — full reference

## Notes

Invariant #19 is a gate in the admission protocol (ADR-003, Gate 4 — zero clippy warnings — includes `non_exhaustive_omitted_patterns = "deny"`). Additions slip is mechanically impossible.
