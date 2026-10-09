---
id: ADR-149
title: "nika-compile-fidelity size-cap member split: the behavioural contract and its judge descend to nika-compile-behavior"
status: accepted
date: "2026-10-09"
phase: "pre-1.0 · compiler architecture"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "compile"]
affects_crates: ["nika-compile-fidelity", "nika-compile-behavior"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-003", "ADR-137", "ADR-138", "ADR-141", "ADR-145", "ADR-146"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.123"
follow_ups: ["the mutation run of the member's 1,271 listed mutants (ADR-003 Gate 5), its kill ratio recorded in the member's spec", "the Linux public API job confirms the member's snapshot and the owner paths in the fidelity, seats and onboarding snapshots, written from a macOS render", "documentation of the 81 public fields and variants the member carried from fidelity without a doc of their own"]
---

# ADR-149: nika-compile-fidelity size-cap member split — nika-compile-behavior

## Context

At `55ad41642` (2026-10-09) `nika-compile-fidelity` (ADR-141) measures **15,050 prod LOC**
against the 15,000 wall: `scripts/ci/prod-loc.py` over the gate's own file set, Rust plus the
two embedded jq laws `decimal/{order, arithmetic}.jq`. The cap is a locked maintainability
budget, not advisory, and the candidate-law work then under way (the composition endpoint
witness and its successor) needs room inside it. Moving candidate laws back into the reader
would undo ADR-141: the reader is the frozen reading of a request, and those laws judge a
candidate.

The behavioural contract is a family apart in that crate. `behavior.rs` and `behavior/` (13
production files, 6,866 prod LOC; 13 unit-test files, 110 tests) state the contract a request
implies, independent of any candidate, and judge the evidence a host hands over after a round
of rehearsals. It reads the reader, `serde_json`, `csv` and `sha2`, and nothing of the laws,
the sketch or the candidate plan but one scalar classifier, `fidelity::instant_shape`.

## Decision

Per **D-2026-07-09-N1** (a size-cap split is ONE architectural unit in several workspace
members, the ADR-137, ADR-138, ADR-141, ADR-145 and ADR-146 precedents), the family descends
from `nika-compile-fidelity` to a new L4 member crate `nika-compile-behavior`, placed BELOW the
candidate laws and ABOVE the reader: `nika-compile-fidelity → nika-compile-behavior →
nika-compile-reader`, never back.

- `crates/nika-compile-fidelity/src/behavior.rs` and its `behavior/` subtree move to
  `crates/nika-compile-behavior/src/` (`git mv`) under `pub mod behavior`, bytes unchanged but
  for the two imports of the classifier (`crate::fidelity::instant_shape` →
  `crate::instant_shape`).
- `instant_shape` (std only: the form and the offset of a date-time text) leaves
  `fidelity/instants.rs` with its two unit tests for the member's private `instants` module,
  byte for byte, exported as `nika_compile_behavior::instant_shape`. Law 25 (the orders an
  expression makes, the evidence it reads, its diagnostic), `Diagnostic` and the record-scope
  walk depend on fidelity and stay there; Law 25 imports the one classifier.
- Fidelity keeps every public path. `#[doc(inline)] pub use nika_compile_behavior::behavior;`
  replaces `pub mod behavior;`, and `fidelity::instant_shape` is a `#[doc(inline)] pub use` of
  the member's function; both name the very same items, never copies. `nika-compile`,
  `nika-compile-cognition`, `nika-compile-seats` and `nika-onboard` keep their imports.
  `crates/nika-compile-fidelity/tests/behavior_contract.rs` (three external-consumer tests)
  stays at the compatibility path, and `tests/behavior_reexport.rs` proves that the two paths
  name one type and one function.
- `sketch::record::contract_projection` matched `behavior::Presence` exhaustively, legal only
  inside the defining crate. Across the boundary the `#[non_exhaustive]` enum takes a wildcard
  arm that records `unknown`, never a kind it knows and never `required` (the ADR-146 posture);
  the seven known kinds keep their words, pinned by a unit test.
- `csv` leaves fidelity's manifest with the CSV reading. `sha2` stays: the kept, semantic and
  source-revision records and the observation basis bind digests.

### Why this boundary, measured

Edges on the tree of `55ad41642`, production code only (the shared `rs_prod_files` set):

| direction | edges |
|---|---|
| the family → the rest of fidelity | **2** · `fidelity::instant_shape` in `behavior/values.rs` and `behavior/evaluate.rs`, whose definition moves with them |
| the rest of fidelity → the family | `sketch/record.rs` · `behavior::{Contract, Presence, Requirement, contract_of_request}`, the request basis and its partial contract projection |
| the family → other crates | `nika-compile-reader` (plan, rules, aggregates, HOT, gates, lexicon, paths, shape, structure) · `serde_json` · `csv` · `sha2` |
| the classifier's other readers | `fidelity/instants.rs` (Law 25) · `observed/temporal.rs`, through `fidelity::instant_shape` |
| outside the unit → the family | through `nika_compile_fidelity::behavior` only: `nika-compile` (`assemble/read`), `nika-compile-cognition` (`rehearsal`, `sketch/evidence`), `nika-compile-seats` (`rehearse/*`), `nika-onboard` (`compile/copy` and its children, whose public allowance, qualification and settlement carry `Usage` and `Limits`) |

## Consequences

- `nika-compile-fidelity` measures **8,145** prod LOC (from 15,050) and `nika-compile-behavior`
  **6,962** (the moved 6,866, the classifier module's 61 and the crate root's 35), measured
  with `scripts/ci/prod-loc.py` over the gate's file set; the reader is unchanged.
- The unit tests move with their files: the family's 110 and the classifier's two. Fidelity's
  integration suites are unchanged; `behavior_reexport.rs` adds two, and the projection test one.
- A type's run-time name (`std::any::type_name`) now names the member; derived `Debug` output
  and every record carry no path and are unchanged.
- Fidelity's public API snapshot loses the module's section for one `pub use` line, and its
  `instant_shape` row becomes a `pub use`. Signatures elsewhere that carry the module's types
  name the member: fidelity's `contract_projection`, the seats' rehearsal record and judged run,
  onboarding's copy allowance, qualification and settlement. No item is removed.
- The member holds no `thiserror` enum: the error one-voice vector has no row to move.
- No judgment, contract, record, wire, budget or Stop behaviour changes. The move does not by
  itself qualify any candidate law or any business result.
- The member inherits the admission of its unit (D-2026-07-09-N1, the ADR-137, ADR-141,
  ADR-145 and ADR-146 posture). Its other ADR-003 gates are measured for it and recorded in its
  spec, none inherited as a pass. Mutation (Gate 5) stays pending evidence, tracked in
  `follow_ups`, never claimed: `cargo mutants --list` names 1,271 mutants, run in a separate
  slot.
- The workspace registries name it: the members, the layer table, the crate spec, the public API
  coverage floor, the typos exclusion its multilingual fixtures need and the generated status
  blocks.

## Alternatives considered

### Alt A — the candidate laws back into the reader

It undoes ADR-141: the reader is the frozen reading of a request and a candidate law judges a
candidate. The reader also stands at 14,817 of its own wall.

### Alt B — the classifier into the reader

It reads no plan and no intent; ADR-141 keeps the reader for the plan-level structural laws,
and the reader would gain an edit for a function its member above reads most.

### Alt C — all of `fidelity/instants.rs` with the family

Law 25's orders, evidence and diagnostic read fidelity's `Diagnostic` and `record_scope`: a
cycle, or Law 25 cut from the walk that finds its orders.

### Alt D — the member's root as the module (`pub use nika_compile_behavior as behavior`)

Every moved path would shift (`super::` and `crate::behavior::` in the files and tests), and the
classifier, public at the member's root, would also appear under fidelity's `behavior` path,
widening a compatibility facade. `pub mod behavior` keeps the files' bytes and the old path's
exact item set.

### Alt E — raise the cap

The 15,000 invariant is a locked maintainability budget, not advisory.

## Decision record

Accepted on 2026-10-09 under the decider's standing mandate for the Nika architecture
convergence: structural and integration fixes proceed without a further ask, and this
extraction was the approved path past fidelity's size wall. The coordinator of the split relayed
that mandate after the admission review asked for an explicit status. Acceptance covers the
boundary and its compatibility paths, and claims no gate: the mutation run (Gate 5) stays
pending evidence, and a miss against the 90% floor is a fix, never a pass.

## Related

- ADR-141 (the candidate laws, where the family was added), ADR-146 (the re-export precedent),
  ADR-137, ADR-138, ADR-145, ADR-003 (admission), D-2026-07-09-N1.
