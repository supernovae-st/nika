---
id: ADR-140
title: "nika-compile size-cap member split: the seats' doors ascend to nika-compile-cognition"
status: proposed
date: "2026-09-23"
phase: "pre-1.0 · compiler war room · V6 authoring"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "compile", "authoring"]
affects_crates: ["nika-compile", "nika-compile-reader", "nika-compile-cognition", "nika-onboard", "nika-cli-host"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-137", "ADR-138"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.121"
follow_ups: ["the `#[non_exhaustive]` ratchet on every outcome type the core exposes", "the COLD proposal decoder's own attestation once it lives in the member"]
---

# ADR-140: nika-compile size-cap member split — nika-compile-cognition

## Context

ADR-138 (2026-09-21) descended the frozen reader to `nika-compile-reader` and left
`nika-compile` at 9,778 production lines. Two days of the V6 authoring lane grew it back to
the 15,000 wall: the native door (a seat writes the `.nika`, the compiler judges it: parser ·
Check · fidelity laws · repairs), the knowledge door (the Foundry snapshot recalled per intent),
the revise door, the sketch door (the plan's W2: structure judged first, typed holes filled,
the document emitted), the observed world, the trigger contract, the typed transform verifier.
On 2026-09-23 the queue W2 → W8 lands at **14,979 / 15,000** after three descents of word
tables to asset files and to the reader (`nika-compile-reader` 14,572 / 15,000); a further
descent of prose and tables measured 6 lines. Both members of the unit are at their walls,
and every prescribed tranche that follows (W3 CallableContract, W6 ProjectContext/ProjectDelta,
the sketch door's hole judges) adds hundreds of lines to the same crate. The cap is a locked
maintainability budget, not advisory; the Diamond architecture audit named `nika-compile` the
God Crate.

## Decision

Per D-2026-07-09-N1 (a size-cap split is ONE architectural unit in TWO workspace members —
the ADR-110, ADR-115, ADR-137 and ADR-138 precedents), the seats' doors ascend to a new L4
member crate `nika-compile-cognition`, ABOVE the core: `nika-compile-cognition` depends on
`nika-compile` and on `nika-compile-reader`; `nika-compile` never depends back.

What ascends (measured on the tree of 2026-09-23, production lines): `cognition.rs` minus its
deterministic doors (~1,000), `cognition/native.rs` (~1,400), `cognition/sketch.rs` (~400),
`cognition/proposal.rs` (1,354), `predicate.rs` (458: it decodes the proposal shape the COLD
door owns), `cognition/transform.rs` (503, with the jaq dependency stack), `cognition/knowledge.rs`
(168), `cognition/backstops.rs` (61), `cognition/instructions.rs` (17), `decide.rs` (272: the
bounded decision seats), and `compose.rs` (983: the COLD candidates' composer, whose only caller
is the COLD door). About **6,600 lines**; `nika-compile` keeps the deterministic compiler at
~8,400: the request and outcome types, the exact skeletons, the support clauses, the HOT
admission (`cognition::hot`), the assembler and its laws, the bindings, the ledger, the edit
door, the record replay (`cognition::replay`), the preview wire.

What stays with the core beside its deterministic doors, measured on the edges: the native
door's RECORD half — a native record applied to a request (`native_apply`: the answers baked
into the recorded source, the gaps disposed of, the model seated, the candidate finished) and
its replay on an answer round — because the core's `replay` runs it with zero calls and no seat,
and the seat side finishes an accepted candidate through the same function. The core's doors
live in one module, `doors.rs`: the HOT admission (`admit_hot` · `lexical_rest_is_explicit`), the
record replay (`replay` · the native record's replay), the unresolved findings, the door records
(`record_route` · `record_retrieval` · `plan_record` · `record_ledger`) and `intent_sha256`.

What the core exposes for the member, as ONE stated module `nika_compile::surface`: the
outcome vocabulary (`initial` · `finding` · `question` · `finish` · `literal_answer` · the strict
`parse`), the door records and `native_apply`, `edit::literal_projection`, `edit_source::{emit,
emit_at}`, `ledger::Ledger`, `assemble::{assemble, unfed}`, `support::{assemble, resolve}`,
`trigger::{TriggerForm, bind_schedule, classify, note, requirement}`, and the request's input
shape (`Input` · `EditChange`, `#[non_exhaustive]` at the boundary). The request is the core's:
`CompileRequest`'s fields and `AuthoringPolicy`'s are public, read by the member, never
constructed by it (the builders stay). Every exposed function is documented as a contract of
the unit, not of the crate; nothing else of the core is read.

The moved code keeps its historical paths: the member's root binds the reader's modules and
the surface under the names the doors have always used (`crate::plan`, `crate::edit`,
`crate::trigger`, `crate::retrieve`, `crate::types`, …) through facade modules, so the ascent is
a move, not a rewrite (the diff inside the moved files is the ONE call `apply` → `native_apply`
and the import of the doors).

The public surface of the unit does not move: `nika-onboard` replaces its crate alias with a
`compile` facade module — `pub use nika_compile::*` beside `nika_compile_cognition::{
compile_with_cognition, compile_with_provider, Cognition, NoProvider, decide}` — at the paths
`nika-cli-host` reads today (`nika_onboard::compile::…`); the host changes nothing. The
compile suites that drive the doors stay beside the core, on a dev edge to the member (a
dev-dependency cycle cargo admits, a lateral L4→L4 edge the layering gate admits).

### Why this boundary, measured

The edges between the two regions on 2026-09-23:

| direction | edges |
|---|---:|
| the core → cognition | 3: the re-export of the two entry points; `replay` and `hot` (deterministic, stay); the native record's replay (`native::replay` → `native_apply`, stays with them) |
| cognition → the core | the outcome vocabulary (`finding` ×36, `question` ×6, `finish`, `literal_answer`, `CompileQuestion`), `edit::literal_projection` ×8, `retrieve` ×4, `trigger` ×5, `support::assemble` ×2, `intent_sha256` |
| cognition → the reader | `paths` ×20, `gates` ×10, `words` ×8, `rules` ×6, `plan`, `columns`, `shape`, `structure`, `unknowns`, `fidelity`, `sketch` — already public |

The deterministic doors that the core's `compile()` calls (`hot`, `replay`, the unresolved
findings) stay in the core; nothing the core runs without a seat leaves it.

## Consequences

### Positive

- `nika-compile` returns to ~8,400 production lines and `nika-compile-cognition` starts at
  ~6,600: the prescribed tranches land without a descent each.
- The seats' doors are one crate with one dependency stack (the providers' types, jaq, the
  pack): the deterministic compiler no longer links jaq.
- The boundary the arena measures (deterministic door vs seat doors) becomes a crate boundary.

### Negative

- A stated public surface of the core (~25 items in one module) that was `pub(crate)`: the
  ratchet (`#[non_exhaustive]` on the input enums, documented contracts) applies to each; the
  request's fields become readable outside the core.
- One more member in the workspace: the layer entry, a crate spec, a README and this ADR (the
  pre-commit `adr-coverage-new-crate` check refuses the crate without it). No public-api pin:
  the unit's crates are outside the coverage floor, as the reader is (the ratchet stays yellow
  until the unit is pinned as one).
- A dev-dependency cycle (`nika-compile` → dev → `nika-compile-cognition` → `nika-compile`):
  cargo admits it; a suite of the core that drives a seat's door reads the member.

### Neutral

- `nika-onboard` keeps re-exporting the unit; no caller outside the unit changes.

## Evidence / Affected code

- `scripts/ci/check-crate-size.sh` on 2026-09-23 (the queue W2 → W8 applied, formatted):
  nika-compile 14,979 · nika-compile-reader 14,572.
- The split executed mechanically (`patch_adr140.py`, replayable on any head of the branch) on
  the tree of 2026-09-23 14:5xZ (laws 20–22, the sketch door, the revise replay in): the official
  counter reads nika-compile 9,002 · nika-compile-cognition 6,119 · nika-compile-reader 14,614.
- The edge counts above: `grep -o "crate::…"` over `cognition.rs` and `cognition/*.rs`.
- the lane's coordination log, 2026-09-23 12:0xZ: « nothing else lands in nika-compile
  before the crate split ».

## Alternatives considered

### Alt A -- descend the COLD proposal path below the core (`nika-compile-cold`)

The proposal decoder and merger write findings into `CompileOutcome`: a member BELOW the core
cannot name it without a third crate for the outcome types (`nika-compile-types`) — two new
crates for the same relief.

### Alt B -- keep descending tables and prose to assets

Measured 2026-09-23: the three descents bought ~600 lines once; the fourth bought 6. The
tables are gone; what remains is logic.

### Alt C -- raise the wall for `nika-compile`

The wall is the Diamond invariant every member honours (`nika-invariants.md`); the audit named
this crate the reason it exists.

## Related

- ADR-137 (the Compile core out of nika-onboard), ADR-138 (the reader), D-2026-07-09-N1.

## Notes

Written while the V6 queue W2 → W8 lands (2026-09-23); to be executed as the lane's first
structural tranche after push #21, behind the shared build lock.
