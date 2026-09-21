---
id: ADR-136
title: "A Serve inbound event binding is a schedule with a second credential on one route family, admitted through the same job door"
status: proposed
date: 2026-09-20
phase: ""
deciders: ["@ThibautMelen"]
tags: ["architecture", "serve", "security", "webhook", "one-door"]
affects_crates: ["nika-serve", "nika-cadence", "nika-vocab"]
affects_layers: ["L0", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-099", "ADR-111", "ADR-116", "ADR-117", "ADR-119", "ADR-132"]
requires: ["ADR-117", "ADR-132"]
enables: []
amends: ["ADR-117"]
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: ""
follow_ups: ["nika#1719", "nika#1720", "nika#1643"]
---

# ADR-136: A Serve inbound event binding is a schedule with a second credential on one route family, admitted through the same job door

## Context

ADR-117 fixed the Serve network boundary: no listener by default,
`--bind` + `--workflows` + `--token-file` inseparable, loopback first,
`--allow-remote` as an exposure acknowledgement, TLS at the operator's
reverse proxy, `X-Forwarded-*` never authenticating, one Bearer credential on
every `/v1/*` route except `/health`. The threat model
(`docs/security/nika-serve-threat-model.md`, "Review triggers") names webhook
triggers as a change of trust boundary that must reopen ADR-117 before any
route lands. This ADR is that reopening.

What exists at engine `860be47d` (0.120.3): `ScheduleWhen::Webhook` is a
unit variant (`crates/nika-cadence/src/schedule.rs`) reachable only from a
project beat's `cadence: on-webhook`; the planner returns `NotDue` for it and
the OS emitter refuses it; `PUT /v1/schedules/{id}` accepts `when.kind` =
`once | cadence` only; `POST /v1/schedules/{id}/trigger` is pinned absent
(`crates/nika-serve/src/server/openapi.rs`, `ABSENT_PATHS`). Job admission
is one path: `Idempotency-Key` plus the SHA-256 of the exact body decide
replay, conflict or creation (ADR-132), declared `inputs` are validated
before creation (`crates/nika-serve/src/server/inputs.rs`), and the resident
now binds a schedule's declared inputs on every fire through the same
validator (#1370, the Serve door).

A third party that must start ONE workflow (a payment provider, a
source-control host, a partner system) can only do so today by holding the
Serve bearer, which grants every workflow under `--workflows`. The published
alternative is the app-owned path: the application terminates the webhook,
verifies the sender with the sender's own library, and calls the SDK with
`inputs` and an idempotency key. That path stays first-class and is qualified
by the `signed-webhook-intake` depth project in nika-client. This ADR adds the
managed door for deployments with no application in between.

## Decision

Add one route family, `POST /v1/ingress/{hook_id}`, authenticated by the
binding's own policy instead of the Serve bearer, and make that binding a
schedule of kind `webhook`. Nothing else in the boundary moves.

1. **One new route family, one new credential class.** `POST /v1/ingress/{hook_id}`
   is the second and last authentication mechanism on the listener. `/health`
   stays the only open route; every other `/v1/*` route keeps the bearer.
2. **The binding is a `ScheduleDefinition`** with `when.kind = webhook`,
   created and updated through `PUT /v1/schedules/{id}` under the existing
   CAS revision contract, read through `GET`, paused with the existing
   `active` / `pauseReason` / `pauseUntil` triple, and bounded by the existing
   `maxCostUsd`. `ScheduleWhen::Webhook` stops being a unit variant and carries
   the policy below. No second registry, no event bus.
3. **The hook id is a routing capability, never the whole authentication.**
   Serve mints it (at least 128 bits, URL-safe) at create, returns it in the
   apply response and the status; rotation is a new revision. It appears in
   job events and proof only as a digest.
4. **Two authentication modes, V0.** `hmac`: Standard Webhooks headers
   (`webhook-id`, `webhook-timestamp`, `webhook-signature` as a space-delimited
   `v1,<base64 HMAC-SHA256(id.timestamp.body)>` list), verified over the exact
   received bytes before any parsing, constant-time comparison, timestamp
   tolerance 300 s by default and never 0. `token`: a per-binding bearer of at
   least 32 bytes, constant-time comparison, documented as weaker (no body
   integrity, no freshness). The secret is a reference
   `{ source: env, key: NAME }` resolved from the resident's process
   environment at verification time, never a literal in the body, the status,
   the events, the proof, the logs or a path.
5. **Admission is the existing path.** The handler verifies, bounds and parses,
   then calls the same coordinator admission as `POST /v1/jobs` with
   `Idempotency-Key = ingress:<schedule_id>:<delivery_id>` (a reserved prefix
   like `schedule:`), `RequestDigest = sha256(raw body)`, the parsed payload
   as the binding's one declared input judged by `inputs::validate`, and
   `origin: Ingress { schedule_id, schedule_revision, hook_id_digest,
   delivery_id, delivery_timestamp, body_digest }`. The handler never executes.
6. **Durable at ack.** A 2xx is written only after the durable record exists:
   200 on replay, 202 on create. 409 when the same delivery id arrives with
   different bytes. 401 for a missing or invalid signature and for a stale
   timestamp (the failure class distinguishes `INGRESS_AUTH` from
   `INGRESS_REPLAY`). 400 on a body that is not JSON, 413 above the ingress
   body cap, 422 when the payload fails the declared input, 410 on a disabled
   binding so well-behaved senders stop, 503 on queue pressure so senders
   retry.
7. **Bounds.** Ingress body cap 1 MiB by default and always below the job cap;
   `application/json` only; no `Content-Encoding`; the existing 5 s request
   timeout and connection ceilings apply. A per-binding token bucket is a later
   slice named here so its absence is never silent.
8. **Prerequisites.** The resident binds non-manual origin inputs through
   `inputs::validate` (landed for schedules in this train); a gated workflow
   started by a binding needs the remote answer door (#1643) before the SaaS
   loop is advertised.

Explicitly rejected:

- A bearer-authenticated `POST /v1/schedules/{id}/trigger`: it solves the
  operator gesture, not the third-party sender, who would then hold the bearer.
- An opaque URL as the only authentication: no body integrity, no freshness,
  leaks through logs and proxies, no rotation without downtime.
- A separate event registry or queue: it would duplicate trigger identity,
  CAS, cost ceiling and pause semantics the schedule already owns.
- Trigger configuration inside the portable `.nika` bytes: it breaks the same
  file running locally, in two workspaces and on a VPS with different bindings.
- Provider adapters first (Stripe, GitHub): the app-owned path covers them
  today; the generic Standard Webhooks mode covers Svix-compatible senders;
  adapters are additive later.
- IP allowlists as identity, TLS inside Nika, auto-approval of gated runs.

## Consequences

### Positive
- Loopback-first and the reverse-proxy deployment shape are preserved: the
  ingress route is an ordinary path behind TLS, and a non-loopback bind still
  requires `--allow-remote`.
- The blast radius of a binding secret is exactly one workflow with its
  declared inputs under its cost ceiling: not listing, cancel, answer or any
  other workflow.
- Idempotency, durability, cancellation, SSE, restart recovery and proof are
  inherited from the one admission path; nothing is reimplemented.
- The same `.nika` bytes run from CLI, SDK, cron and this binding; trigger
  configuration lives in the schedule store and the project file, never in
  the program.

### Negative
- A second credential class doubles the surface the threat model must keep
  honest: the row "binding secret holder" is added with its authority and its
  refusals, and every negative test in the Proof section becomes a release
  gate for nika-serve.
- Resolving a secret reference from the resident's environment ties a binding
  to the deployment that holds that variable; moving a binding means moving
  the secret with it.
- Without a rate limiter, queue pressure is the only back pressure on a public
  path; the 503 is honest but coarse until the token bucket lands.

### Neutral
- `nika.yaml` `cadence: on-webhook` keeps meaning "declared, fires on its
  event"; it now needs a live binding in Serve to fire, and the CLI edge
  still skips it.
- The SDK projects the new `when.kind` on `schedule()` and `scheduleStatus()`
  and exposes the ingress path in the status; it grows no `webhook()` verb and
  no verifier helper.
- The OpenAPI absent-route list shrinks by exactly this family; the SDK
  coverage gate follows the same list.

## Proof to accept

Named nika-serve integration tests for: wrong signature, downgrade scheme,
stale timestamp on both sides of the window, duplicate delivery concurrent and
sequential (one job id), same id with different bytes (409), oversize body,
non-JSON body, compressed body, unknown hook, disabled binding (410), a body
that names a workflow is ignored, a payload failing the declared input type
(422), the secret absent from status, SSE, proof and logs, a restart between
ack and start (the queued job resumes), queue full (503). One golden that
starts the same `.nika` by `POST /v1/jobs`, `PUT /v1/schedules` cadence and
`POST /v1/ingress/{hook}`. The threat model gains its row and its refusal
table in the same train.
