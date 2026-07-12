---
id: ADR-035
title: "Telemetry seams: SpanGuard parent_span_id + SpanRef (OTel-ready)"
status: accepted
date: "2026-04-17"
phase: "Phase D — Wave 4B #1"
deciders: ["@ThibautMelen"]
tags: ["l0.5", "telemetry", "observability", "otel", "span", "reservation", "forward-compat"]
affects_crates: ["nika-kernel", "nika-kernel-mock"]
affects_layers: ["L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-020", "ADR-028", "ADR-034"]
requires: ["ADR-028", "ADR-034"]
enables: []
amends: []
fci: ["FCI-035"]
inv: []
shadow_zones: []
nika_codes: ["NIKA-800", "NIKA-801", "NIKA-802", "NIKA-803", "NIKA-804"]
timeline: "v0.81.0-alpha.4, Wave 4B #1 seed · prose authored + accepted 2026-07-12 (merged-code rule)"
follow_ups:
  - "Finalise SpanRef vs SpanContext (OTel term) naming when an OTel bridge crate lands — note the OTLP need was meanwhile served by `nika trace export` (nika-dap/src/otel.rs), projection-side"
---

# ADR-035: Telemetry seams: SpanGuard parent_span_id + SpanRef (OTel-ready)

Status flipped `proposed → accepted` 2026-07-12 under the merged-code
rule (the #474 ADR ruling): both reservations ship in
`crates/nika-kernel-core/src/infra/trace.rs` (the kernel's core member
— the ADR predates the kernel descent; `nika-kernel` re-exports).

## Context

Wave 4B #1 (commit `861f09bc9`, 2026-04-17) extended the kernel
telemetry types with two additive reservations so span identity can
travel — across await points, channels, spawned tasks — without a
thread-local stack (which the tokio runtime breaks) and without
cloning a live guard.

## Decision

Both shapes ship in `nika-kernel-core/src/infra/trace.rs`:

1. **`SpanGuard.parent_span_id: Option<SpanId>`** — the explicit
   parent link. Nested spans stitch into a tree by data, not by
   ambient state: the module doc states the law (« parent_span_id
   threads parent→child links »), and the field is `Option` because a
   root span honestly has no parent — this is the one place an absent
   value IS the semantics, not a policy hole.
2. **`SpanRef { trace_id, span_id }`** — the lightweight copyable
   handle (with `::new()`, FCI #19) for propagating span identity
   across boundaries where a `SpanGuard` cannot go: channel payloads,
   spawn captures, cross-process IPC. The shape deliberately mirrors
   OTel's `SpanContext` so a future bridge is a field-for-field map.

Both are `#[non_exhaustive]` — OTel's `TraceFlags`/`TraceState` can
join `SpanRef` later without a major.

## Alternatives considered

- **Thread-local span stack** — rejected: tokio task migration breaks
  ambient parenting; the explicit field survives every executor.
- **Clone the `SpanGuard` across boundaries** — rejected: a guard's
  Drop is its END event; two owners means double-close or leaked
  spans. A `Copy`-able ref carries identity without lifecycle.
- **Adopt the `opentelemetry` crate types directly in L0.5** —
  rejected: the kernel stays zero-heavy-deps; mirroring the SHAPE
  keeps the bridge zero-copy while the dependency stays out.

## Empirical validation (what makes this "merged code")

- `nika-kernel-core/src/infra/trace.rs` — `SpanRef` (line ~21, with
  constructor), `parent_span_id: Option<SpanId>` on `SpanGuard`
  (line ~49), module-doc law at the top.
- The OTel direction the reservation aimed at materialized
  projection-side meanwhile: `nika trace export`
  (`nika-dap/src/otel.rs`) projects recorded journals to OTLP/JSON —
  consuming the same span-identity vocabulary these types reserved.

## See also

- ADR-028 — Forward-compat reservation policy (this ADR is an instance).
- ADR-034 — the tracer trait family that emits these shapes.
- FCI-035 addendum — Wave 4A/4B reservation catalog.
- NIKA-800..819 — Observability / telemetry error code range (reserved
  v0.80).
- `nika trace export` — the shipped OTLP projection (journal-side).

🦋
