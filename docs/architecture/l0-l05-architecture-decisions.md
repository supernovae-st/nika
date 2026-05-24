# L0 + L0.5 Architecture Decisions

**Date:** 2026-04-16 (rev. 2 — Q8 reverted + Q9-Q10 added by swarm audit)
**Branch:** `nika-diamond` @ `edda1b0e9`
**Status:** LOCKED (10 questions resolved)
**Research:** 3-4 Rust council agents per question + Phase C research (17 agents, TOPIC_1-5) + post-decision swarm audit (architect + rust-pro + explorer)
**Authority:** POST_AUDIT_REVISIONS.md > this document > crate-layer-registry.md

---

## Context

With **6 crates admitted** (types, error, catalog, catalog-verify, kernel, kernel-mock)
+ **1 WIP** (nika-schema parser) and 33-35 remaining, this document locks the L0 + L0.5
layer architecture. All decisions account for v0.100 forward-compatibility and are
designed to avoid breaking changes as the workspace scales to 42 crates.

---

## Decision index

| Q   | Decision                                                  | Status   |
|-----|-----------------------------------------------------------|----------|
| Q1  | No proc macros in L0 (manual impl + `macro_rules!`)       | LOCKED   |
| Q2  | No `nika-stdx`; split `nika-error` → `nika-types` + `nika-error` | LOCKED · executed |
| Q3  | Extract `nika-catalog-codegen` NOW (testable + reusable)  | LOCKED   |
| Q4  | `nika-event`: 3-layer split (L0 types + L1 store + L2 export) | LOCKED |
| Q5  | Admission order: schema → codegen → event → binding → pck-manifest | LOCKED |
| Q6  | EventKind = scoped sub-enums (Pattern B, ~22 categories)  | LOCKED   |
| Q7  | `nika-kernel` prelude re-export hub (4 lines, 0 new deps) | LOCKED   |
| Q8  | ~~nika-transform = module in binding~~ → **REVERTED to L0 crate** (2nd consumer found in `nika-builtin`) | LOCKED rev.2 |
| Q9  | `Timestamp` + `WallDuration` = module in `nika-types`     | LOCKED rev.2 |
| Q10 | Canonical-JSON (RFC 8785) = module in `nika-types`        | LOCKED rev.2 |
| Q11 | Token-streaming cardinality policy (delta-batching, not 1-event-per-token) | LOCKED rev.3 |
| Q12 | Drop `ObservabilitySink` + add `AuditSink` (5 channels: Event/Metrics/Trace/Billing/Audit) | LOCKED rev.3 · executed |
| Q13 | Bridge OTel GenAI semconv via typed `GenAiAttrs` on Infer{Request,Response} | LOCKED rev.3 · executed |

---

## Layer map — L0 + L0.5 (rev. 2)

```
╭─ L0  (pure, sync, zero I/O) — 9 crates ──────────────────────────────────╮
│ nika-types              [ADMITTED]  Foundation value types               │
│                                     + timestamp module (Q9)              │
│                                     + hash::canonical module (Q10)       │
│ nika-error              [ADMITTED]  Error infra, re-exports nika-types  │
│ nika-catalog            [ADMITTED]  Provider/model data, phf lookup     │
│ nika-catalog-codegen    [PLANNED]   TOML schema types + build codegen   │
│ nika-schema             [WIP S4A]   Workflow AST + parser + DAG          │
│ nika-event              [TO SPEC]   ~22 scoped sub-enums + envelope     │
│ nika-binding            [TO SPEC]   Template engine (depends transform) │
│ nika-transform          [TO SPEC]   65 transforms (Q8 rev.2 — 2 consumers)│
│ nika-pck-manifest       [TO SPEC]   Package manifest TOML types         │
╰────────────────────────────────────────────────────────────────────────────╯
             ▲ (every upper-layer depends down only)
╭─ L0.5  (trait defs + companions, async OK) — 2 crates ─────────────────────╮
│ nika-kernel             [ADMITTED]  40 ISP traits, sealed supertrait      │
│                                     pub mod prelude re-exports types+err  │
│ nika-kernel-mock        [ADMITTED]  40 pure-memory mocks                  │
│ (reserved split: kernel-core/ai/runtime/plugin when >10k LOC OR >50 traits)│
╰────────────────────────────────────────────────────────────────────────────╯
```

> **Killed crates** (per Q1, Q2): `nika-macros`, `nika-macros-core`, `nika-stdx`.
> Triggers de réouverture consolidés en fin de doc (§ Reopen triggers).

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

### nika-event (L0, ~2.5k LOC)

Pure types, zero I/O, zero async. Internal architecture uses **scoped sub-enums**
(Pattern B per Q6 below) — not a mega-enum. Contains:

- **EventKind** — ~239 variants initially (growing to ~647 at v0.100), organized
  as ~22 category sub-enums, all `#[non_exhaustive]`. Generated by
  `macro_rules! event_categories!` (enum + From impls + `category()` match).
  Categories include: Workflow (~15 variants), Infer (~20), Exec, Fetch,
  Invoke, Agent, Streaming, ToolUse, StructuredOutput, MultiModal, HITL,
  Composition, Cost (~6), Eval, Shield, Performance, Scheduling, MCP, Cache,
  Retry, Determinism, Privacy, Config, Observability, Reasoning, Pathology,
  Forensics. Plus reserved: Memory (v0.95 Cortex), Plugin + Wasm (v0.100) —
  empty `#[non_exhaustive]` enums with `Reserved { version }` placeholder.
  Plus escape hatch: `Extension { ns, name, payload }` for community/future.
  Wire format: `{"kind": "infer.chunk_received", "data": {...}}` via custom
  serde bridge (`serde_wire.rs`).

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
| 8 | nika-event | ~2.5k | error | event-store, event-export, all L2 verbs | ~1 | TO WRITE |
| 9 | nika-binding | ~13k* | error + catalog + schema | all L2 verbs | ~2 | TO WRITE |
| 10 | nika-pck-manifest | ~1.5k | error | pck-registry, pck-store, pck | ~0.5 | TO WRITE |

**Rationale:**

1. **schema first** — critical path. Blocks ALL verb crates (L2) and nika-binding.
   Spec is ready (798 lines). Highest leverage admission in the entire project.

2. **catalog-codegen second** — small, cleanly extractable from existing build.rs.
   Provides testable TOML schema types for future consumers. Can slip into any
   gap between larger admissions.

3. **event third** — ~239 EventKind variants initially (647 at v0.100), scoped
   sub-enums per Q6. Independent of schema (depends only on error). TOPIC_5
   provides exhaustive type list. LOC revised down from ~4-5k to ~2.5k (scoped
   sub-enums are more compact than mega-enum).

4. **binding fourth** — depends on schema. Template engine + 65 transforms.
   Second largest L0 crate after schema. Legacy reference: 7,400+ LOC.
   *Includes transforms module (~2k LOC) per Q8 — ~13k total with transforms.

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

## Decision Q6 — EventKind: scoped sub-enums (not mega-enum)

**Research:** 4-agent Rust council (unanimous: Pattern B — scoped sub-enums with thin
aggregation).

**Problem:** nika-event needs ~239 event variants initially, growing to ~647 at v0.100.
Three candidate patterns: (A) mega-enum, (B) scoped sub-enums, (C) trait-based (Bevy).

**Why Pattern B wins:**

1. **~22 category sub-enums** — WorkflowEvent (~15 variants), InferenceEvent (~20),
   CostEvent (~6), etc. Each sub-enum is `#[non_exhaustive]`, lives in its own file.
2. **`macro_rules! event_categories!`** generates the top-level `EventKind` enum +
   `From` impls + `category()` match arm. Zero manual wiring.
3. **Wire format** — `{"kind": "infer.chunk_received", "data": {...}}` via custom
   serde bridge (`serde_wire.rs`). Dot-separated category.variant is stable + human-readable.
4. **Reserved categories** — Memory (v0.95 Cortex), Plugin + Wasm (v0.100) ship as
   empty `#[non_exhaustive]` enums with `Reserved { version }` placeholder variant.
   When the feature lands, variants are added (never breaking — non-exhaustive).
5. **EventLog** — `VecDeque<Event>` ring buffer with size cap. Pure L0, zero I/O.

**Why not mega-enum:** catastrophic match arms at 647 variants, merge conflicts on
every PR touching events, 2,500+ LOC single file, no per-domain file navigation.

**Why not trait-based (Bevy):** no exhaustive matching (dynamic dispatch), needs
`erased_serde` or `reflect` (~15k LOC infra), runtime-only completeness guarantees.

**File layout:** 1 file per category + `lib.rs` + `kind.rs` + `log.rs` + `serde_wire.rs`.

**LOC estimate:** ~2.5k (down from ~4-5k — scoped sub-enums are more compact than
a mega-enum with inline data).

---

## Decision Q7 — nika-kernel prelude re-export hub

**Research:** rust-architect (pattern validation: rust-analyzer `base-db`, Bevy
`bevy_app`, Axum re-exports `axum-core`).

**Problem:** L2+ verb crates each depend on nika-error + nika-types + nika-kernel
(+ potentially nika-event). That is 3-5 explicit dep lines per crate, with version
churn on each. Axum solved this by re-exporting `axum-core` types from the main crate.

**Solution:** Add `pub mod prelude` to `nika-kernel/src/lib.rs`:

```rust
pub mod prelude {
    pub use nika_error::prelude::*;  // includes nika-types re-exports
    // kernel traits + request/response types already in nika-kernel root
}
```

L2+ verb crates depend on `nika-kernel` only (2-3 deps instead of 5).

**NOT a separate `nika-prelude` crate** — same anti-pattern as the killed `nika-stdx`
(Q2). Preludes belong in the crate they serve, not in a standalone wrapper.

**NOT re-exporting nika-catalog** — kernel does not depend on catalog and should not.
Verb crates that need catalog data add it as an explicit dep.

**Impact:** Zero new dep edges, zero compile-time impact. Implementation is 4 lines
in `nika-kernel/src/lib.rs`.

---

## Decision Q8 — nika-transform = standalone L0 crate (REVERTED rev.2 2026-04-16)

**Initial decision** (rev.1, brainstorm session): module in `nika-binding` under the
single-consumer rule (Q2). **REVERTED** by post-decision swarm audit (spn-rust:rust-pro).

**Reason for revert.** Grep of legacy `main` (HEAD `830aa6154`) found a **second
production consumer**: `tools/nika-builtin/src/data/transform.rs` imports
`nika_core::binding::transform::{TransformExpr, navigate_dot_path}` and calls
`.parse()` (l.111, 161) + `.apply()` (l.123, 241). This powers the `nika:map` and
`nika:pluck` builtin tools — a shipped public feature, not speculation. The Q2
threshold (≥2 distinct crate-level consumers in code) is **already met today**.

**Decision.** `nika-transform` extracted as standalone **L0 crate** (~2k LOC, 65 ops,
7 sub-modules: string, collection, encoding, format, datetime, path, parse).

**Dependency graph:**

```
nika-binding   (L0) ──→ nika-transform (L0)   [template engine: ${{ x | upper }}]
nika-builtin-* (L2) ──→ nika-transform (L0)   [MapTool / PluckTool / structured-output coercion]
                          │
                          └──→ nika-error / nika-types (L0)
```

**7 sub-modules** (in `crates/nika-transform/src/`):

| Module | Purpose |
|---|---|
| `string` | case conversion, truncate, pad, regex |
| `collection` | sort, unique, flatten, group_by |
| `encoding` | base64, url_encode, html_escape |
| `format` | json, yaml, toml, csv formatting |
| `datetime` | parse, format, relative, timezone (uses nika-types::timestamp Q9) |
| `path` | join, basename, dirname, extension |
| `parse` | number, bool, split, regex capture |

**L0 crate count impact:** 9 L0 crates (was 8). The layer map at the top of this
document reflects this.

**Forward-compat:**
- v0.95 Cortex memory summaries get `| truncate | to_json` without pulling
  `nika-binding`'s template engine.
- v0.100 WASM plugins can import transforms directly without binding's resolver.
- Mutation testing isolated: `cargo mutants -p nika-transform` skips 11k LOC of
  binding recompilation.

**Trigger for re-merge into binding:** none — extraction is irreversible once
`nika-builtin-*` ships against the standalone crate.

---

## Decision Q9 — Timestamp + WallDuration = module in nika-types (rev.2)

**Research:** spn-rust:rust-architect post-decision audit.

**Gap identified.** `nika-kernel::Clock` returns `std::time::{Instant, SystemTime,
Duration}`. None of these are stable serde types. `nika-event` envelope (Q4) requires
`created_at`. Checkpoint, baggage, retry, billing — all need a serializable wall-clock
timestamp. Today: gap. No `chrono`, `time`, or `jiff` anywhere in the workspace.

**Decision.** Add **module** `nika-types::timestamp` exposing:
- `Timestamp(i64)` — Unix nanoseconds, `#[non_exhaustive]` newtype, `Serialize + Deserialize`
- `WallDuration(i64)` — signed nanoseconds, newtype around `i64`
- Conversions: `From<SystemTime>`, `try_into Instant` (relative to a reference)

**Why module, not crate** (Q2 single-consumer rule): kernel + event + checkpoint will
all consume, but they all already depend on `nika-types`. Adding a 1-dep wrapper crate
(`nika-time`) duplicates what `nika-types` is for.

**Trigger to extract as crate:** v0.95 scheduling verb requires IANA timezone data
(`tz-data` is heavyweight). At that point extract `nika-time` with feature-gated
`tz-data`, keep raw `Timestamp` newtype in `nika-types`.

**Forward-compat:** `#[non_exhaustive]` on the newtype permits adding `nanos` or
`millis` accessors later. Adding a TZ field is breaking — defer to crate extraction.

---

## Decision Q10 — Canonical-JSON (RFC 8785) = module in nika-types (rev.2)

**Research:** spn-rust:rust-architect post-decision audit.

**Gap identified.** `nika-types::hash::{Blake3Hash, BlobRef, ContentDigest}` exists,
but there is no canonicalizer. pck-manifest integrity, event content-hashing
(TOPIC_5), memory-frame dedup (Cortex v0.95), and checkpoint signing all require
deterministic byte-level encoding. Today: hashing arbitrary `serde_json::Value` is
silently non-deterministic (key order, whitespace, number representation).

**Decision.** Add **module** `nika-types::hash::canonical` exposing:
- `to_canonical_bytes(&T) -> Result<Vec<u8>, NikaError>` — RFC 8785 (JCS) wrapper
- `digest_canonical(&T) -> Result<Blake3Hash, NikaError>` — convenience hash

Implementation = thin wrap of `serde_jcs` (well-maintained crate, MIT/Apache).

**Why module, not crate.** Same rationale as Q9 — `nika-types` is the foundation,
adding `serde_jcs` as one dep there is cheaper than birthing a 200-LOC wrapper crate.

**Trigger to extract as crate:** if a non-`nika-types` consumer needs canonical-JSON
**without** the rest of `nika-types`. Unlikely (anyone canonicalizing also handles
IDs/hashes). Module is final form for the foreseeable future.

**Forward-compat:** RFC 8785 is finalized (Sept 2020). No spec churn risk. The
function signature is additive — new `digest_canonical_with(hasher)` would be a
later non-breaking addition.

---

## Decision Q11 — Token-streaming cardinality policy (rev.3)

**Research:** spn-rust:rust-pro telemetry SOTA audit.

**Problem.** TOPIC_5 enumerates `StreamContentBlockDelta` as one of the 647
event types. Naively emitting **one event per streamed token** generates 50k–200k
events for a single long completion (Anthropic extended-thinking, OpenAI o3
reasoning streams already exceed 100k tokens in production). At ingest cost
~1 µs/event + storage ~200 B/event, that is ~50 ms ingest overhead and ~20 MB
of event payload **per request** — a cardinality blow-up that no telemetry
backend (Honeycomb, Datadog, Langfuse, OTLP) accepts unsampled.

**Decision.** Token-streaming events are **delta-batched** before reaching
`EventSink`. The batching contract:

| Trigger | Threshold | Behaviour |
|---|---|---|
| Time | `≥ 100 ms` since last flush | Emit accumulated batch as one `StreamBatchFlushed` event |
| Size | `≥ 32 KiB` accumulated | Flush early to bound memory |
| Boundary | `tool_call_start`, `content_block_end`, `usage_reported`, `finish_reason` | Flush before emitting the boundary event |
| Cancellation | `Cancel` signal received | Flush + emit `StreamCancelled` |

**Wire format** (Q6 dot-notation):

```text
"kind": "infer.stream.batch_flushed"
"data": {
  "delta_count": 47,
  "byte_count": 8192,
  "first_seq": 1023,
  "last_seq": 1069,
  "flush_reason": "time_window"   // | "size_cap" | "boundary" | "cancellation"
}
```

The individual deltas remain available via the kernel `InferEventStream`
(ADR-017 transport) for consumers needing per-token UX (live typing); they are
just **not enumerated as separate `EventKind` variants** in the audit log.

**Why not 1-event-per-token.** Honeycomb's high-cardinality columnar store can
ingest it; SQLite WAL Tier 1 (Q4 store) cannot. Langfuse + Logfire would charge
per event. OTel GenAI semconv (Apr 2026 development draft) explicitly recommends
batching streaming deltas at the SDK level.

**Forward-compat.** `flush_reason` is `#[non_exhaustive]`; new triggers (e.g.
`backpressure`, `quota_exhausted`) are additive.

**Trigger to revisit.** If a user requires per-token forensics (ex.
prompt-injection replay), expose a `forensics_mode: bool` on `InferRequest` that
disables batching for that single call only.

---

## Decision Q12 — Drop `ObservabilitySink`, add `AuditSink` (rev.3)

**Research:** spn-rust:rust-pro telemetry SOTA audit.

**Problem A — over-engineering.** The kernel currently exposes 5 observability
channels: `EventSink`, `BillingSink`, `MetricsExporter`, `TracerProvider`,
`ObservabilitySink`. The fifth (`ObservabilitySink`) is documented as a v0.95
merge-then-v0.100-resplit of Metrics+Trace — three overlapping traits.
OpenTelemetry uses 3 signals (logs/metrics/traces). Nika should not exceed 4.

**Problem B — compliance gap.** Shield (capability override, taint violation,
canary leak) and budget-exhaustion events have **never-sample + tamper-evident
append-only** semantics. Neither `EventSink` (may sample) nor `BillingSink`
(cost-typed) fits. This is a Nika-unique surface vs OTel and is currently
missing.

**Decision.**

1. **Drop `ObservabilitySink`** before any L1+ admission consumes it. Its
   intended Metrics+Trace merge use-case is already covered by adapters at the
   exporter layer (e.g. an OTLP exporter consumes both `MetricsExporter` and
   `TracerProvider` and merges client-side).

2. **Add `AuditSink`** L0.5 sealed trait:

   ```rust
   #[trait_variant::make(AuditSinkDyn: Send)]
   pub trait AuditSink: Send + Sync + sealed::Sealed {
       /// Append-only, never-sampled, must succeed or kill the run.
       /// Implementers MUST persist before returning Ok.
       async fn audit(&self, record: AuditRecord) -> Result<(), AuditSinkError>;
   }
   ```

   `AuditRecord` carries `#[non_exhaustive]` typed variants:
   `CapabilityOverride`, `TaintViolation`, `CanaryLeak`, `BudgetExhausted`,
   `PolicyDenied`, `KeyUsed`, `SecretRedacted`, `Extension { ns, name, payload }`.

3. **Channel count: 4 + 1 = 5 logical**, but the topology is clean:

   | Channel | Sampling | Loss tolerance | Wire |
   |---|---|---|---|
   | EventSink | sampled OK | best-effort | OTel logs |
   | MetricsExporter | aggregated | best-effort | OTel metrics |
   | TracerProvider | tail-sampled | best-effort | OTel traces |
   | BillingSink | never sample | persist-or-fail | TOML/Parquet |
   | **AuditSink** | **never sample** | **persist-or-fail** | **append-only log + Merkle anchor** |

**Forward-compat.** `AuditRecord` is `#[non_exhaustive]`. New audit variants
(GDPR right-to-erasure, EU AI Act incident report) land additively.

**Why this is unique.** OpenTelemetry has no first-class compliance signal.
Honeycomb stores audit as regular events (sampled). Langfuse has scores but
not append-only audit. Datadog APM has a separate audit-trails product.
Nika is the first AI engine to bake compliance into the kernel trait surface.

---

## Decision Q13 — OTel GenAI semconv bridge (`GenAiAttrs` on Infer{Request,Response}, rev.3)

**Research:** spn-rust:rust-pro telemetry SOTA audit.

**Problem.** OTel GenAI semconv defines ~30 `gen_ai.*` attributes
(`gen_ai.system`, `gen_ai.request.model`, `gen_ai.usage.input_tokens`,
`gen_ai.response.finish_reasons`, etc.). Today, kernel `InferRequest` and
`InferResponse` carry the underlying values but in **untyped form** — no
exporter can map them without re-inventing the bridge.

**Decision.** Add a typed module `nika-kernel::genai::GenAiAttrs` with the
~20 stable semconv fields, and embed it as `pub gen_ai: GenAiAttrs` on both
`InferRequest` and `InferResponse` (`#[non_exhaustive]`, `Default` impl,
populated by the kernel's `Provider` trait impl).

**Mapping table (semconv → Nika field):**

| OTel attribute | Nika field |
|---|---|
| `gen_ai.system` | `GenAiAttrs::system: GenAiSystem` enum (Anthropic/OpenAI/etc.) |
| `gen_ai.request.model` | `GenAiAttrs::request_model: ModelId` |
| `gen_ai.request.max_tokens` | `InferRequest::max_tokens` (already present) |
| `gen_ai.request.temperature` | `InferRequest::temperature` (already present) |
| `gen_ai.response.id` | `GenAiAttrs::response_id: Option<String>` |
| `gen_ai.response.model` | `GenAiAttrs::response_model: Option<ModelId>` |
| `gen_ai.response.finish_reasons` | `InferResponse::finish_reason` (already present, vec at semconv) |
| `gen_ai.usage.input_tokens` | `TokenUsage::input` |
| `gen_ai.usage.output_tokens` | `TokenUsage::output` |
| `gen_ai.usage.cached_input_tokens` | `TokenUsage::cached_input` (added rev.3) |
| `gen_ai.usage.reasoning_tokens` | `TokenUsage::reasoning` (added rev.3) |
| `gen_ai.tool.name` | per `ToolCall::name` |

**Why typed, not free-form attributes.** Honeycomb-style freeform tags work
but break the cross-provider parity gate (Pre-launch Gate 2). A typed bridge
enforces every provider populates the same fields, no silent drops.

**Forward-compat.** `GenAiAttrs` is `#[non_exhaustive]`. The OTel GenAI
semconv is still in **Development** status (Apr 2026); fields can change.
`Default` + `pub fn new()` constructors absorb additions without breaking
struct-literal callers.

**Trigger to revisit.** When OTel GenAI semconv reaches Stable (~2026-Q4
expected), re-validate Nika's mapping and pin the fields that semconv has
frozen.

---

## Reopen triggers (consolidated)

Decisions are LOCKED, but each has a **named, observable** trigger that re-opens it.
Triggers must appear in **CODE**, never in speculation (Q2 rule).

| Decision | Trigger condition | Reopens to |
|---|---|---|
| Q1 (no proc macros) | nika-builtin-* manual `BuiltinTool` impls > 300 LOC across 63 builtins | Create `nika-macros` at **L2** (not L0) |
| Q2 (no stdx) | ≥3 admitted crates duplicate the same helper | Create **purpose-named** crate (`nika-text`, `nika-collections`…) |
| Q2 seam | nika-error grows past 5k LOC again | Re-split (already executed once) |
| Q3 (codegen extracted) | catalog-sync or catalog-tools admitted | Just consume with `default-features = false` |
| Q4 (event 3-layer) | EventLog needs persistence | Admit `nika-event-store` (L1, ~3k LOC) |
| Q5 (admission order) | nika-schema admission blocked | Re-prioritize verb-* admissions |
| Q6 (scoped sub-enums) | New event category needed (Cortex Memory, WASM Plugin) | Add sub-enum file + `event_categories!` entry |
| Q7 (prelude hub) | Verb crate needs catalog data directly | Add explicit `nika-catalog` dep (do **not** re-export from kernel) |
| Q8 (nika-transform crate) | n/a — extraction irreversible once builtin-* ships | — |
| Q9 (timestamp module) | v0.95 scheduling verb requires IANA tz-data | Extract `nika-time` crate, keep `Timestamp` newtype in nika-types |
| Q10 (canonical-JSON) | Non-`nika-types` consumer needs canonical-JSON without nika-types | Extract `nika-canonical` crate (unlikely) |
| Q11 (token-batching) | User needs per-token forensics (prompt-injection replay) | Add `forensics_mode: bool` on InferRequest (per-call opt-out of batching) |
| Q12 (drop ObservabilitySink + add AuditSink) | Real Metrics+Trace merge consumer appears (unlikely) | Add adapter at exporter layer, not in kernel |
| Q13 (GenAiAttrs) | OTel GenAI semconv reaches Stable (~2026-Q4) | Pin frozen fields, mark non-stable as `unstable_` prefix |
| Kernel split | nika-kernel > 10k LOC OR > 50 traits | Split into kernel-{core,ai,runtime,plugin} |

---

LOCKED 2026-04-16 — Q1-Q8 resolved in brainstorm session, Q8 reverted + Q9-Q10 added by swarm-2 audit (architect + rust-pro + explorer), Q11-Q13 added by swarm-3 SOTA telemetry audit (architect + rust-pro + web-researcher). See `git log docs/architecture/l0-l05-architecture-decisions.md` for per-Q rationale and ADR-028 for the seams-now-crates-later policy.
