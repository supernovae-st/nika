---
id: ADR-094
title: "Parallel intra-turn tool dispatch — concurrent resolve, request-order fold"
status: accepted
date: 2026-06-12
phase: ""
deciders: ["@ThibautMelen"]
tags: [nika-verb-agent, agent-loop, parallel-function-calling, determinism]
affects_crates: [nika-verb-agent]
affects_layers: [L2]
supersedes: []
superseded_by: []
related: ["ADR-093"]
requires: []
---

# ADR-094 — Parallel intra-turn tool dispatch

- **Status**: Accepted (2026-06-12)
- **Layer**: L2 (`nika-verb-agent`)
- **Relates**: ADR-093 (the intelligence layer this rides in); amends the
  spec §5 « sequential dispatch » fence the same way ADR-093 amended the
  « no reflection » fence — engine-internal, zero YAML surface.

## Context

When a model batches several tool calls into ONE turn, those calls are
independent by construction — the model had no observations between them
to condition on. Dispatching them sequentially serializes I/O latency
for no semantic gain; LLMCompiler (Kim · Moon · Tabrizi et al. 2023,
arXiv:2312.04511) measures the latency win of executing such calls in
parallel, and ReWOO (Xu · Peng · Lei et al. 2023, arXiv:2305.18323)
makes the broader case against interleaving halts where no dependency
exists.

The constraint that matters more than speed: Nika's event stream and
transcript are DETERMINISTIC contracts (replay-stable, byte-comparable
across runs). Naive `join_all` + completion-order folding would leak
scheduling nondeterminism into both.

## Decision

`run_batch` resolves one turn's (already whole-batch whitelist-validated)
tool calls in TWO phases:

1. **Concurrent resolve** — `futures_util::stream::iter(...)
   .buffered(max_parallel_tools)`: up to N calls in flight; `buffered`
   yields in INPUT order. The phase is pure with respect to loop state:
   no observer calls, no router mutation (two concurrent `agent:compose`
   checks must not interleave the event stream).
2. **Sequential fold** (request order): `ComposeChecked` + `ToolCompleted`
   telemetry, the router's recency ledger, the guard signature parts,
   the result blocks.

Consequences, all pinned by tests:
- the transcript feeds results back in REQUEST order — byte-identical to
  sequential dispatch;
- the guard signature is computed over the same (calls, results) in the
  same order — stall detection is unaffected;
- the observer event sequence is identical to sequential;
- `max_parallel_tools: 1` (engine-internal `AgentConfig`) restores
  strictly sequential dispatch exactly;
- CANCEL SAFETY: dropping the batch future drops the buffered stream —
  in-flight calls cancel per their seam contracts (a compose
  `spawn_blocking` completes detached, result discarded).

## Verification

`tests/research_conformance.rs::one_turns_batched_calls_run_concurrently_
and_feed_back_in_request_order` — a rendezvous executor in which EVERY
call waits until 2 calls are in flight: sequential dispatch deadlocks
there (a 5s timeout turns the hang into a loud failure); the pass proves
true intra-turn concurrency, and the same test asserts the fed-back
blocks AND the `ToolCompleted` events arrive in request order. The
pre-existing `multiple_tools_in_one_turn_all_dispatch_in_order` keeps
the mock-determinism contract pinned.

## Alternatives rejected

- **`join_all` + completion-order fold** — leaks scheduler
  nondeterminism into the transcript and the event stream; breaks
  replay determinism for zero additional win over ordered `buffered`.
- **Model-declared dependency graphs inside a turn** (full LLMCompiler
  planner) — Nika already HAS a dependency planner: the workflow DAG.
  Intra-turn, the batch is independent by construction; asking the
  model to re-declare dependencies adds a failure surface the DAG
  already covers at the right layer.
- **Cross-turn speculation (ReWOO-style full plan-ahead)** — changes
  the ReAct contract observable by authors; the spec's verb semantics
  stay untouched at v0.1.
