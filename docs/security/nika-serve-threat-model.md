# `nika serve` network threat model

**Status:** binding design boundary for ADR-117; HTTP implementation is not yet
present. A checked box in this document means the implementation and its test
exist, not merely that the design mentions them.

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
credential source or replace the running binary. Resource exhaustion by an
already authenticated workflow remains bounded by workflow/runtime policy, not
by authentication alone.

## Route policy

| route class | public? | effects? | mandatory controls |
|---|---:|---:|---|
| `GET /health` | yes | no | fixed response schema; EngineIdentity only |
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
| sandbox/permit refusal under structured capture | P0 | closed: a typed authority/transport result fails before structured interpretation; business-process nonzero remains data | exec/runtime / W02 |
| approval replay across processes | P0 | closed: ticket-digest marker is atomically create-once in local `~/.nika/approval-claims`; process clones share an atomic claim | runtime + CLI / W02 |
| network composition | P1 | runtime maps absent/empty net grants to deny and refuses an unavailable declared sandbox; listener auth, bind acknowledgement, proxy and egress composition are still absent | Serve / W06 and VPS / W10 |
| public health metadata | P1 | no route exists; the allowed future projection is `EngineIdentity` only | Serve / W06 |

W02 therefore leaves no known P0/P1 in an existing remote adapter: there is no
adapter yet. The future P0/P1 rows are explicit entry gates for W06/W07, not
permission to ship a partial listener. Secret redaction is defense in depth;
the primary remote rule is still field allowlisting, so a value that was never
approved for projection cannot rely on a string scrubber to become safe.

### Deployment

- Preferred VPS shape: loopback Serve listener behind a same-host TLS reverse
  proxy and firewall.
- Direct cleartext exposure to the public Internet is unsupported.
- Proxy/body/time limits must be no weaker than application limits.
- Application correctness is independent of HTTP/1.1, HTTP/2, or HTTP/3.

## Mandatory adversarial tests

- [ ] no flags means no listener;
- [ ] incomplete flag pairs and `--once`/`--dry` combinations refuse;
- [ ] non-loopback without `--allow-remote` refuses before bind;
- [ ] missing, duplicate, malformed, wrong, and oversized credentials share one
  bounded 401 shape;
- [ ] a parser sentinel proves unauthenticated bodies are never decoded;
- [ ] oversized, slow, invalid, and wrong-content-type bodies never execute;
- [ ] absolute, traversal, separator-confused, extension-confused, and symlink
  workflow names refuse;
- [x] source replacement after capture cannot change checked/executed bytes
  (`nika-execution` includes deterministic root, child, skill, symlink,
  directory, and barrier-interleaving fixtures);
- [ ] identical idempotent replay returns one job across restart;
- [ ] conflicting key reuse and simultaneous duplicates refuse without a second
  effect;
- [ ] opaque job guessing and unknown ids disclose no registry membership;
- [ ] `paused` round-trips through Rust, OpenAPI, fixtures, and TypeScript;
- [ ] SSE auth, resume, stale/future cursors, lag overflow, disconnect, and
  redaction are deterministic;
- [ ] protected responses/logs contain no credential, private path, provider raw
  payload, workflow bytes, or secret-shaped fixture;
- [ ] cancellation and artifact routes are absent until their typed authorities
  are admitted;
- [ ] SIGINT/SIGTERM stop admission, settle in-flight authority, and leave no
  duplicate-runnable idempotency record.

## Review triggers

Reopen ADR-117 before adding multi-tenancy, browser credential storage,
in-process TLS, arbitrary workflow uploads, webhook triggers, cancellation,
artifact download, or a second authentication mechanism. Each changes a trust
boundary rather than merely adding a route.
