# Crate spec — `nika-serve`

| | |
|---|---|
| Status | **WORKSPACE WIP — W10 OPS**. Durable jobs, loopback HTTP, SSE, OpenAPI 3.1, SIGTERM drain, and an honest doctor row for the token file. Cancel/artifacts stay absent. |
| Layer | L4 — remote execution interface projection |
| Purpose | Persist request admission, lifecycle status, and resumable event cursors, and project the first authenticated HTTP routes over that state. |
| LOC budget | ≤5,000 source lines for the state plane; ≤15,000 hard crate cap. |
| File cap | ≤1,500 lines. |
| Function cap | ≤100 lines. |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Publish | `false` — engine-internal interface crate |
| Dependencies | `nika-execution` · `nika-runtime` identity · `nika-fs` · Hyper/Tokio · `http-body-util` · SHA-256 + `subtle` · `zeroize` · `nix` · Serde · `thiserror` · `uuid` |
| NIKA codes | **none** — `JobStoreError` is an L4 transport-surface error, never a workflow/verb error; the HTTP adapter maps it to bounded response classes. |

## 1. Boundary

`nika-serve` is the L4 network projection over the shared execution
authority required by ADR-117. W05 established its state plane. W06 adds a
real Hyper/Tokio TCP listener, deny-by-default Bearer authentication, a
held `.nika.yaml` registry, `ExecutionService` admission, an injected
`ExecutionBackend` seam, and the first job/workflow routes. W07 projects
the durable job journal over `GET /v1/jobs/{id}/events` as SSE. It does not
import `nika-cli`. Default `nika serve` remains the resident ARM firer.
The CLI admits the `--bind` + `--workflows` + `--token-file` pair (and
refuses `--once`/`--dry` with bind); `nika_serve::serve_http` is the
listener entry. Wiring `nika-serve` as a `nika-cli` dependency is a
follow-up pathspec: this file must not mention sockets (Gate 1).

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
- `JobStore` exposes `create_or_replay`, `create_or_replay_bounded`, `get`,
  `transition_with_events`, `append_events`, `events_after`, and
  authority-gated `settle_interrupted_jobs`. `JobStore::open` refuses
  approval appends and existing approval history;
  `open_with_approval_history` is required for those operations. Wire
  adapters parse opaque ids with `JobId::parse`.
- `ServerConfig` requires bind, workflow root, state root, and token-file
  source. `ServerLimits` names body, request, execution, shutdown,
  active-job, queue, connection, SSE-client, header, and durable-job ceilings.
- `BoundServer::bind` validates and acquires all authority before listening;
  `serve_until` stops admission and gives active jobs a bounded grace period.
- `ExecutionBackend` receives only `ExecutionContext` over the immutable
  world admitted by `ExecutionService`. It is asynchronous, cancellable by
  drop, and maps only `Succeeded | Paused | Failed` onto durable status.

No public job mutation accepts a filesystem path. Startup paths live only in
`ServerConfig`; its `Debug` view deliberately omits them and the token source.

### W06 HTTP contract

| method | route | authority | response allowlist |
|---|---|---|---|
| `GET` | `/health` | public | status, service, four `EngineIdentity` fields |
| `GET` | `/v1/workflows` | exactly one Bearer | contained `.nika.yaml` relative names |
| `GET` | `/v1/workflows/{name}` | exactly one Bearer | `{ "workflow": "<contained name>" }` |
| `POST` | `/v1/jobs` | exactly one Bearer + `Idempotency-Key` | opaque id + status |
| `GET` | `/v1/jobs/{id}` | exactly one Bearer | opaque id + status |
| `GET` | `/v1/jobs/{id}/status` | exactly one Bearer | status only |
| `GET` | `/v1/jobs/{id}/events` | exactly one Bearer | SSE `text/event-stream`; `id:` sequence; `data:` `{sequence,kind,status}` |
| `GET` | `/v1/openapi.json` | exactly one Bearer | OpenAPI 3.1 document of the live routes |

Cancel and artifact routes return 404. No route returns source bytes,
idempotency keys, request digests, event payloads, provider/tool data, paths,
token material, or internal error text. CORS headers are not emitted.
`Last-Event-ID` resumes after that sequence. An invalid cursor is 400; a
cursor beyond the latest persisted sequence is a typed 400. The request
timeout does not bound an open event stream. Events become visible only
after durable persist. A slow client is dropped rather than stalling
execution.

On Unix, the token file must be opened no-follow/nonblocking as a regular
owner-only file. It contains 32–512 visible ASCII bytes (one trailing line
ending is accepted); raw bytes are zeroized after hashing. Comparisons use
fixed-size constant-time equality. Compressed request bodies are refused.

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

## 4. W05 + W06 verification

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
- real loopback HTTP health, workflow list/metadata, job-create, job-read,
  status, and job-event SSE requests;
- valid authentication plus uniform missing, duplicate, malformed, wrong, and
  oversized credential refusal;
- auth-before-parse, invalid JSON/content type, coarse and streaming body
  limits, slow-body timeout, contained-path refusal, and absent authority
  routes including cancel/artifacts;
- twelve concurrent identical POSTs producing one backend call and one id;
- `paused` through both public response types;
- bounded execution timeout and bounded graceful shutdown;
- restart settlement of a live job to `interrupted`, followed by identical
  replay with zero calls into the replacement backend;
- exact active-run and queued-job boundaries, durable job capacity, exact and
  excess header counts, connection saturation, credential FIFO refusal, and
  fail-fast store contention followed by clean incarnation release.
- SSE Bearer-before-lookup, allowlisted `{sequence,kind,status}` frames,
  `Last-Event-ID` resume, invalid and future cursors, request-timeout bypass,
  slow-client drop, disconnect without blocking execution, SSE client ceiling,
  and redaction of payload extras including interrupted incarnation fields.

The W05/W07 focused command contract is:

```bash
cargo test -p nika-serve --lib
cargo test -p nika-cli --lib -- serve
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

No TLS · no OpenAPI/SDK projection · no workflow upload · no
cancellation · no artifact authority · no automatic retry of interrupted
execution. The store records the lost ownership but cannot prove whether an
effect committed before the crash. W05 also provides no concrete durable
`ApprovalHistory`; an in-process or same-filesystem sidecar that the state
writer can roll back does not meet the contract. W06's HTTP adapter
establishes the exclusive server incarnation and calls crate-internal
settlement before binding the listener; it does not replace the approval
history authority. Operational retention of that external anchor is a
deployment responsibility this spec does not assign to a wave. Those
capabilities require their own typed authorities and tests before projection.

## 7. Related decisions

- ADR-117 — network access only behind explicit authority; `paused`, durable
  idempotency, and monotone SSE resume are required before routes.
- ADR-118 — descriptor-rooted custody precedent through `nika-fs::OwnedDir`.
- ADR-003 — full 12-gate admission protocol.
