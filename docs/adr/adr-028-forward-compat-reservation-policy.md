---
id: ADR-028
title: "Forward-compat API-type reservation (amended — feature scheduling dropped)"
status: amended
date: "2026-04-16 (amended 2026-04-17 by ADR-037)"
phase: "Phase D — swarm-3 SOTA audit close-out; amended post bottom-up progression lock"
deciders: ["@ThibautMelen", "swarm-3 (rust-architect + rust-pro + web-researcher)"]
tags: ["forward-compat", "reservation", "seams", "yagni", "scalability", "amended"]
affects_crates: ["nika-kernel", "nika-types", "nika-error", "nika-event"]
affects_layers: ["L0", "L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-002", "ADR-006", "ADR-007", "ADR-014", "ADR-020", "ADR-024", "ADR-037"]
requires: ["ADR-007"]
enables: []
amends: []
amended_by: ["ADR-037"]
fci: ["FCI-002", "FCI-003", "FCI-009"]
inv: ["INV-019"]
shadow_zones: []
nika_codes: []
timeline: "v0.81+"
follow_ups:
  - "Wire cargo public-api on the prelude module (Bevy regretted not doing this)"
  - "Document escape hatch in forward-compat-invariants.md §reserved-mutability"
  - "Property tests proving MemoryStore + WasmPluginHost can host realistic backends"
---

# ADR-028: Forward-compat reservation policy — seams now, crates later

## Context

The swarm-3 SOTA audit (rust-architect + rust-pro + web-researcher, 2026-04-16,
HEAD `5e810a94a`) closed a recurring strategic question:

> *"Puisqu'on refait Nika from scratch, est-ce con de différer Cortex memory à v0.95
> et WASM plugin à v0.100 ? Faut-il tout planifier dès v0.90 ?"*

The temptation is real: if every crate is rewritten clean, why not fully bake the
v0.95 Cortex orchestrator (`nika-memory` + 8 satellites) and v0.100 WASM plugin
host (`nika-wasm-host` + `nika-sandbox`) right now, so nothing ever needs to
change? This ADR closes the question.

## Decision

**Two-tier forward-compat policy. Seams are mandatory at v0.90; crate
implementations are deferable to their named v-target.**

### Tier 1 — SEAMS (mandatory, ship at v0.90)

A *seam* is the smallest API surface that fixes the shape of a future feature
without committing to its implementation. Seams must land before v0.90:

| Seam mechanism | Where in Nika today |
|---|---|
| `#[non_exhaustive]` on every public enum + struct | All L0 + L0.5 crates (INV-019, ADR-007) |
| Reserved trait stubs in kernel (`MemoryStore`, `WasmPluginHost`, `Sandbox`, `EmbeddingProvider`, `ToolExecutor`, `ObservabilitySink`) | `crates/nika-kernel/src/` (Wave 2 S1-B) |
| Reserved sub-enums in event log (`EventKind::Memory` v0.95, `EventKind::Plugin` + `Wasm` v0.100) | Q4 + Q6 of `l0-l05-architecture-decisions.md` |
| Reserved fields on Infer{Request,Response} + MemoryFrame | Wave 2 S1-B + S1-C |
| `pub fn new()` constructors blocking struct-literal syntax | Every `#[non_exhaustive]` struct (Q-pattern) |
| `Extension { ns, name, payload }` escape hatch on `EventKind` | Q6 (Pattern B sub-enums) |

Seams are **cheap to add now, ruinous to retrofit later** (Hyrum's Law: every
observable behaviour gets depended on). They cost a few lines of `#[non_exhaustive]`
and `Reserved { version }` placeholders. Their absence creates breaking changes
at every v-tick.

### Tier 2 — CRATE IMPLEMENTATIONS (deferable to named v-target)

A *crate implementation* is the actual code that satisfies a seam. Crate
implementations are deferred until at least one of:

1. A user signal demands the feature (real workflow, not speculation).
2. The upstream design space stabilises (e.g. WASM Component Model Preview 3).
3. A sibling Diamond crate would benefit immediately (≥2 consumers, Q2 rule).

Specific deferrals locked by this ADR:

#### v0.95 Cortex memory — KEEP-DEFER

- Seams already shipped: `MemoryStore`, `EmbeddingProvider`, `MemoryDirective`
  on `InferRequest`, `MemoryFrameRef` on `InferResponse`, `EventKind::Memory`
  reserved sub-enum.
- Crates deferred: `nika-memory` (L2 orchestrator) + 8 satellites
  (`nika-memory-{hnsw,bm25,rrf,fsrs,rdfs-reasoner,temporal,graph-algos,autodesc}`).
- Rationale: 8 stub crates × 15k LOC budget each = ~120k LOC of empty hulls
  with zero user signal. `fsrs` (forgetting curve) and `rdfs-reasoner` design
  spaces are open in 2026 — premature freeze guarantees a v0.95 refactor.
- v0.90 deliverable: 2-3 property tests proving `MemoryStore` can host a
  realistic HNSW backend (no implementation, just the trait shape proof).

#### v0.100 WASM plugin host — KEEP-DEFER

- Seams already shipped: `WasmPluginHost`, `Sandbox`, `EventKind::Plugin` +
  `Wasm` reserved.
- Crates deferred: `nika-wasm-host` + `nika-sandbox` (L3).
- Rationale: WebAssembly Component Model Preview 3 (async, streams) is still
  in flux upstream as of 2026-04. wasmtime tracks; wasmer + wasmi lag. Stubbing
  before Preview 3 stabilises = rebreak guaranteed.
- Escape hatch: if a user demands plugins before v0.100, a Lua/Starlark/wasmi
  L1 crate ships under `unstable-*` (FCI-007 distribution flag).

### What "tout v0.100" actually means

The user proposal — "since we rewrite from scratch, plan everything for v0.100" —
is **half right**: every *seam* must be planned for v0.100 (already done). But
*implementations* follow YAGNI (Beck) + premature-abstraction caution (Metz):
abstraction without ≥3 concrete usages is debt, not investment.

## SOTA evidence (cross-project survey, swarm-3)

| Project | Future-feature pattern | Lesson |
|---|---|---|
| Bevy (ECS, ~50 crates) | No reservation; deliberate breakage every 3 months. | "Break early, break often" — community accepts. Not Nika's contract (forever-v0.x). |
| tokio (~10 crates) | `#[non_exhaustive]` + feature flags. `tokio-uring`, `tokio-console` = **separate crates**, not core refactors. | Isolate innovation in opt-in satellites. |
| Apollo Router (~40 crates) | Tower `Layer<S>` extension points + handshake-negotiated protocols (Federation 2). | Extension points = traits + negotiation, not frozen enums. |
| rust-analyzer (~70 crates) | Salsa query traits (open) + process boundaries (`proc-macro-srv`). | Process boundary buys forward-compat for free. |
| wasmtime | Component Model arrived as **sister crate** (`wasmtime-component`), not a core rewrite. | New major feature = new crate, not surgery. |

**Pattern dominant**: zero surveyed projects pre-stub crates for v+2 milestones.
All five rely on the same three levers Nika already uses (`#[non_exhaustive]`,
features, satellite crates).

## Consequences

**Positive:**

- v0.90 ships with a stable kernel API surface — Cortex and WASM can land
  later as additive admissions without touching admitted L0/L0.5 crates.
- Zero "stub crate debt" at v0.90 (no empty 8-satellite scaffolding).
- Upstream churn (Component Model Preview 3, fsrs design) does not block Nika.
- Any departure from a reserved seam (e.g. adding a non-additive field to
  `MemoryStore`) is a breaking change — caught by `cargo public-api` +
  `cargo semver-checks` (planned, P0 follow-up).

**Negative:**

- Reserved seams that turn out to be *wrong* (e.g. `MemoryStore` shape
  unsuitable for real backends) require a breaking trait change at v0.95.
  Mitigation: property tests in v0.90 (P0 follow-up) prove the seam can host
  a realistic backend before lock.
- Documentation cost: every reserved seam needs a sentence in
  `forward-compat-invariants.md` explaining the v-target and the escape hatch.

**Neutral:**

- Crate count target stays at 40-42 for v0.90, +9 Cortex at v0.95, +2 WASM at
  v0.100 = ~51-53 final. No change to ROADMAP.

## Escape hatch

Reserved kernel traits and reserved event sub-enums **remain modifiable until
their named v-target**. A reserved seam is not a stable contract — it is a
*shape commitment*. The day Cortex lands at v0.95, `MemoryStore`'s final shape
is locked; until then, breaking changes to the trait are allowed and tracked
as `affects_layers: [L0.5]` ADR amendments.

This must be documented in `docs/architecture/forward-compat-invariants.md`
under a new `§reserved-mutability` section (P1 follow-up).

## Related decisions

- **ADR-002** Forever-v0.x — context for "no v1.0 target".
- **ADR-006** Layered kernel ISP traits — defines what trait stubs reserve.
- **ADR-007** Forward-compat invariants — `#[non_exhaustive]` + `::new()` discipline.
- **ADR-014** Sealed kernel traits — prevents downstream impl, makes seam mutability safer.
- **ADR-020** WASM plugin boundary — names the v0.100 deferral target.
- **ADR-024** SOTA Rust patterns — survey context for this ADR.

🦋
