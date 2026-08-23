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

That per-operation lease is not server liveness. A claimed
`ServerIncarnation` therefore owns a separate nonblocking `server.lock` lease
for its entire lifetime. A second process cannot claim a generation or settle
jobs while the first server capability remains alive. Generation allocation is
persisted, and settlement consumes that generation even when it finds no
running jobs; replaying the capability cannot interrupt a job started later.

An `IdempotencyKey` contains bounded visible ASCII and is stored as data. A
`RequestDigest` accepts only canonical lowercase hexadecimal for 32 digest
bytes; uppercase or mixed-case input is rejected rather than normalized. The
first pair creates an opaque random `JobId`; the same pair returns that record;
the same key with a different digest returns `Conflict` without mutation.

### 3. Status and events are persisted contracts

The status vocabulary is exactly `queued | running | interrupted | paused |
succeeded | failed`. The public legal edges are:

```text
queued  -> running | failed
running -> paused | succeeded | failed
paused  -> running | failed
```

Terminal states do not reopen. `interrupted` is also terminal and has no public
incoming edge: only the crate-internal startup settlement may assign it after
the higher layer establishes a new exclusive server incarnation. The public
settlement method requires a leased `ServerIncarnation` capability that
external callers cannot construct. An illegal edge refuses before writing.
Every legal status change uses `transition_with_events`: the new status and at
least one event become visible through one snapshot replacement or neither
does. Startup settlement follows the same invariant internally by appending a
chained `interrupted` event carrying current/prior incarnation generations in
the one replacement that sets the status. `paused` survives restart exactly as recorded; plain opening preserves
`running` until that settlement authority acts.
Replaying an interrupted request returns the existing job, so a restart cannot
create a second runnable job for the same admitted request.

Each `JobEvent` receives a contiguous per-job sequence starting at one.
`append_events` assigns and persists those numbers under the same lease;
`events_after` returns at most a validated `EventPageLimit` from the suffix
strictly after a caller cursor. Payloads are capped at 64 KiB encoded, append
batches at 64 events, pages at 256 events, and the complete snapshot at 4 MiB.
All four limits refuse before durable mutation.

Every event also stores `previous_hash` and `hash`. The SHA-256 preimage is
domain-separated by `nika.job-event.chain`, versioned, and canonically encodes
the job id, request digest, sequence, previous hash, and complete JSON payload.
The chain is unkeyed: it is an internal-consistency check, not a MAC and not a
signature. Every preimage input is data the snapshot itself carries, so any
writer able to rewrite `state.json` can recompute every link. Head and count
duplication plus link validation therefore detect accidental or non-coherent
corruption — partial writes, inconsistent truncation, bit flips, and edits by a
writer that does not recompute the chain. They do not detect a coherent
rewrite: an actor holding the snapshot can delete, reorder, graft, or edit
payloads — including flipping an approval decision from deny to allow — and
emit a chain that validates. The journal, count, and head occupy one rewrite
domain.

Approval one-shot history therefore belongs to a separate authority, not to
the job snapshot. `JobStore::open_with_approval_history` accepts an
`ApprovalHistory` implementation whose retention domain cannot be coherently
rolled back by the actor that can rewrite `state.json`. On every load, the
store requires every journaled approval digest to exist in that authority. On
append, it asks the authority to atomically record the complete digest batch
only if every digest is globally unused, then writes the prepared snapshot.
The authority may be ahead after a refused snapshot write or rollback; that
fail-closed burn is intentional. `JobStore::open` has no such authority, so it
refuses both approval appends and snapshots that already contain approvals.

An `approval_decided` payload still requires a canonical `digest`, and the
event chain places that digest at a job and position inside a self-consistent
journal. That placement is not authentication: the chain does not attest that a
recorded decision is the one a human made, and the snapshot writer can rewrite
the payload beside it. `ApprovalHistory` anchors exactly one property — one-shot
retention and reuse refusal for a digest — outside the snapshot's rewrite
domain; it authenticates neither the decision payload nor the journal. A
coherent approval-tail rewrite can therefore reopen or restate a decision, while
a burned digest still cannot be spent twice as long as the authority retains it.
A second ordinary file controlled by the same snapshot-rewrite actor does not
satisfy the `ApprovalHistory` contract. W06 must supply the runtime adapter and
durable anchor before binding its listener; the retention boundary of that
anchor is a deployment responsibility this ADR does not assign to a wave.
A cursor above the latest durable sequence returns typed
`CursorBeyondLatest` instead of pretending an unknown future position is an
empty suffix. This is the durable resume substrate required by future SSE, not
an SSE implementation.

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
- A live server lease excludes a second incarnation across processes.
- Status and lifecycle events cannot split across two durable writes.
- Approval decisions enter the validated job chain only after their digest is
  burned in a separately anchored, monotonic history, so a replayed digest
  refuses even though the chain itself is rewritable.

### Negative

- The initial snapshot rewrites the complete job-state document per mutation;
  a measured scale signal is required before introducing sharding or a second
  persistence form.
- Advisory locking requires every compliant writer to use `JobStore`; raw
  external edits are detected as corruption only when they break a validated
  invariant. An edit that recomputes the chain breaks none.
- The job snapshot and approval authority are not one atomic persistence
  domain. The approval digest is burned first, so a later snapshot-write
  failure can leave an authority-only record. This sacrifices retry
  availability to preserve one-shot safety; it never exposes a status without
  its event or vice versa.
- The event chain checks internal consistency against its stored head and links
  but is not a MAC, a signature, or a rollback witness. A writer able to rewrite
  the snapshot can construct any coherent history — shorter, reordered, grafted,
  or with edited payloads — and recompute valid links; approval reuse still
  refuses only while the separate authority survives.
- After a new exclusive server incarnation is established, its startup path
  atomically settles every ownerless `running` job as `interrupted` before
  exposure. Plain `JobStore::open` does not make that liveness judgment, so a
  concurrent handle cannot interrupt a live owner. The terminal ambiguity
  prevents automatic replay; explicit retry still waits for typed execution
  settlement authority.

### Neutral

- Event payloads are JSON values because the state plane sequences opaque
  interface events; later route schemas decide their public wire shapes.
- `JobStore` debug rendering is deliberately opaque and never delegates to the
  descriptor holder's path-bearing representation.
- The store tracks the workspace version and is not published independently.
- `Conflict` is an admission verdict, not a storage error.
- `JobStoreError::Io` retains only `std::io::ErrorKind`; path-bearing source
  context is erased at the public state-plane boundary. The W06 HTTP adapter
  must still map every variant to its own bounded response class rather than
  expose `Display`.

## Required evidence

1. Restart plus replay returns the original `JobId`.
2. Conflicting key reuse refuses without mutation.
3. Independently opened stores contend on one kernel lease, and concurrent
   duplicate admissions create exactly one runnable record.
4. Illegal lifecycle edges preserve the prior durable status.
5. Truncated, deleted, renamed-away, and unknown-future initialized state
   refuses at startup without rewrite.
6. Rendering or chaining a typed I/O refusal cannot disclose the durable root.
7. Public Serde construction cannot forge an invalid job id, idempotency key,
   or request digest.
8. `paused` survives restart, while a lifetime-held server lease and persisted
   one-shot generation settle ownerless `running` as terminal `interrupted`
   before exposure without interrupting a later live owner.
9. Root symlinks, a planted `jobs` child, and visible-root replacement cannot
   redirect state.
10. Event ids remain contiguous, bounded `events_after` pages resume strictly
    after their cursor, and a cursor above the latest sequence returns a typed
    error.
11. Digest boundary cases reject uppercase, mixed-case, wrong-length, and
   non-hexadecimal forms.
12. Status and lifecycle events persist atomically; eventless and oversized
    transition events leave both unchanged.
13. Hash-chain validation rejects non-recomputed payload modification, interior
    deletion, permutation, and cross-job graft; approval decisions require a
    chained digest. No evidence claims detection of a rewrite that recomputes
    the chain, which the unkeyed construction cannot provide.
14. A coordinated approval-tail rollback that consistently rewrites the event
    list, count, and head cannot release the digest: subsequent reuse is
    refused by the separate monotonic authority.
15. Debug output carries no durable-root sentinel, and payload, batch, snapshot,
    and page overflows refuse without durable mutation or rewrite.
16. Focused library tests, all-target clippy, and crate formatting pass.

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

### Put another head or approval set beside `state.json`

Rejected. A sidecar that the same actor can roll back together with the
snapshot only relocates the false claim. Approval one-shot history must be
owned by an authority with an independent retention boundary; otherwise the
serve adapter remains fail-closed.

## Related

- ADR-117 — authenticates and confines the future network projection.
- ADR-118 — admits the descriptor-rooted ARM custody precedent.
- `docs/crate-specs/nika-serve.md` — W05 public API and gate ledger.
