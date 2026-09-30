---
id: ADR-142
title: "nika-compile size-cap member split: the trigger reading ascends to nika-compile-trigger"
status: accepted
date: "2026-09-30"
phase: "pre-1.0 · compiler architecture"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "compile"]
affects_crates: ["nika-compile", "nika-compile-trigger", "nika-compile-reader"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-137", "ADR-138", "ADR-140", "ADR-141"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.121"
follow_ups: ["mutation and property attestations of the member, tracked with the unit's pending evidence", "a canonical Linux public API snapshot of the member and of the reader after the tables left it"]
---

# ADR-142: nika-compile size-cap member split — nika-compile-trigger

## Context

On 2026-09-30 a measurement of the development head put `nika-compile` at **14,949 prod LOC** and
`nika-compile-reader` at **14,986**, against the locked 15,000 invariant, while the compiler
still owed the lowering of the two cadence forms the arming grammar holds beyond plain fields
(the last day of a month, `M H L * *`, and an interval of weeks from its start date,
`every N weeks from DATE HH:MM`) and the reader owed the heads that keep those words whole. Further
compiler validation rules needed the same limited capacity.

## Decision

Per **D-2026-07-09-N1** (a size-cap split is ONE architectural unit in several workspace
members — the ADR-110, ADR-115, ADR-137, ADR-138 and ADR-141 precedents), the trigger reading
ascends from `nika-compile` to a new L4 member crate `nika-compile-trigger`, placed ABOVE the
reader and BELOW `nika-compile`: `nika-compile → nika-compile-trigger → nika-compile-reader`,
never back. `nika-compile`'s `trigger` module keeps the binding (the requirement, the cadence,
zone, policy and ceiling questions, the note) and re-exports the three items the rest of the
compiler reads at their historical paths:

```rust
pub(super) use nika_compile_trigger::{TriggerForm, arriving, classify};
```

### Why this boundary, measured

| direction | edges |
|---|---|
| the reader → the member | **0** |
| the member → the reader | `hot::fold` · `shape::led_by_quantifier` · `words::{day_part_compound, recurrence}` · `trigger_words::{DAILY, WEEKDAYS, WEEKLY, MONTHLY, HOURLY, MINUTELY}` |
| `nika-compile` → the member | the reading functions in `trigger.rs`, `words::{MANUAL, WEBHOOK}`, and `classify`, `arriving`, `TriggerForm` through the `trigger` re-export (the assembler, the doors, the bindings, the ledger) |
| outside the unit → the member | **0** |

The reading is pure over folded words and needs nothing of the compiler: no request, no plan,
no outcome, no question. Eleven reader tables (`AT`, `BETWEEN`, `TIME_UNITS`, `NAMED_TIMES`,
`WEBHOOK`, `SEQUENCE_HEADS`, `EVENT_HEADS`, `COMPLETION_WORDS`, `TIME_WORDS`, `ARRIVAL_WORDS`,
`MANUAL`) were read by that reading alone (no reader file reads them, no crate outside the unit
reads `trigger_words`): they ascend with it. The cadence tables the reader reads itself stay.

### What travels with the member

- `crates/nika-compile/src/trigger.rs`: `phrase_words`, `stated_cadence`, `cadence`,
  `time_of_day`, `clock` (→ `reading.rs`) and the clause form section (`TriggerForm`,
  `padded`, `words`, `clock_time`, `arriving`, `classify` → `form.rs`), bytes unchanged but for
  their visibility and imports.
- `crates/nika-compile/src/trigger/{multiple.rs, schedule.rs}` whole, with their unit tests.
- The eleven tables above from `crates/nika-compile-reader/src/trigger_words.rs` (→ `words.rs`).
- No dependency beyond the reader.

## Consequences

- `nika-compile` loses about 480 prod lines and the reader about 310; both regain room under
  the cap for the owed cadence lowering and heads and for the next laws.
- The reader's public API loses the eleven tables (no consumer outside the unit read them);
  a direct import of `nika_compile_reader::trigger_words::{AT, …, MANUAL}` must move to
  `nika_compile_trigger::words`. This is a Rust source break at the old reader paths.
- The member inherits the admission of its unit (the ADR-115, ADR-137, ADR-138 and ADR-141
  posture); mutation and property attestations stay pending evidence, tracked, never claimed.
- `nika-onboard` → `nika-compile` → {`nika-compile-trigger`, `nika-compile-fidelity`} →
  `nika-compile-reader` is ONE architectural unit for the count invariant; the lateral edges
  stay acyclic.

## Alternatives considered

### Alt A — the cadence lowering alone in a new member

It would leave the reading it depends on in `nika-compile` and split one concept across two
crates, buying about 230 lines and nothing for the reader.

### Alt B — the reading below the reader

The reading folds with `hot::fold` and reads the reader's cadence tables and quantifier heads:
the reader would depend on a member that depends on it, or keep a second fold.

### Alt C — raise the cap

The 15,000 invariant is a locked maintainability budget, not advisory.

## Related

- ADR-137 (the Compile core out of nika-onboard), ADR-138 (the reader), ADR-140 (the seats'
  doors), ADR-141 (the candidate laws), D-2026-07-09-N1.
