---
id: ADR-117
title: "admit nika serve network access behind explicit authority"
status: accepted
date: "2026-08-21"
phase: "pre-1.0 · execution access"
deciders: ["@ThibautMelen"]
tags: ["architecture", "serve", "http", "authentication", "execution", "layering"]
affects_crates: ["nika-arm", "nika-cli", "nika-runtime", "nika-serve"]
affects_layers: ["L3", "L4"]
supersedes: ["ADR-116"]
superseded_by: []
related: []
requires: ["ADR-114"]
enables: []
amends: []
fci: ["FCI-001", "FCI-008"]
inv: []
shadow_zones: ["gate-serve-network-auth", "gate-serve-execution-authority"]
nika_codes: []
timeline: "pre-1.0"
follow_ups:
  - "use the admitted nika-arm custody library from every execution interface"
  - "admit the shared L3 owned-byte execution service before the first route"
  - "add typed cancellation and artifact authority before exposing those routes"
---

# ADR-117: admit `nika serve` network access behind explicit authority

## Context

The shipped `nika serve` is a local resident ARM firer. It opens no listener.
Remote execution needs an HTTP boundary, but the current composition still
lives in `nika-cli`: it captures source bytes, resolves child workflows and
skills from a logical path, constructs the runtime, and owns the exact trace
identity. `Runtime::run` alone is not an execution service that a second L4
surface can safely duplicate.

The remote contract also has known prerequisites. The SDK retries effecting
requests without an idempotency key, its job status vocabulary omits the
runtime's real `paused` state, and the engine has neither a general artifact
manifest nor a cancellation seam. Exposing routes first would make transport
invent authority the runtime does not possess.

## Decision

Networked Serve is admitted only as an explicit, authenticated L4 projection
over shared typed authorities. Bare `nika serve` keeps ADR-116's local behavior
and opens no listener.

### 1. Explicit admission at the CLI edge

HTTP mode requires both `--bind <SOCKET_ADDR>` and `--workflows <DIR>`. Either
one alone refuses. The workflow directory is the bounded registry root; a
request cannot name an absolute path, escape it, or submit an arbitrary local
filesystem path.

`--once` and `--dry` refuse when `--bind` is present. Their current meanings
remain local: once executes due beats; dry only previews the due projection.
They are not server modes.

A non-loopback bind additionally requires `--allow-remote`. That flag only
acknowledges exposure; it never disables authentication, request limits, path
confinement, or transport guidance.

### 2. Layer ownership before routes

- `nika-arm` becomes the L4 library for project custody, verified ARM state,
  firing, migration, and OS-unit composition. `nika-cli` becomes its rendering
  adapter.
- `nika-runtime` owns a shared L3 execution service that accepts captured owned
  bytes plus their logical path, performs check + composition once, and returns
  a typed verdict carrying the exact trace path.
- `nika-serve` is the L4 HTTP/job/SSE projection. It does not import
  `nika-cli`, scan trace directories, reimplement check/run composition, or
  interpret ARM journals independently.

No HTTP route may land before those first two authorities are usable by both
CLI and Serve.

### 3. Authentication and disclosure order

Every `/v1/*` route requires exactly one `Authorization: Bearer` credential.
Only `GET /health` is public. Credential material is acquired through a secret
source at startup, never a command-line token, response field, trace field, or
log value. Comparison is constant-time.

Authentication and coarse request-size checks happen before JSON parsing,
workflow lookup, job lookup, or any side effect. Missing, duplicated,
malformed, or invalid credentials receive the same bounded 401 response with a
Bearer challenge. Authenticated authority failures use 403; malformed admitted
requests use 400/422. Errors never expose filesystem paths, secret names,
provider payloads, or parser internals.

`GET /health` returns only `status`, `service`, and the four fields projected
from `EngineIdentity` (`engine_version`, `build_sha`, `spec_sha`,
`api_version`). It exposes no workflow, job, ledger, provider, path, or secret
state.

### 4. Replay and execution safety

Every effecting POST requires a bounded `Idempotency-Key`. The server binds the
key to the authenticated request digest before execution; an identical replay
returns the existing job, while a key reused with different bytes refuses.
Opaque job identifiers are non-sequential and disclose no path or workflow
name.

Workflow discovery accepts only `.nika.yaml`. The existing `.nika.yml`
acceptance in another interface is not copied into Serve. Registry entries are
opened beneath the held workflow root, captured once, and executed from those
exact bytes with their logical base preserved.

The first API status vocabulary includes the runtime's `paused` state. OpenAPI,
Rust response types, and the TypeScript SDK change in one lockstep carrier; no
surface may silently map paused to running or failed.

### 5. Deliberately absent authority

There is no cancellation route until a typed cancellation seam reaches the
runtime and proves terminal settlement. There is no artifact download/list
route until execution emits a typed artifact manifest whose paths are held and
bounded by an artifact authority. A trace filename is evidence, not an
artifact-capability substitute.

### 6. HTTP, browser, and deployment boundary

CORS is disabled by default. A future origin option accepts exact origins only;
wildcards and reflected origins are forbidden. Application semantics are
HTTP-version agnostic: no correctness claim depends on H1, H2, or H3.

The initial service terminates plain HTTP. Supported remote deployment places
it behind an operator-controlled TLS reverse proxy or on a protected private
network. Direct cleartext exposure to the public Internet is outside the
supported boundary. Loopback binding behind a same-host proxy is the preferred
VPS shape.

The detailed trust boundaries, attack table, and mandatory negative tests live
in `docs/security/nika-serve-threat-model.md` and are part of this decision.

## Consequences

### Positive

- Local users do not acquire a listener or network trust boundary by upgrading.
- CLI and HTTP execute the same captured bytes through one L3 authority.
- Authentication, idempotency, status, trace identity, and later artifacts are
  typed contracts instead of route-local conventions.
- The public health response has one compile-bound identity source.

### Negative

- HTTP delivery waits for the ARM split and shared execution service.
- Remote deployments need a token source and TLS/firewall operations outside
  the process.
- Required idempotency adds durable job metadata before the first effecting
  endpoint can be useful.

### Neutral

- `--allow-remote` is an exposure acknowledgement, not a security mode.
- The initial API has no multi-tenant authorization model. One configured
  credential is one authority domain; tenancy requires a later ADR.
- SSE is a projection of the job event journal, never a second execution path.

## Required evidence before the first HTTP implementation merges

1. The threat-model checklist is linked from the crate spec and exercised by
   negative route tests.
2. Auth-before-parse is proven with a body/parser sentinel.
3. Missing, duplicate, malformed, wrong, and oversized credentials are
   indistinguishable at the response boundary.
4. Loopback/default, non-loopback refusal, and `--allow-remote` admission are
   real-binary tests.
5. Traversal, absolute paths, symlink replacement, and non-`.nika.yaml`
   discovery refuse before source execution.
6. Idempotent replay and conflicting key reuse are tested across process
   restart.
7. SSE authentication, resume, monotonic event ids, bounded buffering, and
   secret/redaction rules are tested before SSE is public.
8. OpenAPI, Rust public API, SDK types, and fixtures agree on every status and
   identity field.

## Alternatives considered

### Add routes directly to `nika-cli`

Rejected. It would preserve the current composition accident and make every
future interface depend on a UI crate at the same layer.

### Let HTTP accept arbitrary workflow bytes or paths

Rejected for the first contract. A bounded registry makes relative child and
skill resolution, path confinement, and operator review mechanically visible.

### Trust loopback and omit authentication

Rejected. Browsers, compromised local processes, port forwards, and containers
cross the loopback boundary. Protected routes authenticate everywhere.

### Ship cancel and artifacts as best-effort route behavior

Rejected. A 202 response without runtime settlement authority and a path string
without artifact custody would be a false contract.

## Related

- ADR-093 — existing local-infer HTTP precedent, not Serve authority.
- ADR-095 — execution security architecture.
- ADR-110 — L4 member-split precedent.
- ADR-114 — cadence and verified ARM ledger authority.
- ADR-116 — superseded local-only Serve decision.
- `docs/security/nika-serve-threat-model.md` — binding threat model.
