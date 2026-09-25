---
id: ADR-140
title: "nika-compile size-cap member split: the seats' doors ascend to nika-compile-cognition"
status: accepted
date: "2026-09-23"
phase: "pre-1.0 · V7 architectural consolidation"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "compile", "authoring"]
affects_crates: ["nika-compile", "nika-compile-reader", "nika-compile-fidelity", "nika-compile-cognition", "nika-onboard", "nika-cli-host"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-137", "ADR-138", "ADR-141"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.121"
follow_ups: ["mutation and property attestations inherited from the compile unit", "the existing public-type non_exhaustive ratchet", "canonical unit public API qualification"]
---

# ADR-140: nika-compile size-cap member split — nika-compile-cognition

## Context

ADR-137 extracted the Compile core from `nika-onboard`; ADR-138 extracted the frozen
reader and typed plan. Native authoring, revisions, sketch authoring, knowledge recall,
verified transforms and bounded decision seats subsequently grew the core back to its
15,000-production-line limit. Descending more text tables no longer provides meaningful
space. The original design was proposed on 2026-09-23; the 2026-09-24 authorized V7
architectural consolidation implements that boundary on the composed source.

## Decision

Under D-2026-07-09-N1, this is another member of the already-admitted `nika-onboard`
architectural unit, following ADR-110, ADR-115, ADR-137 and ADR-138. It is a size-cap
member split, not a new independent architectural admission.

The seat cognition doors ascend into L4 `nika-compile-cognition`. Its production edges
point to `nika-compile`, `nika-compile-reader` and `nika-compile-fidelity` (ADR-141).
The deterministic core never depends back on cognition in production. All edges between
these members remain lateral L4 edges; the production graph is acyclic.

The following code moves, with its complete unit tests and assets:

- COLD orchestration, proposal decoding, typed predicates and the finite composer;
- bounded decision seats and verified transform authoring;
- native and sketch authoring, native answer decoding, repair rounds and knowledge recall.

The core keeps requests and outcomes, exact skeletons, bounded support grammar, HOT
admission, assembly and fidelity checks, bindings, ledger, edits, materialization and the
machine wire. `doors.rs` owns deterministic plan replay and the native record half:
answer baking, gap handling, model seating, trigger handling and candidate finishing.
Both cognition's accepted candidate and a zero-call answer round use the same
`native_apply`; no replay policy is duplicated in the member.

### Shared surface

`nika_compile::surface` exposes the core contracts the member needs: outcome helpers,
strict parsing, literal projection, the opaque ledger, input enums, assembly/support
entry points, deterministic admission, replay and provenance records. Native trigger and
constant-edit machinery stays private to core because the native record half stays there.

Additional dependencies identified on the composed source are cut explicitly:

- The spec pin reader lives once in the core surface. Deterministic outcome provenance
  and cognition's knowledge identity use it, preserving the same embedded `SPEC_PIN`.
- The intent digest and knowledge receipts share the existing SHA-256 helper through
  the core surface; its implementation moves once with no change in digest semantics.
- Bindings call `text::exact_excerpt` directly, the existing reader helper that cognition
  previously re-exported. There is no new policy implementation or upward edge.

The `LINES` and `SELECT_BY_FIELD` expression laws are exposed through the surface so the
moved knowledge/transform tests continue to compare against the assembler's actual laws.
They are not copied into cognition. Candidate read-back, fidelity and sketch use the
ADR-141 member above Reader; no Reader logic changes in this split.

Request and authoring-policy fields become public, so external Rust callers can read
and mutate them after construction. Existing builders remain available and enforce their
normalization; the seat entry points still validate token/time/model bounds and clamp
sample/repair counts. `Input` and `EditChange` gain
`#[non_exhaustive]`. `AuthoringReceipt::new` and `Hit::new` replace cross-crate struct
literals. The exposed ledger and support plan are non-exhaustive opaque values produced
by extraction/default and resolution. Public admission/resolution wrappers use typed,
non-exhaustive errors retaining the original messages and ordering; internal core
implementations and error handling remain unchanged. Existing public-type ratchet debt
elsewhere in the unit is not claimed resolved.

### Compatibility and dependencies

`nika-onboard` combines the core and cognition exports in its `compile` facade. Existing
`nika_onboard::compile::{compile_with_cognition, compile_with_provider, Cognition,
NoProvider, decide, ...}` paths remain available. This preserves existing explicit
facade imports. Core adds `surface` and helper APIs; the facade explicitly names its
exports so those member helpers do not become facade APIs. The request/policy types
still gain public fields, and the receipt and retrieval types gain constructors.
Direct imports of those seat exports from `nika_compile` must move to
`nika_compile_cognition` or the onboarding facade; they are source-breaking at the direct
core path. The core cannot re-export cognition without reversing the production dependency.
Hosts continue to inject their selected
providers, admission context and explicit model choices through the same interfaces.

The complete compiler integration suites remain beside core and import seat entry points
from cognition through a development dependency. This development cycle is distinct from
the acyclic production graph. Kernel and Tokio move to core's development dependencies;
the cognition member owns the provider, timeout and jaq/capability stack. No external
dependency is introduced. `Cargo.lock` and canonical public API generation are integration
outputs and must be regenerated and checked on the composed workspace.

## Consequences

- The deterministic core and seat orchestration each regain substantial space below
  the existing production-LOC limit, without deleting tests or adding lint exemptions.
- The core has an explicit shared surface, whose contracts must evolve compatibly.
- The compiler unit spans more workspace members; admission remains inherited from the
  same unit, while mutation/property attestations remain pending and tracked.
- The former direct seat exports from `nika_compile` move to `nika_compile_cognition`;
  the supported `nika_onboard::compile` facade preserves those paths for consumers.

## Evidence and qualification

The implementation is an adaptation of the frozen ADR-140 extraction to the composed
source, preserving later fidelity, native-answer, knowledge and inference-admission work.
Static source comparison, the unchanged production counter and pinned Rust 1.91 formatting
are the worker checks. They do not establish compilation or runtime behavior.

Integration must run the compiler members' complete suites, affected onboarding/host/
Serve/Session checks, strict clippy, docs, formatting, repository hygiene, layering and
size gates, and canonical API validation. This ADR records the accepted and implemented
boundary; it does not assert those checks or the pending mutation/property attestations
passed. Historical checks of the earlier frozen attempt are not evidence for this tree.

## Alternatives considered

- A COLD member below the core would require another outcome-types crate to avoid a
  cycle: its decoder and composer write core outcomes.
- More text-table descents do not provide the space needed by the prescribed work.
- Raising the size limit would abandon the locked maintainability budget.

## Related

ADR-137, ADR-138, ADR-141 and D-2026-07-09-N1.
