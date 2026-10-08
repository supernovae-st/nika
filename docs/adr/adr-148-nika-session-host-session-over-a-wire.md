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
follow_ups: ["the native machine door registered as `nika session --json` and its capability word in the engine identity", "the HTTP door's run port through the resident's job admission in the served project", "an exact candidate source projection in the work snapshot", "several live conversations per project once the history names a conversation"]
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
   only as withdrawn history; once the turn settles, a Stop reports that nothing was left to stop.
   A Run requested by the Session is not stopped by a preparation Stop.
5. **Two doors, one contract** (`nika/session-host@1`): an NDJSON driver for the native machine
   door (stdin read on its own thread; the log written in order) and HTTP routes under
   `/v1/sessions` that `nika serve` delegates to after its own bearer check and body limit,
   outside the generic request deadline, on a server whose operator enables sessions. One live
   Session per served project, because the history is one per (HOME, project).
6. **The run belongs to the door** (ADR-133). The native door runs this binary's machine lane as
   a child and keeps the child across its fresh cost review, so the review's answer reaches that
   child once. A door that cannot start a run says so and observes nothing.

## Consequences

- `nika-serve` and `nika-cli` gain delegation lines only; the contract and its custody live
  once, beside the Session.
- The work snapshot travels verbatim (`nika/session-work@0`); additive Session projections reach
  every door without a second wire.
- The public surface of the new member is reviewed with its unit; admission evidence is pending
  with the WIP unit.
