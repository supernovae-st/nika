# Crate spec · `nika-infer-local`

| Field | Value |
|---|---|
| Status | **SCAFFOLDED** 2026-06-11 (protocol + Backend seam + MockBackend · `17ef5ff35`). candle backend = next, behind `local-infer` feature. NOT yet 12-gate admitted. |
| Layer | **L1.5** service (beside `nika-providers`) |
| Decision | ADR-091 (candle-only · no rig · no mistral.rs · sidecar/subprocess · feature-gated · Metal/CPU-first · CUDA flagged) |
| LOC budget | ≤15k crate · ≤1500/file · ≤100/fn (Diamond caps) |
| err_prefix | `none` (reports via OpenAI-compat HTTP error at the boundary · `NIKA-460..479` band reserved for the engine-facing mapping) |

## 1. Purpose

The sovereign local inference sidecar — the all-Rust replacement for the
ollama (Go daemon) dependency on the local-first default path. An inference
server we **ship and spawn as a supervised child process**, reachable over the
**existing `OpenAiCompat` wire** (`nika-providers`) — to the engine it looks
like any local OpenAI-compatible server, except it is ours, in Rust (candle).

## 2. Public API (current scaffold)

- `protocol::{ChatRequest, ChatResponse, Message, Role, FinishReason, Usage, Choice}`
  — the OpenAI-compat chat subset the sidecar serves.
- `Backend` — the device-agnostic generation seam (`async fn generate(&self,
  &ChatRequest) -> Result<ChatResponse, InferLocalError>`, `Send` future).
- `MockBackend` — deterministic, dependency-free (CI path + self-consistency).
- `GenerationChunk` — the SSE delta unit (streaming layer).
- `InferLocalError` — typed failure modes.

## 3. candle backend design (research-backed · Context7 `/huggingface/candle`)

Behind `local-infer` (off by default → the lean core never links candle).
Deps: `candle-core` · `candle-transformers` · `tokenizers` · `tokio`
(+ `candle-core/metal` under a `metal` sub-feature · `cuda` flagged later).

**The generation loop** (canonical candle quantized path):

```text
load   · gguf_file::Content::read(&mut file) → Qwen2::from_gguf(content, &mut file, &device)
         tokenizer from tokenizer.json (HF `tokenizers` crate)
prefill· model.forward(&prompt_tokens, /*index_pos*/ 0)  → logits for last pos
         (KV-cache lives INSIDE the model · &mut self · index_pos threads position)
decode · loop:
           logits = model.forward(&[next_token], index_pos)
           logits = apply_repeat_penalty(logits, penalty, &recent[..repeat_last_n])?  // opt
           next   = logits_processor.sample(&logits)?
           index_pos += 1
           halt on: next == eos_token  ·  decoded text ends with a stop sequence
                    ·  generated >= max_tokens (→ FinishReason::Length)
sample · LogitsProcessor::from_sampling(seed, Sampling::TopKThenTopP{k,p,temperature})
         seed is REQUIRED (deterministic golden tests · ADR-091 SOTA invariant)
```

Device: `Device::Cpu` / `Device::new_metal(0)` — abstracted, so CUDA is a
compile feature, not a rewrite.

## 4. SOTA invariants (must-not-forget · the candle commit's gates)

- **KV-cache** — internal to the candle model; thread `index_pos` correctly
  (prefill seq_len then 1-token decode). The #1 perf item.
- **Per-family chat template** — Qwen3 ≠ Llama3 ≠ Mistral. Render the model's
  HF `chat_template` (minijinja) or a vetted per-family template. (arxiv agent
  finalizing the Rust approach — mistral.rs precedent.)
- **EOS + stop sequences** — halt on the model's EOS token AND user `stop[]`.
- **Seeded sampling** — `from_sampling(seed, …)`; reproducible output.
- **Structured output** — v1 = schema-validation **retry loop** (the pattern
  `eval/` already proves); v2 = logit-masking / `llguidance` (guaranteed-valid
  first pass · the real SOTA · agent researching).
- **GGUF quantization** — Q4/Q5; local-sized models.
- **Lazy warm load** — load once on first request, keep resident; unload policy.
- **Context cap** — error (`ContextOverflow`) when the rendered prompt exceeds
  the model window.

## 4bis. Known v1 limitations (documented, not hidden)

- **No tool-calling**: `FinishReason` carries `Stop`/`Length` only. The engine's
  `OpenAiCompat` parser also maps the `"tool_calls"` literal — a local model
  that tool-calls would surface as `StopReason::Unknown("tool_calls")`. The
  variant is added WHEN the candle backend emits tool calls (no dead enum
  variants before then · review S4).
- **Sequential requests**: one in-flight generation per sidecar process (v1
  queue) — high-throughput fan-out stays on the `vllm/`/`llamacpp/` providers.

## 5. Isolation (ADR-091)

The backend runs in a **supervised child process** (spawn / health-check /
restart / port-alloc / graceful-shutdown). A model crash (panic · OOM-kill ·
GPU abort) kills only the child; the lean core detects the broken pipe and
restarts/degrades. `catch_unwind` is the inner backstop, the process boundary
is the real containment.

## 6. Verification plan (Test > Implement · Verify > Ship)

- **Unit (CI · no model)** — Sampling config builder · chat-template rendering ·
  stop-sequence detection · the `MockBackend` contract (already green).
- **e2e (gated · real model)** — `#[ignore]` test that loads a small quantized
  GGUF (Qwen3-0.6B-class) and asserts: non-empty output · halts at EOS · honors
  max_tokens (→ Length) · deterministic under a fixed seed (byte-equal on CPU).
  Documented run command; model NOT committed.
- **Self-consistency** — a built `ChatResponse` serialized → parsed by Nika's
  own `OpenAiCompat` client (wired alongside `nika-providers`).

## 7. The 12 gates (readiness map)

| Gate | Status |
|---|---|
| 1 SPEC | ✅ this file + ADR-091 |
| 2 TDD | scaffold: MockBackend tests RED→GREEN ✅ · candle: pending |
| 3 IMPL | scaffold ✅ · candle backend pending |
| 4 CLIPPY 0 | scaffold ✅ |
| 5 MUTATION ≥90% | pending candle impl |
| 6 PROPERTY | protocol round-trip ✅ · sampling/template props pending |
| 7 BENCH | tok/s + TTFT once candle lands (justified deferral until then) |
| 8 DOCS | scaffold ✅ (cargo doc 0) |
| 9 CANARY | a `.nika.yaml` infer-via-local once the verb wires it |
| 10 PARITY | output shape vs the OpenAiCompat client (self-consistency) |
| 11 REVIEW | swarm before candle admission |
| 12 ATOMIC | 1 admission = 1 commit |

🦋 Nika — workflow engine for AI, AGPL, SuperNovae Studio.
