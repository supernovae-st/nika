---
id: ADR-132
title: "Identity, idempotency and protocol: what a duplicate human action yields, and the resident's writer stamp"
status: accepted
date: "2026-09-04"
phase: "pre-1.0 · one door"
deciders: ["@ThibautMelen"]
tags: ["architecture", "one-door", "idempotency", "protocol", "serve"]
affects_crates: ["nika-serve", "nika-cli-host", "nika-cli"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: ["ADR-099", "ADR-117", "ADR-126", "ADR-128", "ADR-129", "ADR-131"]
requires: ["ADR-129"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: ["NIKA-SEC-010"]
timeline: ""
follow_ups: ["the session's duplicate-action replies become typed outcomes (ADR-133 · the portable session)", "the TypeScript SDK reads the resident line from nika doctor --json"]
---

# ADR-132: Identity, idempotency and protocol

## Context

A remote surface retries. The one-door program asked what a duplicate
human action yields on every door, and what protects a resident's
durable stores across an engine upgrade (#1352: an `npm update` left a
0.117 resident firing schedules while a 0.118 SDK did manual runs, and
nothing said so). This ADR records the audit of what is already law on
`main`, and decides the one gap it found.

## What is already law (MAIN · audited 2026-09-04)

| action | identity | a duplicate yields | where |
|---|---|---|---|
| a gate answer on `--resume` | the approval ticket (`content_hash` · `run_nonce` · `step` · `minted_at_ms` · `ttl_seconds`) journaled on the pause frame, its digest claimed once under `.nika/approval-claims` | a settled gate: « this gate already settled · a decided gate stays decided » (ENV) · an expired ticket: the gate re-mints and asks again · a cross-run replay: refused `NIKA-SEC-010` · a consumed ticket: `approval.replayed` `NIKA-SEC-010` | `nika-dap::resume` · `nika-runtime::approval` · `resume_setup.rs` (the durable claim root) |
| a session consent | the change set's witnesses (the bytes previewed) | after the file changed on disk: « changed since this preview — nothing was applied » · twice: « nothing is pending » | `nika-session::change` · `runtime::consent` |
| a session gate answer | the pending gate | twice: « no run is waiting for an answer » | `runtime::answer_gate` |
| a job cancel | the job's status transition | on a settled job: the record, unchanged (200) · a lost race: the current record | `nika-serve` `route::cancel_job` |
| a job submission | `Idempotency-Key` + the request digest | the same bytes: the existing job (200) · other bytes: 409 | `JobStore::create_or_replay` |
| a schedule apply | the schedule revision + an ETag precondition | the same draft: `Unchanged` · a stale revision: precondition failed | `ScheduleStore::apply` |
| an event stream reconnect | the per-job `sequence` + `Last-Event-ID` | replays only persisted events after that sequence · heartbeats are cursor-neutral | `nika-serve` `sse` · the OpenAPI |
| an SDK against a resident | `machineProtocolVersion` on `/health` | an incompatible generation: the SDK refuses before any request | `nika-client` `engine-identity.ts` |

The wave-7 gauntlet's reading « the TTL has no reader » was true of the
0.117.1 it measured and false on `main` since wave 7.f: the book judges
the ticket's nonce, freshness and consumption. Nothing here is built
twice.

## Decision (the gap)

1. **The writer's stamp.** Both resident stores (`jobs/state.json` ·
   `schedules/state.json`) carry `writer: {engine_version,
   machine_protocol_version}`. A store is stamped at creation and
   re-stamped at every open by a different engine (`WriterStamp` ·
   `nika-serve::writer`). A store from before the stamp carries none and
   is served, then stamped.
2. **Fail closed on a newer protocol.** A store last written by an engine
   speaking a NEWER machine protocol refuses to open
   (`WrittenByNewerEngine`, naming both engines): its state is not ours
   to reinterpret. An older writer on the same protocol is served.
3. **The resident line.** `nika doctor` reads the stamps beside the
   server lease (`nika-serve::resident::inspect` · a non-blocking shared
   `flock` on `jobs/server.lock`: held → alive) and says whether the
   running resident is this binary: `✔ alive · engine X (this binary)` ·
   `⚠ alive · engine X — this binary is Y: restart the resident` ·
   `✖ alive · newer than this binary` · `not running · stores last
   written by X`. The host renders (`doctor::run_with`), the binary
   observes: `nika-cli-host` never depends on `nika-serve`.

## Consequences

- Positive: a resident-vs-binary skew is a doctor line, not a silent
  drift; a newer engine's state is never reinterpreted; every duplicate
  human action already yields a semantic reply on every door.
- Negative: two additive fields on the stores' wire; one more doctor
  line when a store exists.
- Follow-ups: the session's replies as typed outcomes (ADR-133).

## Alternatives considered

- Refusing any version skew at open: rejected — an older writer on the
  same protocol is exactly the upgrade path; the stamp refreshes.
- A resident-side check of the SDK's version: rejected — the SDK
  pre-flights `/health` already (one handshake, client-side).
