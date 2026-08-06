---
id: ADR-111
title: "Deliver the pause event outward — operator-configured CloudEvents webhook"
status: accepted
date: 2026-08-06
phase: ""
deciders: ["@ThibautMelen"]
tags: [notify, pause, human-gate, webhook, cloudevents, trace, resume]
affects_crates: [nika-cli-host, nika-cli, nika-event]
affects_layers: [L0, L4]
supersedes: []
superseded_by: []
related: []
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: []
timeline: ""
follow_ups: ["R2 candidates live under Neutral consequences (MCP tasks surface · multi-URL · store-backed secret)"]
---

<!-- accepted 2026-08-06 · implemented same-arc (feat/notify-on-pause ·
kinds 455deaa9e · seam 4f6558edd). Delivery point REFINED at
implementation: the runtime core keeps its zero-tokio-edge posture, so
the lanes (nika-cli run · L4 shim) deliver AFTER the tee splits and
BEFORE surface_trace seals — same contract (after the journal write,
before exit, awaited with timeout, outcome journaled under the chain),
different crate than first sketched (nika-runtime untouched); the module
itself lives in nika-cli-host (the ADR-110 member — the 15k wall fired on
nika-cli at 15150 during this very arc, and compute descends). Proven:
unit golden envelope byte-pinned + HMAC vector verified against
CPython + deterministic id; live three-scenario proof at the binary
plane (signed delivery · default-off silence · ssrf_blocked refusal,
rc=4 unchanged across all three); the four-scenario e2e suite ships in
crates/nika-cli/tests/notify_e2e.rs for CI. -->


# ADR-111: Deliver the pause event outward — operator-configured CloudEvents webhook

## Context

A blocking `nika:prompt` under a non-interactive surface journals a
`workflow_paused` event and exits cleanly with run state `paused`
(spec ADR-099 rider; emission site `crates/nika-runtime/src/lib.rs:1275`,
single-site per INV-024). The pause is durable — but silent outward. The
journal records it; nothing tells a human who has walked away. Discovery today
is polling (`nika trace ls` marks paused traces) or watching the terminal.

Every workflow system that pauses for humans ships an outbound signal:
AWS Step Functions hands out a task token at `.waitForTaskToken`; GitHub
deployment protection rules receive a webhook and answer with a callback;
CNCF Serverless Workflow correlates human callbacks on CloudEvents attributes.
The engine has all the ingredients — the pause payload (task, mode, message,
choices, approval ticket fields), an SSRF-defended HTTP effect crate
(`nika-http`: static + DNS-resolve + per-hop redirect re-check), and an
env-var configuration surface (`NIKA_*`) — but no delivery seam.

Non-goals that bound this decision: no daemon, no inbound listener, no
language change. The workflow file never declares where notifications go —
delivery is a deployment concern, so it rides operator configuration, never
the contract.

## Decision

**At the moment the engine journals `workflow_paused`, it also POSTs that
event — as a CloudEvents 1.0.2 structured JSON envelope, optionally signed
with Standard Webhooks headers — to an operator-configured URL. Default OFF.
Delivery failure never affects the run.**

### Configuration (env — the engine's existing config surface)

- `NIKA_NOTIFY_URL` — the webhook target. Absent ⇒ the feature is OFF and the
  engine opens no socket (the sovereign default).
- `NIKA_NOTIFY_SECRET` — optional, Standard Webhooks `whsec_` base64 secret.
  Present ⇒ requests carry a `webhook-signature` header (`v1,` HMAC-SHA256
  over `{msg_id}.{timestamp}.{payload}`). `webhook-id` and `webhook-timestamp`
  headers are always sent (they cost nothing and give receivers an idempotency
  key for free).

### The envelope (CloudEvents 1.0.2, structured mode)

`Content-Type: application/cloudevents+json`. Required attributes per the
spec: `specversion: "1.0"`, `id`, `source`, `type`. Ours:

- `type`: `sh.nika.run.paused` (reverse-DNS per the CloudEvents convention)
- `source`: the run URI (trace identity)
- `subject`: the pausing task id
- `id`: **deterministic** — derived from the trace id + the pause event's
  chain position. Re-delivering the same pause yields the same id (consumers
  dedup for free); a later re-pause of the same task yields a new id.
- `time`: RFC 3339, from the engine's injected clock
- `data`: the `workflow_paused` journal payload verbatim (workflow, task,
  mode, message?, choices?, approval ticket fields — secret-masked exactly as
  the journal is) plus `trace_path` and the resume teaching line the pause
  note already carries.

One serde struct. No new dependency for the envelope; the signature needs
`hmac` (RustCrypto — `sha2` is already in the tree for the chain).

### Delivery discipline (per the CloudEvents HTTP-Webhook companion spec)

Single POST, 3-second timeout, never follow a redirect (the SSRF per-hop
re-check in `nika-http` already refuses cross-target hops), any 2xx counts as
delivered. No retries in R1. The OPTIONS abuse-protection handshake is
explicitly out of scope: an operator-configured target IS the consent.

### Sequencing — the pause path is sacred

The notifier runs AFTER the `workflow_paused` journal write and BEFORE
process exit, awaited with its timeout (never spawned — the pause path exits
the process, and a detached task would be dropped mid-flight). Its outcome is
journaled as one of two additive event kinds:

- `notify_delivered` (target host, duration)
- `notify_failed` (target host, error class — including the SSRF refusal
  class; the URL is judged by the same `nika-http` floor as every other
  engine egress)

Neither outcome changes the run's state: the run exits `paused` with the same
code whether the webhook succeeded, failed, or was never configured. The
notification is observable history, not control flow.

### What this is not

- Not a language surface: zero envelope change, zero new YAML, zero flags.
  The conformance contract binds required events and semantics, not journal
  bytes; the two kinds are engine-additive.
- Not a queue: one event, one POST, at-most-once. Consumers that need
  fan-out, retries or routing put a relay behind the URL (a self-hosted ntfy
  topic already works as-is for phone push).
- Not the answer path: answering stays `nika run … --resume <trace>
  --answer <task>=<value>` (ADR-099). This ADR only makes the question heard.

## Consequences

### Positive

- Any surface — notification relays, dashboards, atelier tooling, CI — learns
  about a waiting gate the second it happens, through two boring, widely
  implemented standards (CloudEvents envelope, Standard Webhooks signature)
  instead of a bespoke format.
- The deterministic event id + always-on `webhook-id` header give consumers
  exactly-once processing without any server-side state on our side.
- Sovereignty intact: OFF by default, operator-pointed, SSRF-floored,
  journaled. The workflow file stays a pure contract.

### Negative

- The pause path gains up to one timeout (3 s) of latency when a URL is
  configured and the target is slow. Bounded and journaled; acceptable for a
  path whose next step is a human.
- Two more event kinds for run-report consumers to render.
- A secret in `NIKA_NOTIFY_SECRET` is env-borne; store-backed references can
  follow later without changing the header contract.

### Neutral

- R2 candidates, deliberately deferred: multiple URLs; an `on_finished`
  sibling event; secret-store references for the URL/secret; surfacing paused
  runs as MCP tasks in `input_required` state with elicitation for the answer
  (per the MCP 2026-07-28 tasks extension — client support is still uneven,
  so it waits behind a flag until the webhook seam has proven the payload).

## Tests (ship with the implementation — e2e unless noted)

1. Kind unit test — the two kinds serialize snake_case and class as Workflow.
2. Default OFF — a pausing run with no `NIKA_NOTIFY_URL` journals no
   `notify_*` kind and opens no socket.
3. Delivery — a local listener receives ONE structured CloudEvents POST whose
   required attributes and `data` match the journal payload; the trace gains
   `notify_delivered`.
4. SSRF refusal — a metadata-range URL yields no POST, a `notify_failed`
   carrying the SSRF class, and an unchanged `paused` exit.
5. Failure is not control flow — an unreachable target still exits `paused`
   with the same code, within timeout + margin.
6. Envelope golden — the serialized envelope pins the required CloudEvents
   attributes and the deterministic id.
7. Signature vector — with a `whsec_` secret configured, the
   `webhook-signature` header verifies against an independent Standard
   Webhooks implementation's test vector.

## Alternatives considered

- **Author-facing notify on pause** (a workflow-declared hook): rejected —
  couples the contract to deployment topology and breaks "the file is the
  contract". The authored `nika:notify` builtin keeps its own job (a DAG
  task), which structurally cannot fire at pause time.
- **A bespoke JSON payload without an envelope**: rejected — CloudEvents
  costs one struct and buys an ecosystem of consumers; inventing a shape
  buys nothing.
- **A daemon/queue with retries**: rejected — a supervisor contradicts the
  engine's no-daemon posture; operators who need delivery guarantees put a
  relay they own behind the URL.
- **Spawn-and-exit delivery**: rejected — the process exits on pause; a
  detached task is a silent drop. Awaited-with-timeout is the only honest
  sequencing.
