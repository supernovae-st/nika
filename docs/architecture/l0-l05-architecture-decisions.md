# L0 + L0.5 Architecture Decisions

**Date:** 2026-04-16
**Branch:** `nika-diamond` @ `820bd1949`
**Status:** LOCKED (brainstorm complete, 5 questions resolved)
**Research:** 3 Rust council agents per question + existing Phase C research (17 agents, TOPIC_1-5)
**Authority:** POST_AUDIT_REVISIONS.md > this document > crate-layer-registry.md

---

## Context

With 5 crates admitted (error, catalog, catalog-verify, kernel, kernel-mock) and 35-37
remaining, this document locks the L0 + L0.5 layer architecture. All decisions account
for v0.100 forward-compatibility and are designed to avoid breaking changes as the
workspace scales to 40-42 crates.

---

## Layer map — L0 + L0.5 (revised)

```
╭─ L0  (pure, sync, zero I/O) ──────────────────────────────────────────────╮
│ nika-error              [ADMITTED]  Foundation types + error hierarchy     │
│ nika-catalog            [ADMITTED]  Provider/model data, phf lookup       │
│ nika-catalog-codegen    [PLANNED]   TOML schema types + build codegen     │
│ nika-schema             [SPEC READY] Workflow AST + parser + analyzer     │
│ nika-event              [TO SPEC]   647 EventKind sub-enums + envelope    │
│ nika-binding            [TO SPEC]   Template engine + 65 transforms       │
│ nika-pck-manifest       [TO SPEC]   Package manifest TOML types          │
╰────────────────────────────────────────────────────────────────────────────╯
             ▲ (every upper-layer depends down only)
╭─ L0.5  (trait defs + companions, async OK) ────────────────────────────────╮
│ nika-kernel             [ADMITTED]  40 ISP traits, sealed                  │
│ nika-kernel-mock        [ADMITTED]  40 pure-memory mocks                  │
│ (split into kernel-core/ai/runtime/plugin when >10k LOC OR >50 traits)    │
╰────────────────────────────────────────────────────────────────────────────╯
```

### REMOVED from L0 plan

| Crate | Decision | Reason | Trigger for reopening |
|---|---|---|---|
| `nika-macros` | Q1: REMOVED | Manual impl + `macro_rules!` covers all cases. Exhaustive match > derive for internal code. | L2 builtins: if boilerplate >300 lines for 63 builtins, create proc-macro crate at L2 (not L0). |
| `nika-macros-core` | Q1: REMOVED | Same as above. | Same as above. |
| `nika-stdx` | Q2: REMOVED | Zero cross-crate duplication. `nika-error` is the de facto foundation (22 non-error types). Kitchen-sink anti-pattern risk. | If >=3 helpers duplicated across >=3 crates at 15+ admitted crates, create PURPOSE-NAMED crate (never "stdx/utils/common"). |

### REMOVED from L0-proc section

The entire L0-proc layer is removed. No proc-macro crates in Diamond L0.

---

## Decision Q1 — No proc macros in L0

**Research:** rust-architect + web-researcher (SOTA 2026) + local workspace analysis.

**Finding:** syn 2.0.117 is already in the dep tree (transitive via serde_derive,
thiserror-impl, etc.), so compile cost is marginal. However, the architectural
arguments against custom proc macros are stronger:

1. **Exhaustive match > derive for internal code.** Adding an EventKind variant forces
   the compiler to flag every match site. A derive macro hides this decision.
2. **Same line count.** NikaErrorCode: ~1 line/variant either way. EventTaskId: ~1
   line/variant either way (attribute vs match arm).
3. **POST_AUDIT says "inlined"** = no separate crate.
4. **Axum/Tokio pattern:** proc macros only for user-facing APIs, not internal plumbing.

**Forward-compat:** Traits (NikaErrorCode, ToolExecutor) are the stable contract.
Adding derives later is always additive (generates impl, never changes trait). Zero
breaking change.

**Trigger:** When nika-builtin-* lands (L2, ~session 8-10), if manual BuiltinTool
impls exceed 300 lines of repetitive boilerplate across 63 builtins, create
`nika-macros` at L2.

---

## Decision Q2 — No stdx crate; nika-error = foundation

**Research:** rust-architect (analyzed helix-stdx, rust-analyzer stdx, Bevy bevy_utils,
Zed util, rustc_data_structures) + web-researcher + local cross-crate analysis.

**Finding:** nika-error already contains 22 non-error types (RunId, EventId, TraceId,
SpanId, Cost, TrustLevel, Baggage, RetryConfig, Blake3Hash, etc.). Zero duplicated
code across the 5 admitted crates. The "stdx" candidates (case conversion, path
normalization, regex patterns) each have exactly 1 caller.

**Rules established:**

1. **nika-error = L0 foundation.** All cross-cutting pure types live here. The name
   is historical; the role is "foundation types."
2. **1 caller = stays in caller.** No shared crate for single-use helpers.
3. **Purpose-named crates if needed.** If >=3 crates duplicate the same helper,
   extract to `nika-text`, `nika-collections`, etc. — never "stdx" or "utils."
4. **nika-error split — APPROVED 2026-04-16.** At 4,031 LOC with 73% non-error
   types, the split is overdue. Split into:
   - `nika-types` (L0, ~2,821 LOC) — foundation value types (IDs, Cost, Trust,
     Hash, Baggage, Cancel, Checkpoint, Memory, etc.)
   - `nika-error` (L0, ~1,257 LOC) — error infrastructure only
   nika-error does `pub use nika_types::*` for backward compat.
   Zero breaking change for existing consumers.

**Forward-compat:** Adding a purpose-named crate is always additive. Re-exports in
nika-error ensure backward compat for the split scenario.

---

## Decision Q3 — Extract nika-catalog-codegen NOW

**Research:** rust-architect (prost-build/tonic-build pattern) + web-researcher (SOTA
build.rs practices) + rust-perf (compile-time analysis).

**Finding:** The build.rs is 2,564 LOC across 3 files. While the performance impact
is minimal (0.4s rebuild), the architectural case for extraction is strong:

1. **Testability.** Inline build.rs cannot be unit-tested (`#[test]` blocks are never
   executed in build scripts). Extraction enables standard `cargo test --lib`.
2. **Known future consumers.** `nika-catalog-sync` (L1, freshness pipeline) and
   `nika-catalog-tools` (L4, Session 4C) will both need the same TOML schema types.
3. **Feature-gated dual use.** Schema types (serde structs) available without the
   codegen logic via `default-features = false`.
4. **Industry pattern.** prost-build (5,340 LOC), tonic-build (1,973 LOC), and
   windows-bindgen (9,779 LOC) all follow this exact pattern.

**Structure:**

```
crates/nika-catalog-codegen/
  src/
    lib.rs                  # pub API: generate_catalog(toml_dir, out_dir)
    schema/
      providers.rs          # serde structs for llm-providers.toml
      capabilities.rs       # serde structs for model-capabilities.toml
      pricing.rs            # serde structs for model-pricing.toml
    codegen/
      capabilities.rs       # code generation logic (feature "codegen")
      pricing.rs            # code generation logic (feature "codegen")
    validate.rs             # TOML validation with clear error messages
  Cargo.toml
    [features]
    default = ["codegen"]
    codegen = []
```

**Consumers:**
- `nika-catalog` build.rs: `[build-dependencies]` with `features = ["codegen"]`
- `nika-catalog-sync` (L1, future): `[dependencies]` with `default-features = false`
- `nika-catalog-tools` (L4, Session 4C): `[dependencies]` with `default-features = false`

**Quick wins during extraction:**
- Migrate `cargo:rerun-if-changed` to `cargo::rerun-if-changed` (new syntax)
- Remove directory-level rerun-if-changed (per-file already emitted)

---

## Decision Q4 — nika-event: L0 types + L1 store + L2 export

**Research:** rust-architect (event system architecture) + web-researcher (tracing,
OTel, Bevy, CloudEvents, cqrs-es, reth patterns) + rust-pro (kernel Event analysis).

**CRITICAL REFERENCE:** `memory/execution-roadmap/brainstorm-topics/TOPIC_5_event_log.md`
(1,411 lines, 13 research agents, 3 rounds). This is the source of truth for the
event system. All 647 event types are enumerated there.

**Architecture (3-crate split respecting layer model):**

### nika-event (L0, ~4-5k LOC)

Pure types, zero I/O, zero async. Contains:

- **EventKind** — 647 variants organized as ~20 category sub-enums, all
  `#[non_exhaustive]`. Categories include: Workflow, Task, Infer, Exec, Fetch,
  Invoke, Agent, Streaming, ToolUse, StructuredOutput, MultiModal, HITL,
  Composition, Cost, Eval, Shield, Performance, Scheduling, MCP, Cache, Retry,
  Determinism, Privacy, Config, Observability, Reasoning, Pathology, Forensics.
  Plus reserved: Memory (v0.95 Cortex), Plugin (v0.100 WASM).
  Plus escape hatch: `Extension { ns, name, payload }` for community/future.

- **Event envelope types** — schema_version, baggage, blob_refs (matching TOPIC_5
  canonical envelope with ~30 fields).

- **EventLog** — in-memory ring buffer (Tier 0 from TOPIC_5). `VecDeque<Event>`
  with size cap. Pure data structure (Arc<RwLock> is L0-legal — no I/O).

- **Helper types** — FinishReason, Severity, AgentStopReason, AgentTurnKind,
  GuardrailType, ScoredOption (for counterfactual decisions).

- **Conversion** — `EventKind::as_str() -> &'static str` and
  `EventKind::parse(s: &str) -> Option<Self>`. One-way conversion at the
  EventSink boundary. Kernel `Event.kind: String` remains the wire format.

### nika-event-store (L1, ~3k LOC, future)

Tiered storage per TOPIC_5: SQLite WAL (Tier 1) + Parquet+zstd (Tier 2) +
cold archive (Tier 3). Implements `EventSink` from kernel. DuckDB query engine.

### nika-event-export (L2, ~2k LOC, future)

OTel GenAI semconv + CloudEvents + Langfuse (via OTLP) exporters per TOPIC_5.

**Note:** TOPIC_5 designed nika-event as a single L1 crate (~8k LOC). Our split
into L0 types + L1 store + L2 export is more disciplined — it respects the
downward-only dependency rule and keeps 647 EventKind types available to ALL
crates without pulling in SQLite.

**Dependency graph:**

```
nika-event (L0) ────→ nika-error (L0)
nika-kernel (L0.5) ──→ nika-error (L0)    [kernel does NOT depend on event]
verb-* (L2) ─────────→ event + kernel     [producers use both]
nika-event-store (L1) → event + kernel    [implements EventSink]
```

**5 observability channels (all in kernel, orthogonal):**

| Channel | Trait | Purpose | Guarantee |
|---|---|---|---|
| Events | EventSink | Workflow lifecycle | May be sampled |
| Billing | BillingSink | Cost tracking | Never dropped (audit) |
| Metrics | MetricsExporter | Counters/gauges | String-based, OTel-compat |
| Traces | TracerProvider | Latency spans | String-based, OTel-compat |
| Observability | ObservabilitySink | Unified OTel export (v0.100) | — |

Total v0.100 surface: ~647 event types + ~80 metrics + ~50 spans + ~10 billing
= ~787 distinct signals, ~700-1200 time series with labels/dimensions.
This is 10x Netflix, 16x Datadog APM (per TOPIC_5 competitive analysis).

---

## Decision Q5 — Admission order

**Dependency DAG:**

```
error ← catalog ← schema ← binding
  ↑                  (independent)
  ├── event          (depends on error only)
  ├── pck-manifest   (depends on error only)
  └── catalog-codegen (depends on error + serde/toml)
```

**Admission sequence:**

| Order | Crate | LOC est. | Deps | Blocks | Sessions est. | Spec |
|---|---|---|---|---|---|---|
| 6 | nika-schema | ~13k | error + catalog | binding, all L2 verbs | ~2 | READY |
| 7 | nika-catalog-codegen | ~2.5k | error + serde + toml | catalog-sync, catalog-tools | ~0.5 | TO WRITE |
| 8 | nika-event | ~4-5k | error | event-store, event-export, all L2 verbs | ~1 | TO WRITE |
| 9 | nika-binding | ~13k | error + catalog + schema | all L2 verbs | ~2 | TO WRITE |
| 10 | nika-pck-manifest | ~1.5k | error | pck-registry, pck-store, pck | ~0.5 | TO WRITE |

**Rationale:**

1. **schema first** — critical path. Blocks ALL verb crates (L2) and nika-binding.
   Spec is ready (798 lines). Highest leverage admission in the entire project.

2. **catalog-codegen second** — small, cleanly extractable from existing build.rs.
   Provides testable TOML schema types for future consumers. Can slip into any
   gap between larger admissions.

3. **event third** — 647 EventKind types needed by every verb crate. Independent
   of schema (depends only on error). TOPIC_5 provides exhaustive type list.

4. **binding fourth** — depends on schema. Template engine + 65 transforms.
   Second largest L0 crate after schema. Legacy reference: 7,400+ LOC.

5. **pck-manifest last** — small, independent, only blocks pck subsystem.
   The pck subsystem is v0.90+ priority, not urgent for verb crate admission.

**Specs to write (3 remaining):**

| Crate | Input source | Estimated spec size |
|---|---|---|
| nika-event | TOPIC_5 (1,411 lines, 647 types) | ~400 lines |
| nika-binding | legacy binding/ (7,400 LOC) + TOPIC_4 | ~500 lines |
| nika-pck-manifest | TOPIC_1 (262 lines) + POST_AUDIT | ~200 lines |

nika-catalog-codegen spec is derived from the existing build.rs code — minimal
spec needed (extract + reorganize, no new design).

---

## Forward-compatibility summary

| Decision | v0.95 Cortex impact | v0.100 WASM impact | Breaking change risk |
|---|---|---|---|
| No proc macros | None | None | Zero — derives are additive |
| No stdx | None | If WASM ABI types needed → `nika-wasm-abi` (purpose-named) | Zero — new crate is additive |
| Extract catalog-codegen | catalog-sync uses schema types | None | Zero — build-dep is invisible |
| Event L0/L1/L2 split | Memory sub-enums reserved | Plugin sub-enums + Extension escape hatch | Zero — `#[non_exhaustive]` |
| nika-error split seam | Split if >5k LOC | Same | Zero — re-exports |

---

## Audit trail

| Date | Decision | Author |
|---|---|---|
| 2026-04-16 | Q1: nika-macros REMOVED from L0 | Brainstorm session |
| 2026-04-16 | Q2: nika-stdx REMOVED from L0 | Brainstorm session |
| 2026-04-16 | Q3: nika-catalog-codegen EXTRACT NOW | Brainstorm session (revised from DEFER) |
| 2026-04-16 | Q4: nika-event L0/L1/L2 split (647 types from TOPIC_5) | Brainstorm session |
| 2026-04-16 | Q5: Admission order locked (schema → codegen → event → binding → pck-manifest) | Brainstorm session |
