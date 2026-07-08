---
id: ADR-100
title: "Trace retention — bounded by default, never silently, never a pending gate"
status: accepted
date: 2026-07-05
phase: ""
deciders: ["@ThibautMelen"]
tags: [trace, retention, gc, resume, pause, cli, storage]
affects_crates: [nika-cli, nika-runtime]
affects_layers: [L2, L4]
supersedes: []
superseded_by: []
related: ["ADR-099"]
requires: ["ADR-099"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: ""
follow_ups: []
---

# ADR-100 — Trace retention · bounded by default, never silently, never a pending gate

- **Status**: Accepted (2026-07-05) — implemented by the engine
  `feat/trace-retention` arc: the D1 policy (keep-10 per workflow ·
  30d age cap · 256MB budget · the paused + newest-per-workflow
  absolute exemptions) · D2 opportunistic GC at `nika run` start
  (fail-open · exactly ONE stderr line · `--no-gc`) · D3
  `nika trace ls` (age · size · workflow · state · the ★
  resume-candidate marker) + `nika trace rm` (paused refuses without
  `--force` and names the task) · D4 knobs on the `NIKA_TRACE_*` env
  family, reported by `nika doctor` · the 4 conformance fixtures as
  engine tests (`crates/nika-cli/src/verbs/trace/{retention,manage}.rs`).
- **Surface**: CLI + runtime housekeeping ONLY. Zero envelope change ·
  zero new YAML.
- **Home**: the CANONICAL prose lives in the `nika-spec` repo's `adr/`
  (language-surface decisions); this copy registers the number in the
  shared Nika ADR series (the engine `docs/adr/` registry is the
  series' index).

## Context

`.nika/traces/` grows without bound: every `nika run` appends one NDJSON
journal and nothing ever removes one (verified 2026-07-05 · zero
retention/GC language anywhere in spec v1). This was tolerable when a
trace was a debugging artifact. Since ADR-099 a trace is **three things
at once**:

1. an observability record (`nika trace show|replay` and the round-7
   comprehension surfaces),
2. **the checkpoint** (`--resume` folds it · deleting a trace deletes
   resumability),
3. **a pending human gate** when its last state is `workflow_paused` —
   deleting THAT trace silently destroys an unanswered approval.

Unbounded growth is a real cost (traces carry full task outputs since
ADR-099 rehydration — multi-MB per run on payload-heavy workflows), but
naive GC would violate the resume and pause contracts. The legacy engine
had trace auto-GC; v1 must get the bounded-but-safe version.

## Decision

### D1 · Retention policy (defaults · engine-enforced)

Per workflow (keyed by workflow name within a project's `.nika/traces/`):

| Rule | Default | Rationale |
|---|---|---|
| keep the last **N** completed-run traces | `10` | the observability window |
| age cap | `30d` | stale traces exit even under N |
| project total size budget | `256MB` | the hard stop |

**Absolute exemptions — never collected regardless of any cap:**

- a trace whose last workflow state is `paused` (a pending gate is an
  OBLIGATION, not garbage) — it ages out only once answered or explicitly
  removed with `nika trace rm`,
- the most recent trace of each workflow (the standing resume candidate).

### D2 · When GC runs

Opportunistically at `nika run` start (maintenance rides usage — no
daemon, no cron, the Nextflow/cargo school). A collection that removes
anything prints exactly ONE line to stderr:

```
trace gc · removed 3 (2 aged · 1 over-budget) · kept 12 · 41MB
```

Silent deletion is forbidden (the anti-hidden-magic invariant). `--no-gc`
skips collection for that invocation.

### D3 · The explicit surface

- `nika trace ls` — traces with age · size · workflow · terminal state
  (`completed`/`failed`/**`paused`**) · the resume-candidate marker.
- `nika trace rm <trace>|--older-than <dur>|--all` — explicit removal ·
  removing a `paused` trace requires `--force` and says what it destroys
  (`this trace carries an unanswered prompt for task <id>`).

### D4 · Configuration

The three knobs ride the engine's existing config surface (`nika doctor`
reports the active values). Config names and wiring are the
implementation arc's to fix; the DEFAULTS above are the spec contract.

## Alternatives considered

- **No GC (status quo)** — rejected: unbounded disk on every real user ·
  the docs would have to teach manual cleanup of a dir the tool owns.
- **A `nika trace gc` verb only (fully manual)** — rejected: nobody runs
  maintenance verbs · bounded-by-default is the 2026 baseline · the
  explicit surface exists IN ADDITION (D3).
- **GC daemon / background thread** — rejected: no daemon is a v1
  identity trait · opportunistic-at-run is sufficient and testable.
- **Content-addressed dedup of outputs before GC** — deferred: real
  (outputs repeat across runs) but it is a storage-format change ·
  retention must not wait for it.

## Consequences

- Disk is bounded by default; resume and pause contracts survive GC by
  construction (the exemptions are the contract).
- `--resume` against a collected trace keeps its ADR-099 behavior
  (readable error · nothing skips) — retention adds the `trace ls`
  surface to SEE what is resumable.
- Traces stop being an unbounded liability in CI (the size budget is the
  cap that matters there).

## Conformance sketch (ships with the implementation)

1. **paused-survives** · a `workflow_paused` trace older than every cap
   survives GC · `trace ls` marks it `paused`.
2. **resume-candidate-survives** · the newest trace of a workflow
   survives even when over-budget alone.
3. **visible-collection** · an over-N history collects oldest-first ·
   exactly one stderr line reports it · `--no-gc` leaves it intact.
4. **forced-removal-speaks** · `trace rm` on a paused trace refuses
   without `--force` and names the unanswered task.

## Notes

- Interplay with idempotency keys (the §Persistence deferral) is
  unchanged — retention never re-runs anything by itself.
- The engine index registration + `nika trace ls|rm` land with the
  implementation arc.
