---
id: ADR-148
title: "The Session over a wire: nika-session-host keeps the actual runtime behind a native machine door and HTTP routes"
status: proposed
date: "2026-10-08"
phase: "pre-1.0 · native session"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crates", "size-cap", "session", "one-door", "idempotency"]
affects_crates: ["nika-session", "nika-session-host", "nika-serve", "nika-cli"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-125", "ADR-126", "ADR-132", "ADR-133", "ADR-144"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.123"
follow_ups: ["a paused job's resume over HTTP once the resident has a resume route (its gate's answer is run_not_started until then)", "several live conversations per project once the history names a conversation"]
---

# ADR-148: the Session over a wire — nika-session-host

## Context

ADR-133 made the Session machine portable: typed outcomes, proposal, gate and question
identities, and a host that judges by identity. It left « the session over a wire (the
resident's session door · the SDK) » as a follow-up. A remote client cannot drive
`SessionRuntime` directly: a question's identity holds the incarnation that asked it in memory
only, a turn may run for minutes on a provider call while Stop and reads must stay responsive,
and a network retry must never become a second effect.

The owners that would naturally host this adapter stand at the 15,000 prod-LOC wall
(`scripts/ci/check-crate-size.sh` at `7d98023f9`): `nika-serve` 14,998, `nika-cli` 14,988,
`nika-session` 14,773.

## Decision

1. **One host adapter, one runtime.** `nika-session-host` (L4, WIP size-cap member of the
   nika-session unit, lateral `nika-session-host → nika-session`, never back) keeps the actual
   `SessionRuntime` on one worker thread and runs one turn at a time through
   `SessionRuntime::submit`. What a client reads or decides while a turn runs lives apart behind
   one short lock: the snapshot published last with the very `Waiting` value it showed, the
   handles published before it, an idempotent command ledger, the event log and the Stop token
   of the turn under way. No second state machine, no line classification, no compile.
2. **A line names the snapshot it answers.** A submit reaches the Session only when it names the
   current published snapshot, and the Session receives that snapshot's retained `Waiting`,
   never one rebuilt from the wire. Snapshot handles and the Session identity are random per
   incarnation; a handle from another Session or from before a restart resolves nowhere.
3. **Identity before freshness.** A command identity already known answers first: the same bytes
   (op, snapshot, line) give the recorded result again, an original still running is awaited and
   never run twice, other bytes are a conflict. Only then: one turn at a time, and the current
   snapshot.
4. **Stop is linearized with the settlement.** A Stop is bound to the turn it found. If it lands
   while the turn prepares, the late result is withdrawn
   (`SessionRuntime::withdraw_cancelled_preparation`) before anything is published and is kept
   only as withdrawn history, and the run that turn requested is never admitted
   (`run_not_started`); once the turn settles, a Stop reports that nothing was left to stop. An
   admitted run is not stopped by a preparation Stop: its cancellation is the run door's own.
5. **Two doors, one contract** (`nika/session-host@1`): an NDJSON driver for the native machine
   door (stdin read on its own thread; the log written in order) and HTTP routes under
   `/v1/sessions` that `nika serve` delegates to after its own bearer check and body limit,
   outside the generic request deadline, on a server whose operator enables sessions. One live
   Session per served project, because the history is one per (HOME, project).
6. **The run belongs to the door** (ADR-133). The native door runs this binary's machine lane as a
   child and keeps the child across its fresh cost review, so the review's answer reaches that child
   once. The HTTP door lends its Session the resident's own job admission (a `Jobs` port): a run is
   a job of the served project, admitted by name through the served registry and the resident's
   literal input law (never the server's environment), then observed in the job store until it
   settles or pauses. The door admits a run only when the world it captured by name is the one the
   Session checked for it: the root's bytes (`RunRequest::admits`) and the closure of the workflow,
   every child workflow it reaches and every skill (`RunRequest::admits_world`). A workflow or a
   child rewritten since, or a request naming no checked world, is `run_not_started`, and the
   admission or its review uses that very capture. The job runs under the run's ceiling restricted by the resident's per-run
   ceiling, never a raised one: the job record keeps that ceiling, bound into its admission event,
   so a replayed or restarted job runs under it too. A run the server's cost review holds is framed
   over the world the door admitted, with the run's ceiling as the invocation default the human's
   one approval overrides once, as `nika run` does; that approval admits the reviewed job once (a
   decline admits none). The resident has no resume route, so a paused job's gate answer is
   `run_not_started`. A door that cannot start a run says so and observes nothing. The native
   door's child line carries the witness of the checked bytes and the closure of their world
   (`RunRequest::args`), and `nika run` refuses any other before anything starts.
7. **A run is named by what it observed of itself, never invented.** Each door hands its
   Session the run's identity with its observation, folded once (`RunIdentity`): the native door
   from the child's frames, the HTTP door from the job's receipt and journal. Over HTTP the run
   names the receipt's execution and its opaque trace identity, which the resident's own trace
   door resolves, and the source hash only of this execution's own journal start. Every Session
   text names a trace relative to the project root, or `<journal>` when the project does not hold
   it; the trace is still read on its real path.
8. **A conversation may name its intelligence.** An opener may name the census's own
   first-screen words (`intelligence` on the HTTP open, `--intelligence` on the native door).
   The choice holds for that conversation only: it is recorded in its history, resumes with it,
   and is never written as the operator's default; words the census does not read open nothing.
   A choice made inside the conversation holds the same way. Both doors advertise
   `sessionIntelligence`.

## Consequences

- `nika-cli` gains delegation lines only; `nika-serve` gains its delegation and the job port it
  lends a Session (the resident's admission, cost review and wait). The contract and its custody
  live once, beside the Session. The pure remote compile request law moved to
  `nika-compile-seats::remote::{input, bounds}` to keep `nika-serve` under its wall.
- The work snapshot travels verbatim (`nika/session-work@0`); additive Session projections reach
  every door without a second wire: the candidate's exact bytes, the selection's scope
  (`conversation` or `operator_default`) and the waiting question as the compiler asks it.
- The public surface of the new member is reviewed with its unit; admission evidence is pending
  with the WIP unit.
