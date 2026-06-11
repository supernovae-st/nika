# Crate spec — `nika-providers`

| | |
|---|---|
| Status | **ADMITTED 2026-06-11** (Phase-B slice step 8.5 · before the verbs per D-2026-05-22-N17 · announce ladder per D-2026-06-10-N6) · shipped at §4 **Option B** scope (anthropic + openai-compat + mock = 13/14 wired · gemini profile present · adapter s8.6 before the v0.81.0 tag) |
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

## §2 · Public API (as implemented · admission shape)

```rust
pub struct ProviderRegistry<H = NoHttp> { /* profiles + injected http effect + config */ }
impl<H: HttpPostDyn + Send + Sync + 'static> ProviderRegistry<H> {
    pub fn new(http: Arc<H>, config: ProvidersConfig) -> Self;
    pub fn profiles(&self) -> &[Profile];
    /// `model: provider/name` (pillar ⑤) → profile + nickname→wire-model +
    /// key + endpoint, fail-fast (unknown provider · missing key · no http).
    pub fn resolve(&self, model: &str) -> Result<ResolvedProvider<H>, ProviderError>;
}
impl ProviderRegistry<NoHttp> {
    /// Mock-only registry (doc examples · zero-network tests).
    pub fn without_http(config: ProvidersConfig) -> Self;
}

/// One resolved provider — fully OWNED (no registry borrow · streams are
/// 'static as the kernel contract requires). Implements ProviderInferDyn +
/// ProviderStreamDyn + ProviderMeta (sealed super-trait opt-in · workspace crate).
pub struct ResolvedProvider<H = NoHttp> { /* profile + wire_model + base_url + key + http */ }

pub struct ProvidersConfig {       // builder · operator-owned
    pub fn with_base_url(self, provider, url) -> Self;   // local/openrouter escape hatch
    pub fn with_key(self, provider, key: Secret) -> Self; // the ONLY key path
}
```

Key sovereignty (refined at impl · supersedes the env-read sketch): this
crate **never reads process env** (clippy `disallowed-methods` bans
`std::env::var` workspace-wide — the composition root resolves secrets via
the kernel `SecretResolver` or env at the L4 CLI and injects them through
`ProvidersConfig::with_key`). The `NIKA_<ID>_API_KEY` → conventional-var
ladder lives on `Profile::env_candidates()` as **data** — consumed by the
missing-key error message (prints the exact `export` line · first-error UX ·
B7.2) and later by `nika doctor`. Keys are kernel `Secret` (zeroize-on-drop ·
redacted `Debug` · never serialized). No key needed for `mock` + the 5 local
providers.

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

## §4 · Adapter scope — resolved at Option B (recommendation executed)

The ladder note said « ship `anthropic` first for the Phase B demo ». Three
calibrations were tabled; **B shipped** (autonomous-arc execution of the
standing recommendation · 2026-06-11):

| Option | Scope at s8.5 admission | Coverage | Outcome |
|---|---|---|---|
| A | `anthropic` + `mock` | 2/14 | not taken |
| **B ✅ SHIPPED** | `anthropic` + `openai-compat` + `mock` | **13/14** | covers ALL local providers → `infer` works offline day-1 · gemini = fast-follow s8.6 before tag |
| C | all three wires | 14/14 | not taken (gemini quirks eat review time) |

The openai-compat adapter is the highest-leverage file in the slice (12
profiles · the local-sovereignty story at announce) · gemini follows as a
small PR before the v0.81.0 tag (~07-28) — its profile row + honest
`s8.6` error are already in place. The announce claim « 14 providers »
is honest at tag time.

## §4bis · 12-gate admission table (2026-06-11)

| Gate | Verdict | Evidence |
|---|---|---|
| 1 SPEC | ✅ | this file (design 2026-06-10 · pre-dated the impl) |
| 2 TDD | ✅ | tests-first per module (profile seeding · registry resolve · wire fixtures · SSE proptest) · RED observed on the Pin-projection + fixture iterations |
| 3 IMPL | ✅ | 3245 LOC src incl. in-file tests · max file 711 (caps ≤15k/≤1500 GREEN · live · `scripts/crate-metrics.sh nika-providers`) · 65 lib tests |
| 4 CLIPPY | ✅ | `--all-targets -D warnings` = 0 |
| 5 MUTATION | ✅ | **100%** (139/139 viable caught · 0 missed · 1 timeout non-missed · 58 unviable) |
| 6 PROPERTY | ✅ | SSE parser = sensitive parser → proptest chunking-invariance + linear-scan cursor test |
| 7 BENCH | N/A | network-bound service crate · no algorithmic hot path (http precedent) |
| 8 DOCS | ✅ | `RUSTDOCFLAGS=-D warnings cargo doc --no-deps` 0 |
| 9 CANARY | N/A | L1.5 service · no `.nika.yaml` surface until L2 verbs (clock/fs/http precedent) |
| 10 PARITY | ✅ | cross-provider parity matrix (same assertions × every wired profile · the house rule executable) · brouillon rig-construction intentionally NOT carried (CRAFT · §1) · 14-profile set = canon.yaml projection |
| 11 REVIEW | ✅ | 3-agent swarm 2026-06-11 · 0 P0 · P1s fixed same-session (stream non-2xx typed via `stream_status_error` · SSE quadratic rescan → linear cursor · clippy Gate-4 casts via `Duration::try_from_secs_f64` · layers metadata · spec §2 drift rewritten) · P2s fixed (in-band error transient mapping + terminal contract · extras first-write-wins · stream_options cloud-gated · post-[DONE] guard · catalog-join drift guard · empty-model fail-fast) |
| 12 ATOMIC | ✅ | 1 commit · Nika 🦋 trailer |

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
- Depends on: `nika-kernel` (the facade — `ai::provider` traits/DTOs ·
  `http` traits · `secret::Secret` · the L1 convention, exec-runner
  precedent) · `nika-catalog` (`providers` feature · profile rows) ·
  dev-only: tokio + proptest (nothing network-bound).
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
