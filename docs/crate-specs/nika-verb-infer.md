# Crate spec — `nika-verb-infer`

| | |
|---|---|
| Status | **SPEC** (Gate 1 · authored 2026-06-11 · announce-ladder step s9 per D-2026-06-10-N6 · NIKA_ROADMAP S4) |
| Layer | **L2 — FIRST verb crate** · domain executor for the `infer` verb (one of the 4 verbs locked forever per D-2026-05-22-N18) |
| Design | consumes `nika-providers` (L1.5 · s8.5 · per D-N17 the shared layer so `verb-infer` + `verb-agent` never grow a sideways verb→verb dep) + the L0.5 `nika_kernel_ai` DTOs · zero transport of its own |
| LOC budget | ≤3k src (brouillon reference was ~1k lib.rs + emit + error) · caps ≤1500/file · ≤15k/crate (vectors 12+24) |
| Crate version | tracks workspace (`0.80.0`) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — internal L2 verb crate |
| NIKA codes | **NIKA_430–439** claimed inside the reserved Verb range 430–479 (`nika-error/src/codes.rs:61` · `Category::Verb`) · maps to the spec-level `NIKA-INFER-001/002` rows (`nika-spec spec/05-errors.md`) |

---

## §0 · Architecture — the seam already exists end-to-end (verified 2026-06-11)

Like s8.5, the s9 seam needs **zero new kernel traits**. Verified against `main`:

1. **The provider contract is consumed, not implemented.**
   `nika_kernel_ai::provider` ships `InferRequest`/`InferResponse` (all
   `#[non_exhaustive]` · provider.rs:186/280) + the `ProviderInferDyn`
   atomic trait. `nika-providers::ProviderRegistry::resolve("provider/model")`
   returns a `ResolvedProvider` that already speaks it. The verb crate builds
   the request, calls `.infer()`, shapes the output. It never touches wire
   formats, auth, or HTTP.

2. **The language contract is the spec, not this crate.**
   `nika-spec spec/02-verbs.md §infer` is the SSOT for the task surface:
   required `prompt` · optional `system` / `model` / `temperature` (0–2) /
   `max_tokens` / `schema` (JSON Schema → structured output) / `thinking` /
   `vision`. `${{ }}` CEL resolution happens UPSTREAM (binding layer / future
   L3 engine) — this crate receives **already-resolved** strings. Unknown
   fields are rejected upstream by `nika-schema`.

3. **Structured output is request-side + bounded validation, in-crate.**
   Per spec §infer conformance: « validate schema if present · MAY auto-retry
   validation before emitting NIKA-INFER-002 ». The verb owns the floor
   layers of the structured-output recipe: (a) inject
   `ResponseFormat::JsonSchema` on the request when the profile supports it
   (`ProviderMeta::supports_response_format`) + prompt-side instruction
   fallback when it doesn't · (b) extract raw text · (c) validate against the
   schema · (d) bounded retry (default 2 · configurable) re-sending the
   validation error. LLM-judge coercion + canary scanning stay OUT
   (engine/Shield scope · pre-launch Gate 2 owns cross-provider parity).

4. **Events from exactly one site (INV-024).** The verb emits its
   provider-call lifecycle through `nika-event` from a single `emit` module —
   the brouillon's `ProviderResponded` shape, re-CRAFTed against the Diamond
   `EventKind`.

```text
   future L3 nika-engine ── schedules ──┐
                                        v
   L2  nika-verb-infer   run(InferInput) → InferOutput
         │ resolve(model)                       │ emit (1 site)
         v                                      v
   L1.5 nika-providers  ProviderRegistry → ResolvedProvider::infer()
   L0.5 nika-kernel-ai  InferRequest / InferResponse / ResponseFormat
   L0   nika-event · nika-error · nika-types
```

## §1 · Public API (admission shape)

```rust
/// One-shot LLM inference — the `infer` verb executor.
pub struct InferVerb<H = NoHttp> {
    registry: Arc<ProviderRegistry<H>>,
    defaults: InferDefaults, // default model · retry budget
}

#[non_exhaustive]
pub struct InferInput {
    pub prompt: String,                 // required (spec §infer)
    pub system: Option<String>,
    pub model: Option<String>,          // `provider/name` override
    pub temperature: Option<f64>,       // 0–2 · validated here (NIKA_432)
    pub max_tokens: Option<u32>,
    pub schema: Option<serde_json::Value>, // JSON Schema → structured mode
    pub thinking: Option<ThinkingDirective>,
}

#[non_exhaustive]
pub struct InferOutput {
    pub output: InferValue,             // Text(String) | Structured(Value)
    pub usage: Usage,                   // from InferResponse
    pub model_resolved: String,
    pub response: InferResponse,        // full kernel DTO for the engine
}

impl<H: HttpPostDyn + Send + Sync + 'static> InferVerb<H> {
    pub fn new(registry: Arc<ProviderRegistry<H>>, defaults: InferDefaults) -> Self;
    /// CANCEL SAFETY: cancel-safe at the provider transport (kernel contract).
    pub async fn run(&self, input: InferInput) -> Result<InferOutput, VerbInferError>;
}
```

Per Invariant #19: `new()` constructors on every `#[non_exhaustive]` struct.
Per Invariant #25: `VerbInferError` is `#[non_exhaustive]` from day one.

## §2 · Error model (one-voice · vector 37)

| Code | Variant | Spec mapping | transient |
|---|---|---|---|
| NIKA_430 | `ProviderCall` (wraps `ProviderError`) | NIKA-INFER-001 | inherited from ProviderError |
| NIKA_431 | `SchemaValidation { attempts }` | NIKA-INFER-002 | `false` |
| NIKA_432 | `InvalidParam` (temperature range · empty prompt) | upstream-reject class | `false` |
| NIKA_433 | `ModelResolution` (registry resolve failed) | NIKA-INFER-001 family | `false` |

NIKA_434–439 stay reserved for the verb's future (vision staging · streaming
passthrough). Range registered in the kernel range-registry hub at admission.

## §3 · Scope fences (what this crate is NOT)

- **NOT streaming** — `ProviderStreamDyn` passthrough is the engine/agent
  surface (brouillon kept it in the engine bridge; Diamond will too until
  `verb-agent` s15 needs it). Seam note: the registry already resolves
  stream-capable providers; adding `run_stream` later is additive.
- **NOT vision** — `vision:` staging couples to media (deferred §10bis).
  `InferInput` carries no vision field at v1; the spec field maps when
  `nika-media-*` lands. (Spec allows partial conformance pre-v0.90; the
  conformance fixture for vision is marked pending.)
- **NOT template/CEL resolution** — `${{ }}` is resolved upstream.
- **NOT retry-on-transport** — transport retry/backoff policy belongs to the
  engine scheduler; the verb retries ONLY schema-validation (spec-sanctioned).

## §4 · Testing strategy (Gates 2–7)

- **TDD**: mock-first via `ProviderRegistry::mock_only()` (zero network ·
  deterministic) — RED before GREEN on: prompt→message shaping · system
  placement · param validation bounds · structured happy path · validation
  retry exhaustion → NIKA_431 · resolve failure → NIKA_433.
- **Property** (Gate 6 — applies: parser-adjacent): proptest on temperature
  bounds + schema-retry loop invariants (attempts ≤ budget · terminal states).
- **Mutation** (Gate 5): ≥90 % via `cargo mutants -p nika-verb-infer`.
- **Parity** (Gate 10): golden test vs brouillon `tools/nika-verb-infer`
  request-shaping output (`git show brouillon:tools/nika-verb-infer/src/lib.rs`
  read-only reference) on the mock provider.
- **Canary** (Gate 9): N/A justified — no L3 runner admitted yet; the
  `tests/canary-infer.nika.yaml` lands with `nika-engine` (step 17), same
  exemption class as prior pre-engine crates.
- **Benchmarks** (Gate 7): N/A — network-bound path, no hot loop.

## §5 · First-of-layer wiring pass (the s8.5 lesson)

First L2 crate → verify each gate surface, expected state checked 2026-06-11:

| Surface | State | Action at admission |
|---|---|---|
| `scripts/ci/check-layering.sh` | L2 rank present (`:39` · rank 4) | none |
| `Cargo.toml [workspace.metadata.diamond] layers` | no L2 rows yet | add `layers.nika-verb-infer = "L2"` |
| `deny.toml` bans/wrappers | verb crates pre-cited (`:10-11`) | uncomment/add wrapper row |
| `scripts/refresh-status.sh` | L2 row renders (auto-block `L2 | 0`) | none |
| `scripts/roadmap.sh` | L2 row renders | none |
| hygiene vector 33 layer-deps | derives from Cargo.toml | re-run |

## §6 · Dependencies

```toml
[dependencies]  # all workspace-inherited
nika-kernel-ai = { workspace = true }   # DTOs + ProviderInferDyn
nika-providers = { workspace = true }   # registry resolve (L2→L1.5 ok)
nika-event     = { workspace = true }   # single-site emission
nika-error     = { workspace = true }   # NikaErrorCode one-voice
jsonschema     = { workspace = true }   # schema validation (already vetted? → verify at impl; else schemars/jsonschema audit via cargo-deny)
serde_json, thiserror, tracing, tokio (rt only), async-trait/trait_variant per kernel canon
[dev-dependencies]
nika-kernel-mock · proptest · insta
```

⚠️ `jsonschema` crate choice is the one OPEN dep decision — resolve at impl
start: prefer what `nika-schema` (WIP) already pulled; one validator in the
workspace, not two.

## §7 · Update log

```
2026-06-11  v0.1 — Gate 1 SPEC authored (night arc · s9 lane) ·
              architecture verified against main (kernel-ai DTOs ·
              providers registry · spec 02-verbs.md §infer · 05-errors.md
              NIKA-INFER rows · brouillon read-only reference) ·
              per-verb-crate framing confirmed vs ADR-056 queued proposal
              (see nika-invariants.md status precision 6fbf5b6).
```
