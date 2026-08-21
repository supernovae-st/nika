---
id: ADR-118
title: "admit nika-arm as the shared ARM custody library"
status: accepted
date: "2026-08-21"
phase: "pre-1.0 · ARM custody"
deciders: ["@ThibautMelen"]
tags: ["architecture", "crate-admission", "arm", "filesystem", "layering"]
affects_crates: ["nika-arm", "nika-cadence", "nika-cli", "nika-fs"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: []
requires: ["ADR-114"]
enables: []
amends: []
fci: []
inv: []
shadow_zones: ["transitive-workflow-input-custody", "serve-network-boundary"]
nika_codes: []
timeline: "pre-1.0"
follow_ups:
  - "move child-workflow and skill reads behind one shared owned-byte execution service"
  - "keep networked Serve behind ADR-117 until that execution service and its security gates are admitted"
---

# ADR-118: admit `nika-arm` as the shared ARM custody library

## Context

ARM firing and persisted state were implemented inside `nika-cli`, even though
both the `nika arm` command and the resident local `nika serve` loop consume
them. That ownership mixed interface rendering with kernel leases, durable
ledger mutation, descriptor-relative traversal, workflow capture, and exact
trace attribution.

The pure schedule and ledger decisions already belong to `nika-cadence` at L0.
The remaining effectful transaction cannot move there because it opens files,
holds advisory locks, fsyncs claims and receipts, rotates evidence, waits, and
invokes execution. Leaving it in `nika-cli` would make every other interface
depend upward on a command adapter or duplicate the custody rules.

## Decision

Admit `nika-arm` as an engine-internal L4 library. It owns the effectful ARM
custody once for every interface, while `nika-cadence` remains the pure
decision authority and `nika-cli` becomes a rendering and dependency-injection
adapter.

The named-beat tick classifier (`tick_decision` · `TickDecision` ·
`v0_unsupported`) stays in `nika-cadence`. `due()` already drops inactive and
cloud beats; OS units fire a named beat and must journal the skip reason.
That total function is L0. Emit and the firer share one D6 policy set — they
must never disagree. The L4 crate locks, claims, pins bytes, and runs.

### 1. One effectful ARM transaction

`nika-arm` owns:

- the descriptor-rooted `.nika/arm/` state and verified replay;
- kernel leases, claim-before-run ordering, fenced receipts, healing, and
  archive rotation;
- the firing transaction from the final schedule decision through terminal
  receipt;
- capture of the exact primary workflow bytes and their declared logical path;
- receipt of the exact typed trace returned by the injected executor.

Interfaces inject time, waiting, process execution, and presentation. They do
not mutate journals directly, infer a trace by scanning a directory, or
reinterpret the cadence state machine.

### 2. Filesystem authority is held, not reconstructed

`ArmState` and `FireCtx` retain a project directory capability. State and
workflow traversal stays beneath that capability and opens each workflow
component with no-follow semantics. A visible pathname replacement after
context construction cannot redirect the held project root.

The generation binds the validated beat and the exact captured primary
workflow bytes. The injected executor receives those bytes, the logical
workflow path, the held project capability, and the spend ceiling as one
`RunShot`; it cannot substitute a second discovery pass for that transaction.

This decision also extends `nika-fs::OwnedDir` so absolute roots are opened one
component at a time rather than accepted after a single pathname lookup.
Traversal above the root and symlink components refuse.

### 3. The public boundary remains deliberately narrow

The crate is `publish = false`, forbids unsafe code, and exposes no arbitrary
sidecar pathname or raw ledger mutation. Public structs have private fields and
read-only accessors. Cross-crate fixtures require the non-default
`test-support` feature and do not enter the normal public API snapshot.

The extraction introduces no HTTP listener, authentication scheme, job API,
cancellation contract, artifact authority, or runtime implementation. Those
are separate decisions and remain outside this admission.

### 4. Security carry is named, not hidden

This boundary pins the ARM registry, state, and primary workflow. It does not
yet pin every transitive workflow input. The current composition adapter can
still reopen child workflows and skill files by pathname. Closing that gap
requires one shared owned-byte execution service used by every interface; it
is a required follow-up and a prerequisite to ADR-117's first network route.

## Consequences

### Positive

- CLI and resident Serve share one lease, replay, firing, and receipt authority.
- A captured primary workflow keeps its logical base without executing from a
  temporary pathname.
- Exact trace attribution is returned by the executor instead of guessed from
  global directory order.
- The pure L0 cadence rules remain free of filesystem and process effects.

### Negative

- `nika-arm` is an L4 library rather than a generally reusable low-level crate.
- Interface tests must cover adapter parity in addition to the library suite.
- Transitive child and skill custody remains an explicit security carry until
  the shared execution service lands.

### Neutral

- The crate tracks the workspace version and is not published independently.
- Existing CLI output and resident Serve behavior remain the parity oracle.
- No network behavior is admitted by this split.

## Required evidence

1. `nika-arm --lib`, `nika-fs --lib`, `nika-cli --lib`, and the workspace
   library suite pass.
2. Clippy, rustdoc, formatting, layering, public API, LOC, function-size,
   unwrap, expect, dependency, privacy, and estate gates pass.
3. Real CLI ARM and resident Serve tests retain behavior and exact trace
   attribution.
4. Mutation testing kills at least 90% of viable `nika-arm` mutants.
5. The admission commit contains the crate spec, API snapshot, layer registry,
   estate update, and this ADR.

## Alternatives considered

### Keep the implementation in `nika-cli`

Rejected. A second L4 interface would have to depend on a command adapter or
copy the custody rules.

### Move the transaction into `nika-cadence`

Rejected. Locks, fsync, path traversal, waiting, and process execution would
break the pure deterministic boundary admitted by ADR-114.

### Build the network service during the extraction

Rejected. It would combine a behavior-preserving ownership change with a new
remote trust boundary before transitive input custody is closed.
