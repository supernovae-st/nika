---
id: ADR-008
title: "TOML-driven catalog with build-time codegen and perfect-hash lookup"
status: accepted
date: "2026-04-14"
phase: "Phase D -- Session 2a"
deciders: ["@ThibautMelen"]
tags: ["catalog", "toml", "codegen", "build-time", "phf"]
affects_crates: ["nika-catalog", "nika-catalog-verify"]
affects_layers: ["L0", "L4"]
supersedes: []
superseded_by: []
related: ["ADR-004", "ADR-007"]
requires: ["ADR-004"]
enables: []
amends: []
fci: []
inv: ["INV-019"]
shadow_zones: []
nika_codes: ["NIKA-300"]
timeline: "v0.80.0, Session 2a -- expanded Session 2b/3"
follow_ups: ["Phase E2: full TOML-driven pricing migration"]
---

# ADR-008: TOML-driven catalog with build-time codegen + perfect-hash lookup

## Context

Nika needs to resolve a user-supplied `provider: claude` → canonical provider id → model → capabilities → pricing in a single YAML field. Legacy used hardcoded Rust arrays. That had two problems:

1. **Contribution friction:** adding a provider required writing Rust + rebuilding. Community contributors were blocked.
2. **Silent rot:** the Phase A audit (2026-03) discovered **29 broken MCP aliases** — entries pointing at servers that no longer existed. No one noticed for months because the data lived inside compiled code.

Session 2a (2026-04-14) set the target: move all catalog data to TOML with schema validation at build time, and add a model-capabilities rule table that replaces a 900-line `match` cascade.

## Decision

**All catalog data lives in `data/*.toml`** with versioned schemas:

- `nika/llm-providers@1.0` — 21 providers, each with `canonical_id`, `aliases`, `api_dialect` (closed enum: anthropic | openai-chat | openai-responses | gemini | cohere | ai21 | bedrock | voyage | mock)
- `nika/mcp-servers@1.0` — MCP server aliases + trust labels
- `nika/model-capabilities@1.0` — 9 first-match-wins rules (Session 2a), projected to 28 (Session 2b)
- `nika/builtins@1.0` — builtin tool definitions + safety tags

**`build.rs` does the heavy lifting:**

1. Parse each TOML file against its schema version
2. Enforce invariants: every `scope.providers[i]` in `model-capabilities.toml` must be a canonical id declared in `llm-providers.toml` (foreign-key check). `api_dialect` in rule scopes must be in the closed dialect set.
3. Emit static Rust arrays + `phf::Map` indexes into `$OUT_DIR` for zero-runtime-cost lookup.

**Lookup strategy is hybrid** (see `docs/crate-specs/nika-catalog.md`):

- `phf::Map` + `unicase` for case-insensitive entities (providers, MCP aliases) — O(1)
- Sorted arrays + `binary_search` for case-sensitive entities (builtins, transforms) — O(log n)
- First-match-wins rule table for model capabilities — O(n) but n = 9–28

`nika-catalog-verify` (L4 binary, also admitted) provides an **online verification layer**: fetches provider API endpoints nightly, confirms MCP aliases still respond, flags drift in CI.

**Session 2a concrete additions** (shipped, grep-verified):

- `data/model-capabilities.toml` — 9 rules covering OpenAI o-series, GPT-5, Claude family, Anthropic catch-all, DeepSeek reasoner, DeepSeek any, xAI Grok-4
- `build/capabilities.rs` — 380 LOC codegen, extracted from `build.rs` to respect the 1,500-LOC file cap
- `src/types/capabilities.rs` — 276 LOC (`CapPatch` + `Matcher` + `Rule`), `const fn merge_with`, `fn materialize`
- `api_dialect` field added to all 21 providers in `llm-providers.toml`
- 16 new constructors (invariant #19 complete — see ADR-007)
- Proptest parity harness — 10,000 random (provider, model) pairs compared against frozen legacy
- Insta snapshot — 31 golden (provider, model) pairs reviewable

## Consequences

### Positive
- Community contributors edit TOML, not Rust. Learning curve drops to "know YAML/TOML."
- Build-time FK checks catch typos before they ship.
- Zero runtime overhead — all data in static memory.
- `nika-catalog-verify` catches silent rot (29-alias class) nightly.
- Schema versions (`nika/model-capabilities@1.0`) make future migrations explicit.

### Negative
- Build times grow with data (current: catalog build ~8s cold). Acceptable; amortized over cargo caches.
- **Rule-authoring sharp edge:** first-match-wins means a single rule must carry **all** capabilities for a matched (provider, model) pair. Documented extensively in `model-capabilities.toml` header (lines 32–48). Getting this wrong silently drops capabilities.
- Adding a new capability field requires running the 10k-pair proptest parity harness (~30s).

### Neutral
- The TOML-vs-YAML choice: TOML for code-authored config (typing explicit, no indentation fragility, comments allowed). YAML stays for Nika workflow files (user-authored DSL).

## Evidence

- `crates/nika-catalog/data/model-capabilities.toml` lines 1–80 — schema, FK invariants, resolution algorithm, first-match-wins authoring note
- `crates/nika-catalog/data/llm-providers.toml` lines 1–50 — `api_dialect` on every provider
- `crates/nika-catalog/build.rs` lines 1–50 — build script overview
- `crates/nika-catalog/build/capabilities.rs` — 380 LOC codegen (extracted per 1,500 LOC cap, ADR-004)
- `crates/nika-catalog/src/types/capabilities.rs` — 276 LOC: `CapPatch`, `Matcher`, `Rule`
- `docs/crate-specs/nika-catalog.md` lines 1–60 — hybrid lookup rationale
- `CHANGELOG.md` — Session 2a entry "TOML-driven model capabilities" (Unreleased section)
- Related crates: **`nika-catalog`** (L0, 4,690 LOC, 154 tests), **`nika-catalog-verify`** (L4 binary, online check)

## Alternatives considered

### Alt A — Hardcoded Rust arrays (legacy approach)
Rejected. See context (contribution friction + 29-alias rot).

### Alt B — Runtime TOML loading (no codegen)
Rejected. Adds startup cost, loses compile-time FK checks, introduces a "which TOML did we load?" bug class.

### Alt C — Database (sqlite) instead of TOML
Rejected. Git-based review workflow (PR diffs, blame, history) is more valuable than query flexibility for a 21-provider catalog. sqlite reconsidered if catalog exceeds ~500 entries.

### Alt D — JSON instead of TOML
Rejected. No comments, more fragile indentation, harder to review in diff.

## Related

- ADR-004 — context-window sizing (catalog is 4,690 LOC; `build/capabilities.rs` extracted to respect 1,500 LOC file cap)
- ADR-007 — forward-compat invariants (schema versions + `#[non_exhaustive]` in generated types)
- `docs/crate-specs/nika-catalog.md` — full hybrid-lookup spec

## Notes

Session 2b (next) expands the rule table to 28 entries + 5 new ModelCapabilities fields (Modality, TokenizerFamily, ParamFlag, supports_system_messages, supports_reasoning). Session 3 adds pricing schema + 8 providers without rules + 4 new providers (qwen, minimax, moonshot, zhipu). Schema version stays `@1.0` — invariants #19 and #11 of `forward-compat-invariants.md` allow extension without version bump.

## Implementation notes — Session 2b / Session 3

The P0-1 "two sources of truth" hazard (Rust `Default` impl + TOML `[defaults]` diverging silently) is now closed by three coordinated measures:

- `CapPatch::materialize(&CAPABILITY_DEFAULTS)` draws every unset field from the TOML `[defaults]` block at resolve time. The Rust `ModelCapabilities::default()` impl is kept only as an internal-crate shortcut for test scaffolding and struct-literal ergonomics; it is never consulted by the resolver.
- The `materialize_defaults_agree_with_rust_default_impl` unit test (in `src/types/capabilities.rs`) asserts that the TOML-shaped defaults materialise into the same `ModelCapabilities` as the Rust `Default` impl — any edit to one without the other fails the test.
- Session 3 Phase G tightens `validate_caps_patch(&file.defaults, "[defaults]", require_all=true)` so the `[defaults]` block must explicitly set every field that has an `unwrap_or` fallback in `materialize` (8 of 9 slots; `tokenizer` stays legitimately optional by design). The hard-coded fallbacks in `materialize` become structurally unreachable for any TOML that passes build-time validation; `debug_assert!` lines in `materialize` catch runtime regressions in debug mode at zero release cost.

Session 3 also retires the coarse `ModelCapabilities::supports_vision: bool` — consumers migrate to `input_modalities.contains(&Modality::Image)`, which was the Session 2b replacement. The pricing catalog is extended with three `Option<f64>` axes (cached input, image, reasoning tokens); the full TOML-driven pricing migration (new `data/model-pricing.toml` + `build/pricing.rs`) is scheduled as Phase E2 of Session 4. The "two sources of truth" hazard is now **scoped**: defaults is closed (commit 731f11bfe — `require_all` on `[defaults]` + `debug_assert!` in `materialize`); pricing closure lands with Phase E2. The Rust `ALL_PRICING` slice remains the only source until the TOML migration, so the hazard has no bite today.
