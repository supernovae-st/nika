---
id: ADR-016
title: "Cancellation model: future-drop primary, CancelCtx cooperative module"
status: accepted
date: "2026-04-16"
phase: "Phase D -- Session S1-E (Wave 2)"
deciders: ["@ThibautMelen"]
tags: ["kernel", "async", "cancellation", "forward-compat", "tokio-free"]
affects_crates: ["nika-kernel"]
affects_layers: ["L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-006", "ADR-007", "ADR-014", "ADR-017", "ADR-018", "ADR-020", "ADR-034", "ADR-038", "ADR-078", "ADR-081", "ADR-083", "ADR-095"]
requires: ["ADR-007"]
enables: ["ADR-034"]
amends: []
fci: ["pattern-1-kernel-traits-upfront", "pattern-2-non-exhaustive"]
inv: []
shadow_zones: []
nika_codes: []
timeline: "v0.81.0-alpha.1, Wave 2 / Session S1-E"
follow_ups:
  - "Wire CancelCtx into DAG executor (v0.95)"
  - "Define Cancellable + CancelPropagator traits (v0.95)"
  - "Decide whether to wrap tokio_util::CancellationToken inside CancelCtx (v0.95)"
---

# ADR-016: Cancellation model: future-drop primary, CancelCtx cooperative module

## Context

The kernel today has exactly one cancellation mechanism — `ShellCancel` — and it
is intentionally narrow.

Design decision #9 in `crates/nika-kernel-core/src/io/process.rs:9-11` states:
"cancel is a `fn cancel(&self, id)` method, not a `CancellationToken` field on
`ShellCommand`. This keeps `tokio-util` out of `nika-kernel`." The trait method
itself is `ShellCancel::cancel(&self, id: &str)` at
`crates/nika-kernel-core/src/io/process.rs:156-159` and exists for subprocess
management (kill the child by id), not for cooperative task cancellation.

No other cancellation mechanism existed before Wave 2:

- `InferRequest` (`crates/nika-kernel-ai/src/provider.rs:197-220`, 11 fields
  pre-S1-B) had no cancel field.
- `ContextCompressor::compress`
  (`crates/nika-kernel-ai/src/context.rs:48-58`) takes `messages` and
  `policy` only — no cancel parameter.
- `MemoryStore::recall` (`crates/nika-kernel-ai/src/memory.rs:275-279`) is
  purely query in / hits out.

For v0.95 (DAG executor, multi-step agent loops, streaming inference) we need
cooperative cancellation that propagates through a tree of in-flight tasks.
Tokio's `select!` plus future-drop already covers single futures, but agent
loops and streams must check a cancellation flag between iterations to flush
partial work cleanly.

The kernel sits at L0.5 and must remain runtime-agnostic. Today's
`crates/nika-kernel/Cargo.toml:18-26` lists only `nika-error`,
`trait-variant`, `thiserror`, `miette`, `serde`, `serde_json`, `bytes`,
`futures-core`. Adding `tokio` or `tokio-util` here would violate the layer
contract (`scripts/ci/check-layering.sh`).

## Decision

**Cancellation is layered, not unified.** Three coexisting mechanisms, each
solving a different problem:

1. **Primary: future drop.** Dropping a task's future stops its work.
   `tokio::select!` and `tokio::time::timeout` are the day-to-day tools.
   No new API is needed for the common case.

2. **Cooperative: `CancelCtx`** (new in S1-B,
   `crates/nika-kernel-core/src/cancel.rs:35-67`). `Arc<AtomicBool>`-backed
   token that all clones share. `cancel()` flips the flag; long-running
   loops poll `is_cancelled()` between iterations. Reserved field
   `InferRequest.cancel: Option<CancelCtx>` is pre-planted at v0.81 so
   future activation is purely additive (`#[non_exhaustive]` ratchet).

3. **Process management: `ShellCancel`** (existing,
   `crates/nika-kernel-core/src/io/process.rs:156-159`). Kill-by-id for
   subprocesses. Stays as-is — different problem class.

`CancelCtx` deliberately uses `std::sync::atomic::AtomicBool` (no `tokio`,
no `tokio-util`). v0.95 may *internally* wrap a
`tokio_util::CancellationToken` if we need richer behavior (timeout-as-cancel,
parent-child trees), but the public API of `CancelCtx` will not change.

Two explicit cancel traits planned for v0.95: `Cancellable` (poll-style check)
and `CancelPropagator` (tree-wide propagation). Both will be defined when the
DAG executor lands; defining them now would lock in shape choices we have not
benchmarked.

## Consequences

### Positive

- Pre-planting `InferRequest.cancel` at v0.81 turns "wire cancellation" into a
  zero-breaking-change activation later (Pattern 2, ROI 6.7x per ADR-007).
- Kernel stays free of `tokio-util` — `cargo deny` continues to enforce L0.5
  purity.
- `CancelCtx` is testable in isolation today (`crates/nika-kernel-core/src/cancel.rs`
  ships with 6 unit tests including clone-state propagation and Send+Sync).
- Three orthogonal mechanisms keep mental model clean: drop = default,
  cooperative = loops, process = subprocess.

### Negative

- Two cancellation idioms (drop vs cooperative) means contributors must learn
  when to use which. Mitigated by docs in `cancel.rs` module header.
- `CancelCtx` does not propagate timeouts. v0.95 must layer
  `tokio::time::timeout` on top, not replace it.
- Pre-planted `Option<CancelCtx>` field is dead weight until v0.95 — accepted
  cost, ~16 bytes per `InferRequest`.

### Neutral

- ShellCancel's id-based signature stays unchanged. It is not migrated to
  `CancelCtx` because subprocess management is a different problem (the OS
  kills, not cooperative checking).

## Evidence / Affected code

- `crates/nika-kernel-core/src/cancel.rs:35-67` — `CancelCtx` struct + impl
  (Wave 2 / S1-B).
- `crates/nika-kernel-core/src/cancel.rs:78-128` — 6 unit tests, including
  clone-state propagation, idempotent cancel, Send+Sync.
- `crates/nika-kernel-ai/src/provider.rs:21` — `use crate::cancel::CancelCtx;`
- `crates/nika-kernel-ai/src/provider.rs` — `InferRequest.cancel:
  Option<CancelCtx>` reserved field (S1-B).
- `crates/nika-kernel-core/src/io/process.rs:9-11` — design decision #9 rationale
  (no `tokio-util` in kernel).
- `crates/nika-kernel-core/src/io/process.rs:156-159` —
  `ShellCancel::cancel(&self, id: &str)` method.
- `crates/nika-kernel/Cargo.toml:18-26` — kernel deps (no tokio, no
  tokio-util).
- `docs/architecture/forward-compat-invariants.md` — Pattern 1 (kernel traits
  upfront), Pattern 2 (`#[non_exhaustive]`).

## Alternatives considered

### Alt A -- `tokio_util::CancellationToken` directly in kernel
Use `CancellationToken` as the public type in `InferRequest.cancel`.
Rejected: pulls `tokio-util` into L0.5. Kernel must remain
runtime-agnostic; `CancellationToken` is not `no_std` compatible and
blocks future WASM compilation of kernel types. Wrapping it inside a
private `CancelCtx` is an option for v0.95 — that keeps the public API
runtime-agnostic.

### Alt B -- No cancellation field until v0.95
Add `cancel` only when the DAG executor needs it.
Rejected: even with `#[non_exhaustive]` we must update every call site
that builds `InferRequest`. Pre-planting the `Option<CancelCtx>` is
~16 bytes and zero ergonomic cost (constructor sets `None`). ROI 6.7x
per ADR-007's calculation.

### Alt C -- Cancel as a method on `ProviderInfer`
Expose `provider.cancel(request_id)` instead of a field on the request.
Rejected: providers do not own request lifetimes; the runtime does. A
field on the DTO lets the runtime cancel without round-tripping through
the provider trait, and it composes (a workflow can hand the same
`CancelCtx` to many providers in parallel).

### Alt D -- Single global cancel signal
Process-wide cancellation flag (e.g., on receipt of SIGINT).
Rejected: too coarse. We need per-task and per-DAG-subtree cancellation
so the user can cancel one branch of a workflow without killing the rest.

## Related

- ADR-006 — kernel ISP trait design. ADR-016 adds `CancelCtx` as a
  data type, not a kernel trait, so it does not extend ADR-006's trait
  list.
- ADR-007 — forward-compat invariants. The pre-planted `cancel` field is
  Pattern 2 (`#[non_exhaustive]`) applied to a v0.95 hook.
- ADR-014 — sealed kernel traits. `CancelCtx` is a struct, not a trait,
  so sealing does not apply.
- ADR-018 — runtime + sync primitives (chooses tokio rt + parking_lot).
  ADR-016 explicitly does not depend on the runtime choice.
- `docs/architecture/forward-compat-invariants.md` — Pattern 1, Pattern 2.

## Notes

Follow-ups:

- v0.95 DAG executor: define `Cancellable` (poll-check trait) and
  `CancelPropagator` (tree-wide propagation trait).
- v0.95 may wrap `tokio_util::CancellationToken` inside `CancelCtx` —
  decision deferred until benchmarks show whether the wrap is worth the
  added dep.
- Document the "drop vs cooperative" choice in the agent self-serve
  docs (Phase E).
