---
id: ADR-128
title: "The run's settlement is built once by the runtime and projected by every door"
status: accepted
date: "2026-09-04"
phase: "pre-1.0 · one door"
deciders: ["@ThibautMelen"]
tags: ["architecture", "runtime", "events", "one-door", "settlement"]
affects_crates: ["nika-event", "nika-runtime", "nika-cli-host", "nika-cli", "nika-service-execution", "nika-trace", "nika-dap"]
affects_layers: ["L0", "L3", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-099", "ADR-122", "ADR-123", "ADR-125", "ADR-127", "ADR-129", "ADR-130", "ADR-131"]
requires: ["ADR-127"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: ["NIKA-1704", "NIKA-VAR-009"]
timeline: ""
follow_ups: ["one status vocabulary published in the serve OpenAPI (JobStatus as a projection · #1443)", "trace ls liveness and trace verify's INCOMPLETE exit (#1442)", "nika-tui-core reads the settlement from the terminal frame"]
---

# ADR-128: The run's settlement is built once by the runtime and projected by every door

## Context

On `main` at `2dd1996c` the same run's terminal truth was folded seven
times, in seven places, with two vocabularies. The runtime wrote a
`status` word on the terminal frame (`completed` · `failed` ·
`cancelled`) beside a task tally and the cost fields (wave 7.e ·
#1247); the CLI's `run_settled` envelope derived `succeeded` · `failed`
· `paused` · `cancelled` from the outcome's booleans (`settled_status`);
the CLI's exit code derived from the same booleans a second time; the
service boundary (`ServiceExecutionStatus::from_runtime`) folded them a
third time with its own budget rule; the resident's `JobStatus` folded
that; `trace outputs --json` folded the journal's task frames into a run
`state` that included an invented `recovered` state; `nika-dap`'s
`TraceState` printed `completed` on `trace ls`. `nika-tui-core` and the
TypeScript SDK folded the frames again on their side.

Two doors could — and did (#1443) — report different words for one run.
A future surface (the native TUI · a remote session host · Telegram)
would have had to reimplement the fold to say what a run settled as.
The one-door program's law is « many doors, one judgment, one execution,
one evidence trail »: a settlement is engine truth, computed once.

## Decision

1. **One type, at L0.** `nika_event::settlement::RunSettlement` carries
   the run's state (`RunState` · `succeeded` · `failed` · `paused` ·
   `cancelled`), WHY (`RunCause` · `normal` · `human_gate` ·
   `task_failed` · `output_contract` · `budget` · `operator` ·
   `refused`), the elapsed time, the task tally (`TaskTally` · total ·
   ok · failed · recovered · skipped · cancelled · never_started), the
   spend (`Spend` · the metered total when any leaf was priced ·
   priced/unpriced counts · a `CostQualifier` · `priced` ·
   `partially_priced` · `unpriced` · `unmetered` · the pricing snapshot
   · the per-source attribution) and the failure named
   (`SettlementError` · code · message · the task, or none for a
   run-level cause).
2. **One writer.** `nika-runtime` builds the settlement exactly once, at
   the boundary that ends the run (`settlement::settle_run`): the normal
   close (`finalize_outputs`), the operator's cancellation and the budget
   stop (`abort_unran`), the human gate (`emit_paused`). The terminal
   frame is written by `emit_terminal` from the settlement — its kind IS
   the state's kind, its flat fields ARE `RunSettlement::fields()`. The
   outcome carries the settlement (`RunOutcome::settlement`); `ok` ·
   `cancelled` · `budget_exceeded` and the cost fields are its
   projections, set together by `with_settlement`.
3. **One reader.** `RunSettlement::from_event` reads a terminal frame
   back; `from_events` finds a journal's last terminal. The kind names
   the state (a `status` word an older engine wrote is ignored); the
   tally is `None` on a frame that predates it (absent, never zero); the
   cause falls back to the least the state proves.
4. **Every door projects.** The CLI's exit code is a `match` on the
   state; `run_settled` flattens the settlement (`status` · `cause` ·
   `elapsed_ms` · `tasks` · `spend` · `error`) beside its locators; a
   launch refusal settles as `failed` · `refused` with its code
   (`refusal_settlement`). The service boundary's status is a `match` on
   the state, its error the settlement's. `trace outputs --json` reads
   the terminal frame through the one reader and carries the
   `settlement` object; `trace ls` prints the state word. The invented
   `recovered` run state is gone: recovery is a tally on the settlement
   and a fact on the task row.
5. **One vocabulary.** A frame KIND (`workflow_completed`) is an event
   name; a STATE (`succeeded`) is a settlement word. The two are mapped
   in one place (`RunState::terminal_kind` · `from_terminal_kind`). The
   word `completed` no longer names a run state on any door.
6. **Honesty laws kept.** `total_cost_usd` is absent when nothing was
   metered; the qualifier says what the total covers; unknown is never
   zero; a paused run keeps `ok` (a decision point, never a failure) and
   is not final; the budget stop is a `failed` run with cause `budget`
   and the `NIKA-1704` error named on the settlement.

## Consequences

- Positive: one fold, one vocabulary, one reader — a surface that wants
  the run's verdict deserializes the settlement; none refolds task frames.
  The `run_settled` envelope now says WHY and how much, not only what.
- Negative: the `status` field on the terminal frame changed word for a
  successful run (`completed` → `succeeded`), and `trace ls` prints
  `succeeded`; `nika-runtime-laws::compat_record` still twins the task
  vocabulary (its deletion is the next slice).
- Follow-ups: the resident publishes `JobStatus` as a projection of
  `RunState` with `interrupted` defined as ownership lost + evidence
  incomplete (#1443); `trace ls` learns whether a `running` trace's
  writer is alive and `trace verify` exits non-zero on INCOMPLETE
  (#1442); `nika-tui-core` reads the settlement instead of folding.

## Alternatives considered

- A `nika-wire` crate for every cross-door contract: rejected — the
  event crate already owns the frame vocabulary and every reader depends
  on it; a new crate adds a layer, not an authority.
- Keeping `completed` as the state word: rejected — `run_settled`, the
  resident and the SDK already spoke `succeeded`; one word had to win and
  the kind keeps `completed`.
- Making `RunSettlement` a runtime type: rejected — readers (`nika-trace`
  · `nika-dap` · the resident · tui-core) must not depend on the runtime
  to read a frame.
