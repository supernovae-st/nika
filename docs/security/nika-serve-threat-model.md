# `nika serve` network threat model

**Status:** binding design boundary for ADR-117; W06 admits authenticated
loopback HTTP. A checked box in this document means the implementation and
its test exist, not merely that the design mentions them.

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
| workflow registry | path/name from request | held registry root | relative `.nika.yaml` entry only; no absolute, traversal, or replaced link |
| source capture | mutable filesystem | owned bytes + logical base | one capture; check and run consume the same bytes |
| execution | admitted request | shared L3 service | idempotency bound before effects; runtime owns the verdict |
| event stream | job journal | SSE client | same auth as job; monotonic resume cursor; redacted payloads |
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
| `/v1/workflows` | no | no | Bearer auth before listing; `.nika.yaml` names only |
| `/v1/workflows/{name}` | no | no | Bearer auth; contained relative name metadata; no source bytes |
| `/v1/jobs/{opaque-id}` | no | no | Bearer auth before lookup; uniform unknown-id response |
| `/v1/jobs/{opaque-id}/events` | no | no | Bearer auth; bounded SSE buffer; monotonic `Last-Event-ID`; redaction |
| effecting `/v1/*` POST | no | yes | auth before parse; body limit; content type; idempotency before execution |
| cancel routes | absent | — | remain absent until typed runtime cancellation settles terminal state |
| artifact routes | absent | — | remain absent until a held typed artifact manifest exists |

The exact route spelling beyond `/health` and the `/v1` authority prefix is
owned by the later OpenAPI carrier. Adding a route cannot weaken this table.

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

- Discovery accepts `.nika.yaml` only.
- Requested names are relative to one held registry root and cannot contain an
  absolute prefix, `..`, NUL, or platform separator ambiguity.
- Source, child workflows, and skills are captured with a logical base; the
  checked bytes are the executed bytes.
- A symlink or directory replacement cannot redirect a held open beneath the
  admitted root.
- Client input cannot select an arbitrary trace, ledger, secret, or artifact
  filesystem path.

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
- [ ] SSE auth, resume, stale/future cursors, lag overflow, disconnect, and
  redaction are deterministic;
- [ ] protected responses/logs contain no credential, private path, provider raw
  payload, workflow bytes, or secret-shaped fixture;
- [x] cancellation and artifact routes are absent until their typed authorities
  are admitted;
- [ ] SIGINT/SIGTERM stop admission, settle in-flight authority, and leave no
  duplicate-runnable idempotency record.

## Review triggers

Reopen ADR-117 before adding multi-tenancy, browser credential storage,
in-process TLS, arbitrary workflow uploads, webhook triggers, cancellation,
artifact download, or a second authentication mechanism. Each changes a trust
boundary rather than merely adding a route.
