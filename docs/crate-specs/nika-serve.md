# Crate spec — `nika-serve`

| | |
|---|---|
| Status | **WORKSPACE WIP — W05 STATE PLANE**. The durable job API is present; transport and full 12-gate admission remain separate waves. |
| Layer | L4 — remote execution interface projection |
| Purpose | Persist request admission, lifecycle status, and resumable event cursors before any HTTP route exists. |
| LOC budget | ≤5,000 source lines for the state plane; ≤15,000 hard crate cap. |
| File cap | ≤1,500 lines. |
| Function cap | ≤100 lines. |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Publish | `false` — engine-internal interface crate |
| Dependencies | `nika-fs` (`OwnedDir`) · `nix` (kernel advisory lease) · `serde`/`serde_json` · `sha2` · `thiserror` · `uuid` |
| NIKA codes | **none** — `JobStoreError` is an L4 transport-surface error, never a workflow/verb error; the future HTTP adapter maps it to bounded response classes. |

## 1. Boundary

`nika-serve` is the future L4 network projection over the shared execution
authority required by ADR-117. W05 implements only its state plane. It owns no
listener, route, authentication, SSE stream, workflow lookup, execution
composition, cancellation, or artifact path.

The store accepts one existing operator-owned root, opens it once through
`nika_fs::OwnedDir`, creates the contained `jobs` directory through held
descriptors, and never trusts the visible root path again. Its fixed children
are `store.lock`, `server.lock`, `initialized.json`, and `state.json`; caller input never
becomes a child name.

## 2. Public surface

- `JobId` is an opaque random UUID. It is non-sequential and carries no path or
  workflow name. Its public Serde decoder rejects every non-canonical UUID v4,
  so deserialization is not a second constructor.
- `IdempotencyKey` accepts 1–255 visible ASCII bytes. It is data inside the
  snapshot, never a filename.
- `RequestDigest` is a canonical 32-byte digest encoded as lowercase hex.
  Uppercase and mixed-case strings are rejected, never normalized.
- `JobStatus` is exactly `queued | running | interrupted | paused | succeeded |
  failed`.
- `JobRecord` binds id, key, request digest, and status.
- `JobEvent` carries one JSON payload, a store-assigned per-job sequence, and
  its previous/current chain hashes.
- `EventPageLimit` admits 1–256 events; callers cannot request an unbounded
  durable suffix.
- `JobMutation` returns a status and the events committed with it in one
  snapshot replacement.
- `Admission` returns `Created`, `Existing`, or `Conflict`, each with the
  durable record that decides the verdict.
- `ServerIncarnation` is an unforgeable, generation-bound capability that owns
  the lifetime `server.lock` lease.
- `ApprovalHistory` is the injected monotonic authority for approval digests.
  Its implementation must atomically record a batch outside the job
  snapshot's rollback domain and verify that every journaled digest is already
  anchored. It anchors one-shot digest retention and reuse refusal only; it
  authenticates neither a decision payload nor the journal.
  `ApprovalHistoryError` exposes only bounded refusal classes.
- `JobStore` exposes `create_or_replay`, `get`, `transition_with_events`,
  `append_events`, `events_after`, and authority-gated
  `settle_interrupted_jobs`. `JobStore::open` refuses approval appends and
  existing approval history; `open_with_approval_history` is required for
  those operations.

No public mutation accepts a filesystem path.

## 3. Durability and idempotence laws

1. Every operation takes an in-process mutex and a kernel advisory exclusive
   lease, then reloads and validates durable state.
2. A mutation becomes visible only through `OwnedDir::write_atomic`: synced
   temporary file, descriptor-relative rename, then directory sync.
3. The first open persists an empty `state.json` plus an explicit
   `initialized.json` marker under the kernel lease. After that marker exists,
   missing or renamed-away state is corruption, not a new empty store. A state
   file without its marker also refuses.
   This guarantee assumes the admitted `jobs` directory or at least one witness
   survives. Coordinated removal of the directory, marker, and snapshot is
   host-authority destruction and is indistinguishable from intentional fresh
   provisioning; W10 owns root protection and backup.
4. Startup and every later operation reject malformed JSON, an unknown state
   version, invalid identifiers, duplicate ids or keys, and non-contiguous
   event sequences. Corrupt state is never interpreted partially.
5. The same idempotency key plus the same digest returns the same record. The
   same key plus another digest returns `Conflict` without mutation.
6. A new record starts `queued`. Legal edges are:

   ```text
   queued  -> running | failed
   running -> paused | succeeded | failed
   paused  -> running | failed
   ```

   `interrupted`, `succeeded`, and `failed` are terminal. `interrupted` has no
   public incoming edge; only the crate-internal startup settlement may assign
   it after the higher layer establishes a new exclusive server incarnation.
   Every other edge refuses before the snapshot changes. A legal transition
   requires at least one event and persists status plus events atomically.
   Startup settlement likewise appends a chained `interrupted` event with
   incarnation metadata in the same replacement as the status.
7. Event sequences start at one and increase contiguously per job. An overflow
   refuses before durable mutation. Payloads are at most 64 KiB encoded, append
   batches contain at most 64 events, the complete snapshot is at most 4 MiB,
   and `events_after` requires a 1–256 event `EventPageLimit`.
8. `paused` survives restart unchanged. After the new server incarnation owns
   the lifetime lease, its persisted generation settles ownerless `running` as
   terminal `interrupted` exactly once. Another process cannot claim the root
   until that capability drops; reusing a consumed capability is inert.
9. Event hashes use a versioned, domain-separated canonical preimage over job
   id, request digest, sequence, predecessor, and payload. Head/count plus each
   link are validated on every load. The chain is unkeyed and every preimage
   input lives inside the snapshot, so this is an internal-consistency check: it
   detects accidental or non-coherent corruption — partial writes, inconsistent
   truncation, and edits by a writer that does not recompute the links. It does
   not detect a coherent rewrite, which can delete, reorder, graft, or edit
   payloads — including deny to allow — and recompute a chain that validates.
10. `approval_decided` requires a canonical `digest`, placing the runtime claim
    identity inside the chain. Before snapshot persistence, the injected
    `ApprovalHistory` atomically burns the digest in a retention domain that
    the job-state writer cannot coherently roll back. That authority anchors
    one-shot retention and reuse refusal only; it does not authenticate the
    decision payload or the journal, so a coherent rewrite can still restate a
    decision. The history may be ahead after a failed snapshot write; reuse
    still refuses. A same-authority sidecar file is not sufficient. W06 supplies
    the real adapter and durable anchor; until then approval operations fail
    closed.
11. `Debug` for `JobStore` is opaque and cannot expose its held root.

## 4. W05 verification

Inline library tests cover:

- restart plus identical replay;
- conflicting key reuse;
- duplicate admission raced through independently opened stores, with exactly
  one `Created` verdict and one durable runnable record;
- nonblocking cross-open proof that separate stores contend on the same kernel
  lease;
- illegal transition with unchanged durable status;
- durable empty initialization plus truncated, deleted, renamed-away, and
  unknown-future snapshot refusal without rewrite;
- typed I/O refusal with path-bearing source context erased before the public
  boundary, including `Display` and error-chain non-disclosure;
- `paused` round-trip across restart;
- symlinked roots, a planted `jobs` child, and visible-root replacement after
  descriptor admission;
- monotone hash-chained event append, hard-capped resume pagination, and typed
  future-cursor refusal;
- explicit interrupted `running` settlement and replay with exactly one stored
  job, lifetime server-lease exclusion, and persisted one-shot generation;
- digest boundary-table rejection for uppercase, mixed-case, wrong-length, and
  non-hexadecimal inputs.
- public Serde forgery rejection for job ids, idempotency keys, and request
  digests, including control-character input.
- atomic status-plus-event refusal/success across reopen;
- non-recomputed modification, interior deletion, permutation, and cross-job
  event graft refusal — the unkeyed chain cannot refuse a recomputed rewrite,
  and no test claims it does;
- approval-digest chain binding, fail-closed missing/mismatched authority, and
  coordinated approval-tail rollback with consistent count/head mutation whose
  later digest reuse the retained authority refuses;
- sentinel-root debug non-disclosure;
- payload, batch, snapshot, and page boundary refusals without durable mutation.

The W05 command contract is:

```bash
cargo test -p nika-serve --lib
cargo clippy -p nika-serve --all-targets -- -D warnings
cargo fmt -p nika-serve -- --check
```

## 5. Admission ledger

This member stays in `[workspace.metadata.diamond].wip` until the later Serve
admission wave closes the gates whose authority does not exist in W05.

| Gate | W05 evidence |
|---|---|
| 1 SPEC | this document |
| 2 TDD | W05 job-store tests were observed RED before implementation, then GREEN |
| 3 IMPL | focused `nika-serve --lib` suite |
| 4 CLIPPY | focused all-targets command with warnings denied |
| 5 MUTATION | pending full crate admission |
| 6 PROPERTY | adversarial concurrency, corruption, and descriptor tests present; property floor pending admission |
| 7 BENCHMARKS | not applicable to the durability contract; filesystem sync dominates and no throughput claim is made |
| 8 DOCS | public API documented; dedicated rustdoc gate pending admission |
| 9 CANARY | pending the shared execution service and route projection |
| 10 PARITY | not applicable; this is a new authority required by ADR-117 |
| 11 REVIEW | pending full crate admission |
| 12 ATOMIC | W05 is one scoped state-plane diff; full crate admission remains pending |

## 6. Explicit non-goals

No `ExecutionService` integration · no HTTP · no SSE · no authentication · no
listener · no CLI wiring · no workflow registry · no cancellation · no
artifact authority · no automatic retry of interrupted execution. The store
records the lost ownership but cannot prove whether an effect committed before
the crash. W05 also provides no concrete durable `ApprovalHistory`; an
in-process or same-filesystem sidecar that the state writer can roll back does
not meet the contract. W06 must supply that adapter, establish its exclusive
server incarnation, and call the crate-internal settlement before binding the
listener. Operational retention of that external anchor is a deployment
responsibility this spec does not assign to a wave. Those capabilities require
their own typed authorities and tests before projection.

## 7. Related decisions

- ADR-117 — network access only behind explicit authority; `paused`, durable
  idempotency, and monotone SSE resume are required before routes.
- ADR-118 — descriptor-rooted custody precedent through `nika-fs::OwnedDir`.
- ADR-003 — full 12-gate admission protocol.
