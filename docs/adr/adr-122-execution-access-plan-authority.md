---
id: ADR-122
title: "One access plan is the execution authority: resolved once, executed as resolved"
status: accepted
date: "2026-09-03"
phase: "pre-1.0 · one door"
deciders: ["@ThibautMelen"]
tags: ["architecture", "access", "harness", "providers", "runtime", "cli", "one-door"]
affects_crates: ["nika-providers", "nika-runtime", "nika-cli-host", "nika-cli", "nika-service-execution", "nika-harness", "nika-verb-infer", "nika-verb-agent"]
affects_layers: ["L1.5", "L2", "L3", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-120", "ADR-099", "ADR-003", "ADR-131"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: ["NIKA-1800", "NIKA-1801"]
timeline: "v0.118"
follow_ups: ["the trace and the resume identity carry the plan (serve · arm · answered legs)", "the layered check verdicts read the plan (VALID · ACCESS READY · CAPACITY FIT · RUN READY)"]
---

# ADR-122: one access plan is the execution authority

## Context

A run has to answer one question before its first task: **which path
reaches each model on this machine** — a provider API with a key, a
local server, the mock, or a subscription seat driven through an agentic
CLI. Until this decision the question was answered by five independent
resolutions on one `nika run` path, and none of them was what the
dispatcher read:

| surface | resolved by | consumed by execution |
|---|---|---|
| the `--dry-run` preview | `access_plan_map` over the report's models, pin only | no |
| the boot manifest (`access_plan` field) | `access_plan_map` again, in the composer | no |
| the human announce | `resolve_access` per model, after composition | no |
| `check --json` `access_plan` rows | `resolve_access` per model, no pin | no |
| the admission gate | `access_pin_refusal`, pin only | only under a pin |

The seat itself was gated on the **spelling** of `--access`: without a
typed pin the runtime discarded the ready seat it had just computed and
the task dialed the provider API with whatever key was in the
environment. Measured on the shipped 0.116.2 (census B of the one-door
refactor pack): the announce said `codex`, the run dialed OpenAI with a
dead key and failed inside the task; `--model mock/echo` announced the
file's model; a pinned seat was priced on the API lane. Five answers to
one question cannot stay equal, and the one that mattered was not asked.

## Decision

**The access plan is resolved once per execution attempt and then
executed as resolved.** Nothing on the run path resolves access a second
time.

1. **One resolver, one value.** `nika_providers::resolve_execution_plan`
   folds the run's needs (`ModelNeed` = each effective model with the
   verbs that read it, `--model` already applied) with this machine's
   probe rows and the `--access` pin into a frozen
   `ExecutionAccessPlan`: one `LaneVerdict` per static model (admitted
   with its `AccessPlan` and candidate count, or refused with every
   witness), the pin, the ONE seat the run holds, and the pin refusal
   when the pin cannot be honored. `nika_cli_host::access::resolve_plan`
   is the single composition of needs + probes for the CLI; it runs
   once in `run_admitted_context` and once per answered gate leg.
2. **The runtime executes the plan.** The composer attaches it
   (`AuthorizedRuntime::with_access_plan`); the seat is built from
   `plan.seat` — never from the pin's spelling and never from « a seat
   exists »; the admission belt refuses from the plan
   (`nika_runtime::plan_refusal`: an unsatisfied pin is `NIKA-1801`, a
   lane with no ready path is `NIKA-1800`, before the first task and
   with the witnesses); each `infer:`/`agent:` task routes by **its
   lane** (`Runtime::seat_for(model)`) — a pinned seat serves every
   model, a resolved seat serves only its harness lanes, and an agent
   whose lane is a provider path runs the native loop even while a seat
   is attached for another lane.
3. **Everything else is a projection.** The announce, the `--dry-run`
   preview (text and JSON), `check --json`'s `access_plan` rows,
   `nika explain`'s access section, the boot manifest's `access_pin` and
   `access_plan` stamps, and the task terminal's `access` · `access_id`
   · `billing` · `provider` fields all read the plan. They can render
   it; they cannot disagree with it.
4. **Eligibility is part of resolution.** A harness candidate is
   eligible for a lane only when the seat can drive every verb that
   reads the model: an ACP-only seat (`claude-code`, `gemini-cli`, …)
   never serves an `infer:` lane; only an infer-grade seat (`codex`)
   does. A second harness candidate is ineligible once another seat
   holds the run (one seat per run). A pinned seat's billing class is
   `unknown` until the adapter's own surface attests it — never a
   fabricated included-quota.

## Consequences

- A model with no ready path now refuses **before task 1** on the
  environment exit, naming the model and every rejected candidate,
  instead of failing inside the task with a provider error after work
  may have started. This is a behavior change for keyless runs of cloud
  models; the refusal teaches the fix (`nika doctor`, the key's
  variable, or `--model`).
- The sovereign order (`local < mock < harness < oauth < api`) now
  actually routes: a ready seat wins over a present API key, unpinned.
- `nika_harness::seat_from_pin` is deleted; `first_ready_infer_harness`
  and the pin-only `access_pin_refusal` remain only as the planless
  embedder's path (a runtime composed without a plan keeps the old
  admission law).
- Proof: `crates/nika-cli/tests/access_plan_e2e.rs` drives the real
  binary with a scripted `codex` on `PATH` beside a dead OpenAI key aimed
  at a closed loopback port — the seat serves, the key is never dialed,
  the announce and `check --json` name the path the terminal frame
  records, `--access api` never borrows the seat, `--model mock/echo`
  announces nothing, and no path refuses with exit 3 before any frame.
  `nika_providers::plan` carries the unit law (verb eligibility, one
  seat, pin refusal, templated models unjudged).

## Follow-ups

- Delivered in wave 1b: the resume judges the recorded lanes against
  the live plan (a moved lane refuses unless `--access` names it), the
  lane joins the resume identity, `nika serve` resolves and attaches
  the plan through `ServiceExecutionOptions`, ARM inherits through the
  CLI door, and `nika_service_execution::access` is the one resolver
  every door reads.
- Wave 2: the layered `check` verdicts (VALID · ACCESS READY · CAPACITY
  FIT · RUN READY) read the same plan.

## Amendment · closing audit, 2026-09-04

Static model resolution, thinking and known capacity are now judged at
`ExecutionService` admission for the root and every captured workflow,
before an execution identity is minted. The root's envelope override is
applied for that judgment; explicit task models and child workflow models
retain their own authority. `AuthorizedRuntime` rechecks the actual
override before its prologue, so admitting under one model does not permit
running under another invalid model. The judges remain the provider and
checker functions, not copies in CLI, Serve or the SDK.

A child workflow is a distinct composition over captured bytes. It carries
the root's explicit access pin and probe facts, but resolves its own model
needs into its own frozen plan before child composition. Copying the
root's lane map would omit child-only models; losing the pin would permit
a different access path. Child boot fields and execution both read the
child plan. The CLI boot-field helper delegates to the service projection.

`AuthorizedRuntime` cannot execute without an attached frozen plan, even
for a workflow whose plan has no model lanes. `ServiceExecutionDriver::
execute` derives an omitted plan from its captured facts and effective
model; direct builder callers must attach theirs. The generic low-level
`Runtime` remains an injected execution substrate, not a second authorized
service door. The model-less gate judges the effective envelope too: a
legal override supplies a missing default but never replaces a task model.
The predicate is owned by `nika-runtime::first_modelless_task` and
re-exported by service access. The oracle evaluates it alongside all
static lanes: an admitted model cannot hide another task's missing model,
and a seat cannot clear a refused lane or pin.

The closure-wide static check does not claim to predict dynamically chosen
models or future remote availability. Tests cover known capacity refusal
before events, explicit task-model precedence, child-plan propagation and
redaction of nested settlement errors. Provider calls and harness behavior
require their separate executable fixtures.

See [One Door proof boundaries](../architecture/one-door-proof-boundaries.md)
for the distinction between source tests, same-job replay preservation,
cross-run comparison and published-artifact provenance.
