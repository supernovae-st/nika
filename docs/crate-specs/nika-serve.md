# Crate spec — `nika-serve`

| | |
|---|---|
| Status | **WORKSPACE WIP**. Durable jobs, loopback HTTP, idempotent engine-token cancellation, resumable SSE with heartbeat/reconnect guidance, OpenAPI 3.1, SIGTERM drain, typed token-file refusal, failed-job NIKA codes, and 422 capture diagnosis. Artifacts/`POST /v1/run` stay absent; trace verification returns an honest typed refusal until a real remote journal authority exists. 12-gate crate admission still pending. |
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


### Journal mirror evidence

The resident inspects the trace writer's first error before and after sealing
and finalization. A completed execution with a failed mirror keeps its runtime
status and carries `JournalEvidence::MirrorLost` (`write_failed` or
`record_refused`). The same terminal event owns this metadata in durable state,
GET job and SSE. Raw I/O text and paths are never projected; a lost mirror
cannot advertise a receipt chain head. An absent field makes no health claim.
Resuming clears the current-leg projection while retaining the previous
pause's evidence in its event. The read-only resident report counts jobs with
recorded losses for `nika doctor`; that census does not verify journals.


## 1. Boundary

`nika-serve` is the L4 network projection over the shared execution
authority required by ADR-117. W05 established its state plane. W06 adds a
real Hyper/Tokio TCP listener, deny-by-default Bearer authentication, a
held `.nika` registry, `ExecutionService` admission, an injected
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
- `JobStatus` is exactly
  `queued | running | interrupted | paused | succeeded | failed | cancelled`.
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
  `serve_until` stops admission and gives running and queued jobs one shared
  grace period (30 seconds by default, with four concurrent workers).
- `ExecutionBackend` receives only `ExecutionContext` over the immutable
  world admitted by `ExecutionService`. It is asynchronous, receives the
  run-scoped `CancelCtx` through an additive default method, remains
  cancellable by drop, and maps `Succeeded | Paused | Failed | Cancelled`
  onto durable status.

No public job mutation accepts a filesystem path. Startup paths live only in
`ServerConfig`; its `Debug` view deliberately omits them and the token source.

### W06 HTTP contract

| method | route | authority | response allowlist |
|---|---|---|---|
| `GET` | `/health` | public | status, service, engine/protocol identity and `storeFormatVersion` for jobs and schedules |
| `GET` | `/v1/workflows` | exactly one Bearer | contained `.nika` relative names |
| `GET` | `/v1/workflows/{name}` | exactly one Bearer | `{ "workflow": "<contained name>" }` |
| `POST` | `/v1/jobs` | exactly one Bearer + `Idempotency-Key` | opaque id + status · 422 `{error:{code,message}}` names the capture NIKA code when stamped · also 400/408/409/413/415/503/507 |
| `GET` | `/v1/jobs/{id}` | exactly one Bearer | opaque id + status · optional `{error:{code,message}}` on `failed` |
| `GET` | `/v1/jobs/{id}/status` | exactly one Bearer | status only · diagnosis lives on GET job and SSE |
| `GET` | `/v1/jobs/{id}/events` | exactly one Bearer | SSE `text/event-stream`; `id:` sequence; `data:` `{sequence,kind,status}` plus optional redacted `{code,message}` |
| `POST` | `/v1/jobs/{id}/cancel` | exactly one Bearer | idempotent terminal job result; active runs receive the engine cancellation token before durable `cancelled` settlement |
| `GET` | `/v1/jobs/{id}/trace/verify` | exactly one Bearer | typed `unavailable` verdict; no path or invented verification while the remote trace-journal authority is absent |
| `POST` | `/v1/compile` | exactly one Bearer | the Compile core's machine document (`compile_version` 1; 2 when a native call happened) as authoring DATA: 200 for `ready`, `incomplete` and `refused` · 422 typed protocol refusals · also 408/413/415/500/503 · 409 on a native server (below) |
| `GET` | `/v1/openapi.json` | exactly one Bearer | OpenAPI 3.1 document of the live routes |

`/health` advertises `jobInputs` when the named job envelope accepts and
validates literal JSON input bindings. Clients must require this capability
before sending inputs: older residents may accept unknown request fields
without applying them. This capability does not authorize snapshot overlays.

### Authoring door (`POST /v1/compile` · #1670)

The compile route is the HTTP transport of the one stateless Compile core
(`nika_onboard::compile::compile`), reached through a lateral L4→L4 edge (the
`nika-cli-host → nika-onboard` precedent). It holds no authoring semantics: no
routing, assembly, policy hole or Check projection lives in this crate. The
response body is `nika_onboard::compile::outcome_document`, the same document
`nika compile --json` prints, without the CLI-only `written`.

Foundation scope, unchanged by this transport: CREATE resolves an exact
embedded skeleton name (or `hello`), EDIT changes one existing constant,
answers are explicit JSON literals. Any other intent is the core's
`incomplete`, never a substitute workflow. General language assembly,
HOT/WARM/COLD resolution, setup requirements and suggested bindings are
absent, and exact-skeleton reuse is not a measured HOT admission.

| concern | contract |
|---|---|
| request | `{compile_version: 1, mode: "create", intent, workflow_id?, answers?}` or `{compile_version: 1, mode: "edit", source, change, answers?}` where `change` is `{text}` or `{set_constant: {name, value}}` · optional `cognition: "deterministicOnly"` |
| shape policy | unknown fields, a present `null`, duplicate keys (envelope and `answers`) and positional arrays refuse `malformed_compile_request`; foreign vocabulary is named: `compile_version_unsupported`, `compile_mode_unsupported`, `compile_cognition_unsupported` |
| literals | `answers` values and `set_constant.value` reach the core as the exact text the caller sent (`serde_json` `RawValue`), parsed once, as the CLI's `KEY=JSON_LITERAL` is |
| bounds | body `min(listener ceiling, 1 MiB)` → 413 · `intent`/`change.text` 4 KiB · `source` 512 KiB · `workflow_id`/`set_constant.name` 128 B · 64 answers × (256 B key, 64 KiB literal) → 422 `compile_limit` · all UTF-8 bytes |
| custody | no path field exists; an EDIT base travels inline; the served registry is never opened; nothing is written |
| effects | none: no job, run, approval, trace, schedule or provider contact. `check_preview` is a REVIEW of the source alone; `POST /v1/jobs` judges a candidate again |
| concurrency | `ServerLimits::with_max_compile_requests` (default 4) compile slots; a slot lives inside the blocking closure, so a timed-out or disconnected caller does not free CPU still in use; excess → 503 `compile_busy`, nothing queues |
| machinery failure | `CompileError` or a panicked task → 500 `internal_error`; nothing is echoed |

`/health` advertises `compile` exactly when this route is served. The token
means "this door speaks the `compile_version` 1 foundation wire" and promises
no authoring cognition beyond what the document's provenance states. The
native door advertises its own `compile` token for `nika compile --json`; the
two capability lists are separate projections, so neither door can advertise
a route only the other serves. A resident without the token must be refused
by the client, never replaced by a local compile with a different core.

### Native authoring (`POST /v1/compile` generation 2 · S06)

Off unless the operator seats it when building the server:
`ServerConfig::with_native_authoring(NativeAuthoring::new(model, providers))`
or `nika serve … --authoring-model provider/name` (requires `--bind`; the
provider configuration is read from the environment only then, through
`nika_runtime::compose::config_from_env`). A default server is byte-identical
for every request: generation 2 there is `422 compile_version_unsupported`,
and `/health` never lists `compileNativeV2`. On a native server generation 1
keeps its parser, core call, slot and request deadline, byte for byte.

| concern | contract |
|---|---|
| operator seat | ONE direct provider model (a harness seat, an unknown provider or a missing key refuses startup) · strategy fixed `only`, one sample, no decision seat · bounds: output tokens per call 1..=32768 (default 8192), call timeout ≤ 600 s (120), request deadline ≤ 3600 s (300), repairs 0..=5 (3) · optional Foundry snapshot opened, verified and pinned (manifest and rows sha256) at attach through the shared `nika_cli_host::compile::{config, knowledge}` · replay store 1..=1024 rounds (32) for ≤ 24 h (30 min) · all validated in `BoundServer::attach` before bind (`ServerError::NativeAuthoring`) |
| fresh request | `{compile_version: 2, cognition: "explicitProvider", mode: "create", intent, workflow_id?, answers?, limits?}` or `mode: "edit"` with `source` and `change` (a `change.text` requires `original_intent`; `set_constant` refuses it; `workflow_id` is create-only) · `limits: {repairs?, max_tokens?, call_timeout_ms?, deadline_ms?}` may only narrow the operator's bounds (above → `422 compile_limit`, never clamped) · `answers["intent.clarification"]` → `422 compile_new_intent_required` |
| replay request | the same input repeated byte for byte with `cognition: "deterministicOnly"` and `replay_token` (64 lowercase hex); no `limits` · zero provider calls |
| shape policy | the generation-1 policy plus: a literal that repeats an object key at any depth (or nests past the parser's 128 levels) → `422 malformed_compile_request`; caller-named model, endpoint, credential, path, snapshot, strategy or plan fields are unknown fields |
| answer | 200 with the core's unchanged `outcome_document` (`compile_version` 2 iff a call happened; a skeleton, a structured constant or a replay answers 1) · `Cache-Control: no-store` · a fresh round that leaves a native plan carries `Nika-Compile-Replay: <token>` |
| knowledge | every generation-2 round reopens the pinned snapshot and compares its manifest and rows (`409 compile_context_changed` before any call); a fresh round composes the pack for its intent (a revision's `original_intent` + change) and records the identity with the snapshot directory and files root removed; `pack_sha256` and the instruction sha256 in the receipt name what the seat read |
| provider | a per-request client over the runtime's provider transport (`provider_http` · `ProviderRegistry`) behind a gate: at most `1 + repairs` LOGICAL calls (one `infer` each, the receipt's `calls`), none once the round must stop, and every `ProviderError` reaches the core as a fixed reason (HTTP status, rate limit, credentials refused, model not served, connection cut) — never the provider's text · one logical call may be several HTTP attempts: the transport resends the same request after a 429, 503 or 529 (at most 3 more, inside the call's timeout), so the HTTP envelope is at most `4 × (1 + repairs)` requests · the receipt's backend is `{kind: direct_api, provider, cost_basis}` |
| deadlines and slots | on a native server the route leaves `/v1/compile` to bound itself: intake, generation 1 and replays keep the request deadline (`408 request_timeout`); a fresh round runs under its seat deadline, ABSOLUTE from admission (`408 compile_deadline_exceeded`: the work stopped or never began, nothing kept, a call in flight may still be billed) — a round that starts after it (a busy blocking pool) never begins, the stop is raced against the work and checked before every call, and an outcome that arrives after it is never answered or kept · the compile slot, then a replay place, are taken before any call and live inside the blocking work, so a disconnected or timed-out caller never frees them early (`503 compile_busy` · `503 compile_replay_capacity`) |
| shutdown | a stopping server first joins its connections, then halts every native round of its seat (no further call, `503 stopping` for a round still answering) and waits — within the shutdown grace, else `ServerError::ShutdownTimeout` — until every compile slot is free before the authority drains; a round's provider request is dropped, not awaited |
| replay store | in memory, per bound server: a restart or another instance knows no token (`409 compile_replay_unavailable`) · the exact input tuple is compared (`409 compile_replay_input_changed`) · expiry on the monotonic clock, never renewed · ≤ 2 MiB per round (larger: no token, the answer unchanged) · 256-bit `getrandom` tokens, never reflected in a refusal |
| disclosure | a document carrying a withheld value is refused whole: `500 compile_disclosure_refused` · withheld: the key the seat's provider RESOLVES (`ResolvedProvider::key` — a typed `ProvidersConfig` key or the environment's, by the configuration's own precedence), plus every `NativeAuthoring::with_withheld` value · every nonempty value counts, however short, raw or JSON-escaped |
| effects | none beyond the seat's calls: no job, run, approval, trace, schedule, file, registry entry or permission; `POST /v1/jobs` judges any candidate again |

Limits: the store is not a deduplication of paid work — a first answer lost in
transit leaves no token and a new fresh round spends again; the compiler never
repeats a logical call (the provider transport's own 429/503/529 resend is the
one exception, counted as HTTP attempts, never as calls). Remote billing cannot
be stopped by a local deadline or a shutdown. Harness seats,
decision seats, other strategies, knowledge pack files and the observed-world
reader are not served remotely. The committed `openapi.json` still describes
generation 1 only: the generation-2 schema, the header and the 409 codes are
owed in a coordinated OpenAPI/SDK update.

Artifact routes return 404. No route returns the bytes of a served workflow,
idempotency keys, request digests, event payloads, provider/tool data, paths,
token material, or internal error text. CORS headers are not emitted.
`Last-Event-ID` resumes after that sequence. An invalid cursor is 400; a
cursor beyond the latest persisted sequence is a typed 400. The request
timeout does not bound an open event stream. Events become visible only
after durable persist. A slow client is dropped rather than stalling
execution. Every stream advertises a 100–30,000 ms bounded reconnect delay;
heartbeat comments carry no `id:` and therefore never advance replay state.

### Persistent resident shutdown

Ctrl-C/SIGINT and, on Unix, SIGTERM take the same shutdown path: close the
HTTP listener and its connections, stop new scheduling and admission, and
drain **all already-admitted work**, including the queue. The default is one
30-second grace period with four concurrent workers, not 30 seconds per job.
Thus forty jobs lasting three seconds each can occupy roughly thirty seconds;
a resident still draining after five seconds has not exceeded this contract.
Embedders can select another grace with `ServerLimits`; the CLI uses the default.

If the queue finishes within the grace, the resident exits 0. At grace expiry,
it aborts the remaining execution futures, records running jobs as `interrupted`
with durable terminal events, preserves jobs still `queued`, and exits 1
(`ShutdownTimeout`). Restart with the same `--state-root` resumes queued jobs
from the snapshots captured at admission, even if the live workflow files have
changed. Interrupted jobs retain their receipts and are not automatically retried.
SIGKILL skips cleanup; the next resident first interrupts ownerless running jobs,
then resumes the still-queued jobs. Killing a process does not prove that its
external effects did not happen.

The grace bounds asynchronous execution draining. It is not a hard deadline
for process exit: cancellation must yield, scheduler/backend cleanup must join,
and durable settlement requires filesystem writes. A stuck backend or filesystem
can delay these steps. Supervisors should allow additional cleanup time beyond
30 seconds before forcing SIGKILL; the supplied systemd unit uses 45 seconds
(30 for draining plus 15 for cleanup). This allowance cannot bound a stuck
filesystem; a forced kill may defer settlement to restart.

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
   queued  -> running | failed | cancelled
   running -> paused | succeeded | failed | cancelled
   paused  -> running | failed | cancelled
   ```

   `interrupted`, `succeeded`, `failed`, and `cancelled` are terminal. `interrupted` has no
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
  limits, slow-body timeout, contained-path refusal, and absent artifact
  authority routes;
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
  bounded reconnect guidance, cursor-neutral heartbeat comments, and redaction
  of payload extras including interrupted incarnation fields.
- queued cancellation without backend entry, twelve-way running cancel races,
  terminal idempotent replay, durable cancellation receipt identity, and
  authentication before job lookup.
- run-scoped trace verification returns a typed `unavailable` reason without a
  remote filesystem path; no artifact or trace store is invented.

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

No TLS · no workflow upload · no artifact authority · no `POST /v1/run` · no
automatic retry of interrupted
execution. OpenAPI 3.1 is the live authenticated route table
(`GET /v1/openapi.json`): it names the live POST statuses and omits
artifacts and `POST /v1/run`. The typed trace-verification route refuses
`unavailable` until execution supplies a held remote journal authority; a
chain head alone is not verification. The store records the lost
ownership but cannot prove whether an effect committed before the
crash. W05 also provides no concrete durable
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

## Caller inputs on named jobs (#1642)

`POST /v1/jobs` accepts the closed by-name envelope
`{ workflow, inputs?, access? }`. A present `inputs` must be a JSON object,
and a present access pin must be a nonempty string; null never erases either.
Other envelope fields are refused. Inputs use the declared workflow keys and
the canonical TypeExpr fit, with the runtime's required-input refusal before a
job is persisted. There is no CLI coercion, `@env:` lookup or expression
interpretation: JSON strings are literal data. Declared defaults remain in the
workflow; source, permits and model are never rewritten.

Source-only `POST /v1/check` continues to accept required-input declarations
without launch values; it rejects supplied input maps rather than ignoring them.
The snapshot form rejects input overlays, including empty objects and null.
A snapshot launch with unsupplied required inputs is also refused before a job
exists; source-only Check can still accept its declarations.
A snapshot remains the frozen byte world; no request field can silently overlay
it. No model/spend controls or jobs collection route are added.

Exact request bytes, including inputs, remain the idempotency identity. The
validated caller map is persisted on the durable job, with input-bearing jobs
receiving a queued event whose preimage binds that map. Execution and terminal
event identity also bind inputs. Jobs without overrides retain the previous
preimages. Queue recovery reloads the validated store record and frozen world,
not the live registry. The production backend supplies the map through
`ServiceExecutionOptions::with_inputs`; custom backends must opt into the input
method or refuse nonempty bindings. Existing access pins keep their path and
fail-closed backend contract.

Input origins are journaled at boot: supplied API values are `api-caller`, and
authored defaults remain `file`. The additive `InputOrigin::ApiCaller` owner is
`nika-types/src/origins.rs`; it must align with the closed origin vocabulary in
`nika-spec/spec/04-variables.md` (Input origins, NEP-0014 law 2) before integration.
No actor identity, human consent, CI context or grant follows from this channel.
The receipt continues to name job/execution/trace/snapshot identity; input origin
claims belong to the journal and its evidence projection, not a fabricated
receipt field. Hash checks detect inconsistent edits, not a coherent rewrite by
an attacker controlling the entire local store and its unkeyed hashes.
