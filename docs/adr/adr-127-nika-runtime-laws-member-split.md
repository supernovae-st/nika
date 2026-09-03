---
id: ADR-127
title: "nika-runtime size-cap member split: the run's laws descend to nika-runtime-laws"
status: accepted
date: "2026-09-03"
phase: "pre-1.0 · one door"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "split", "size-cap", "runtime"]
affects_crates: ["nika-runtime", "nika-runtime-laws"]
affects_layers: ["L3"]
supersedes: []
superseded_by: []
related: ["ADR-110", "ADR-022", "ADR-024", "ADR-125", "ADR-126"]
requires: ["ADR-110"]
---

# ADR-127: nika-runtime size-cap member split — nika-runtime-laws

## Context

`nika-runtime` stands at 14 999 of the 15 000 production lines the
crate-size vector allows (vector 24 · the pre-push gate blocks at the
wall). Wave 7 of the one-door program added the operator's cancellation
at the wave boundary (ADR-125's session runs through it) and the project
fingerprint on the opening frame; the next three slices the program
owes — the terminal frame's summary (#1247), the per-item `for_each`
terminals (#1276 · #1397) and the V7 credential seams (S1–S6) — all grow
the runtime. Moving code between files inside the crate frees nothing:
the counter reads every `src/` file the index tracks.

The cap is a locked maintainability budget, not a suggestion; the
descent law (ADR-110 · D-2026-07-09-N1) says a size-cap split is ONE
architectural unit in TWO workspace members, the boundary drawn with an
ADR.

## Decision

The run's LAWS — what a run obeys before and after it executes, none of
which dispatches a task or folds a definition — descend to a new L3
member crate `nika-runtime-laws`:

- `errors` (the one-voice `RuntimeError` and its codes)
- `contract` (the typed `outputs:` contract · `first_output_type_violation`)
- `compat_record` (the public `TaskRecord` / `TaskStatus` / `TerminalCause` mirror)
- `origins` (`InputOrigin` · `input_origins`)
- `identity` (the engine identity · the build-support pins)
- `integrity` (the record integrity law · `ValueTaint`)
- `secret` (`WorkflowSecretResolver` · `RedactingSink` · the payload
  field list — the V7 custody seams' future home)
- `sandbox_select` (the sandbox verdict for a command)
- `witness` · `stamp` (the event stamp seams the laws write through)
- `resume_fields` (the resume projection's payload field names the
  secret custody reads)

The boot trust judgement (`trust`) and the semantic IR (`proof::ir`)
stay in the runtime: both read the definition fold (`definition_value`
and its helper family), which is the resume projection's and stays with
it.

`nika-runtime` re-exports every public item at its historical path
(`nika_runtime::{RuntimeError, TaskRecord, TaskStatus, TerminalCause,
InputOrigin, input_origins, WorkflowSecretResolver, identity,
sandbox_select, resume::fields, EventSink, Stamper, …}`), so call sites
in `nika-cli`, `nika-cli-host`, `nika-service-execution`, `nika-serve`,
`nika-session` and the tests stay untouched. The laws' crate-private
seams the core reaches (`TaskContract` · `ValueTaint` · `task_integrity`
· `scrub_outputs` · `SandboxDecision` · the witness aliases) are `pub`
in the member and `pub(crate)` re-imports in the runtime.

## Alternatives rejected

- Descending the composition root (`compose` · `harness_seat` ·
  `simulated`): it depends ON the runtime (it builds `Runtime<…>`), so
  the operator crate could not re-export it — every consumer would
  switch imports, and the public surface would fork.
- Descending `trust` and `proof::ir` with the laws: they read the
  definition fold, which would have dragged the resume projection
  (a runtime-core subsystem) into the member or duplicated it.
- A file split inside the crate: moves no production line out of the
  budget (the counter is the crate's, not a file's).
- Raising the cap: the cap exists to force exactly this.

## Consequences

- Vector 24 returns GREEN with real headroom (`nika-runtime` 13 234
  lines · 1 766 below the wall · `nika-runtime-laws` ≈ 1.8k; the boot
  trust and the semantic IR stayed with the definition fold, so the
  member is smaller than the draft's 3.3k).
- `nika-runtime-laws` is a member of the `nika-runtime` unit, not a
  new architectural unit: the same L3 row, `publish = false`, one
  public surface re-exported by the operator crate.
- A new `crates/nika-runtime-laws/public-api.txt` baseline joins the
  diff gate; `crates/nika-runtime/public-api.txt` re-baselines (items
  become re-exports).
- The V7 credential seams (S1–S6) land in the member: `secret` is
  theirs.
- The next growth inside `nika-runtime` should descend the pause /
  approval / resume plane (≈ 2.4k) before the wall bites again.

## Security boundary

No trust boundary moves. The laws keep their seams inside the member;
the operator crate reaches them through the same paths. The secret
resolver's zeroizing type and the redacting sink are moved, not changed.

## Rollback

Reverse `git mv` + drop the member from the workspace `members` and
from `nika-runtime`'s dependencies; the re-exports become definitions
again. One commit.
