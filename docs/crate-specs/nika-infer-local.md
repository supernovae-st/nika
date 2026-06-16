# Crate spec · `nika-infer-local`

| Field | Value |
|---|---|
| Status | **12-GATE ADMITTED 2026-06-16** (scaffold `17ef5ff35` → review-fix `ce287e8c` → candle backend behind `local-infer` → final admission swarm + Gate-9 HTTP wire-conformance canary). Real-model e2e (Qwen3-1.7B Q8 · CPU) — see §6. Removed from `workspace.metadata.diamond.wip`. |
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

**The generation loop** (canonical candle quantized path · AS BUILT):

```text
load   · gguf_file::Content::read(&mut file)
         → context_window from metadata "qwen3.context_length" (BEFORE the move ·
           absent key degrades OPEN to usize::MAX)
         → Qwen3::from_gguf(content, &mut file, &device)
         tokenizer from tokenizer.json (HF `tokenizers`) · EOS = the SET
         {<|im_end|>, <|endoftext|>} (dual-EOS guard)
guard  · prompt_tokens.len() >= context_window → ContextOverflow (BEFORE prefill ·
         `>=` keeps one slot for a generated token)
prefill· clear_kv_cache() (MANDATORY between requests) →
         model.forward(&prompt_tokens, /*offset*/ 0) → sample_next
decode · loop:
           halt on: EOS-set hit (token popped) · stop-string on the decoded TAIL
                    (bounded window = max_stop_len/2 + 4 tokens · O(window)/step)
                    · generated >= max_tokens (→ FinishReason::Length)
           logits = model.forward(&[next], prompt_len + index)
           pre-softmax (one to_vec1 round-trip when active):
             apply_repeat_penalty(…, penalty, recent[..repeat_last_n])   // opt
             apply_top_n_sigma(…, n)            // opt · arXiv:2411.07641
           next = sample_next(lp, logits, min_p)
sample · sample_next = lp.sample_f(|prs| apply_min_p(prs, p)) when min_p set
         (post-softmax hook · candle skips it on ArgMax — greedy needs no
         truncation) · else lp.sample · LogitsProcessor::from_sampling(seed, …)
         seed REQUIRED (deterministic golden tests · ADR-091 SOTA invariant)
errors · OOM message-classified to OutOfMemory · Timeout reserved to the
         SERVER layer (the sync loop has no preemption point)
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
- **No per-connection read timeout** (`tiny_http` exposes none): a slow-reader
  (Slowloris-style) client can stall the single worker thread. Out of the v1
  threat model — the sidecar is loopback-only, no auth, the supervisor owns the
  port (ADR-093) — so there is no remote surface; tracked for the streaming /
  `hyper` re-open where a read deadline lands. (Gate-11 security pass P2-2,
  2026-06-16.)

## 5. Isolation (ADR-091)

The backend runs in a **supervised child process** (spawn / health-check /
restart / port-alloc / graceful-shutdown). A model crash (panic · OOM-kill ·
GPU abort) kills only the child; the lean core detects the broken pipe and
restarts/degrades. `catch_unwind` is the inner backstop, the process boundary
is the real containment.

## 5bis. Connection path — wiring the island into a `.nika.yaml`

**Status update 2026-06-11 (same day · the server hop is BUILT):** the
`server` module ships behind an orthogonal `server` feature (ADR-093 ·
tiny_http · `POST /v1/chat/completions` + `GET /health` · stream refused
honestly · 8 integration tests over a real TCP round-trip + MockBackend).
**Remaining hop: the `local` provider profile.** That row cascades on the
spec provider canon (`canon.yaml` provider list + `providers-v0.1.md`) and
the provider-prefix validation surface (nika-schema) — DEFERRED to a window
where those canon surfaces are not mid-arc in a concurrent session. Until
then the crate is a *served* island: reachable over HTTP, not yet routable
from a `.nika.yaml`.

**The path (3 hops · each side already exists):**

```
.nika.yaml `model: local/qwen3`
   │
   ▼  nika-providers · a NEW "local" Profile (wire: OpenAiCompat · base_url:
   │   http://127.0.0.1:<port> · requires_key: false) — ONE catalog row, the
   │   same shape as the ollama/lmstudio/llamacpp local profiles already there
   ▼  the existing OpenAiCompat wire adapter speaks to localhost
   │
   ▼  nika-infer-local SERVER (the one unbuilt piece): a minimal
   │   POST /v1/chat/completions over BackendDyn — deserialize ChatRequest
   │   (protocol.rs already IS the wire type) → generate → serialize
   │   ChatResponse (wire-contract test already pins the exact shape)
   ▼  CandleBackend (built · proven)
```

**Why this is mechanical, not speculative:**
- `protocol.rs` types ARE the OpenAI-compat wire (the `wire_contract` test
  already proves a `ChatResponse` parses through nika-providers' own parser).
- `nika-providers` adding a `local` profile = one `CATALOG_WIRED` row (the
  local-provider precedent — ollama/lmstudio — is right there).
- The model alias (`local/qwen3` → which GGUF) resolves via the model-pull
  CAS (§marketplace doc) — a forkable alias pack, not a hardcoded table.

**The ONE real decision (ADR-level · operator/architecture):** the server's
HTTP framework. Options, ranked by the lean-core/sovereign doctrine:
1. **`tiny_http`** (minimal · ~1 dep · one blocking handler · fits "one
   endpoint, one in-flight generation" v1) — *recommended for v1*.
2. **`hyper`** (lower-level · more control · heavier) — if streaming SSE
   becomes load-bearing.
3. **`axum`** (the ecosystem default · heaviest) — overkill for one endpoint;
   reconsider only if the sidecar grows many routes.
All ride behind the `local-infer` feature (the default `nika` build links
none). The choice is genuinely load-bearing (20-yr dep) → an ADR, not a
quick-win, and it wants a quiet Cargo.lock window (not mid-hot-concurrent-
session — the lean-core dep tree must not churn under another session's WIP).

**Subprocess supervisor** (ADR-091 isolation): `nika` spawns the server child,
health-checks `/health`, restarts on broken pipe. Lives in the runtime/daemon
layer (L3), not here — this crate stays the backend + the (thin) server.

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
| 2 TDD | ✅ MockBackend RED→GREEN · mutant-killer tests from the Gate-5 audit |
| 3 IMPL | ✅ candle backend wired (generation loop · min-p via `sample_f` · top-nσ + penalty pre-softmax · bounded stop-tail decode) |
| 4 CLIPPY 0 | ✅ both feature axes (default + `local-infer`) |
| 5 MUTATION ≥90% | ✅ pure surface ~99% (87/111 caught · every pure survivor killed 2026-06-11) · 21 remaining missed = `candle_backend.rs` **model-gated exemption** (only exercisable by the real-GGUF e2e, which the mutants harness cannot run — same class as nika-ocr's Rule-2 inference exemption). 2026-06-16: the P2-1 clamp lives in the pure `budget` module (5 tests pinning the `min` / `saturating_sub` / `unwrap_or` mutants) — extends the pure tested surface, NOT the exemption. |
| 6 PROPERTY | ✅ protocol round-trip · min-p (keeps-max + floor) · top-nσ (temperature-invariance + keeps-max) · token-mask (exact-zeroing) · repeat-penalty (never-raises) |
| 7 BENCH | ✅ decode-loop hot path (`benches/logits_bench.rs` · criterion · vocab 151,936): min-p **~126µs** · top-nσ **~438µs** · token-mask/repeat-penalty single-pass — all negligible vs a multi-ms CPU forward step (the per-token overhead budget). Model tok/s is e2e-gated (the real-GGUF test prints it) |
| 8 DOCS | ✅ cargo doc 0 both axes |
| 9 CANARY | ✅ end-to-end HTTP canary — `tests/server_http.rs::served_bytes_are_wire_valid_at_the_openai_compat_paths` drives a real TCP request → the live `serve()` sidecar → response, asserting the UNTYPED JSON pointer paths (`/choices/0/message/{content,role}` · `/choices/0/finish_reason` · `/usage/*` · `/id`) the engine's `nika-providers` OpenAiCompat parser walks — on the served bytes, so a serde rename can't pass silently. No verb consumes the crate yet (grep-verified), so the workflow-level `.nika.yaml` canary lands with the verb that routes `model: local/<x>` here. |
| 10 PARITY | ✅ wire-level self-consistency (tests/wire_contract.rs pins the exact JSON paths + literals nika-providers' parser reads) · real-model e2e green (Qwen3-1.7B Q8 · CPU · EOS/Length/determinism) |
| 11 REVIEW | ✅ scaffold 3-lens (ce287e8c) + candle-backend adversarial review folded (2 P1 + 3 P2/P3: stop-tail window made conservative · OOM phrase-match not substring · tail-decode error propagated · device stored at load · bench black_box) · **final admission swarm 2026-06-16**: rust-security PASS no-blockers (network/model/permits SOUND · 0 production unwrap clippy-enforced on default+`server`) + spn-refuter (sole gap = Gate-9, now closed). Folded — **P2-1** wire `max_tokens` clamped to remaining context (pure `budget` module · 5 unit tests · no OOM / no past-window desync · honest doc) · **P2-3** token math saturating (local invariant) · **P2-2** Slowloris noted (Known Limitations §4bis · loopback-only, out of threat model). |
| 12 ATOMIC | 1 admission = 1 commit |

🦋 Nika — workflow engine for AI, AGPL, SuperNovae Studio.
