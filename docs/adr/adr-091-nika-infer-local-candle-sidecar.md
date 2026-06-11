---
id: ADR-091
title: "Sovereign local inference · nika-infer-local · pure-Rust candle sidecar, OpenAI-compat reuse, no rig / no mistral.rs"
status: proposed
date: 2026-06-11
phase: "Phase 2 · post-v0.81 announce ladder"
deciders: ["@ThibautMelen"]
tags: ["inference", "local", "sovereign", "candle", "sidecar", "providers", "l1.5"]
affects_crates: ["nika-infer-local", "nika-providers", "nika-verb-infer"]
affects_layers: ["L1.5"]
supersedes: []
superseded_by: []
related: ["ADR-003", "ADR-081", "ADR-090", "ADR-093", "ADR-094"]
requires: ["ADR-003"]
enables: []
amends: []
inv: ["INV-017", "INV-027"]
nika_codes: ["NIKA-460..479 nika-infer-local (reserved · final range at scaffold)"]
---

# ADR-091 · Sovereign local inference — `nika-infer-local` (pure-Rust candle sidecar)

## Context

Nika's v0.1 provider canon ships **14 providers** (`spec/canon.yaml`): 8 cloud + 5
local + 1 mock. All 5 local providers (`ollama` · `lmstudio` · `llamacpp` ·
`localai` · `vllm`) are **external OpenAI-compatible HTTP servers** — the engine
talks to them over localhost. The in-process GGUF runtime `native` was
**DEFERRED** (`spec/stdlib/providers-v0.1.md`: "*mistral.rs crashed the host*")
pending "a candle/llama.cpp binding stabilizes + 30-day crash-free cohort +
cross-platform conformance."

This leaves a **sovereignty gap**: the local-first default (`ollama/<model>`)
delegates inference to an external **Go** daemon. SuperNovae preaches local-first
sovereign (alignment Rule 3), yet the inference runtime is the one non-Rust,
non-ours link in the chain. We want to close it — to **own** the local inference
path, in Rust.

## Decision

Build **`nika-infer-local`**: a pure-Rust, **candle**-backed local inference
server we ship and spawn, that the engine talks to over the **existing
`OpenAiCompat` wire** (zero new dispatch seam). It replaces the dependency on
ollama for the sovereign-default path.

Locked choices:

1. **candle only** (`candle-core` + `candle-transformers` + `tokenizers`). The
   model zoo (Llama3 · Qwen3 · Mistral · Phi · Gemma) + quantized GGUF loaders
   ship in `candle-transformers`. Pure Rust, no C compiler, no FFI.
2. **NO rig.** rig is an app/agent framework, not a runtime, and Diamond already
   rejected it (`nika-providers` talks wire directly — `providers-v0.1.md`).
   Re-introducing it reverses a locked decision and duplicates
   `nika-verb-agent` + `nika-providers`.
3. **NO mistral.rs** (by default). It is candle + a far larger surface (vision ·
   audio · image-gen · LoRA · AnyMoE · MCP-client · its own `unsafe` CUDA) than a
   chat-completion sidecar needs, and moves fast (0.8.x, thin stable surface).
   We own a bounded generation loop instead (CRAFT). Revisit only if the
   self-authored loop proves insufficient.
4. **Sidecar, not in-process** (Option B). The runtime runs in a **supervised
   child process** the engine spawns and owns (not an external daemon). A crash
   (panic · OOM-kill · GPU abort) kills only the child; the lean core detects
   exit/broken-pipe and restarts/degrades. This is the only model that contains
   memory-corruption crashes (`catch_unwind` cannot) — directly answering the
   defer's "must not crash the host" bar. Reuses `OpenAiCompat` → **zero new
   dispatch code**.
5. **Feature-gated, off by default.** candle never enters the lean core build
   unless the `local-infer` feature is enabled (feature-defaults discipline). A
   default `nika` binary ships zero inference deps.
6. **Metal + CPU first, CUDA flagged.** candle abstracts the device
   (`Device::Cpu | Metal | Cuda`). v1 targets CPU + Metal — testable on the dev
   machine (Apple Silicon · `cargo test` real · 12 gates verifiable locally),
   and the safest crash profile (the researched crash classes — mistral.rs
   CUDA-graph UAF, candle u32÷f64 silent-zeroing, stream-consistency — are all
   CUDA-specific). `cuda` is a compile feature activated when NVIDIA hardware is
   in the test loop; the device-agnostic serving loop means CUDA is a flag + a
   hardware test pass, **not a rewrite**. Datacenter-scale CUDA throughput stays
   delegated to the `vllm/` · `llamacpp/` providers.

## SOTA checklist (must-not-forget · from the 2026-06-11 Socratic pass)

Correctness killers (v1): KV-cache wired (O(n²) without it) · per-family chat
template (wrong template = garbage) · SSE token streaming · EOS + stop-sequence
halt · full sampling + **seed** (determinism for tests) · OpenAI-compat shape
fidelity verified by feeding our output through Nika's *own* `OpenAiCompat`
parser (in-process self-consistency test). Scope/safety: subprocess
lifecycle + cancellation (CANCEL SAFETY) · sequential request queue v1
(documented limit · batching deferred) · GGUF quantization · lazy warm model
load · typed errors (model-not-found · OOM · context-overflow · gen-timeout).
Default model: **Qwen3** (BFCL tool-calling + NVIDIA SLM-agents thesis).

## Open sub-decisions (operator ruling pending · recommended defaults shown)

- **Structured output v1** → recommend **retry-loop** (generate → validate vs
  schema → re-prompt with the error; the exact pattern the `eval/` harness
  already proves) · **logit-masking / GBNF** (guaranteed-valid first token,
  the real SOTA) deferred to v2.
- **Embeddings in the same sidecar** → recommend **YES (v1.1)**: candle serves
  embeddings (bge/e5), making the Connectome/memory stack 100% sovereign Rust.
  A large lever; sequence after chat-completion lands.
- **Crate boundary vs reserved `nika-vision-local`** (ADR-081, NIKA-1500..1599)
  → recommend `nika-infer-local` = text (+ embeddings later); `nika-vision-local`
  stays separate (heavy vision deps); align NIKA code ranges at scaffold.

## Alternatives rejected

- **Option A · in-process `native`** — purest single binary, lowest latency, but
  re-imports the crash-takes-down-engine risk that caused the original defer.
  Kept as a possible v2 behind the same feature flag once candle proves a
  30-day crash-free cohort (the canon's re-admission gate).
- **llama-cpp-rs** (`llama-cpp-2`) — broadest model/quant/backend coverage and
  most battle-tested, but its README states "*not safe … do not use where UB is
  not acceptable*", needs clang+bindgen (breaks pure-Rust single-binary), and
  ships live multi-backend crash classes. Contradicts the all-Rust + must-not-
  crash thesis in-process; only viable behind the same subprocess isolation,
  and then candle is the cleaner Rust-native default.
- **Burn-LM** — promising pure-Rust multi-backend runtime, but **alpha** (Aug
  2025). Re-evaluate ~2 release cycles out.

## Consequences

- Kills the ollama (Go daemon) dependency for the sovereign-default path —
  closes the alignment Rule 3 gap.
- The lean core stays lean (feature off by default · candle isolated).
- The engine gains an all-Rust, crash-isolated, air-gapped local inference path
  reusing the existing wire — no new dispatch seam, no new provider model.
- Re-admission of `native` (Option A, in-process) becomes a future flag flip,
  not a redesign.

🦋 Nika — workflow engine for AI, AGPL, SuperNovae Studio.
