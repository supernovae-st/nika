# Crate spec — `nika-catalog`

| | |
|---|---|
| Status | Phase 1 — Step 2 of `nika-core` split |
| Layer | L0 (PURE, zero I/O, zero async) |
| Design | **Hybrid lookup** — phf+unicase (case-insensitive) + sorted arrays (case-sensitive) |
| LOC budget | ≤5,000 src (target ~3,500, alarm at 5,000) |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Source on `main` (reference) | `tools/nika-core/src/catalogs/` (10 files, ~4,000 LOC) |
| Crate version | tracks workspace (bumped to `0.90.0-alpha.1` at Phase 1 close) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |

---

## 1. Purpose

`nika-catalog` provides **static, compile-time catalogs** for every known
entity in the Nika workflow engine: LLM providers, MCP server aliases,
builtin tools, pipe transforms, model capabilities, and model pricing.

Key design principle: **lookups return `Option`, not `NikaError`**. The catalog
answers "is this known?", the caller decides if "unknown" is an error.

### 3 architectural decisions locked (8-agent research, 2026-04-13)

**A. Case sensitivity** — per-catalog policy:
- Providers + MCP aliases → case-insensitive (phf + unicase, zero-alloc)
- Builtins + transforms + models → case-sensitive (engine-controlled, always lowercase)

**B. Provider catalog = LLM-only** — 16 entries (7 cloud + 7 OpenAI-compat + native + mock).
11 ex-MCP providers migrate to `McpAlias` catalog. `ProviderCategory` enum deleted.

**C. Lookup strategy = hybrid**:
- phf + UniCase for case-insensitive catalogs (providers, MCP aliases)
- Sorted arrays + binary_search for case-sensitive catalogs (builtins, transforms)
- Pattern-matching function for model capabilities (open-ended model names)
- 2-pass matching for pricing (exact then contains)

See `CATALOG_RESEARCH_SYNTHESIS.md` for full rationale.

---

## 2. Public API surface

```rust
// ── Types ─────────────────────────────────────────────────────
pub struct Provider { pub id, name, aliases, env_var, key_prefix, default_model, cheap_model, requires_key, description }
pub struct McpAlias { pub name, package, category, pricing, env_var, key_prefix }
pub enum McpPricing { Free, Freemium, Paid }
pub struct Builtin { pub name: &'static str, pub category: BuiltinCategory }
pub enum BuiltinCategory { Core, File, Data, DataSprint2, Introspection, CostRecords, Agent, MediaAlwaysOn, MediaCore, MediaOptIn }
pub struct TransformDef { pub name, arity, null_behavior, category }
pub enum TransformArity { Nullary, Unary, Variadic }
pub enum NullBehavior { Propagate, Fail }
pub enum TransformCategory { String, Array, Aggregation, Numeric, Type, Logic, Introspection, Parametric, Query, StringTest, Url, Encoding, Jq, System }
pub struct ModelPricing { pub provider, model_pattern, input_per_million, output_per_million }
pub struct CostEstimate { pub usd, input_rate_per_million, output_rate_per_million, model, provider }
pub enum TokenLimitParam { MaxTokens, MaxCompletionTokens }
pub struct ModelCapabilities { pub token_limit_param, supports_temperature, supports_stop_sequences, supports_thinking, supports_vision }

// ── Lookup (returns Option, NOT NikaError) ────────────────────
pub fn find_provider(name: &str) -> Option<&'static Provider>;         // phf+unicase O(1)
pub fn find_mcp_alias(name: &str) -> Option<&'static McpAlias>;       // phf+unicase O(1)
pub fn find_builtin(name: &str) -> Option<&'static Builtin>;          // binary_search O(log n)
pub fn is_known_builtin(name: &str) -> bool;                          // binary_search O(log n)
pub fn find_transform(name: &str) -> Option<&'static TransformDef>;   // binary_search O(log n)
pub fn is_known_transform(name: &str) -> bool;                        // binary_search O(log n)
pub fn model_capabilities(provider: &str, model: &str) -> ModelCapabilities;  // pattern match
pub fn find_pricing(model: &str) -> Option<&'static ModelPricing>;    // 2-pass (exact+contains)
pub fn estimate_cost(model: &str, input_tokens: u64, output_tokens: u64) -> Option<CostEstimate>;

// ── Iteration ─────────────────────────────────────────────────
pub fn all_providers() -> &'static [Provider];
pub fn all_mcp_aliases() -> &'static [McpAlias];
pub fn all_builtins() -> &'static [Builtin];
pub fn all_transforms() -> &'static [TransformDef];
pub fn all_pricing() -> &'static [ModelPricing];

// ── Validation ────────────────────────────────────────────────
pub fn validate_key_format(provider: &Provider, key: &str) -> bool;
pub fn validate_catalog_integrity() -> Vec<CatalogError>;

// ── Error ─────────────────────────────────────────────────────
pub enum CatalogError { ... }
impl NikaErrorCode for CatalogError;
```

---

## 3. Catalog entry counts

| Catalog | Entries | Lookup | Case |
|---|---|---|---|
| Providers | 16 (7 cloud + 7 compat + native + mock) | phf+unicase | insensitive |
| Provider aliases | ~20 (claude→anthropic, gpt→openai, etc.) | merged into provider map | insensitive |
| MCP aliases | ~125 (114 existing + 11 ex-MCP providers) | phf+unicase | insensitive |
| Builtins | 63 | sorted + binary_search | sensitive |
| Transforms | 63 | sorted + binary_search | sensitive |
| Pricing | 62 patterns | sorted + 2-pass match | N/A |

---

## 4. File layout

```
tools/nika-catalog/
  Cargo.toml
  src/
    lib.rs              (~50 LOC — re-exports)
    error.rs            (~60 LOC — CatalogError impl NikaErrorCode)
    types/
      mod.rs            (~20 LOC — re-exports)
      provider.rs       (~80 LOC — Provider struct)
      model.rs          (~100 LOC — ModelCapabilities, ModelPricing, TokenLimitParam, CostEstimate)
      builtin.rs        (~60 LOC — Builtin, BuiltinCategory)
      mcp_alias.rs      (~60 LOC — McpAlias, McpPricing)
      transform.rs      (~80 LOC — TransformDef, TransformArity, NullBehavior, TransformCategory)
    data/
      mod.rs            (~20 LOC — re-exports)
      providers.rs      (~300 LOC — 16 entries + aliases, phf+unicase)
      models.rs         (~600 LOC — capabilities fn + 62 pricing, sorted array)
      builtins.rs       (~200 LOC — 63 entries, sorted array)
      mcp_aliases.rs    (~1,200 LOC — ~125 entries, phf+unicase)
      transforms.rs     (~400 LOC — 63 entries, sorted array)
    lookup.rs           (~200 LOC — public find_* API)
    validate.rs         (~100 LOC — cross-ref validation)
```

Target: ~3,500 LOC src, ~800 LOC tests = ~4,300 LOC total.

---

## 5. Dependencies

```toml
[dependencies]
nika-error = { path = "../nika-error" }
phf        = { workspace = true }
unicase    = { workspace = true }
serde      = { workspace = true, optional = true }

[features]
default = ["serde"]
serde = ["dep:serde"]

[dev-dependencies]
insta      = { workspace = true }
proptest   = { workspace = true }
rstest     = { workspace = true }
serde_json = { workspace = true }
```

---

## 6. Test plan

| Test location | Scope | Count |
|---|---|---|
| `src/data/providers.rs` #[cfg(test)] | count, find by id, find by alias, case-insensitive | ~8 |
| `src/data/builtins.rs` #[cfg(test)] | count, sorted order, find, not-found | ~5 |
| `src/data/mcp_aliases.rs` #[cfg(test)] | count, find, ex-MCP providers present | ~5 |
| `src/data/transforms.rs` #[cfg(test)] | count, sorted order, find | ~5 |
| `src/data/models.rs` #[cfg(test)] | capabilities by provider+model, pricing 2-pass | ~10 |
| `src/lookup.rs` #[cfg(test)] | public API integration, unknown→None | ~8 |
| `src/validate.rs` #[cfg(test)] | cross-ref integrity | ~3 |
| proptest | arbitrary names→None, case variants→same result | ~4 |
| **Total** | | **~48** |

All unit tests inline (`#[cfg(test)] mod tests`), run with `cargo test --lib`.

---

## 7. Gate exemptions

- **Gate 7 (Benchmarks)**: Exempt. L0 static data, phf is O(1), sorted arrays are O(log n).
  No hot path in startup — lookups are called during workflow analysis, not in inner loop.
- **Gate 9 (Canary E2E)**: Exempt. No runtime yet.

---

## 8. Actual metrics (post-admission)

| Metric | Value |
|---|---|
| Total LOC | 2,235 |
| Tests | 85 |
| Mutation score | 94.7% (71/75 viable) |
| Clippy warnings | 0 |
| Doc warnings | 0 |
| Largest file | models.rs (434 LOC) |
| Commit | `55a451695` |

## 9. Audit trail

| Date | Author | Change |
|---|---|---|
| 2026-04-13 | Phase 1 S2 | Initial spec reflecting 3 locked decisions from 8-agent research. |
| 2026-04-13 | Phase 1 S2 | Admitted to workspace. 3-agent review: all P0/P1 fixed. |

🦋
