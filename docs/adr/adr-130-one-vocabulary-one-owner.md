---
id: ADR-130
title: "One vocabulary, one owner: the task-record twin dies, the resident's status projects the settlement, copied spellings are pinned"
status: accepted
date: "2026-09-04"
phase: "pre-1.0 · one door"
deciders: ["@ThibautMelen"]
tags: ["architecture", "events", "one-door", "vocabulary"]
affects_crates: ["nika-runtime-laws", "nika-runtime", "nika-serve", "nika-tui-core"]
affects_layers: ["L2", "L3", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-127", "ADR-128", "ADR-129"]
requires: ["ADR-128"]
enables: []
amends: ["ADR-127"]
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: ""
follow_ups: ["the TypeScript SDK's event and status types generated from the engine (never hand-typed)", "the seal's teardown outcome word"]
---

# ADR-130: One vocabulary, one owner

## Context

Three copies of one vocabulary survived ADR-128. `nika-runtime-laws::compat_record`
twinned `nika-dataflow`'s `TaskStatus` · `TerminalCause` · `TaskErrorRecord` ·
`TaskRecord` with `From` conversions "to preserve the historical source API"
— two enums with the same variants, a struct copied field by field, and a
conversion at every `RunOutcome`. The resident's `JobStatus` mapped the
execution disposition to its own words by hand. `nika-tui-core` folds the
journal with string literals for the frame kinds and depends on nothing
that owns them — a rename on either side would fail nowhere.

## Decision

1. **The twin dies.** `compat_record` is deleted; `nika_runtime::{TaskStatus,
   TerminalCause, TaskErrorRecord, TaskRecord, legal}` are re-exports of
   `nika-dataflow`, the owner. `RunOutcome::records` holds the owner's
   records; the conversion is gone.
2. **One mapping.** `JobStatus: From<RunState>` is the resident's only
   projection of a run state; `ExecutionDisposition: Into<RunState>` names
   the four states it can carry; `JobStatus::run_state` says which statuses
   are run states at all (`queued` · `running` · `interrupted` are the
   job's own — ownership and evidence, never a run state). The words are
   equal by construction and proven by a test.
3. **Copied spellings are pinned.** A crate that must not depend on the
   owner (the wasm-light `nika-tui-core`) may copy a spelling only under a
   test that compares the copy to the owner (`tests/wire_pins.rs` · a
   dev-dependency edge). The rule generalises to every repository: a
   contract is AUTHORITATIVE, GENERATED, or PINNED by a gate — never a
   hand-written duplicate nothing checks.

## Consequences

- Positive: one task vocabulary, one status projection, one pin per copy;
  `nika-runtime-laws` sheds a third of its lines.
- Negative: `nika_runtime::TaskStatus` and `TerminalCause` are now
  `#[non_exhaustive]` (the owner's contract): an exhaustive match outside
  the crate needs a wildcard arm.
- Follow-ups: the SDK's types (generated · ADR follows F9); the teardown's
  outcome word.

## Alternatives considered

- Keeping the twin for source compatibility: rejected — no external
  consumer exists (the runtime crates are `publish = false`), and a twin
  nothing needs is the drift the program exists to remove.
- A `nika-wire` crate: rejected as in ADR-128.
