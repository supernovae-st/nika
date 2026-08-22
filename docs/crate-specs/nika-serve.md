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
| Dependencies | `nika-fs` (`OwnedDir`) · `nix` (kernel advisory lease) · `serde`/`serde_json` · `thiserror` · `uuid` |

## 1. Boundary

`nika-serve` is the future L4 network projection over the shared execution
authority required by ADR-117. W05 implements only its state plane. It owns no
listener, route, authentication, SSE stream, workflow lookup, execution
composition, cancellation, or artifact path.

The store accepts one existing operator-owned root, opens it once through
`nika_fs::OwnedDir`, creates the contained `jobs` directory through held
descriptors, and never trusts the visible root path again. Its fixed children
are `store.lock` and `state.json`; caller input never becomes a child name.

## 2. Public surface

- `JobId` is an opaque random UUID. It is non-sequential and carries no path or
  workflow name.
- `IdempotencyKey` accepts 1–255 visible ASCII bytes. It is data inside the
  snapshot, never a filename.
- `RequestDigest` is a canonical 32-byte digest encoded as lowercase hex.
- `JobStatus` is exactly `queued | running | paused | succeeded | failed`.
- `JobRecord` binds id, key, request digest, and status.
- `JobEvent` carries one JSON payload and a store-assigned per-job sequence.
- `Admission` returns `Created`, `Existing`, or `Conflict`, each with the
  durable record that decides the verdict.
- `JobStore` exposes `create_or_replay`, `get`, `transition`, `append_events`,
  and `events_after` after construction with `JobStore::open`.

No public mutation accepts a filesystem path.

## 3. Durability and idempotence laws

1. Every operation takes an in-process mutex and a kernel advisory exclusive
   lease, then reloads and validates durable state.
2. A mutation becomes visible only through `OwnedDir::write_atomic`: synced
   temporary file, descriptor-relative rename, then directory sync.
3. Startup and every later operation reject malformed JSON, an unknown state
   version, invalid identifiers, duplicate ids or keys, and non-contiguous
   event sequences. Corrupt state is never interpreted partially.
4. The same idempotency key plus the same digest returns the same record. The
   same key plus another digest returns `Conflict` without mutation.
5. A new record starts `queued`. Legal edges are:

   ```text
   queued  -> running | failed
   running -> paused | succeeded | failed
   paused  -> running | failed
   ```

   `succeeded` and `failed` are terminal. Every other edge refuses before the
   snapshot changes.
6. Event sequences start at one and increase contiguously per job. An overflow
   refuses before durable mutation. `events_after` is the future SSE resume
   cursor; it is not a streaming implementation.
7. `running` and `paused` survive restart unchanged. Replaying an interrupted
   request returns the existing job instead of manufacturing another runnable
   job.

## 4. W05 verification

Inline library tests cover:

- restart plus identical replay;
- conflicting key reuse;
- concurrent duplicate admission;
- illegal transition with unchanged durable status;
- truncated snapshot refusal;
- `paused` round-trip across restart;
- symlinked roots, a planted `jobs` child, and visible-root replacement after
  descriptor admission;
- monotone event append and resume cursor;
- interrupted `running` replay with exactly one stored job.

The W05 command contract is:

```bash
cargo test -p nika-serve --lib
cargo clippy -p nika-serve --all-targets -- -D warnings
cargo fmt --all --check
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
| 12 ATOMIC | W05 ships as one scoped state-plane commit |

## 6. Explicit non-goals

No `ExecutionService` integration · no HTTP · no SSE · no authentication · no
listener · no CLI wiring · no workflow registry · no cancellation · no
artifact authority · no automatic retry of interrupted execution. Those
capabilities require their own typed authorities and tests before projection.

## 7. Related decisions

- ADR-117 — network access only behind explicit authority; `paused`, durable
  idempotency, and monotone SSE resume are required before routes.
- ADR-118 — descriptor-rooted custody precedent through `nika-fs::OwnedDir`.
- ADR-003 — full 12-gate admission protocol.
