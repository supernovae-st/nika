---
id: ADR-017
title: "Streaming policy: bounded mpsc(32), ReceiverStream at kernel boundary"
status: accepted
date: "2026-04-16"
phase: "Phase D -- Session S1-E (Wave 2)"
deciders: ["@ThibautMelen"]
tags: ["kernel", "async", "streaming", "backpressure", "channels"]
affects_crates: ["nika-kernel"]
affects_layers: ["L0.5"]
supersedes: []
superseded_by: []
related: ["ADR-006", "ADR-016", "ADR-018"]
requires: []
enables: []
amends: []
fci: []
inv: ["INV-016"]
shadow_zones: []
nika_codes: []
timeline: "v0.81.0-alpha.1, Wave 2 / Session S1-E"
follow_ups:
  - "Audit verb-* crates for unbounded channels when they are admitted"
  - "Add clippy lint custom check banning unbounded channels in workspace"
---

# ADR-017: Streaming policy: bounded mpsc(32), ReceiverStream at kernel boundary

## Context

The kernel exposes one streaming type alias and one streaming trait method.

Type alias (`crates/nika-kernel/src/provider.rs:380`):

```rust
pub type InferEventStream =
    Pin<Box<dyn Stream<Item = Result<InferEvent, ProviderError>> + Send>>;
```

Trait method (`crates/nika-kernel/src/provider.rs:451-455`):

```rust
#[trait_variant::make(ProviderStreamDyn: Send)]
pub trait ProviderStream: Send + Sync {
    async fn infer_stream(&self, request: InferRequest)
        -> Result<InferEventStream, ProviderError>;
}
```

`InferEventStream` is a boxed `dyn Stream` from `futures_core::Stream`
(`crates/nika-kernel/src/provider.rs:18` import). The kernel deliberately
depends on `futures-core` (`crates/nika-kernel/Cargo.toml:25`) but not on
`tokio` — channel implementation lives in L1/L2.

The mock provider (`crates/nika-kernel-mock/src/provider.rs`) demonstrates the
pattern by hand-rolling an inline `EmptyStream` struct (no channel needed for
an always-empty mock). Real providers will internally use a Tokio `mpsc`
channel and wrap the receiver via `tokio_stream::wrappers::ReceiverStream`.

LLM streaming responses can produce thousands of SSE events. Anthropic's
extended thinking traces and OpenAI's `o3` reasoning-token streams already
exceed 100k tokens per request in the wild. Unbounded queues at the boundary
risk OOM if the consumer is slower than the wire (typical when the consumer
also runs tool calls between deltas).

Invariant #16 (`crates/nika-kernel/src/lib.rs:31` reference) reminds that
`parking_lot::RwLockReadGuard` is `!Send` — guards must never be held across
`.await`. This applies to stream consumers: don't hold a lock while pulling
from a stream.

## Decision

**All streaming in Nika is bounded; the kernel boundary erases the channel
type.**

- At the kernel boundary, `ProviderStream::infer_stream` returns
  `InferEventStream = Pin<Box<dyn Stream<Item = Result<InferEvent, ProviderError>> + Send>>`.
  This is `futures_core::Stream` (runtime-agnostic), not a Tokio channel.
- L1/L2 implementations internally use `tokio::sync::mpsc::channel(32)`
  paired with `tokio_stream::wrappers::ReceiverStream` to produce the boxed
  stream returned to the kernel.
- **Buffer size is 32.** Big enough to absorb bursty SSE chunks from
  Anthropic/OpenAI without blocking the HTTP reader. Small enough to enforce
  backpressure when the consumer pauses (e.g., to run a tool call). Tested
  against rig-core's typical SSE rate.
- **Unbounded channels are banned** anywhere in the workspace
  (`tokio::sync::mpsc::unbounded_channel`, `flume::unbounded`, etc.). Future
  custom clippy lint will enforce.
- Consumer side: `StreamExt::next()` loop with explicit timeout
  (`tokio::time::timeout`). **No `.collect::<Vec<_>>()`** on streams (unbounded
  memory). If the caller wants the whole response, they must fold with a
  bounded buffer or use the non-streaming `ProviderInfer::infer`.

## Consequences

### Positive

- Bounded buffer ⇒ predictable memory usage even on extended thinking
  responses with 100k+ tokens.
- Backpressure flows from consumer to provider naturally; slow consumers
  pause the HTTP reader, no manual flow-control knobs.
- Boxed `dyn Stream` keeps `Provider` trait object-safe — needed for the
  registry of providers in `nika-runtime`.
- Kernel stays runtime-agnostic. Channel choice is an L1/L2 implementation
  detail; swapping `mpsc` for another bounded channel does not break the
  trait.

### Negative

- `Pin<Box<dyn Stream>>` allocates once per inference. Not free, but cheap
  next to the inference itself (~ns vs. seconds).
- "32" is a magic number. We accept it because benchmarking with rig-core
  showed no throughput difference at 64 or 128, and 32 saves memory in the
  common case (most streams are <32 events/sec).
- Hand-rolled `EmptyStream` in the mock duplicates the `Stream` impl
  boilerplate — accepted because pulling `tokio` into the mock dev-deps
  just for this would be heavier.

### Neutral

- `futures-core` (not `futures` or `futures-util`) stays the only stream
  dep in kernel. L1/L2 may add `tokio-stream` and `futures-util` as needed.

## Evidence / Affected code

- `crates/nika-kernel/src/provider.rs:380` — `InferEventStream` type alias.
- `crates/nika-kernel/src/provider.rs:18` — `use futures_core::Stream`.
- `crates/nika-kernel/src/provider.rs:451-455` — `ProviderStream` trait.
- `crates/nika-kernel/Cargo.toml:25` — `futures-core = { workspace = true }`.
- `crates/nika-kernel-mock/src/provider.rs` — hand-rolled `EmptyStream`
  (no channel) in the mock.
- `crates/nika-kernel/src/lib.rs` — INV-016 reference (locks not across
  `.await`).

## Alternatives considered

### Alt A -- Unbounded channels (simpler API)
`tokio::sync::mpsc::unbounded_channel` everywhere.
Rejected: OOM risk on long streaming responses. Real Anthropic extended
thinking traces have already hit 100k+ tokens; an unbounded queue would
buffer all of them in RAM.

### Alt B -- `async-channel` instead of Tokio mpsc
Third-party crate with simpler ergonomics.
Rejected: Tokio mpsc is already in the dep tree (via `tokio` itself in L1).
Adding another channel impl is dependency bloat for no functional gain.

### Alt C -- Return `impl Stream` instead of `Pin<Box<dyn Stream>>`
Avoid the box allocation.
Rejected: not object-safe. The provider registry stores `Box<dyn Provider>`
trait objects; method-level `impl Stream` would force every consumer to be
generic, which kills the registry pattern.

### Alt D -- Larger buffer (64 or 128)
"More buffer = more throughput."
Rejected: benchmarking showed no throughput change. Bigger buffer means more
in-flight events held in RAM under slow-consumer scenarios.

### Alt E -- Smaller buffer (8 or 16)
Tighter backpressure.
Rejected: bursty SSE from Anthropic can deliver 20+ events in a single TCP
read. A buffer of 16 would block the HTTP reader on every burst, hurting
TTFT.

## Related

- ADR-006 — Provider trait ISP design (defines `ProviderStream` separately
  from `ProviderInfer`).
- ADR-016 — Cancellation. Stream consumers should hold a `CancelCtx` and
  poll `is_cancelled()` between events for cooperative shutdown.
- ADR-018 — Runtime + sync primitives. Locking guidance applies to stream
  consumers (no guards across `.await`).

## Notes

Follow-ups:

- When verb-* crates (verb-infer, verb-agent) are admitted, audit them for
  unbounded channels.
- Add a custom clippy lint (`nika-lints::no_unbounded_channels`) when
  `nika-lints` ships (Phase 4+).
- Document the "no `.collect()` on streams" rule in the agent self-serve
  docs (Phase E).
