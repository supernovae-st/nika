# Crate spec — `nika-catalog`

| | |
|---|---|
| Status | **Admitted** — Phase D Sessions 2a+2b+3 hardened (see `CHANGELOG.md` for live stats) |
| Layer | L0 (PURE, zero I/O, zero async) |
| Design | **Hybrid lookup** — phf+unicase (case-insensitive) + sorted arrays (case-sensitive) + TOML-driven rule table (capabilities) |
| LOC budget | ≤15,000 src (current: ~6,100) |
| File cap | ≤1,500 LOC each (current max: `build.rs` 1,264) |
| Function cap | ≤100 lines each |
| Crate version | tracks workspace (`0.80.0`, forever-v0.x) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |

---

## 1. Purpose

`nika-catalog` provides **static, compile-time catalogs** for every known
entity the Nika workflow engine talks about: LLM providers (with per-model
token limits + api_dialect), MCP servers (multi-distribution: npm / pypi /
oci / cargo / mcpb / remote), builtin tools, pipe transforms, embedding
models, model capabilities (TOML-driven rule table), and model pricing.

Key design principles:

- **Lookups return `Option`, not `NikaError`**. The catalog answers "is
  this known?", the caller decides if "unknown" is an error.
- **Zero runtime I/O**. Every catalog is materialised at build time from
  `data/*.toml` into `&'static` slices + `phf::Map` indexes. Callers walk
  static memory, never touch the filesystem or the network.
- **Every public type is `#[non_exhaustive]`** with an explicit `pub fn
  new()` constructor (invariant #19), so adding fields in future sessions
  is not a breaking change for community-extension crates.

### Architectural decisions (locked)

**A. Case sensitivity** — per-catalog policy:

- Providers + MCP aliases → case-insensitive (phf + unicase, zero-alloc)
- Builtins + transforms → case-sensitive (engine-controlled, always
  lowercase)
- Model capabilities → case-insensitive ASCII on both provider and model
  strings (`eq_ignore_ascii_case`)

**B. Data source = TOML**, emitted to Rust at build time. Fails the build
on schema mismatch, FK violation, duplicate id, or unknown enum variant.
`@1.0` schemas: `nika/mcp-servers`, `nika/llm-providers`, `nika/embeddings`,
`nika/model-capabilities`.

**C. Lookup strategy = hybrid**:

- `phf::Map<UniCase<&'static str>, usize>` for providers, MCP servers,
  embeddings — O(1) case-insensitive lookup.
- Sorted `&[T]` + `binary_search` for builtins + transforms — O(log n)
  case-sensitive lookup.
- `&[Rule]` + first-match-wins merge for model capabilities — a
  TOML-driven rule table with 4 matcher kinds (`any | exact | exact_any |
  prefix_any`). Zero heap alloc, `<80 ns` per call on cache-hot path.
- 2-pass matching (exact then `contains()`) for pricing patterns, scoped
  per provider to prevent cross-provider collisions.

---

## 2. Public API surface

```rust
// ── Types (all #[non_exhaustive] with new() constructor) ───────────────

pub struct Provider {
    pub id, name, aliases, env_var, key_prefixes,
    pub default_model, cheap_model, requires_key, description,
    pub models: &'static [ProviderModel],
    pub api_dialect: Option<&'static str>,   // Session 2a
    pub tags: &'static [Tag],
    pub extra_tags: &'static [&'static str],
}
pub struct ProviderModel { pub id, model, context_window_tokens, max_output_tokens }

pub struct McpServer {
    pub id, aliases, title, description,
    pub packages: &'static [McpPackage],     // multi-distribution
    pub remotes: &'static [McpRemote],
    pub env_vars: &'static [EnvVarSpec],
    pub homepage: Option<&'static str>,
    pub category: Category, pricing: McpPricing, last_verified,
    pub tags: &'static [Tag], extra_tags: &'static [&'static str],
}
pub struct McpPackage { pub registry_type, identifier, version, transport, runner }
pub struct McpRemote  { pub transport, url, auth }
pub struct EnvVarSpec { pub name, key_prefixes, required, is_secret, description }
pub enum   McpPricing    { Free, Freemium, Paid }
pub enum   RegistryType  { Npm, Pypi, Oci, Cargo, Mcpb }
pub enum   Transport     { Stdio, StreamableHttp, Sse }
pub enum   AuthMode      { None, Bearer, OAuth }
pub enum   PyRunner      { Uvx, Pipx }
pub enum   Category      { /* 18 variants */ }

pub struct Embedding {
    pub id, provider, model,
    pub dimensions, max_input_tokens, normalized_by_default,
    pub similarity: Similarity,
    pub input_per_million: f64,
    pub description,
    pub tags, extra_tags,
}
pub enum Similarity { Cosine, DotProduct, L2 }

pub struct Builtin      { pub name, category: BuiltinCategory }
pub enum  BuiltinCategory { Core, File, Data, DataSprint2, Introspection,
                            CostRecords, Agent, MediaAlwaysOn, MediaCore, MediaOptIn }

pub struct TransformDef    { pub name, arity, null_behavior, category }
pub enum  TransformArity   { Nullary, Unary, Variadic }
pub enum  NullBehavior     { Propagate, Fail }
pub enum  TransformCategory {
    String, Array, Aggregation, Numeric, Type, Logic, Introspection,
    Parametric, Query, StringTest, Url, Encoding, Jq, System, Escape,
}

pub struct ModelPricing { pub provider, model_pattern, input_per_million, output_per_million }
pub struct CostEstimate { pub usd, input_rate_per_million, output_rate_per_million, model, provider }
pub enum TokenLimitParam { MaxTokens, MaxCompletionTokens, MaxOutputTokens }
pub struct ModelCapabilities {
    pub token_limit_param,
    pub supports_temperature,
    pub supports_stop_sequences,
    pub reasoning,              // renamed from supports_thinking in Session 2a
    pub supports_vision,
}

pub enum Tag { /* 42 variants: capabilities, economics, deployment, domain, MCP permissioning */ }
pub struct ParseTagError      { pub input: String }
pub struct ParseCategoryError { pub input: String }

// ── Lookup (returns Option, never NikaError) ───────────────────────────
pub fn find_provider(name: &str) -> Option<&'static Provider>;   // phf+unicase O(1)
pub fn find_mcp_server(name: &str) -> Option<&'static McpServer>;// phf+unicase O(1)
pub fn resolve_mcp_name(name: &str) -> Option<String>;           // alias → npm pkg
pub fn is_known_mcp_server(name: &str) -> bool;
pub fn find_embedding(id: &str) -> Option<&'static Embedding>;   // phf+unicase O(1)
pub fn find_builtin(name: &str) -> Option<&'static Builtin>;     // binary_search O(log n)
pub fn is_known_builtin(name: &str) -> bool;
pub fn find_transform(name: &str) -> Option<&'static TransformDef>;
pub fn is_known_transform(name: &str) -> bool;

pub fn model_capabilities(provider: &str, model: &str) -> ModelCapabilities;
pub fn find_pricing(model: &str) -> Option<&'static ModelPricing>;                // 2-pass
pub fn find_pricing_scoped(provider: &str, model: &str) -> Option<&'static ModelPricing>;
pub fn estimate_cost(model: &str, input_tokens: u64, output_tokens: u64) -> Option<CostEstimate>;

// ── Iteration ──────────────────────────────────────────────────────────
pub fn all_providers()   -> &'static [Provider];    // 21
pub fn all_mcp_servers() -> &'static [McpServer];   // 105
pub fn all_embeddings()  -> &'static [Embedding];
pub fn all_builtins()    -> &'static [Builtin];     // 63
pub fn all_transforms()  -> &'static [TransformDef];// 65
pub fn all_pricing()     -> &'static [ModelPricing];// 62

// ── Validation ─────────────────────────────────────────────────────────
pub fn validate_key_format(provider: &Provider, key: &str) -> bool;
pub fn validate_catalog_integrity() -> Vec<CatalogError>; // full-feature only

// ── Fuzzy suggest ──────────────────────────────────────────────────────
pub fn suggest(query: &str) -> Vec<Suggestion>;                   // Jaro-Winkler ≥ 0.7
pub fn suggest_in(query: &str, namespace: Namespace) -> Vec<Suggestion>;
pub struct Suggestion { pub name, namespace, score }
pub enum Namespace { Provider, McpServer, Embedding, Builtin, Transform }

// ── Error ──────────────────────────────────────────────────────────────
pub enum CatalogError { /* #[non_exhaustive] */ }
impl nika_error::NikaErrorCode for CatalogError;
```

---

## 3. Catalog entry counts (grep-verified)

| Catalog | Entries | Lookup | Case |
|---|---|---|---|
| Providers | **21** (7 cloud + 7 openai-compat + 5 enterprise + native + mock) | phf+unicase | insensitive |
| Provider aliases | merged into provider map (`claude→anthropic`, `gpt→openai`, `gcp→vertex`, …) | phf+unicase | insensitive |
| MCP servers | **105** (post Phase A cleanup, multi-distribution) | phf+unicase | insensitive |
| Embeddings | **13** (voyage, OpenAI, Cohere, Gemini families) | phf+unicase | insensitive |
| Builtins | **63** (across 10 categories) | sorted + binary_search | sensitive |
| Transforms | **65** (across 15 categories incl. Escape) | sorted + binary_search | sensitive |
| Pricing | **62** patterns | sorted + 2-pass match | N/A |
| Capability rules | **9** rules + 1 defaults | TOML + first-match-wins | insensitive (ASCII) |
| `Tag` variants | **42** | enum | N/A |

---

## 4. File layout

```
crates/nika-catalog/
├── Cargo.toml
├── COMMUNITY_EXTENSIONS.md       ← extension-author guide
├── build.rs                      (~1,264 LOC — MCP/providers/embeddings parse+emit)
├── build/
│   └── capabilities.rs           (~380 LOC — capabilities TOML codegen, split Session 2a)
├── data/
│   ├── mcp-servers.toml
│   ├── llm-providers.toml        ← 21 providers + api_dialect
│   ├── embeddings.toml
│   └── model-capabilities.toml   ← 9 rules, @1.0 schema
└── src/
    ├── lib.rs                    ← public re-exports, feature-gated
    ├── error.rs                  ← CatalogError impl NikaErrorCode
    ├── lookup.rs                 ← public find_* API
    ├── suggest.rs                ← Jaro-Winkler fuzzy match
    ├── validate.rs               ← cross-ref integrity (full-feature only)
    ├── data/
    │   ├── mod.rs
    │   ├── generated.rs          ← include!($OUT_DIR/{mcp_servers,providers,embeddings,model_capabilities}.rs)
    │   ├── models.rs             (~775 LOC — model_capabilities resolver + pricing catalog + parity harness)
    │   ├── builtins.rs
    │   ├── transforms.rs
    │   └── snapshots/            ← insta .snap files
    └── types/
        ├── mod.rs
        ├── capabilities.rs       ← CapPatch + Matcher + Rule (pub(crate))
        ├── category.rs           ← Category enum + ParseCategoryError
        ├── distribution.rs       ← McpPackage + McpRemote + EnvVarSpec + RegistryType + Transport + AuthMode + PyRunner
        ├── embedding.rs          ← Embedding + Similarity
        ├── mcp_server.rs         ← McpServer + McpPricing
        ├── model.rs              ← ModelCapabilities + TokenLimitParam + ModelPricing + CostEstimate
        ├── provider.rs           ← Provider + ProviderModel + validate_key_format
        ├── tags.rs               ← Tag (42 variants) + ParseTagError
        ├── transform.rs          ← TransformDef + TransformArity + NullBehavior + TransformCategory
        └── builtin.rs            ← Builtin + BuiltinCategory
```

**LOC totals (grep-verified)**:

- `src/**/*.rs` ≈ **4,350 LOC** (lib + error + lookup + suggest + validate + data/ + types/)
- `build.rs` + `build/capabilities.rs` ≈ **1,644 LOC**
- Largest source file: `src/data/models.rs` at **775 LOC** (well under 1,500 budget)

---

## 5. Cargo features

```toml
default = ["full", "serde"]
full    = ["mcp", "providers", "embeddings", "pricing", "capabilities", "builtins-transforms"]
minimal = []                     # types + Tag only, for community-extension crates
extension-author = ["minimal"]   # alias for DX
serde   = ["dep:serde"]

mcp                 = []
providers           = []
embeddings          = ["providers"]          # FK check
pricing             = ["providers"]          # find_pricing_scoped needs providers
capabilities        = ["providers"]          # resolver canonicalises via find_provider
builtins-transforms = []
```

Feature matrix is CI-gated (8 subsets always green).

---

## 6. Dependencies

```toml
[dependencies]
nika-error = { path = "../nika-error" }
phf        = { workspace = true }
unicase    = { workspace = true }
thiserror  = { workspace = true }
miette     = { workspace = true }
serde      = { workspace = true, optional = true }
strsim     = { workspace = true }  # Jaro-Winkler for suggest

[build-dependencies]
phf_codegen = "0.13"
phf_shared  = { version = "0.13", features = ["unicase"] }
toml        = "1.1"
serde       = { workspace = true, features = ["derive"] }
unicase     = { workspace = true }

[dev-dependencies]
insta      = { workspace = true }
proptest   = { workspace = true }
rstest     = { workspace = true }
serde_json = { workspace = true }
```

---

## 7. Test plan

| Test location | Scope | Count |
|---|---|---|
| `src/data/generated.rs` `#[cfg(test)] mod tests` | cross-catalog invariants (provider ↔ embedding FK, dual-role services, tag XOR) | ~22 |
| `src/data/models.rs` `#[cfg(test)]` | capabilities by (provider, model), pricing 2-pass, scoped pricing | ~25 |
| `src/data/models.rs::parity_tests` | **proptest 10 000 cases** + 26 spot-checks + insta snapshot vs extracted pre-Session-2a body (DELETE at v0.90 — see module doc) | ~3 |
| `src/types/*` `#[cfg(test)]` | constructors, Copy/Send/Sync asserts, TOML roundtrip, Tag enum count=42 | ~20 |
| `src/suggest.rs` `#[cfg(test)]` | typo correction, case-insensitivity, namespace filtering | ~7 |
| `src/validate.rs` `#[cfg(test)]` | cross-ref integrity | ~6 |
| `src/lookup.rs` `#[cfg(test)]` | public API integration, unknown → None | ~7 |
| **Total current** | | **385** |

All unit tests inline (`#[cfg(test)] mod tests`), run with `cargo test --workspace --lib`.

---

## 8. Gate status (12 gates per Diamond discipline)

| Gate | Status | Note |
|---|---|---|
| 1. SPEC | ✅ | this document |
| 2. TDD | ✅ | RED→GREEN verified on Session 2a rename + rule table |
| 3. IMPL | ✅ | minimal, compiles clean, no `# TEMP` markers |
| 4. CLIPPY 0 | ✅ | `--workspace --all-targets -D warnings` |
| 5. MUTATION ≥ 90% | ⏳ | deferred to Phase E2 (crate pre-v0.90) |
| 6. PROPERTY | ✅ | proptest 10 000 cases for capabilities parity |
| 7. BENCHMARKS | ⚠️ | **Exempt** — L0 static data, proptest + structural zero-alloc refactor is the regression guard. Criterion may be added in a future session when signal-to-noise matters. |
| 8. DOCS | ✅ | `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` clean |
| 9. CANARY E2E | ⚠️ | **Exempt** — no runtime yet |
| 10. PARITY LEGACY | ✅ | proptest vs pre-Session-2a body at HEAD `1a29bd32f` |
| 11. REVIEW SWARM | ✅ | 3-agent pre-commit + 5-agent post-commit on Session 2a |
| 12. ATOMIC COMMIT | ✅ | Session 2a = 2 commits (feat + hardening), Nika 🦋 co-authored |

---

## 9. Actual metrics (HEAD post-hardening Session 2a)

| Metric | Value |
|---|---|
| Total src LOC | ~4,350 |
| Build-script LOC | 1,264 + 380 = 1,644 |
| Tests | **385** (was 369 pre-Session-2a) |
| Mutation score | TBD (Phase E2) |
| Clippy warnings | 0 |
| Doc warnings | 0 |
| `.unwrap()` / `.expect(` in prod src | 0 (all in tests) |
| `#[allow(dead_code)]` | 0 |
| `#[non_exhaustive]` public types without `new()` | 0 |
| Largest file | `build.rs` 1,264 LOC |
| Feature matrix | 8 subsets, all green |

---

## 10. Forward-compat — Session 2b outlook

Session 2b will extend `ModelCapabilities` with modalities, tokenizer,
and supported parameters — addressing shadow-zone Gate 2 (cross-provider
structured output parity). The `@1.0` schema stays: every new field
is additive via `Option<T>` + `#[serde(default)]`.

Planned additions:

- `input_modalities: &'static [Modality]` + `Modality` enum (`Text`,
  `Image`, `Audio`, `Video`, `Pdf`) in `src/types/modality.rs`
- `output_modalities: &'static [Modality]`
- `tokenizer: Option<TokenizerFamily>` (`Tiktoken*`, `Claude`, `Llama`,
  `Gemini`, `Mistral`) in `src/types/tokenizer.rs`
- `supported_parameters: &'static [ParamFlag]` (~15 variants incl.
  `ParallelToolCalls`) in `src/types/param_flag.rs`

Rules in `data/model-capabilities.toml` will grow from 9 to ~20.
`CapPatch` extends from 5 `Option<T>` fields to ~9. `emit_cap_patch`
already refactored to a one-line-per-field pattern (Session 2a
hardening) so growth is mechanical.

---

## 11. Audit trail

| Date | Author | Change |
|---|---|---|
| 2026-04-13 | Phase 1 S2 | Initial spec — 3 locked decisions from 8-agent research. |
| 2026-04-13 | Phase 1 S2 | Admitted to workspace. 3-agent review: all P0/P1 fixed. |
| 2026-04-14 | Phase D S1 | `Tag` enum + Cargo features + Shield XOR. |
| 2026-04-14 | Phase D S2a | **TOML-driven `model_capabilities`** + rename `supports_thinking`→`reasoning` + `api_dialect` field + `MaxOutputTokens` variant. Tests 369 → 381. Commits `8c5cb4866` + `e766a122c` (hardening). |
| 2026-04-14 | Phase D S2a post-hardening | 5-agent deep audit + 12 `new()` constructors + build.rs split under 1500 LOC + rustdoc Gate 8 green + emit_cap_patch helper refactor. Tests 381 → 385. |

🦋
