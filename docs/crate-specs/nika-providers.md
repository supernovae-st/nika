# Crate spec — `nika-providers`

| | |
|---|---|
| Status | **L1.5 admission target · DESIGN PROPOSAL** (Phase-B slice step 8.5 · before the verbs per D-2026-05-22-N17 · announce ladder per D-2026-06-10-N6) · architecture mostly **determined by existing canon** (§0) · ONE open fork for operator lock (§4) |
| Layer | L1.5 — service crate · the shared LLM-provider layer BOTH `nika-verb-infer` (s9) and `nika-verb-agent` depend on (no verb→verb sideways dep · D-N17) |
| Design | impls of the EXISTING L0.5 `nika_kernel_ai::provider` ISP traits (`ProviderInferDyn` · `ProviderStreamDyn` · `ProviderMeta`) · transport via the L0.5 `nika_kernel::http` traits (injected effect · NOT its own `reqwest`) |
| LOC budget | under the ≤1500/file + ≤15k/crate caps (vectors 12+24) · live count · `scripts/crate-metrics.sh nika-providers` |
| Crate version | tracks workspace (`0.80.0`) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L1.5 service crate |
| NIKA codes | none new — speaks the kernel-side `ProviderError` (Pattern A · codes **NIKA_330–379 already registered** in `nika-kernel-ai/src/errors.rs` · api/model-not-found/rate-limited/auth-failed/…) |

---

## §0 · Architecture — determined by existing canon (verified 2026-06-10)

Unlike s8 `nika-policy` (which needed a NEW kernel trait), the s8.5 seam
**already exists end-to-end**. Verified empirically against `main`:

1. **The provider contract is L0.5-complete.** `nika-kernel-ai/src/provider.rs`
   (706 LOC · admitted with the kernel 4-way split) ships the ISP decomposition
   `ProviderInfer` / `ProviderStream` / `ProviderMeta` (+ opt-in `ProviderEmbed` ·
   `ProviderVision`), each async trait with its `*Dyn` (`Send`) companion via
   `trait_variant`. The combined `Provider` super-trait is **sealed**
   (workspace crates only — this crate is the intended implementor). DTOs are
   rich and locked: `InferRequest` (model · messages · tools · tool_choice ·
   `response_format` · stop · thinking_budget · extras · budget/baggage/tenant ·
   cancel) · `Message`/`ContentBlock` (text · image · tool_use · tool_result ·
   thinking) · `Role` (descended to `nika-error`).

2. **Transport goes through the kernel http seam — NOT a second `reqwest`.**
   `nika-http` is canonically « the only production site touching `reqwest` »
   (its crate spec · admitted s5) and the kernel `HttpPost` trait already
   carries `send_streaming → HttpStreamResponse` with the mid-stream
   `TooLarge` counting cap — i.e. **SSE streaming is already supported by the
   effect layer**. `nika-providers` therefore takes an injected
   `Arc<dyn HttpClientDyn>` (the wiring layer hands it `ReqwestHttp`) and
   never owns a TLS/connection stack. One http production site · provider
   calls inherit the effect floor (timeouts · size caps · redirect discipline).

3. **The 14-provider registry is data, not code, and it is already generated.**
   `nika-catalog` (L0 · admitted · codegen WIRE per D-2026-06-10-N4 ·
   byte-identical proof) exposes `all_providers()` + `all_pricing()` projected
   from `nika-spec/canon.yaml` (SSOT · 8 cloud + 5 local + 1 mock = 14 per
   D-2026-06-10-N2). This crate consumes the catalog rows as **provider
   profiles** — it never hardcodes the list.

4. **Observability parity is a kernel invariant, not an adapter choice.**
   `GenAiAttrs` (OTel GenAI semconv bridge · `nika-kernel-ai/src/genai.rs`) is
   embedded on `InferRequest`/`InferResponse` — « no provider can silently
   drop an attribute the kernel exports » (Pre-launch Gate 2). Every adapter
   populates it; the cross-provider parity test (§5) enforces it.

```text
   L2 verb-infer (s9) · verb-agent ──── depend on ────┐
                                                      v
   L0.5 nika-kernel-ai::provider  ProviderInferDyn / ProviderStreamDyn / ProviderMeta
                                                      ^
   L1.5 nika-providers ── implements ─────────────────┘
        ProviderRegistry · profiles (from nika-catalog) · wire adapters
              │  transport = injected kernel http (Arc<dyn HttpClientDyn>)
              v
   L1 nika-http (ReqwestHttp · the ONE reqwest site · SSE via send_streaming)
```

## §1 · The wire-format insight — 14 providers ≠ 14 adapters

The 14 canonical providers collapse onto **three wire formats** (verified
against the brouillon reference `tools/nika-engine/src/provider/` +
`tools/nika-core/src/catalogs/` — CRAFT rewrite, zero copy-paste; the
brouillon's `rig`-based construction is NOT carried — Diamond talks wire
directly through the kernel http seam):

| Wire adapter | Providers covered | Notes |
|---|---|---|
| `anthropic` (Messages API) | anthropic | native · thinking blocks · first for the Phase-B demo |
| `openai-compat` (Chat Completions) | openai · deepseek · mistral · xai · groq · openrouter + ALL 5 local (ollama · lmstudio · llamacpp · localai · vllm) | ONE adapter × 12 profiles (endpoint + auth header + quirk flags) · openrouter/local = `base_url` profile rows |
| `gemini` (generateContent) | gemini | distinct request/response shape |
| `mock` | mock | in-crate test provider (deterministic · zero network) — the engine-test + `hello.yaml` zero-key surface |

A **profile** is data: `{ key, wire: Anthropic|OpenAiCompat|Gemini|Mock, base_url,
auth: Bearer|XApiKey|None, env_key: NIKA_<PROVIDER>_API_KEY ladder, quirks }`,
seeded from `nika-catalog::all_providers()`. Adding provider №15 (post-announce)
= a canon.yaml row + a profile mapping — usually zero new wire code.

## §2 · Public API (the crate shape)

```rust
pub struct ProviderRegistry { /* profiles + the injected http effect */ }
impl ProviderRegistry {
    pub fn new(http: Arc<dyn HttpClientDyn>, config: ProvidersConfig) -> Self;
    /// `model: provider/name` (pillar ⑤) → resolve the profile + concrete model.
    pub fn resolve(&self, model: &str) -> Result<ResolvedProvider<'_>, ProviderError>;
    pub fn provider(&self, key: &str) -> Option<&dyn ProviderHandle>;
}

/// One resolved provider — implements the kernel Dyn traits.
/// (ProviderInferDyn + ProviderStreamDyn + ProviderMeta · sealed super-trait OK — workspace crate.)
pub struct ResolvedProvider<'r> { /* profile + http + model */ }

pub struct ProvidersConfig {       // serde · operator-owned
    // per-provider overrides: base_url (local/openrouter) · api_key env override ·
    // timeout · default model — additive on top of catalog profile rows
}
```

Key resolution: API keys read from env (`NIKA_ANTHROPIC_API_KEY` →
`ANTHROPIC_API_KEY` fallback ladder) at construction · **never logged · never
serialized** (`Debug` redacts · same discipline as the brouillon vault rule) ·
missing-key error message prints the exact `export` line (first-error UX ·
B7.2). No key needed for `mock` and the 5 local providers (auth `None`).

## §3 · Security posture

- **SSRF interplay** · cloud profiles target pinned `https://` hosts (catalog
  rows · not attacker-influenced). Local profiles (ollama `127.0.0.1:11434` …)
  are **operator-configured endpoints** — the provider call path constructs
  its http requests against the resolved profile `base_url` ONLY (workflow
  content never becomes a URL here · the SSRF-sensitive surface stays
  `nika:fetch`/`nika-http` floor territory). `base_url` override accepted
  exclusively from operator config — never from workflow YAML.
- **Budget seam** · `InferRequest.budget` (kernel `BudgetDirective`) flows
  through untouched; enforcement is `nika-policy` (s8 · `check_budget`) — this
  crate REPORTS usage (`Cost` from `nika-error::cost` + catalog pricing rows) ·
  it does not gate. Compose-only, mirror of the s8 split.
- **Zero telemetry** · adapters emit `GenAiAttrs` on the response DTO — *data
  for the caller*, no exporter, no network beyond the provider call itself
  (telemetry-canon).

## §4 · The ONE open fork — adapter scope at admission (operator lock)

The ladder note says « ship `anthropic` first for the Phase B demo ». Three
calibrations possible for the **admission commit** (12 gates GREEN on
whichever is chosen):

| Option | Scope at s8.5 admission | Coverage | Risk |
|---|---|---|---|
| A | `anthropic` + `mock` | 2/14 | smallest reviewable diff · openai-compat lands s8.6 |
| **B ⭐** | `anthropic` + `openai-compat` + `mock` | **13/14** | one more adapter · covers ALL local providers → `infer` works offline day-1 · gemini = fast-follow before tag |
| C | all three wires | 14/14 | largest single admission · gemini quirks (safety settings · parts) eat review time |

**Recommendation · B.** The openai-compat adapter is the highest-leverage 200
lines in the slice (12 profiles · the local-sovereignty story at announce) ·
gemini follows as a small PR before the v0.81.0 tag (~07-28). Option B keeps
the announce claim « 14 providers » honest at tag time, not at s8.5 time.

## §5 · Test strategy (12-gate plan)

- **TDD against the seam** · unit tests drive `ResolvedProvider` through the
  kernel `*Dyn` traits with a **fake `HttpClientDyn`** (in-crate test double
  returning canned wire responses · no wiremock dependency needed — the http
  effect is already behind a trait · this is the dividend of §0.2).
- **Cross-provider parity matrix** (the house rule · « same test on ALL
  providers — failure = engine bug ») · ONE test suite parameterized over
  every profile × {infer · infer_stream · tool_use · response_format ·
  GenAiAttrs populated · error mapping 330-379} · wire fixtures per format.
- **Streaming** · SSE chunk reassembly tested against recorded anthropic +
  openai event fixtures · mid-stream `TooLarge`/cancel propagation from the
  effect layer surfaces as clean `ProviderError`.
- **No live-network tests in `--lib`** (Keychain/CI discipline) · live smoke =
  manual `nika doctor` territory later (s19).
- Gates: fmt · clippy -D · ≤caps · 0 unwrap · mutation ≥90% · review swarm
  (spn-nika:code-reviewer + spn-rust:rust-pro + feature-dev:code-reviewer) ·
  public-api floor · insta where DTO-shaped.

## §6 · Sequencing (concurrent-session discipline)

- **No kernel edits needed** (the seam is admitted) → ZERO collision with the
  session-B kernel-migration lane. The crate is net-new territory
  (`crates/nika-providers/`) + one workspace-members line.
- Depends on: `nika-kernel-ai` (traits/DTOs) · `nika-kernel` (http traits
  re-export) · `nika-catalog` (profiles/pricing) · `nika-error` (codes/cost) ·
  dev-only: nothing network-bound.
- Unblocks: **s9 `nika-verb-infer`** (the INFER half of the announce floor) ·
  later `verb-agent` shares it (D-N17's whole point).
- `nika-native` (in-process candle/mistral.rs · L1.5 step 30) stays a
  SEPARATE crate — its profiles slot into the same registry later
  (`--features native` · 2-track local story per the modality ledger).

## §7 · Related

- `docs/crate-specs/nika-policy.md` (s8 sister · the compose-only precedent)
- `docs/crate-specs/nika-http.md` (s5 · the transport floor this crate rides)
- `nika/02-engineering/architecture/blueprint/crate-admission-order.md` (step 8.5 row + D-N17 note)
- `nika-spec/canon.yaml` `providers:` (SSOT 14) + `stdlib/providers-v0.1.md`
- brouillon reference (read-only · `git show brouillon:tools/nika-engine/src/provider/…` · rig construction NOT carried)
