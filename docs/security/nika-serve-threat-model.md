# `nika serve` network threat model

**Status:** binding design boundary for ADR-117; W06 admits authenticated
loopback HTTP and W07 projects the durable job journal over SSE. A checked
box in this document means the implementation and its test exist, not
merely that the design mentions them.

## Security objective

An authenticated operator may start one of the workflows deliberately exposed
under a held registry root and observe its typed job events. An unauthenticated
or malformed request must not cause parsing, lookup, execution, state mutation,
or disclosure beyond the minimal public health identity.

## Assets

- bearer credential and secret-source metadata;
- captured workflow, child-workflow, and skill bytes plus their logical paths;
- job inputs, outputs, status, cost, trace identity, and event journal;
- ARM claims, terminal receipts, and verified ledger projection;
- provider credentials and tool results;
- future artifact capabilities and cancellation authority;
- process availability, concurrency slots, and spend ceilings.

## Trust boundaries

| boundary | untrusted side | trusted side | admission law |
|---|---|---|---|
| socket | network client, including loopback peers | HTTP adapter | explicit bind; protected routes authenticate before parse |
| reverse proxy | forwarded headers and connection metadata | application identity | proxy does not grant auth; only the Bearer credential does |
| workflow registry | path/name from request | held registry root | relative `.nika` entry only; no absolute, traversal, or replaced link |
| source capture | mutable filesystem | owned bytes + logical base | one capture; check and run consume the same bytes |
| execution | admitted request | shared L3 service | idempotency bound before effects; runtime owns the verdict |
| event stream | job journal | SSE client | same auth as job; monotonic resume cursor; redacted payloads |
| cancellation | authenticated job id | run-scoped execution token | signal token before terminal mutation; idempotent replay cannot revive a job |
| trace verification | job trace identity | future remote journal authority | typed unavailable verdict today; never scan paths or claim verification from a chain head alone |
| artifact path | execution output | future download route | route absent until a typed held artifact manifest exists |

Loopback is not a trust boundary. A browser, local process, container, SSH
forward, or compromised developer tool can reach it.

## Attacker model

The design assumes an attacker can:

- connect repeatedly, pipeline or multiplex requests, disconnect mid-body, and
  reconnect SSE with arbitrary cursors;
- send duplicated/conflicting headers, invalid UTF-8, deep or oversized JSON,
  compressed bombs, slow bodies, and misleading content types;
- guess job ids and idempotency keys, replay old requests, and race identical
  requests;
- control names inside an otherwise operator-managed workflow tree and race
  path replacement where the platform permits it;
- author a workflow whose providers/tools return hostile, secret-shaped, or
  prompt-injection content;
- observe status codes, response sizes, timing, logs, and public health data;
- cause process restart between admission, execution, and response.

The design does not claim protection from an attacker who can read the server's
credential source or replace the running binary. It equally does not claim
protection from one who can write the durable job root: the event chain is
unkeyed and every preimage input lives in the snapshot, so such an actor can
delete, reorder, graft, or edit journal payloads — including flipping an
approval decision from deny to allow — and recompute a chain that validates.
Only the separately anchored `ApprovalHistory` survives that actor, and only to
refuse reuse of an already burned digest; it authenticates neither the decision
payload nor the journal. Resource exhaustion by an already authenticated
workflow remains bounded by workflow/runtime policy, not by authentication
alone.

## Route policy

| route class | public? | effects? | mandatory controls |
|---|---:|---:|---|
| `GET /health` | yes | no | fixed response schema; EngineIdentity only |
| `/v1/workflows` | no | no | Bearer auth before listing; `.nika` names only |
| `/v1/workflows/{name}` | no | no | Bearer auth; contained relative name metadata; no source bytes |
| `/v1/jobs/{opaque-id}` | no | no | Bearer auth before lookup; uniform unknown-id response |
| `/v1/jobs/{opaque-id}/events` | no | no | Bearer auth; bounded SSE buffer and reconnect delay; monotonic `Last-Event-ID`; cursor-neutral heartbeats; redaction |
| `POST /v1/jobs/{opaque-id}/cancel` | no | yes | Bearer auth before lookup; run-scoped engine token; one durable terminal receipt; idempotent replay |
| `GET /v1/jobs/{opaque-id}/trace/verify` | no | no | Bearer auth; typed unavailable verdict until a real journal authority exists; no paths |
| `GET /v1/openapi.json` | no | no | Bearer auth; live-route document; no credential examples or artifact paths |
| effecting `/v1/*` POST | no | yes | auth before parse; body limit; content type; idempotency before execution |
| `POST /v1/compile` | no | no (native server: the seat's calls only) | auth before parse; its own 1 MiB body ceiling and per-field bounds; content type; bounded concurrency; returns authoring data only; generation 2 exists only under an explicit operator seat, bounded in calls, tokens, time, slots and kept rounds |
| artifact routes | absent | — | remain absent until a held typed artifact manifest exists |

Adding a route cannot weaken this table. The OpenAPI document is a projection
of the live routes, never a second authority.

## Required controls

### Listener admission

- Bare `nika serve` opens no socket.
- `--bind` and `--workflows` are an inseparable pair.
- `--once`/`--dry` with `--bind` refuse before binding.
- Non-loopback addresses require `--allow-remote`.
- The listener never interprets `X-Forwarded-*` as authentication.

### Credential handling

- Credential bytes never enter argv, structured logs, traces, errors, panic
  messages, OpenAPI examples, or health output.
- Exactly one bounded Bearer value is accepted. Duplicate Authorization
  headers, alternate schemes, whitespace ambiguity, and oversized values
  refuse.
- Comparison is constant-time after a fixed-shape parse.
- Authentication and a coarse body-size gate precede body decoding and all
  workflow/job lookups.
- CORS is off by default. Any later allowlist uses exact origins; `*`, suffix
  matching, regex origins, and reflection are forbidden.

### Workflow and path custody

- Discovery accepts `.nika` only.
- Requested names are relative to one held registry root and cannot contain an
  absolute prefix, `..`, NUL, or platform separator ambiguity.
- Source, child workflows, and skills are captured with a logical base; the
  checked bytes are the executed bytes.
- A symlink or directory replacement cannot redirect a held open beneath the
  admitted root.
- Client input cannot select an arbitrary trace, ledger, secret, or artifact
  filesystem path.

### Authoring (`POST /v1/compile`)

The compile door is the HTTP transport of the one stateless Compile core. It
is not an effecting route, and it widens no other route's authority.

- It creates no job, run, approval, schedule or trace, and writes no file. No
  request field names a host path: an EDIT base travels inline, and the door
  never opens the served registry, so it cannot disclose a served workflow.
  The only source it returns is derived from the request's own inline source
  or from a skeleton embedded in the binary.
- On a default server it reads no environment variable and contacts no
  provider. The one authoring cognition is `deterministicOnly`; any other
  requested cognition is a typed refusal. Ambient provider keys are never
  consent. A native seat exists only when the operator builds it (below).
- `check_preview` is a review of the source alone. It grants nothing and admits
  nothing: a candidate reaches execution only through `POST /v1/jobs`, which
  judges it again under the real launch bindings.
- Caller source reaches the same strict parser the snapshot door already feeds
  (ADR-131). It is bounded tighter here (512 KiB) because authoring work runs
  on the blocking pool and cannot be cancelled once started.
- Concurrency is fenced by compile slots. A slot belongs to the CPU work, not
  to the request: a caller that times out or disconnects does not free it, so
  repeated cancellation cannot stack unbounded blocking work. Excess requests
  refuse `compile_busy`; nothing queues.
- Refusals are fixed strings. Intent, source, answers and literals never
  appear in an error body, and this route adds no logging.

### Native authoring (compile generation 2)

The only new effect is a call to the ONE provider model the operator seated
(`ServerConfig::with_native_authoring` · `nika serve --authoring-model`). The
server's Bearer is the whole authority domain: every holder may spend the
operator's provider budget up to `compile slots × (1 + repairs)` logical calls
in flight (each up to 4 HTTP attempts when the provider answers 429, 503 or
529) and one request deadline each, so the operator sizes those bounds and the
provider key's own quota for the credential's audience.

| threat | control |
|---|---|
| a default server, an unauthenticated or a malformed request reaches a provider | the seat exists only by operator construction (environment provider config is read only after the explicit flag); auth precedes the body; shape, vocabulary and bounds are judged before the slot, the kept-round place and any call — every refusal is zero calls |
| the caller chooses the model, endpoint, credential, host file, snapshot, strategy or plan | no such field exists (unknown fields refuse); `answers.model` is candidate data; paths in intent, source or answers stay data and are never opened |
| cost amplification | strategy `only`, one sample, a call gate at `1 + repairs` logical calls; output tokens, per-call timeout and request deadline are operator ceilings a caller may only narrow; the compiler repeats no call and nothing queues — only the provider transport resends a request the provider refused with 429/503/529 (at most 3 more HTTP attempts per logical call, inside its timeout) |
| cancelled work keeps spending or is freed early | the compile slot and the kept-round place live inside the blocking work; the round's deadline is absolute from admission, so work that starts late never begins and a caller already told « stopped » never pays later; the stop is raced against the work and checked before every call; an outcome after it is never kept; a provider may still bill a call cut in flight (stated in the refusal) |
| a stopping server leaves paid work running | shutdown halts every native round (no further call, no repair) and waits within its grace until every compile slot is free before the authority drains; provider requests in flight are dropped |
| provider error text leaks an endpoint, a body or a credential | every `ProviderError` reaches the core as a fixed reason; the transport's text never enters the document |
| the seat echoes the operator's credential | the key the seat's provider resolves — typed or from the environment, by the configuration's own precedence — and every operator-withheld value (any nonempty length, raw or JSON-escaped) refuse the whole document, never rewritten (`compile_disclosure_refused`) |
| host paths disclosed through the knowledge record | the recorded snapshot identity drops its directory and files root; hashes and selection stay |
| a forged or foreign plan buys a candidate | no plan field; kept rounds are server-owned, in memory per bound server, bound to their exact input, expiring on the monotonic clock, keyed by 256-bit random tokens that no refusal reflects |
| a changed snapshot presented under the pinned identity | every generation-2 round reopens the snapshot and compares manifest and rows to the pin; a presented file is compared to its pin; mismatch refuses before any call |
| store exhaustion | bounded rounds and bytes per round; a full store refuses a fresh round before it spends |

Residual: kept rounds are not a deduplication of paid work (a first answer
lost in transit leaves no token; a new fresh round spends again); holders of
the one Bearer share every token; a restart forgets every token; remote
billing cannot be proven stopped by a local deadline.

### Replay, jobs, and concurrency

- Every effecting POST requires a bounded Idempotency-Key.
- Key + authenticated authority + canonical request digest is committed before
  effects. Identical replay returns the original job; conflicting reuse refuses.
- Concurrent identical requests cannot start two runs.
- Job ids use cryptographic randomness, are non-sequential, and carry no source
  name or timestamp.
- Queue, active-run, request-body, SSE-client, event-buffer, and graceful-stop
  limits are explicit and tested at their boundary values.
- Process restart preserves enough idempotency/job state to avoid a duplicate
  effect or else fails closed before rerun.

### Responses, events, and logs

- The status vocabulary includes `paused`; unknown future values remain
  forward-compatible in SDK consumers.
- Error bodies have one typed public envelope and never include absolute paths,
  backtraces, provider response bodies, secret identifiers, or unredacted task
  output.
- SSE event ids are monotonic per job. Resume returns only events after the
  admitted cursor; stale/future/foreign cursors refuse without cross-job data.
- Provider/tool content is untrusted data. It passes the same runtime
  capability, injection, secret, and output-redaction controls as CLI runs.
- Logs identify request/job outcomes without logging credentials, raw workflow
  bytes, raw inputs, or model/tool payloads.

### W02 remote-security baseline and gap register

No network route may be implemented around an unresolved P0/P1 row. “Closed”
below means the named local primitive and its test exist; it does not claim an
HTTP surface that has not landed.

| surface | severity | baseline at W02 | owner / wave |
|---|---:|---|---|
| trace + event secret bytes | P0 | closed: provenance-based `RedactingSink` covers raw, JSON-escaped, nested, output-side-channel, and tool/provider echo shapes | runtime; W11 reruns adversarial suite |
| debug/error/log projection | P0 | snapshot, admitted context, and generic verdict `Debug` expose identity/digest only; HTTP error and request-log allowlists do not yet exist | Serve adapter / W06, blocking before bind |
| provider/tool secret-shaped payload | P0 | runtime events redact known secret provenance; arbitrary raw provider/tool bodies remain unfit for remote serialization | Serve projection / W06, SSE projection / W07 |
| prompt injection versus permits | P0 | runtime `dispatch::regate`, `permit_regate`, and adversarial F1–F4 fixtures keep model/tool text as data and re-check effect arguments | runtime / W02 closed; W11 refutes end-to-end |
| remote workflow projection | P0 | deny-by-default: `AdmittedExecution` fields are private and its debug view has no bytes; there is no remote serializer | Serve / W06 must add an explicit field allowlist, never `Serialize` the admitted world |
| sandbox/permit refusal under structured capture | P0 | closed: the production runner retains the Seatbelt/landlock classifier through drain and attaches its typed receipt before structured interpretation; status 126 and launcher diagnostics refuse, while an unmarked business nonzero remains data | exec/runtime / W02 |
| remote terminal classification | P0 | no remote process adapter exists; the kernel receipt table maps authority to blocked, transport to error, and missing/unsupported remote terminal receipts to fail-closed transport error | Serve worker adapter / W06 must attach a receipt on every terminal envelope |
| approval replay across processes | P0 | closed against concurrent/repeated use: ticket-digest marker is atomically create-once in local `~/.nika/approval-claims`; process clones share an atomic claim | runtime + CLI / W02 |
| approval marker rollback/deletion | P1 | W05 fail-closed boundary present: `approval_decided` requires its canonical digest plus an injected monotonic `ApprovalHistory` outside the job snapshot's rollback domain. The unkeyed chain is an internal-consistency check: it rejects non-coherent modification, interior deletion, permutation, and graft, but an actor who can rewrite `state.json` recomputes every link and can delete, reorder, graft, or edit payloads, including deny to allow. The retained history anchors one-shot digest retention and reuse refusal only — not the decision payload or the journal — so a coherent tail rewrite can reopen while a burned digest still cannot be spent twice. A same-authority sidecar is insufficient. | Serve worker / W06 must supply the real history adapter and anchor before listener bind; the anchor's retention boundary is a deployment responsibility with no wave assigned here |
| network composition | P1 | runtime maps absent/empty net grants to deny and refuses an unavailable declared sandbox; listener auth, bind acknowledgement, proxy and egress composition are still absent | Serve / W06 and VPS / W10 |
| public health metadata | P1 | no route exists; the allowed future projection is `EngineIdentity` only | Serve / W06 |

W02 therefore leaves no known P0/P1 in an existing remote adapter: there is no
adapter yet. The future P0/P1 rows are explicit entry gates for W06/W07, not
permission to ship a partial listener. Secret redaction is defense in depth;
the primary remote rule is still field allowlisting, so a value that was never
approved for projection cannot rely on a string scrubber to become safe.

The reviewer-v2 counterexamples were identified on `0d5c744c`: the production
runner discarded the sandbox backend before constructing `ShellResult`, and
`run --require-signature` reopened the workflow pathname after `RunSource`
capture. The barrier regressions landed with the fixes; no historical RED test
transcript is claimed where no such test existed on that SHA.

### Deployment

- Preferred VPS shape: loopback Serve listener behind a same-host TLS reverse
  proxy and firewall.
- Direct cleartext exposure to the public Internet is unsupported.
- Proxy/body/time limits must be no weaker than application limits.
- Application correctness is independent of HTTP/1.1, HTTP/2, or HTTP/3.

## Mandatory adversarial tests

- [x] no flags means no listener;
- [x] incomplete flag pairs and `--once`/`--dry` combinations refuse;
- [x] non-loopback without `--allow-remote` refuses before bind;
- [x] missing, duplicate, malformed, wrong, and oversized credentials share one
  bounded 401 shape;
- [x] a parser sentinel proves unauthenticated bodies are never decoded;
- [x] oversized, slow, invalid, and wrong-content-type bodies never execute;
- [x] absolute, traversal, separator-confused, extension-confused, and symlink
  workflow names refuse;
- [x] source replacement after capture cannot change checked/executed bytes
  (`nika-execution` includes deterministic barrier-interleaved root, child,
  nested-child, skill, symlink, and directory fixtures; DAP separately binds
  workflow signatures to captured bytes);
- [x] identical idempotent replay returns one job across restart;
- [x] conflicting key reuse and simultaneous duplicates refuse without a second
  effect;
- [x] durable event-chain modification, interior deletion, permutation, and
  cross-job graft refuse **when the mutation does not recompute the chain**;
  `approval_decided` cannot persist without its claim digest and injected
  history, and coordinated tail rollback cannot release that digest while the
  external authority survives. No box here claims detection of a coherent
  rewrite: the chain is unkeyed, so authenticating the journal or an approval
  payload would need a key or signature that W05 does not introduce;
- [x] opaque job guessing and unknown ids disclose no registry membership;
- [ ] `paused` round-trips through Rust, OpenAPI, fixtures, and TypeScript
  (Rust HTTP projection is proven; OpenAPI/TypeScript remain a later carrier);
- [x] SSE auth, resume, stale/future cursors, lag overflow, disconnect, and
  redaction are deterministic;
- [ ] protected responses/logs contain no credential, private path, provider raw
  payload, workflow bytes, or secret-shaped fixture;
- [x] cancellation signals the engine token before a durable `cancelled`
  receipt, queued cancellation never enters the backend, and concurrent/lost-
  response retries converge on the same terminal record;
- [x] artifact routes remain absent until their typed authority is admitted;
- [x] compile: unauthenticated bodies are never collected or judged; unknown fields, present
  nulls, duplicate keys (envelope and `answers`), positional arrays and foreign
  vocabulary refuse with fixed strings that echo no request byte; every bound
  accepts its exact limit and refuses the next byte; hostile source and
  literals are bounded data, never a server fault; no job, file or registry
  entry appears; a `ready` candidate the launch cannot satisfy is still refused
  by `POST /v1/jobs`; a timed-out or disconnected caller keeps its compile slot
  until the CPU work ends (`server::tests::compile` exercises these boundaries;
  the shared parity corpus also pins unresolved MCP source as `incomplete`
  without MCP or provider I/O);
- [x] native lifetime and credentials (S19): a round admitted but started after
  its deadline never reaches the seat; a round its caller heard 408 for never
  starts later; a stopping server cancels and joins its rounds (no repair
  after release, every slot back) — the real binary on SIGTERM too; one
  logical call the transport resends after a 503 is one call in the receipt
  and two HTTP requests; a typed `ProvidersConfig` key, a short value and an
  escaped key are withheld; the key withheld is the key the seat sends, by the
  typed and the environment's own precedence
  (`server::tests::compile::native::{lifecycle, withheld}` · `crates/nika-cli/tests/serve.rs`);
- [x] native compile: generation 1 answers byte for byte alike on a native and
  a default server and never reaches the seat; a default server refuses
  generation 2 and never advertises it; every shape, vocabulary, bound,
  caller-named-authority and replay refusal reaches no seat; one call under
  the operator's model and bound, the pinned pack presented byte for byte, no
  host path returned; replays repeat their exact input with zero calls or
  refuse (another instance, expiry, changed input, clarification); provider
  error text never reaches the document; an echoed credential is refused
  whole; `1 + repairs` bounds the calls; a disconnected or expired round keeps
  its slot until its work stops; a native round outlives the request deadline
  that still bounds generation 1; a changed snapshot and a full store refuse
  before any call; an invalid seat refuses before bind
  (`server::tests::compile::native`; the real binary over HTTP in
  `crates/nika-cli/tests/serve.rs`);
- [x] SIGINT/SIGTERM stop admission, settle in-flight authority, and leave no
  duplicate-runnable idempotency record.

## Review triggers

Reopen ADR-117 before adding multi-tenancy, browser credential storage,
in-process TLS, arbitrary workflow uploads, webhook triggers,
artifact download, or a second authentication mechanism. Each changes a trust
boundary rather than merely adding a route. Reopen this model before a native
seat gains a harness or decision seat, another strategy or sample count, a
caller-chosen model or knowledge source, durable or shared kept rounds, or an
automatic retry of paid work.

Webhook triggers are reopened by ADR-136 (proposed): one route family,
`POST /v1/ingress/{hook_id}`, authenticated by a per-binding policy (Standard
Webhooks HMAC over the raw bytes, or a per-binding token) with a secret held
as an environment reference, admitted through the same job door with
`ingress:<schedule>:<delivery-id>` idempotency. Until that ADR is accepted and
its proof lands, the listener still exposes no ingress route, and the
app-owned path (the application verifies the sender, then calls the SDK)
remains the only inbound webhook contract.
