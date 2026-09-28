# Crate spec — `nika-providers`

| | |
|---|---|
| Status | **ADMITTED 2026-06-11** (Phase-B slice step 8.5 · before the verbs per D-2026-05-22-N17 · announce ladder per D-2026-06-10-N6) · shipped at §4 **Option B** scope · **gemini wired s8.6 (2026-06-11) → 14/14** |
| Layer | L1.5 — service crate · the shared LLM-provider layer BOTH `nika-verb-infer` (s9) and `nika-verb-agent` depend on (no verb→verb sideways dep · D-N17) |
| Design | impls of the EXISTING L0.5 `nika_kernel_ai::provider` ISP traits (`ProviderInferDyn` · `ProviderStreamDyn` · `ProviderMeta`) · transport via the L0.5 `nika_kernel::http` traits (injected effect · NOT its own `reqwest`) |
| LOC budget | under the ≤1500/file + ≤15k/crate caps (vectors 12+24) · live count · `scripts/crate-metrics.sh nika-providers` |
| Crate version | tracks workspace |
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

Connection interruptions preserve the typed `ProviderError::Connection` across
all HTTP wires, including failures after a stream opens. It uses the existing
NIKA-339 provider transport/other range and remains `NIKA-INFER-001` at the infer
verb. Both error-trait and inherent `is_transient()` agree. The runtime may retry
only under the authored attempt/backoff/timeout policy; the provider layer never
replays independently. The timeout adapter retains its existing API-408 terminal
policy. A missing response does not prove that no tokens were generated or billed.
Streaming yields the error once and does not synthesize a successful `Done`.
For an agent, a connection failure after any tool completed suppresses automatic
whole-task replay, including an `on_codes` override. The transport remains
transient; evidence of prior effects vetoes replay. This conservative guard does
not claim per-tool idempotency or solve the wider effect-classifier work (#1470).

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
| **B ✅ SHIPPED** | `anthropic` + `openai-compat` + `mock` | **13/14** | covers ALL local providers → `infer` works offline day-1 · gemini = fast-follow s8.6 (✅ wired 2026-06-11 → **14/14**) |
| C | all three wires | 14/14 | not taken (gemini quirks eat review time) |

The openai-compat adapter is the highest-leverage file in the slice (12
profiles · the local-sovereignty story at announce) · gemini follows as a
small PR before the 1.0.0 tag (~07-28) — its profile row + honest
`s8.6` error are already in place. The announce claim « 14 providers »
is honest at tag time.

## §4bis · 12-gate admission table (2026-06-11)

| Gate | Verdict | Evidence |
|---|---|---|
| 1 SPEC | ✅ | this file (design 2026-06-10 · pre-dated the impl) |
| 2 TDD | ✅ | tests-first per module (profile seeding · registry resolve · wire fixtures · SSE proptest) · RED observed on the Pin-projection + fixture iterations |
| 3 IMPL | ✅ | 4088 LOC src incl. in-file tests · max file 850 (caps ≤15k/≤1500 GREEN · live · `scripts/crate-metrics.sh nika-providers`) · 76 lib tests (gemini s8.6 wired 2026-06-11) |
| 4 CLIPPY | ✅ | `--all-targets -D warnings` = 0 |
| 5 MUTATION | ✅ | **100%** (139/139 viable caught · 0 missed · 1 timeout non-missed · 58 unviable) |
| 6 PROPERTY | ✅ | SSE parser = sensitive parser → proptest chunking-invariance + linear-scan cursor test |
| 7 BENCH | N/A | network-bound service crate · no algorithmic hot path (http precedent) |
| 8 DOCS | ✅ | `RUSTDOCFLAGS=-D warnings cargo doc --no-deps` 0 |
| 9 CANARY | N/A | L1.5 service · no `.nika` surface until L2 verbs (clock/fs/http precedent) |
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
  `tokio` (the workspace pin, since 2026-09-28, for `task_local` only: the
  dispatch journal) · dev-only: proptest (nothing network-bound).
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

## Sanitized HTTP failures

The shared buffered and stream-open non-2xx boundary returns the additive,
in-process `ProviderError::HttpResponse` variant. Its `ProviderHttpError`
metadata retains the status, recognized provider `error.code`/`error.type`,
and a bounded Retry-After (numeric seconds or preferred HTTP-date form).
Response messages, raw bodies, request IDs, unknown identifiers, and invalid
headers are omitted. The identifier vocabulary is deliberately closed: even
an identifier-shaped value can contain a credential. New provider codes need
an explicit vocabulary update before they become visible diagnostics.

`insufficient_quota` or `credit_balance_exhausted` makes the failure terminal,
even when the provider includes Retry-After. Other 429 responses remain
transient, as do 5xx responses. This uses the existing `is_transient` retry
seam and NIKA code ranges: exhausted quota maps to the existing API-error
code, transient 429 to the existing rate-limit code. Authentication failures
retain their operator guidance. Legacy error variants remain constructible.

HTTP-date Retry-After is preserved without consulting a clock; only numeric
seconds produce `retry_after_ms`. A malformed or unknown provider body cannot
prove quota exhaustion: its status classification remains the fallback.
Gemini and Anthropic in-band errors use this same sanitization boundary.

An error response supplies no verified token usage or billing evidence. The
diagnostic therefore says usage and billing are unknown; it does not create
an inference response or claim zero spend. This change does not alter strict
Compile response schemas or serialized receipt fields. It does not add an
OpenAI-compatible in-band error protocol, which is separate from HTTP rejection.
Tests inject the kernel HTTP effect; no provider credentials or network calls
are required.

## Catalog-backed inference admission

`InferenceAdmission` is a cloneable, mutex-protected account of nano-USD
reservations and catalog estimates, separate from invoices and Run consent.
`ProviderRegistry::with_inference_admission` preserves unbounded defaults and
threads the same account into resolved providers. Exact endpoint/model binding,
text-only serialized body ≤1 MiB, explicit positive output bounds and a kernel
HTTP single-attempt capability are required before dispatch. Streaming refuses.
The bounded path disables registry retry and redirects; the HTTP effect must
disable protocol retries. Complete validated usage settles once, releasing only
the unused reservation. Dropped sent futures, errors, missing/partial usage and
contradictions retain exposure and freeze the account. Over-bound observations
remain in receipts. `billed` is unknown; catalog math never becomes an invoice.
Amending the total retains spend; defaults never infer an unpriced call is free.

`InferenceAdmission::unbudgeted()` observes qualified calls that an operator
started without any monetary ceiling. It has no allowance: each full-context
reservation is recorded as exposure and never compared with a limit, so it
neither admits against a cap nor invents one. Qualification, the bounded
single-attempt transport, settlement and the contradiction and uncertainty
rules above are unchanged; an unsettled or contradicted attempt freezes that
account, and `amend` refuses, so an observation never becomes an allowance.
Its receipt reads `unbudgeted`; its observation carries `"unbudgeted": true`
and a null limit. Every other receipt and observation keeps its exact shape.

Bounded nonstreaming response parsing refuses duplicate decoded object keys before
usage validation or model binding, including equal duplicates and nested/escaped
keys. Such ambiguity retains the sent reservation as unknown charge. Unbounded
JSON parsing remains compatible with its existing last-value behavior.

## Host cost review contract

`admission::{CostRoute, CapEvidence, CostHostEvidence, CostReview,
monetary_default, native_catalog_price_known}` owns the shared pending review
beside `UnknownCostChoice` and its account. Hosts supply observed configuration
evidence and obtain explicit confirmation; Providers does not read the host's
files, environment or terminal. Missing evidence remains Unknown. The existing
Runtime `cost_choice` path is a narrow compatibility export. Moving ownership
does not change any finite bound, default override, hard-cap refusal, exact route
or candidate binding, observation format, or the separate subscription plane.

`admission::unknown_cost_route(model, config)` (C4 · 2026-09-28) is the one
predicate of a Run's monetary class for one API route: `Ok(None)` when a
qualified admission tariff or a native catalog price admits it, `Ok(Some(route))`
when its USD cost is unknown (only a fresh choice may admit it), `Err` when
nothing can judge it. The host's static review and a route rendered at run time
use it alike, so they cannot drift. `InferenceAdmission::observe_run()` is
`observe_declared_free()` for a Run whose `model:` may be rendered at run time:
`ProviderRegistry::resolve` judges the rendered model's exact route, observes a
declared-free one as before, and refuses before any byte an API route that
predicate does not admit, since no fresh choice covers a run-time route. The
refusal is recorded on the receipt and the account stays Open (nothing was
sent). Local, mock, native catalog-priced and positive-tariff routes keep their
host policy and transport; `observe_declared_free()` keeps its pass-through for
library hosts. When a bounded attempt's settlement refuses complete usage
(output over the requested bound, input over the tariff context, identity or
incompleteness), its per-dispatch `InferenceCall` drops `estimated_usd` and keeps
the usage evidence: no frame prices a charge the account holds unknown (E13 F2).

`CostHostEvidence::unknown_cost_refusal()` (B12 · 2026-09-28) returns the
refusal the evidence itself gives every unknown-cost choice (a hard cap, a
denied or unknown layer), in the words `CostReview::new` gives, or `None`. A
host can therefore teach its own cap's remedy beside that refusal only, never
beside an unrelated shape, lease or witness refusal.

`CostChallenge::display` is the first screen of a fresh Run decision: the
provider/model and the endpoint's origin, the review's own unknown-USD, request,
output-token, time, default and hard-cap sentences, the native catalog line
(never an invoice, never a converted price) and `yes / no / details`.
`CostChallenge::details` projects the same challenge whole: nonce, full
endpoint, source and input digests, candidate, invocation, native price and the
host/cap evidence record. Both read `&self`; neither changes the challenge, its
nonce or any authority.

`CostReview::for_session` applies the Session preparation bounds: at most seven
requests, 32768 output tokens and 180 seconds per request. The displayed review
and consuming admission use these same values. `CostReview::new` and
`with_run_requests` retain the Run per-request limits (8192 tokens, 120 seconds);
unknown outcomes freeze their account and never grant a transport retry (for a
Run that authored retries, a received 429 or 503 answers its attempt instead:
see Dispatch multiplicity).

### Dispatch multiplicity (B12 · 2026-09-28)

A confirmed unknown-cost choice bounds two independent quantities: the original
total of physical requests (`max_requests`) and the requests in flight at once.
`UnknownCostChoice::with_max_in_flight(n)` and `CostReview::with_concurrency(n)`
widen the second past its historical one (`0 < n <= total`, otherwise refused).
Each reservation takes one of both under the account mutex; the total counts
every reservation, sent or not, and is never recomputed from a remaining
snapshot. A reservation's in-flight slot is released exactly once: by complete
settlement, by an answered status, or by its drop. A sequential choice without
authored retries serializes byte for byte as before; a widened one adds
`max_in_flight` to its observation, and one whose Run authored retries adds
`"authored_retry": true`.

`UnknownCostChoice::with_authored_retry()` (`CostReview::with_authored_retry`)
records that the Run authored retries inside its total. The authorization law
is explicit: only a typed authored `retry.max_attempts` above one sets it (the
host's `DispatchBound::authored_retry`). Fan cardinality, the total and schema
re-asks never set it, and a schema re-ask, being an extra call inside one
attempt, is never a transport resend. The flag is part of the confirmed choice
and its observation. The human approves it through the question (the retry
line), which the CLI challenge and the Serve witness bind. Only for such a
choice does a 429 or 503 received from the unchanged reserved endpoint answer
its attempt. Its usage and USD cost stay unknown: it counts in `unknown_calls`
and is never marked as not billed. The attempt count and the account state are
kept; in particular, an answer never lifts an Uncertain left by a sibling. An
authored retry inside the original total may then reserve again, and only while
the account is Open. Without authored retries the historical law holds and the
attempt leaves the account Uncertain. Session reviews, and Run reviews with
single attempts (every legacy V1 review), keep this conservative law explicitly. Any other status, a changed endpoint, an ambiguous transport
outcome, a cancelled or timed-out send and an identity contradiction always
leave the account Uncertain. After that, reserved siblings cannot send, new
reservations are refused whatever slots are free, and responses already in
flight are still recorded.

`CostReview::with_breakdown(lines)` adds the host's per-task lines to the
question, followed by the in-flight bound and, for authored retries, the retry
rule. A review with neither a breakdown, concurrency nor authored retries keeps
its historical question bytes.

`ExecutionAccessPlan::admits_api_lane(provider)` (C6, descended from Serve's
cost-review door) answers whether an admitted lane of that canonical provider,
named by the lane's model prefix, runs on the API access class: the lane whose
key the executing process itself reads. A host keeps its own words for that
credential custody (Serve says `HOST_SERVER_MEMORY`).

## Opt-in local model listing

`probe::probe_model_listing` sends one bodyless GET through the kernel HTTP seam.
`ModelListing` distinguishes unobserved transport, incompatible HTTP/JSON, an empty
compatible list and advertised models. Duplicate fields, malformed rows, redirects,
changed endpoints and bounded-size/cardinality violations never establish availability.
The host supplies the no-retry transport and opts in. `ProviderReadiness.model_listing`
is additive; the existing constructor initializes it absent. `model_available` means
an advertised model exists, not that a selected model supports inference or authoring.

OpenAI-compatible usage requires both prompt and completion counts to parse as unsigned integers. Missing, partial, negative, fractional or string-valued pairs remain unreported at the single-response door and emit no Usage frame at the stream door. Explicit integer zero remains an observed zero. This base-token observation is separate from `usage_completeness`, whose existing tariff-meter validation remains unchanged.

`authoring::redact_authoring_error` projects provider failures without remote text. Local AdmissionDenied keeps its type and only an engine-authored remedy. The Host compatibility re-export shares this exact projection with CLI, Session and Serve; it adds no call, retry or monetary authority.

## Dispatch journal (B7 · 2026-09-28)

`dispatch_journal::DispatchJournal::observe(dispatch, lost)` runs one dispatch
with its physical requests recorded where the dispatch's own future cannot
take them down: a `fail_fast` sibling or a `timeout:` drops the future, not the
request it sent. The registry opens an entry before each wire call (pre-send),
the wire marks it sent at its send point (nothing awaits before its post), and
the registry settles it with the returned `InferenceCall` or withdraws it when
the wire refused first. A dispatch that returns calls nothing; one dropped first
hands `lost` every sent or returned request, once. A pre-send entry is never
reported as sent. Outside `observe`, nothing is recorded, so unscoped callers
are unchanged. The runtime scopes each attempt, and a nested workflow's
attempts keep their own scope. The scope is a `tokio::task_local`, the only
production use of this crate's tokio edge.

## Model-spend computation (descended 2026-09-28)

`spend::{spend_for_calls, price_failed_spend, spend_for_model}` turn the
per-dispatch `InferenceCall` evidence this crate produces into a known USD
subtotal and the honest-absence reason for the rest. They descended verbatim
from `nika-runtime`'s `dispatch/spend.rs` at the runtime's 15k wall, beside the
tariffs and settlement refusals they read. Their bodies descended unchanged;
the per-call reason was split out afterwards (below). The
runtime's dispatch seam calls them, and it keeps `failed_usage_split`, which
builds the runtime's own usage receipt.

The reason a call carries no known estimate (E17-F4, B7) is read from its own
evidence, never from a fresh catalog lookup:
- missing or partial meters: `provider_did_not_report_usage`;
- complete meters while the pricing provenance recorded at dispatch names a
  USD tariff (catalog or operator-declared): `usage_rejected`. The settlement
  refused the usage (over the admitted output bound or context, another
  response model, contradictory meters), and the wire cleared the estimate;
- otherwise, no USD price for the route (`unknown` provenance, another
  currency, no safe route): `missing_catalog_price`.

A snapshot-priced route whose usage is rejected before pricing records
`unknown` provenance and still reads as `missing_catalog_price`. That label
lives in `retry/billing.rs`, which this change leaves untouched.
