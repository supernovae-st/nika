---
id: ADR-035
title: "Telemetry seams: SpanGuard parent_span_id + SpanRef (OTel-ready)"
status: proposed
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
timeline: "v0.81.0-alpha.4, Wave 4B #1 seed (prose pending Phase C)"
follow_ups:
  - "Flesh out full rationale + alternatives during Phase C ADR prose sweep"
  - "Finalise SpanRef vs SpanContext (OTel term) naming when nika-observability-otel lands v0.100"
  - "Verify W3C trace-context inject/extract bridges cleanly off this surface"
---

# ADR-035: Telemetry seams: SpanGuard parent_span_id + SpanRef (OTel-ready)

> **STUB — prose pending Phase C.** Decision is load-bearing in code; this
> file exists so vector 19 (adr-orphan-proposed) can surface it at day 31
> and force full prose authoring.

## Context

Wave 4B #1 (commit `861f09bc9`, 2026-04-17) extends the kernel telemetry
types at `crates/nika-kernel-core/src/infra/trace.rs` with two additive
reservations:

1. `SpanGuard.parent_span_id: Option<SpanId>` — explicit parent link so
   nested spans can be stitched into a tree without relying on a
   thread-local stack (which the tokio runtime breaks).
2. `SpanRef { trace_id, span_id }` — lightweight copyable handle that lets
   callers propagate span identity across channel boundaries (tokio::mpsc
   SendError consumers, spawned tasks, cross-process IPC) without cloning
   the full `SpanGuard`.

Both additions are designed to bridge into OpenTelemetry's
`SpanContext` at v0.100 without any v0.8x-breaking change.

## Decision (preliminary)

`SpanGuard` gains `parent_span_id: Option<SpanId>` behind
`#[non_exhaustive]`. `SpanRef` is a new `#[non_exhaustive]` struct with
a `::new()` constructor and `Copy`-able inner IDs. The types intentionally
mirror OTel's `SpanContext` shape so the v0.100 `nika-observability-otel`
crate can implement a zero-copy bridge. Full rationale (tree-stitching
rationale, W3C trace-context compatibility, SpanRef vs SpanContext naming,
copy semantics, ID-gen ownership) to be authored during Phase C ADR sweep
before v0.90.

## See also

- ADR-028 — Forward-compat reservation policy (this ADR is an instance).
- ADR-034 — `TracerProvider` trait that will emit `SpanRef` instances.
- FCI-035 addendum — Wave 4A/4B reservation catalog.
- NIKA-800..819 — Observability / telemetry error code range (reserved v0.80).

🦋
