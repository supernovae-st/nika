---
id: ADR-093
title: "nika-infer-local HTTP server · tiny_http for the v1 sidecar endpoint"
status: proposed
date: 2026-06-11
phase: "Phase 2 · post-v0.81 announce ladder"
deciders: ["@ThibautMelen"]
tags: ["inference", "local", "sidecar", "http", "server", "tiny_http", "l1.5"]
affects_crates: ["nika-infer-local", "nika-providers"]
affects_layers: ["L1.5"]
supersedes: []
superseded_by: []
related: ["ADR-003", "ADR-091", "ADR-092", "ADR-105"]
requires: ["ADR-091"]
enables: []
amends: ["ADR-091"]
inv: ["INV-027"]
nika_codes: ["NIKA-460..479 (reserved by ADR-091 · the server maps backend errors to OpenAI-compat error JSON)"]
timeline: "server lands behind `local-infer` in a quiet dependency window; the `local` provider profile row lands with it"
follow_ups:
  - "SSE streaming (`stream: true`) → re-evaluate hyper IF tiny_http chunked writes prove insufficient"
  - "subprocess supervisor (spawn/health/restart) lives at the runtime layer (L3) per ADR-091 §isolation"
---

# ADR-093: nika-infer-local HTTP server — tiny_http for the v1 sidecar endpoint

## Context

ADR-091 decided the sovereign local-inference architecture: a candle backend
behind the `local-infer` feature, **reached over the existing `OpenAiCompat`
wire** (no new dispatch seam). The crate spec (`docs/crate-specs/
nika-infer-local.md` §5bis) verified the remaining gap: the backend is proven
(real-GGUF e2e green) but **nothing serves it** — the engine reaches models
only through `nika-providers` HTTP profiles. The one open decision was the
server's HTTP framework, deliberately deferred to its own ADR because a
dependency choice on the inference path is a 20-year commitment (Diamond
forward-compat doctrine).

The v1 server surface is intentionally tiny:

- `POST /v1/chat/completions` — deserialize `protocol::ChatRequest`, call
  `BackendDyn::generate`, serialize `protocol::ChatResponse` (the wire-contract
  test already pins the exact JSON shape `nika-providers` parses).
- `GET /health` — liveness for the future supervisor (ADR-091 isolation).
- **One in-flight generation** (v1 queue discipline per the crate spec §4bis):
  concurrency is a `Mutex` around the backend, not an async executor problem.
- No TLS (loopback only) · no auth (loopback only · the supervisor owns the
  port) · no streaming yet (`stream: true` returns the non-streamed body v1).

## Options considered

| Option | Deps pulled | Model | Verdict |
|---|---|---|---|
| **`tiny_http`** | ~1 (+ ascii) | blocking · thread-per-connection | ✅ **chosen** — fits "one endpoint · one in-flight" exactly; trivially auditable; zero async runtime requirement in the server loop |
| `hyper` (raw) | ~10 (tokio tower http body…) | async · full control | deferred — the control it buys (streamed bodies · backpressure) is only needed when SSE streaming becomes load-bearing |
| `axum` | ~20+ | async framework · routing/extractors | refused for v1 — a router/extractor framework for two routes is dependency surface without benefit (lean-core doctrine · alignment Rule 3 ranking) |
| hand-rolled `std::net::TcpListener` + HTTP parse | 0 | blocking | refused — re-implementing HTTP/1.1 parsing (chunked encoding · header folding · continue) is the one wheel not worth re-inventing on a security boundary |

## Decision

**`tiny_http` serves the v1 sidecar**, behind a dedicated `server` feature,
orthogonal to `local-infer` (both off by default — the default `nika` build
links neither candle nor the server; CI exercises the full HTTP round-trip
against `MockBackend` without building the candle stack; the sidecar binary
enables both). Sizing facts that drove the call:

1. **The workload is sequential by design.** ADR-091 + the crate spec lock
   one in-flight generation per sidecar process; a multi-minute CPU/Metal
   `forward` loop dominates every request. An async stack cannot speed this
   up; it can only add dependency surface around a blocking core.
2. **The dependency tree is the moat surface.** `tiny_http` keeps the
   `local-infer` feature's *server* increment near zero; the audit story
   ("what runs when I enable local inference?") stays answerable in one
   sitting.
3. **Escape hatch is real, not hypothetical.** The server module is ~200 LOC
   over `BackendDyn` + the `protocol` types. If SSE streaming graduates from
   follow-up to requirement, swapping the listener layer for hyper is a
   contained rewrite of one module — the wire types, backend seam, and tests
   all survive unchanged.
4. **Tests stay hermetic.** A blocking server on an ephemeral port +
   `MockBackend` gives a full client→server→backend round-trip in a plain
   `#[test]` (no tokio runtime juggling), including the self-consistency
   check: `nika-providers`' own `OpenAiCompat` parser reading a live response.

## Consequences

- `nika-infer-local` gains a `server` module (feature `local-infer`):
  `serve(addr, backend) -> ServerHandle` with graceful shutdown; errors map
  to OpenAI-compat error JSON (`{"error": {"message", "type"}}`) so the
  existing provider-side error mapping in `nika-providers` applies unchanged.
- `nika-providers` gains the `local` profile row (wire `OpenAiCompat` ·
  `base_url http://127.0.0.1:<port>` · `requires_key: false`) — one catalog
  row, the same shape as `ollama`/`lmstudio`/`llamacpp`.
- The engine's local-first default path becomes fully ours in Rust:
  `.nika.yaml` `model: local/<alias>` → providers wire → tiny_http server →
  candle backend. The external-daemon profiles (`ollama` etc.) remain as
  peer options — additive, no removal (the catalog is a menu, not a fork).
- Streaming (`stream: true` SSE deltas) is explicitly a follow-up: the
  `GenerationChunk` type exists; the transport decision re-opens (hyper)
  only if tiny_http chunked responses prove insufficient in practice.

🦋 Nika — workflow engine for AI, AGPL, SuperNovae Studio.
