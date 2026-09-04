---
id: ADR-129
title: "Run state and evidence state are distinct: the writer's lease, the INCOMPLETE exit, the resident's interrupted"
status: accepted
date: "2026-09-04"
phase: "pre-1.0 · one door"
deciders: ["@ThibautMelen"]
tags: ["architecture", "trace", "evidence", "one-door", "liveness"]
affects_crates: ["nika-dap", "nika-trace", "nika-cli-host", "nika-serve"]
affects_layers: ["L3", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-099", "ADR-100", "ADR-128", "ADR-130", "ADR-132"]
requires: ["ADR-128"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: ""
follow_ups: ["one status vocabulary published in the serve OpenAPI (#1443 · the mapping matrix)", "the seal's teardown `outcome` word converges on the settlement's (the evidence format's own cadence)"]
---

# ADR-129: Run state and evidence state are distinct

## Context

A journal with no terminal frame is either a run in flight or the remains
of a writer that died. The store folded frames, and a dead process writes
no frame, so `trace ls` said `running` forever for a killed run (#1442,
measured twice by the wave-7 gauntlet), while `trace verify` printed
INCOMPLETE and exited 0 — a monitor wired on the exit code greened a dead
run. The resident's `interrupted` (ownership lost after a restart) had no
journal twin and read like a fifth run state (#1443). ADR-128 gave the run
ONE settlement; the evidence needed its own truth, kept apart from it.

## Decision

1. **A dead process proves incomplete evidence, never a failed run.** No
   door invents a terminal frame for a writer that died; the run's own
   settlement is its terminal frame, and there is none.
2. **The writer's lease.** The run that writes a journal holds
   `<trace>.lock` (owner-only, `{"pid","host"}`) under an exclusive
   advisory lock for the journal's lifetime (`nika_dap::liveness`, taken
   by the trace sink at open); the kernel releases it when the process
   ends, however it ends. A reader asks the lock: held → `alive`, free on
   this host → `dead`, no lease or another host → `unknown`. It never
   guesses. The lease dies with its journal (`trace rm` · retention).
3. **The words.** `trace ls` prints `dead` (both faces · `liveness` on the
   machine one) for a running trace whose writer died; `running` for a
   live writer or one this host cannot judge. The run state stays
   `Running` — the word is about the evidence.
4. **The exit.** `trace verify` exits `INCOMPLETE` (5) for a journal that
   never reached a terminal frame, and names the writer's liveness; FILE
   (2) stays the broken or forged chain, ENV (3) the missing input. The
   UNSEALED tier keeps its opt-in strictness (`--require-seal`).
5. **The resident's `interrupted`** is an evidence state — ownership lost,
   effect settlement unknown, the journal INCOMPLETE — published as such
   in the OpenAPI, never mapped from a run state.

## Consequences

- Positive: a dead run is told from a live one on every door; CI cannot
  green incomplete evidence; the vocabulary keeps RunState and
  EvidenceState apart as the one-door invariants require.
- Negative: `trace verify` on a run in flight now exits 5 (honest: not yet
  complete); a lease sidecar rides beside every journal (0600, tens of
  bytes); a journal copied to another host reads `unknown`.
- Follow-ups: the serve status matrix (#1443); the seal's teardown word.

## Alternatives considered

- The pid on the opening frame: not portable and not a liveness proof (a
  reused pid lies); the lease is judged by the kernel.
- Inventing `workflow_failed` for a dead writer: rejected — a reader would
  be writing evidence it does not have.
