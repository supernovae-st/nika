---
id: ADR-019
title: "Retry + timeout ownership: stratified by layer"
status: accepted
date: "2026-04-16"
phase: "Phase D -- Session S1-E (Wave 2)"
deciders: ["@ThibautMelen"]
tags: ["retry", "timeout", "reliability", "layer-discipline"]
affects_crates: ["nika-kernel"]
affects_layers: ["L0.5", "L1", "L2"]
supersedes: []
superseded_by: []
related: ["ADR-006", "ADR-007", "ADR-018", "ADR-033"]
requires: []
enables: []
amends: []
fci: []
inv: []
shadow_zones: []
nika_codes: ["NIKA-100", "NIKA-107"]
timeline: "v0.81.0-alpha.1, Wave 2 / Session S1-E"
follow_ups:
  - "Define RetryConfig + ErrorCategory in nika-error (ADR-033, S1-C)"
  - "Implement HTTP-layer retry inside nika-http when L1 lands"
  - "Document task-level retry: block schema in workflow YAML reference"
---

# ADR-019: Retry + timeout ownership: stratified by layer

## Context

The kernel already exposes building blocks for retry decisions but does not
own a retry policy.

`ProviderError` (`crates/nika-kernel/src/ai/provider.rs:386-424`) carries the
information needed to decide retryability:

- `RateLimited { retry_after_ms: Option<u64> }`
  (`provider.rs:407-410`) — explicit hint from the provider.
- `Api { status: u16, message: String }` — generic API failure with an HTTP
  status code.
- `is_transient(&self) -> bool` (`provider.rs:428-439`) — returns `true` for
  `RateLimited` and `Api { status: 500..=599, .. }`.

`ShellCommand` (`crates/nika-kernel/src/io/process.rs`) already has a `timeout:
Option<Duration>` field; `ShellError::Timeout { duration_ms: u64 }` is the
matching error variant.

`ToolExecError` (`crates/nika-kernel/src/runtime/tool_executor.rs`) has `NotAvailable`,
`NotFound`, `Timeout`, but no `is_transient()` — tool failures are
predominantly semantic (wrong args, tool missing). Retrying them masks bugs.

`AgentLoopConfig` (`crates/nika-kernel/src/runtime/agent.rs`) controls iterations and
budgets, but has no `retry:` block — agent loop control and provider retry
are different concerns.

Workflow YAML supports a `retry:` block at the task level today (legacy).
Engine-level retry needs to respect it without re-implementing the policy at
every layer.

NIKA-100 + NIKA-107 (MCP connection errors) already surface to users with
guidance to retry; a sane default retry helps avoid noisy false negatives.

## Decision

**Retry and timeout ownership is stratified by architectural layer. Each
layer owns the policy that matches its failure mode.**

### Retry layers

1. **L1 -- HTTP transport retry** (owned by `nika-http`):
   - Connect / DNS / TLS handshake failures.
   - 3 attempts, exponential backoff (250ms / 1s / 4s), 50% jitter.
   - Configurable per-request via `RequestOptions`.
   - Respects `Retry-After` header on 429/503.

2. **L2 -- Provider transient retry** (owned by verb-* crates):
   - 429 (RateLimited) and 5xx (Api) failures from `ProviderError::is_transient`.
   - Default: 3 attempts, exponential backoff (1s / 4s / 16s), jitter.
   - Respects `RateLimited.retry_after_ms` when present (overrides backoff).
   - Configurable per-task via workflow YAML `retry:` block.

3. **L2 -- ToolExecutor: no automatic retry.**
   - Tool failures are semantic (wrong params, tool missing, semantic error).
   - Retrying masks bugs.
   - User opts in explicitly via workflow `retry:` block on the task.

### Timeout ownership

| Boundary | Owner | Default | Source |
|---|---|---|---|
| HTTP connect | L1 (`nika-http`) | 10 s | `RequestOptions.connect_timeout` |
| HTTP read | L1 (`nika-http`) | 120 s | `RequestOptions.read_timeout` (long LLM responses) |
| Tool execution | L2 (`nika-process` or tool impl) | per `ShellCommand.timeout` | workflow YAML |
| Task | L2 (runtime) | none | workflow YAML `timeout:` field |
| Agent loop total | L2 (verb-agent) | none | `AgentLoopConfig.limits.max_duration_secs` |
| Whole workflow | none | unlimited | by design |

### Shared types (planned, ships in S1-C)

`RetryConfig` and `ErrorCategory` will live in `nika-error` (L0) so every
layer can speak the same vocabulary without depending on L0.5 kernel:

```rust
// Sketch (real impl in S1-C / ADR-033)
#[non_exhaustive]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
    pub jitter: f32,
}

#[non_exhaustive]
pub enum ErrorCategory {
    Transient,
    RateLimited,
    Auth,
    NotFound,
    Permanent,
}
```

The implementation chooses *when* to apply a `RetryConfig` based on
`ErrorCategory`; the kernel does not pick a policy.

## Consequences

### Positive

- Each retry layer has a single responsibility — easy to reason about
  failures.
- HTTP retries cannot mask provider-level rate limits (which need
  `retry-after` respect).
- Tool failures are not silently retried — bugs surface immediately.
- One vocabulary (`RetryConfig` + `ErrorCategory`) shared across crates,
  no per-crate retry types.

### Negative

- Three retry layers means the user must understand which one is firing
  when something goes wrong. Mitigated by tagging retry events with
  the layer name (`http`, `provider`, `task`) in the EventLog.
- Cumulative latency under cascading failures (HTTP retried 3× then
  provider retried 3× = up to 9 round-trips). Accepted because each
  layer's max-attempts is small and bounded.
- Workflow-level retry config has to flow through to the right layer —
  needs careful documentation in the YAML reference.

### Neutral

- `ProviderError::is_transient` stays as the kernel's only retry
  classifier. No new traits needed at L0.5.
- `tower::retry::Retry` is not adopted (see Alt D); we may revisit at
  v0.100 when tower's middleware gets seriously evaluated.

## Evidence / Affected code

- `crates/nika-kernel/src/ai/provider.rs:407-410` — `RateLimited { retry_after_ms }`.
- `crates/nika-kernel/src/ai/provider.rs:428-439` — `is_transient()` impl.
- `crates/nika-kernel/src/io/process.rs` — `ShellCommand.timeout`,
  `ShellError::Timeout`.
- `crates/nika-kernel/src/runtime/tool_executor.rs` — `ToolExecError` (no
  Timeout-as-retry signal — intentional).
- `crates/nika-kernel/src/runtime/agent.rs` — `AgentLoopConfig` (no retry block —
  retry is per-task, not per-loop).
- Workflow YAML schema (legacy) — `retry: { max_attempts, delay_ms, backoff }`.
- ADR-033 (S1-C) — adds `RetryConfig` + `ErrorCategory` to `nika-error` (L0).

## Alternatives considered

### Alt A -- Retry everything (including tool calls)
Engine retries on any error.
Rejected: masks semantic errors. A tool returning "file not found" should
not be retried — the path is wrong, retrying makes the symptom worse
(user waits, then sees the same error).

### Alt B -- Single global retry policy
One `RetryConfig` for the whole engine.
Rejected: HTTP retries operate on millisecond-scale and benefit from
small backoffs; provider 429 retries operate on second-scale (the
`retry-after` is often 5-30s). One config cannot serve both.

### Alt C -- No retry in the engine, push to user
Every workflow author writes their own retry block.
Rejected: 429 handling is so common that boilerplate would dominate
workflow files. Sane defaults at the engine level reduce friction
without taking away user override.

### Alt D -- `tower::retry::Retry` middleware
Use the tower ecosystem.
Rejected for v0.81: `tower` is not in the dep tree before v0.100. The
MEGA_ULTRATHINK rejections list (Batch B) defers tower adoption until
the runtime crate decides whether to standardize on it. We can revisit.

### Alt E -- Retry as a kernel trait (`Retryable`)
Make every error type implement `is_retryable()`.
Rejected: `is_transient()` already exists on `ProviderError` — promoting
it to a trait would force every error type (including `BlobError`,
`MemoryError`) to implement it even when retry doesn't apply.

## Related

- ADR-006 — layer stratification. ADR-019 is the retry-policy projection
  of ADR-006.
- ADR-007 — forward-compat invariants. `ProviderError::RateLimited`
  carries `retry_after_ms` as a future-proof hint; ADR-019 names the
  consumer.
- ADR-018 — runtime + sync primitives. `tokio::time::sleep` and
  `tokio::time::timeout` are the implementation primitives.
- ADR-033 (S1-C) — adds `RetryConfig` + `ErrorCategory` to `nika-error`.

## Notes

Follow-ups:

- ADR-033 + S1-C will land `RetryConfig` and `ErrorCategory` in
  `nika-error`.
- L1 retry implementation lands when `nika-http` is admitted.
- L2 retry implementation lands when verb-* crates are admitted.
- Document the task-level `retry:` block schema in the workflow YAML
  reference (Phase E docs).
