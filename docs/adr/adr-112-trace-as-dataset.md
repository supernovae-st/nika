---
id: ADR-112
title: "Trace-as-dataset — OTel GenAI semconv now, input-capture before the SFT export"
status: proposed
date: 2026-08-06
phase: ""
deciders: ["@ThibautMelen"]
tags: [trace, otel, semconv, dataset, observability, content-policy]
affects_crates: [nika-dap, nika-runtime, nika-cli]
affects_layers: [L3, L4]
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
follow_ups: ["Part 2 (input capture + --format dataset) needs an operator go — it changes the trace content policy"]
---

# ADR-112: Trace-as-dataset — OTel GenAI semconv now, input-capture before the SFT export

## Context

Every run writes a hash-chained trace, and `nika trace export` already
projects it to OTLP/JSON lines (`nika_dap::otel` — the one projection
module). The competitive read (omnara-steals SOTA sweep, 2026-08-06)
named a cheap, sovereign win: **your own runs are your eval/fine-tune
dataset — locally, jq-able, no vendor store**. Two pieces make it real:
(1) the OTLP export must speak the *current* OTel GenAI semantic
conventions so any OTel/eval tool reads the model, and (2) a
`--format dataset` projection must emit the interchange shape trainers
consume.

A grounding pass on `main` (2026-08-06) turned up the load-bearing
constraint: **the trace captures outputs, never inputs.** An infer/agent
terminal emits `output` (only under `include_content`) plus the access
facts `model`/`provider` (D-2026-08-04-N1) — but the resolved *prompt*
and *system* messages are dropped after `dispatch.rs` renders them
(grep of `emit_task.rs`: no `prompt`/`system`/`messages` field is ever
pushed). A `{messages:[{role:user,…},{role:assistant,…}]}` SFT record
cannot be derived from the trace alone: the user turn is not there.

## Decision

**Split the work. Part 1 ships now; Part 2 is proposed and gated because
it changes the trace's content policy.**

### Part 1 — GenAI semconv mapping (SHIPPED this arc)

`nika_dap::otel::one_task_span` now projects the access facts to the
**current** semconv names, in the one projection module:

- `gen_ai.provider.name` ← the terminal's `provider` field
- `gen_ai.request.model` / `gen_ai.response.model` ← the model NAME
  (the part after the `provider/` prefix; a slash-less value stays whole)

Never `gen_ai.system` (deprecated in semconv v1.37.0, Aug 2025). The
GenAI semconv is still "Development" and recently moved repos with no
pinned release, so the mapping lives in exactly one function — a rename
upstream is one edit, never a scatter. Additive: existing `nika.*`
attributes are untouched.

### Part 2 — input capture, then `--format dataset` (PROPOSED · gated)

To make a real dataset, the resolved prompt + system must be captured on
the infer/agent terminal, **symmetric with `output` and under the same
`include_content` content policy** (default OFF; masked exactly as the
journal masks — a prompt can carry a resolved secret value, and the
existing secret-masking MUST apply to it before it is ever written).
Then:

- the OTLP export gains `gen_ai.input.messages` / `gen_ai.output.messages`
  (the semconv content attributes, under `include_content`), and
- a new `nika_dap::dataset` projection + a `nika trace export --format
  {otlp|dataset}` flag (default `otlp`, backward-compatible) emits
  **OpenAI chat-messages JSONL** (`{"messages":[…]}` + optional `tools`
  + `metadata` {run, task, model, provider, cost, tokens, trace_id,
  chain_ok}) — the shape TRL/Axolotl/Unsloth/LLaMA-Factory consume with
  zero transform — plus a `--format dataset=eval` variant shaped
  `{id, input, target, metadata}` (UK AISI Inspect, the least-lock-in
  eval sample). Both are pure projections of the journal; masking is
  inherited, never re-derived.

Part 2 is gated on an operator go because capturing prompts widens what
a trace contains — a content-policy decision, not a mechanical port.

## Consequences

### Positive

- Part 1 makes every OTel-native viewer and eval tool (Jaeger, Grafana,
  Langfuse, Phoenix via translation) read the model + provider off a
  nika trace today, with zero vendor capture.
- Part 2, when it lands, turns the sovereign contrast into a feature:
  "your own runs are your eval set — locally, on files you own." The
  research backs the pattern (SWE-Gym, ICML 2025 · doi:10.1145/…/… —
  arXiv:2412.21139 — SFT on 491 rejection-sampled successful agent
  trajectories lifted SWE-bench Verified +14%).

### Negative / Neutral

- Part 2 grows the trace under `include_content` (prompts are larger than
  outputs); the default-OFF policy contains it, but the masking path
  gains a new surface that MUST be proven (a resolved secret in a prompt
  must never reach the file). That proof is why Part 2 is its own arc.
- The GenAI semconv is pre-stable; the single-module mapping is the hedge.

## Alternatives considered

- **Output-only "dataset" from the trace as-is** — rejected. A dataset of
  assistant outputs without the prompts fine-tunes nothing; shipping it
  under the name `dataset` would be a dishonest label. Better no export
  than a misleading one.
- **Recover prompts from the workflow file at export time** — rejected.
  The file holds unresolved `${{ }}` templates; the *resolved* prompt
  (the actual model input) exists only at runtime. Only capture-at-run
  gives a faithful record.

## Related

- `nika_dap::otel` — the one projection module (Part 1 lands here).
- ADR-099 — the trace is the checkpoint (the same journal this projects).
- `emit_task.rs` — the terminal emission site Part 2 extends (under the
  `include_content` content policy, symmetric with `output`).
