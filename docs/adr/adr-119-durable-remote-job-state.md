---
id: ADR-119
title: "make remote job admission durable before transport"
status: accepted
date: "2026-08-22"
phase: "pre-1.0 · execution access"
deciders: ["@ThibautMelen"]
tags: ["architecture", "serve", "durability", "idempotency", "filesystem"]
affects_crates: ["nika-serve", "nika-fs"]
affects_layers: ["L4"]
supersedes: []
superseded_by: []
related: []
requires: ["ADR-117"]
enables: []
amends: []
fci: ["FCI-001", "FCI-008"]
inv: []
shadow_zones: ["serve-network-boundary", "serve-execution-authority"]
nika_codes: []
timeline: "pre-1.0"
follow_ups:
  - "integrate the shared owned-byte execution service before the first effecting route"
  - "project authenticated HTTP and resumable SSE without creating a second execution path"
---

# ADR-119: make remote job admission durable before transport

## Context

ADR-117 requires every effecting remote request to carry an idempotency key,
binds that key to the authenticated request digest before execution, and names
`paused` as a real first-version status. It also makes SSE a projection of a
job event journal rather than a second execution path. None of those contracts
can be added honestly as route-local memory: a restart would forget the key,
could create a second runnable job, lose `paused`, and reset event cursors.

The shared execution service and HTTP boundary are separate work. The state
plane must therefore stand alone without importing `nika-runtime`,
`nika-cli`, transport, authentication, or workflow discovery. Its filesystem
authority must follow the descriptor-rooted precedent established by
`nika-fs::OwnedDir` and exercised by `nika-arm` under ADR-118.

## Decision

Create `nika-serve` as an L4 workspace member whose first surface is a durable,
transport-free `JobStore`.

### 1. One descriptor-rooted store

`JobStore::open` admits one existing operator-owned root through
`nika_fs::OwnedDir` and creates a contained `jobs` directory. After that
admission, all reads, locks, temporary writes, renames, and directory syncs are
relative to held descriptors. Caller data is never used as a child pathname.

The store keeps an explicit `initialized.json` marker beside one versioned JSON
snapshot in `state.json`. Under the first kernel lease, a pristine store with
neither file persists an empty snapshot and then the marker before opening
succeeds. Once the marker exists, a missing or renamed-away `state.json` is
corruption rather than an empty store. A state file without its marker also
refuses, so an interrupted first initialization cannot be silently adopted.

The witness is scoped to the admitted `jobs` directory. An actor able to remove
that directory, or both marker and snapshot together, has already crossed the
operator-owned storage boundary; the next open cannot distinguish that event
from intentional provisioning of a new store. W10 operations must protect and
back up the root. W05 detects partial loss and corruption, not total estate
destruction by an actor with host filesystem authority.

Every mutation writes and syncs a temporary regular file, renames it
descriptor-relatively, then syncs the held directory. A missing, malformed,
truncated, unsupported, or invariant-breaking initialized snapshot refuses at
startup and before every later operation; there is no partial recovery or
lossy default.

### 2. Concurrency precedes idempotency

Every operation holds both an in-process mutex and a kernel advisory exclusive
lease. Under that lease it reloads the durable snapshot before deciding. This
makes two threads, independently opened store instances, or processes observe
one key-binding decision rather than race independent in-memory maps. The
focused proof opens separate `JobStore` values on one root, verifies their
nonblocking leases contend on the same kernel lock, then races their admission
calls.

An `IdempotencyKey` contains bounded visible ASCII and is stored as data. A
`RequestDigest` accepts only canonical lowercase hexadecimal for 32 digest
bytes; uppercase or mixed-case input is rejected rather than normalized. The
first pair creates an opaque random `JobId`; the same pair returns that record;
the same key with a different digest returns `Conflict` without mutation.

### 3. Status and events are persisted contracts

The status vocabulary is exactly `queued | running | paused | succeeded |
failed`. The legal edges are:

```text
queued  -> running | failed
running -> paused | succeeded | failed
paused  -> running | failed
```

Terminal states do not reopen. An illegal edge refuses before writing.
`running` and `paused` survive restart exactly as recorded; W05 does not invent
automatic retry or recovery authority. Replaying an interrupted request
returns the existing job, so a restart cannot create a second runnable job for
the same admitted request.

Each `JobEvent` receives a contiguous per-job sequence starting at one.
`append_events` assigns and persists those numbers under the same lease;
`events_after` returns the suffix strictly after a caller cursor. A cursor above
the latest durable sequence returns typed `CursorBeyondLatest` instead of
pretending an unknown future position is an empty suffix. This is the durable
resume substrate required by future SSE, not an SSE implementation.

### 4. The first boundary is intentionally smaller than Serve

W05 adds no listener, HTTP route, authentication, source lookup,
`ExecutionService` integration, cancellation, artifact path, retry worker, or
trace scan. `nika-serve` remains a workspace WIP until the later admission wave
closes the full twelve-gate ledger.

## Consequences

### Positive

- Process restart cannot erase an idempotency binding, `paused`, or an event
  resume cursor.
- Empty first initialization is durable, while later `state.json` loss fails
  closed instead of manufacturing a pristine store.
- Conflict and transition verdicts are decided from validated durable state
  while both local and kernel exclusion are held.
- Visible root replacement and planted symlinks cannot redirect admitted I/O.
- W06 can consume one typed state API without coupling execution to HTTP.

### Negative

- The initial snapshot rewrites the complete job-state document per mutation;
  a measured scale signal is required before introducing sharding or a second
  persistence form.
- Advisory locking requires every compliant writer to use `JobStore`; raw
  external edits are detected as corruption only when they break validated
  invariants.
- Interrupted `running` jobs remain explicitly unresolved. Automatic recovery
  waits for typed execution settlement authority.

### Neutral

- Event payloads are JSON values because the state plane sequences opaque
  interface events; later route schemas decide their public wire shapes.
- The store tracks the workspace version and is not published independently.
- `Conflict` is an admission verdict, not a storage error.

## Required evidence

1. Restart plus replay returns the original `JobId`.
2. Conflicting key reuse refuses without mutation.
3. Independently opened stores contend on one kernel lease, and concurrent
   duplicate admissions create exactly one runnable record.
4. Illegal lifecycle edges preserve the prior durable status.
5. Truncated, deleted, and renamed-away initialized state refuses at startup.
6. `paused` and interrupted `running` records survive restart.
7. Root symlinks, a planted `jobs` child, and visible-root replacement cannot
   redirect state.
8. Event ids remain contiguous, `events_after` resumes strictly after its
   cursor, and a cursor above the latest sequence returns a typed error.
9. Digest boundary cases reject uppercase, mixed-case, wrong-length, and
   non-hexadecimal forms.
10. Focused library tests, all-target clippy, and workspace formatting pass.

## Alternatives considered

### Keep jobs in route-local memory

Rejected. Restart would erase idempotency, status, and resume cursors, making
ADR-117's replay contract false.

### Add HTTP and persistence in one wave

Rejected. It would combine storage semantics, authentication order, path
confinement, execution composition, and transport behavior in one trust-boundary
change before the shared execution authority is integrated.

### Use caller keys or job ids as filesystem names

Rejected. It would turn wire input into traversal surface and make directory
layout part of the public contract. Keys and ids remain data in one held store.

### Recover malformed state best-effort

Rejected. Dropping a damaged key binding or event suffix can authorize a
duplicate execution. Corruption therefore fails closed.

## Related

- ADR-117 — authenticates and confines the future network projection.
- ADR-118 — admits the descriptor-rooted ARM custody precedent.
- `docs/crate-specs/nika-serve.md` — W05 public API and gate ledger.
