---
id: ADR-152
title: "nika-compile-cognition size cap: the authority over a seat's requests descends to nika-compile-seats"
status: accepted
date: "2026-10-09"
phase: "pre-1.0 · compiler architecture"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "size-cap", "compile"]
affects_crates: ["nika-compile-cognition", "nika-compile-seats"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-140", "ADR-144", "ADR-145", "ADR-146", "ADR-147"]
requires: []
enables: []
amends: ["ADR-146"]
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.123"
follow_ups: ["the public API snapshots of both members are rendered again by the public API job: the cognition's authority section becomes one `pub use` line, the member gains the section"]
---

# ADR-152: the authority over a seat's requests descends to nika-compile-seats

## Context

At `5167aaf5d` (2026-10-09) `nika-compile-cognition` measures **14,991 prod LOC** against the
15,000 wall (`scripts/ci/check-crate-size.sh`, the gate's own counter). The document door needs
a provenance channel for the selections an author states (a public source the request names, a
source chosen within a delegation, a routine new output): its answer, judgment and record grow by
a few dozen lines, which the wall cannot hold.

`authority.rs` stands apart from the seats' doors. It resolves the request bound a door states
against what its caller typed (`Authority`, `Typed`, `Door`, `Refusal`), re-exports the provider
layer's counters that enforce it (`Envelope`, `Seat`, `Wire`), and states the request-independent
accounting a receipt keeps (`worst_case_of`, `least_requests`, `recovery_requests`,
`usage_complete`, the deprecated historical `worst_case`). It reads nothing of the doors but one
constant of the historical estimate (`WHOLE_QUESTIONS`, 2), used by nothing else.

## Decision

Per D-2026-07-09-N1 and the ADR-146 precedent, `authority` descends whole to the member below the
seats' doors: `crates/nika-compile-cognition/src/authority.rs` moves to
`crates/nika-compile-seats/src/authority.rs` (`git mv`), bytes unchanged but for its paths:
`NativeMode` is read from `nika-compile`, and the historical constant moves with the only code
that reads it. The member gains the production edge `nika-compile-seats → nika-providers` (a
downward L4 → L1.5 edge) for the counters it re-exports.

The cognition keeps the public path: `pub use nika_compile_seats::authority;` (`#[doc(inline)]`)
replaces `pub mod authority;`, and `nika_onboard::compile::authority` re-exports it from there as
before. `nika-onboard`, `nika-session`, `nika-cli-host`, `nika-serve` and the compile suites
compile without an edit; `crates/nika-compile-cognition/tests/authority_reexport.rs` compiles
against the cognition path as an external consumer.

## Consequences

- `nika-compile-cognition` measures **14,565** prod LOC and `nika-compile-seats` **12,666**
  (the gate's own counter, on the working tree of this change).
- The library tests move with the file; behaviour, records and receipts are unchanged.
- A type's run-time name (`std::any::type_name`) now names the member; derived `Debug` output and
  the receipts carry no path.
- The public API snapshots change shape only: the cognition's `authority` section becomes a
  `pub use` line, the member's snapshot gains it.
- The member inherits the admission of its unit (the ADR-146 posture); its mutation and property
  attestations stay pending evidence, tracked, never claimed.

## Alternatives considered

- **The forensic summary (`cognition/forensic.rs`, 502 LOC)** reads the doors' knowledge record
  and route words: a member below the doors would depend on them.
- **The answer decoder (`cognition/native/decode.rs`)** carries the door's talk and journal.
- **Raise the cap**: the 15,000 invariant is a locked maintainability budget.

## Related

- ADR-146 (the member and its re-export precedent), ADR-140 (the seats' doors), ADR-144,
  ADR-145, ADR-147, D-2026-07-09-N1.
