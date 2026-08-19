---
id: ADR-116
title: "`nika serve` — the resident firer: one law, two edges, and the three draft answers"
status: accepted
date: "2026-08-19"
phase: "pre-1.0 · the ARM+SERVE arc"
deciders: ["@ThibautMelen"]
tags: ["architecture", "cadence", "arm", "serve", "input-trust", "layering"]
affects_crates: ["nika-cli", "nika-cadence"]
affects_layers: ["L0", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-114", "ADR-110"]
requires: ["ADR-114"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: ["gate-1-serve-input-trust"]
nika_codes: []
timeline: "v0.110"
follow_ups:
  - "the cloud (③) carries `serve_tokens` — remote firing is explicitly NOT v0 (no port, no socket, no input door)"
  - "`chevauchement: remplacer` · `après_saut: à-complétion` · `manqué: rattraper` · `décalage:` arrive with serve v0.2 — the firer refuses them by name today (D6)"
---

# ADR-116: `nika serve` — the resident firer

## Context

W2 gave the engine ONE firer (`nika arm fire <label>` · `18253bb95`):
the on-time window, the miss policy, the overlap lock, the per-tick
ceiling and the record, decided once in `crates/nika-cli/src/verbs/arm/fire.rs`.
W3 renders the OS units (launchd · systemd) that call it. But a
container, a VPS, or the cloud has no launchd to bootstrap — the firer
must be able to live long. That is `nika serve` (②), W5 of the
ARM+SERVE arc.

Three questions stayed open from the draft (`T-serve-heritage-brouillon`),
and the diamond-discipline Gate 1 (« nika serve input trust », P0)
bounded what serve may read at all.

## Decision

`nika serve` is the SAME firer, resident — never a second scheduler.
The loop (`crates/nika-cli/src/verbs/serve.rs`) owns exactly four
things the one-shot firer does not:

- **The clock at the edge (D5)** — production reads `Zoned::now`; the
  hidden `--now` starts a deterministic replay whose waits ADVANCE the
  scripted instant instead of sleeping (the `VirtualClock` trap — a
  sleeping loop whose clock never moves — is closed by construction),
  and `--until` bounds it. A scripted loop without `--once` or
  `--until` refuses at the edge: it would spin, never serve.
- **The re-read, never a cache** — `nika.yaml` is re-parsed and
  re-validated whenever its mtime moves; a reload that refuses POISONS
  the served set until the file reads again (a beat the operator has
  just disarmed must never fire from memory), and the refusal is said
  on stderr.
- **The bounded wait** — sleep until `earliest_next`, capped at one
  minute so a moved file is picked up within the minute; SIGINT/SIGTERM
  stop the loop clean (exit 0), and the fire in flight — synchronous —
  always finishes first.
- **The server exit convention** — `0` clean stop, `1` serve's own
  fault. A beat's failure is recorded in the beat's history and never
  moves serve's exit (the `lsp`/`dap` arms' precedent in `main.rs`).

What is due and what happens to it stay in the W2 law:
`nika_cadence::due` names the beats (active · local · a slot in
`(last_fired, now]`), `fire_beat` decides and fires each one — window,
miss policy, lock, ceiling, record. `--dry` rehearses the REAL
`decide` (crate-visible now), so the rehearsal can never drift from
the law it rehearses. A beat the planner does not name prints nothing:
the resident log is the fires, not the silence.

### The three draft answers (T-serve-heritage-brouillon → shipped)

1. **`max_retries` at the job level → NO.** A beat starts from zero
   (N2): every fire is a fresh run, and a failed run is recorded and
   over. The retry of a failed TASK lives in the workflow (`retry:`),
   never in the scheduler — a scheduler that retries double-spends the
   ceiling the workflow already bounded.
2. **The job state enum → it is `history.ndjson`.** The sidecar journal
   (`fired` · `skipped` · `paused` · `failed` · disarmed) IS the state
   — append-only, one line per decision, read by `last_fired` and the
   `nika arm` report. No second store, no scheduler-local database.
3. **`serve_tokens` (who fires a beat from a distance) → NOT v0.** No
   port, no socket, no remote trigger of any shape (Gate 1 below). The
   cloud (③) will carry remote firing; v0 serve is local-only by
   construction.

### Gate 1 — serve input trust (P0), resolved 2026-08-19

Serve reads ONLY the project's `nika.yaml` — judged by the vocab's
shape and the cadence grammar BEFORE any firing — and its own
`.nika/arm/` sidecar. No socket, no port, no network read, no external
argument: `--once`/`--dry` are the whole public surface (`--now` /
`--until` stay hidden replay hooks). Pinned by
`serve_has_no_input_but_the_registry_and_its_state`
(`crates/nika-cli/tests/serve.rs`): the `--help` surface carries no
input door, and a full scripted loop leaves a tree holding only the
registry, the workflow shelf, and serve's own state.

## Consequences

### Positive

- One firer, one law: every policy correction (window · miss · lock)
  lands in `fire_beat` and binds both edges at once.
- A replay is fully deterministic — the whole loop testable without a
  wall-clock sleep (8 integration + 7 unit tests, zero real waits
  except the SIGTERM one).
- A broken edit mid-serve stops the firing (safe direction) and says
  so; the next good edit resumes it — no restart.
- The `--dry` rehearsal cannot lie: it runs the same `decide` the
  firer runs.

### Negative

- The `--emit` unit (W3) fires one beat per OS tick; serve fires every
  due beat per pass — two shapes of the same law, both correct, and an
  operator must still choose one (running both double-fires; the
  overlap lock catches it per beat, journaled `skipped · overlap`).
- A signal observed only at a wait can defer the stop by up to one
  minute if the operator SIGTERMs between the fire and the wait's
  first instant — bounded, and the fire always finishes first (N2's
  fresh-run law makes that cheap).
- `nika-cli` grows by ~560 prod LOC inside its yellow descent band —
  the ratchet's warning, not its wall; no split is owed for this.

### Neutral

- The tokio workspace feature set gains `signal` (additive; no new
  crate enters the tree — `signal-hook-registry` already rides the
  `process` feature).
- `FireCtx` is reused as-is: serve re-points `index`/`label` per due
  beat and moves the registry in and out of the one context per pass.

## Evidence / Affected code

- `crates/nika-cli/src/verbs/serve.rs` — the loop, the clock doors, the
  re-read, the unit tests (7).
- `crates/nika-cli/tests/serve.rs` — the binary contract (8 tests):
  once · two slots in order · mid-loop reload · SIGTERM = exit 0 ·
  cloud never fires · the Gate-1 pin · the bound refusal · the boot
  refusal.
- `crates/nika-cli/src/verbs/arm/fire.rs` — `decide`/`Decision` become
  crate-visible (the `--dry` rehearsal reads the REAL decision).
- `crates/nika-cli/src/main.rs` — `Command::Serve` (hidden ·
  display_order 73) + the dispatch arm.
- `Cargo.toml` — tokio `signal` feature.
- `.claude/rules/diamond-discipline.md` — Rule 5 Gate 1, closed with
  the date.

## Alternatives considered

- **A `nika-serve` crate** — rejected: the loop is ~350 prod LOC of
  pure L4 orchestration over `fire_beat` and `nika-cadence`; a crate
  would buy a boundary nothing reads (the serve token surface, its
  only real payload, is explicitly not v0).
- **Keep the last good registry when a reload refuses** — rejected on
  the disarm direction: the operator's `actif: false` must take effect
  even when the file momentarily refuses, so a broken file serves
  NOTHING until it reads.
- **Retry a failed beat at the scheduler level** — rejected (draft
  answer 1): N2's fresh-start law plus the workflow's own `retry:`
  cover the need without double-spending the per-tick ceiling.
