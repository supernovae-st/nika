---
id: ADR-114
title: "`nika-cadence` — the pure arming domain, at L0"
status: accepted
date: 2026-08-15
phase: ""
deciders: ["@ThibautMelen"]
tags: [crate-admission, cadence, arm, grammar, layering, hermeticity]
affects_crates: [nika-cadence, nika-cli, nika-tui-core]
affects_layers: [L0, L4]
supersedes: []
superseded_by: []
related: [ADR-001, ADR-003]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: ""
follow_ups:
  - "`nika serve` is the second L4 consumer this crate was placed at L0 for — it does not exist yet, so the layering argument is currently carried by ONE consumer plus an intent"
  - "Eight cadence keys are reachable in the registry grammar but unreachable through the project file — pinned by a test in `nika-cli`, not yet resolved either way"
  - "`signature:` and `budget:` are refused BY NAME, waiting on ② and on a measured lack respectively"
---

# ADR-114: `nika-cadence` — the pure arming domain, at L0

## Context

This ADR is written **after** the crate landed, and the reason it is late is
itself worth recording.

`nika-cadence` arrived on `main` in `508b61ae1` (#928) carrying no ADR. The
repository has a gate for exactly this — `adr-coverage-new-crate` — and it did
not fire. Two independent reasons, both measured:

1. The gate is **warn-only** (it exits 0 even on a miss; its own header says
   « Promote to fail when coverage… »).
2. Its one branch that *does* block reads `git diff --cached`, so it lives in a
   **local pre-commit hook** — and a squash-merge performed on GitHub never runs
   a local hook.

> A gate that judges the staged diff is blind to the tree, and a gate that only
> exists in a local hook is blind to the merge path.

The crate's rationale was never missing — it is written at length in
`crates/nika-cadence/src/lib.rs`, citing its own locks. What was missing is the
ADR file the admission process expects. This document transcribes that
rationale and adds what has been measured since.

## Decision

`nika-cadence` is admitted at **L0 · pure · zero I/O · zero async**. It owns the
four pure parts of the arming domain, and no effect adapters:

- the **grammar** of the arming registry (the `arm:` block of `nika.yaml`, per
  D-2026-08-10-N3),
- the **pure next-slot calculator** (hand-counted 5-field cron, embedded IANA
  tzdb),
- the **typed firing state machine** (`SlotId`, fencing, generation, transition,
  fold, and policy decision),
- the **pure ledger codec and replay fold** (canonical line construction,
  hash-chain verification, claim/receipt reconciliation, and projection
  rebuilding from journal texts).

The interface boundary is the byte/text seam: `nika-cadence` accepts timestamps,
typed events, and borrowed journal text and returns typed decisions and rebuilt
projections. `nika-cli` alone owns paths, file discovery, locks, fsync, atomic
renames, W2 archive rotation, and rendering the operator verdict. The L0 crate
therefore remains deterministic and hermetic even though it now owns the state
machines that judge persisted evidence.

This amendment was forced into the open by W7's ledger reversal. Keeping its
pure codec and fold in `nika-cli` crossed the 15,000-production-LOC hard cap and
would have made the future `nika serve` consumer depend upward or duplicate the
judge. Extracting only the pure boundary removed that pressure while leaving
every effect at L4. The size gate exposed a missing architectural seam; it was
not solved by hiding or compressing code.

### Why L0 and not inside a CLI crate

Two L4 consumers are intended to read this registry — `nika arm` today,
`nika serve` at ②. Shared logic beneath more than one L4 consumer belongs at
L0; putting it in a CLI crate would make the second consumer an upward
dependency. This follows the precedent recorded in `nika-check`'s manifest
(« THREE L0 consumers make any higher layer an upward-dep violation »).

**Honest state of that argument**: only ONE of the two consumers exists today.
The layering choice is therefore carried by one consumer plus a stated
intention, and it is listed as a follow-up rather than presented as settled.

### The four locks — one law at four moments

The crate encodes D-2026-08-11-N1→N4, which are four faces of a single law ·
**the file proposes, the machine disposes**:

| Lock | Law |
|---|---|
| **N1 · DST** | A slot that does not exist fires at the FIRST VALID instant (02:00 absent ⇒ 03:00); a doubled slot fires ONCE, at its first occurrence. Written policy, never a guess. |
| **N2 · no resume** | A beat starts from ZERO; every tick is a new run. The pure firing fold describes a prior lifecycle, but never resumes or executes one. |
| **N3 · identity** | The MACHINE's key authorizes. `par:` DECLARES the human and proves nothing — a merge arms nothing. |
| **N4 · absence** | Removing a line does NOT disarm. That gesture is `arm --disarm`, an L4 act this crate knows nothing about. |

### The three hermeticity constraints

1. **No kernel `Clock` dependency** — that trait has no civil surface. The
   calculator takes a `jiff::Zoned`; the clock lives at the L4 edge.
   Determinism becomes trivial: tests use literal instants, with no clock to
   drive.
2. **The calculator never sleeps** — a virtual clock's `sleep` does not advance
   time, so a sleeping loop would spin forever under the very mode meant to
   prove it. The caller sleeps; the calculator never does.
3. **`jiff`'s `TimeZone::get` is forbidden here** — it prefers the host's
   `/usr/share/zoneinfo`, which is a hermeticity hole. Zones resolve from the
   EMBEDDED tzdb only (`jiff_tzdb::get` + `TimeZone::tzif`).

### Refusals teach

Every law is validated at parse, and every refusal is named and carries its
fix. The grammar publishes 23 spec codes, all under the `cadence.` namespace —
a test pins that prefix, so a code cannot silently escape it.

Round 1 refuses two keys **by name** rather than ignoring them: `signature:`
(verification belongs to ②) and `budget:` (it waits on a measured lack). An
unknown key is a refusal, not a shrug — the grammar is closed.

## Consequences

Measured consumers today: `nika-cli` (the `nika arm` verb) and `nika-tui-core`.

The ledger extraction makes the dependency direction explicit: the CLI is a
filesystem adapter over the L0 judge. Deleting the adapter removes all I/O;
deleting the L0 ledger module leaves the adapter unable to classify or rebuild
state. No dependency cycle is introduced (`nika-cadence` has no dependency on
`nika-cli`).

Two findings recorded since the crate landed, both from building its first
consumer:

- **Eight cadence keys are unreachable through the project file.** They parse in
  the registry grammar but no path through `nika.yaml` can carry them. This is
  pinned by a test in `nika-cli` so it cannot drift silently. It is not yet
  resolved in either direction — the keys may gain a path, or lose their place
  in the grammar.
- **The two readers must agree on the beat count.** `nika arm` checks
  `project.arm().len()` against the registry's own count, because the project
  file and the registry are parsed by two different paths over the same bytes.
  A divergence there would be the same class of defect as the two `registry:`
  parsers that disagreed in both directions (fixed in #936).

One defect surfaced since admission and has been repaired (#940):
the `WorkflowPath` refusal taught its fix with a path borrowed from the private
monorepo tree — a leak in a public engine, and an example meaningless to anyone
outside the studio. The repair carries a test pinning the general law ·
**a remedy a gate displays must itself pass the rule that gate teaches**.
