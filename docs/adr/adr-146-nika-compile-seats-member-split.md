---
id: ADR-146
title: "nika-compile-cognition size-cap member split: the decision seats and the rehearsal port descend to nika-compile-seats"
status: accepted
date: "2026-10-07"
phase: "pre-1.0 · compiler architecture"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "compile"]
affects_crates: ["nika-compile-cognition", "nika-compile-seats"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-137", "ADR-140", "ADR-141", "ADR-142", "ADR-144", "ADR-145"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.123"
follow_ups: ["mutation and property attestations of the member, tracked with the unit's pending evidence", "the Linux public API job confirms the member's snapshot and the platform rows kept in the onboarding and host snapshots, written from a macOS render"]
---

# ADR-146: nika-compile-cognition size-cap member split — nika-compile-seats

## Context

At the integration head `75073eb0c` (2026-10-07) `nika-compile-cognition` (ADR-140) measures
**16,632 prod LOC** against the 15,000 wall (`scripts/ci/check-crate-size.sh`, the gate's own
counter), and still **16,346** once the clause readings ascended to `nika-compile-clauses`
(ADR-145). The cap is a locked maintainability budget, not advisory.

Two modules of the seats' doors stand apart: `decide` (the bounded decision seats of the WARM
strategy: a closed choice among admissible options or NONE, a provider seated through a
JSON-schema enum, the answer revalidated and recorded) and `rehearse` (the rehearsal port: a host
runs a candidate in a safe room built from the observed world and reports what the run did, mapped
to the behavioural judge's run). They are the two capabilities a host lends a preparation; the
doors read them, and they read nothing of the doors but the two reasoning helpers of the
authoring receipt (`effort`, `reasoning_record`) the decision call shares.

## Decision

Per **D-2026-07-09-N1** (a size-cap split is ONE architectural unit in several workspace members,
the ADR-137, ADR-140, ADR-141, ADR-142, ADR-144 and ADR-145 precedents), the two modules descend
from `nika-compile-cognition` to a new L4 member crate `nika-compile-seats`, placed BELOW the
seats' doors: `nika-compile-cognition → nika-compile-seats → nika-compile`, never back.

- `crates/nika-compile-cognition/src/decide.rs` with `decide/answer_tests.rs`, and
  `rehearse.rs` with `rehearse/{judged.rs, judged_tests.rs, observed.rs}`, move to
  `crates/nika-compile-seats/src/` (`git mv`), bytes unchanged but for the paths and the
  visibilities the move changes.
- `effort` and `reasoning_record` leave `cognition/receipt.rs` for the member's `reasoning`
  module; the doors keep calling them at `crate::cognition::{effort, reasoning_record}`.
- The cognition keeps every public path: `pub use nika_compile_seats::{decide, rehearse};`
  (`#[doc(inline)]`) replaces the two `pub mod` lines, and `nika-onboard` re-exports them from
  there as before. `nika-onboard`, `nika-session`, `nika-cli-host`, `nika-cli` and `nika-serve`
  compile without an edit; `crates/nika-compile-cognition/tests/seats_reexport.rs` compiles
  against the cognition paths as an external consumer.

### Why this boundary, measured

Edges on the tree of `75073eb0c`, production code only (the shared `rs_prod_files` set):

| direction | edges |
|---|---|
| the two modules → the rest of the cognition | **2** · `cognition::{effort, reasoning_record}`, which move with them |
| the rest of the cognition → the two modules | `decide::{ChoiceOption, ChoiceQuestion, ChoiceAnswer, DecisionError, DecisionSeat, NONE_OPTION}` and five crate-private items (below) · `rehearse::{Rehearse, RehearsalReport, Rehearsal, Attempt, judged_run, FinalState, Held, RecordedCause, Refusal}` |
| the two modules → other crates | `nika-compile` (`AuthoringReasoning`) · `nika-kernel` (the provider seam) · `nika-compile-fidelity` (`behavior`) · `serde_json` · `thiserror` · `tokio` |
| outside the unit → the two modules | through `nika_compile_cognition::{decide, rehearse}` and `nika_onboard::compile::{decide, rehearse}` only: `nika-onboard`, `nika-session`, `nika-cli-host`, `nika-cli` |

The five crate-private items the doors read become public items of the member, the crate
boundary leaving no narrower visibility: `decide::{closed_choice, answer_text, decoded, admit,
record}`, with `reasoning::{effort, reasoning_record}`. Each is documented, `#[must_use]` where it
returns a plain value, with its `# Errors` where it returns a `Result`.

## Consequences

- `nika-compile-cognition` measures **14,887** prod LOC (from 16,346 after ADR-145) and
  `nika-compile-seats` **1,523**, the gate's own counter.
- The rehearsal's enums stay `#[non_exhaustive]`. Across the member boundary the doors' matches on
  them take a wildcard arm that decides nothing new: the rehearsal record names an unknown kind
  `unknown`, never one of the known kinds, and the rehearsal decision stops on an outcome it does
  not read, never proceeds.
- The library tests move with their files (`decide`'s three answer tests and `rehearse`'s
  sixteen judged tests); the cognition's integration suites are unchanged and
  `seats_reexport.rs` adds two.
- A type's run-time name (`std::any::type_name`) now names the member; derived `Debug` output and
  the provenance records carry no path and are unchanged.
- The cognition's public API snapshot loses the two modules' sections, which become `pub use`
  lines; signatures elsewhere that carry their types name `nika_compile_seats::…` (the
  onboarding surface and `nika-cli-host`).
- The member holds no `thiserror` enum (`DecisionError` is a struct): the error one-voice vector
  has no row to move.
- The member inherits the admission of its unit (the ADR-137, ADR-140, ADR-141, ADR-142 and
  ADR-145 posture); mutation and property attestations stay pending evidence, tracked, never
  claimed.
- The workspace registries name it: the members, the layer table, the crate spec, the public API
  coverage floor and the generated status blocks.

## Alternatives considered

### Alt A — the verifier (`cognition/verify`) as the member

It is the doors' largest reader of the decision seat, but it reads the doors themselves (the
policy, the authoring call, the outcome, the knowledge reference, the settle): a member below the
doors would depend on them.

### Alt B — the proposal merge (`cognition/proposal`)

It reads the core's surface, the plan and the backstops of the COLD door; it would carry half the
doors with it.

### Alt C — raise the cap

The 15,000 invariant is a locked maintainability budget, not advisory.

## Related

- ADR-140 (the seats' doors), ADR-145 (the clause readings), ADR-144 (the session's change set,
  the re-export precedent), ADR-137, ADR-141, ADR-142, D-2026-07-09-N1.
