---
lastUpdated: 2026-05-09
status: Admitted
---

# Crate spec — `nika-catalog-codegen`

| | |
|---|---|
| Status | **ADMITTED 2026-05-10** (`23ab2fef8`) · codegen WIRED byte-identical (D-2026-06-10-N4) · was: Gate-1 SPEC pending |
| Layer | L0 (PURE build-time, zero I/O at runtime, zero async, no runtime code at all) |
| Sub-tier | L0-tier-0 — leaf crate, zero `nika-*` dependencies. Consumed only by `nika-catalog`'s `build.rs`. |
| Design | **Pure build-time transformation** — `TOML bytes → validated schema → Rust source String`. Extracted from `nika-catalog/build.rs` + `nika-catalog/build/{capabilities,pricing}.rs` per ADR-008 + Q5 (Foundation v0.81 lock). |
| LOC budget | ≤3,500 src (target ~2,800, alarm at 3,200) |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Source on `main` (ex `nika-diamond` · renamed) (reference) | `crates/nika-catalog/build.rs` (1,386 LOC) + `crates/nika-catalog/build/capabilities.rs` (978 LOC) + `crates/nika-catalog/build/pricing.rs` (200 LOC) = **2,564 LOC** to migrate |
| Crate version | tracks workspace |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |
| Publish | `false` — foundation crate, never on crates.io (per ADR-017 + `feedback_publish_false_foundation_strategy.md`). Internal build-time tool only. |

---

## 1. Purpose

`nika-catalog-codegen` is the **build-time Rust source generator** for every
TOML-driven catalog that `nika-catalog` exposes. It lifts the current
~2,564 LOC of logic in `crates/nika-catalog/build.rs` + `build/*.rs` into a
first-class library crate, so:

- The **runtime** crate (`nika-catalog`) ships zero `toml` / `phf_codegen` /
  `unicase` build-dependency surface beyond a one-line call
  `nika_catalog_codegen::generate(&data_dir, &out_dir)?` from its (now
  trivial) `build.rs`.
- The **codegen** concern is independently testable with unit tests,
  proptest, and `insta` snapshots — instead of hiding inside a build
  script that only runs under cargo's build-dir gymnastics.
- Future codegen extensions (Phase E2 full-pricing TOML migration, Session
  2b capability-field additions, `nika-mcp-servers` catalog spin-out)
  stay localised inside one crate and never bloat `nika-catalog/build.rs`
  past the 1,500-LOC file cap again (vector 24 / ADR-023).

### Architectural decisions (locked by ADR-008 + Q5)

**A. Codegen is a lib crate, not a binary.** `nika-catalog/build.rs`
consumes it as a `[build-dependencies]` entry (`nika-catalog-codegen = {
path = "../nika-catalog-codegen" }`). There is no `nika-catalog-codegen`
bin, no CLI. Build scripts are the only consumer.

**B. Inputs are `&Path` + `&[u8]`, outputs are `String` + `&Path`.** The
library does filesystem reads + writes through `std::fs` because that is
what cargo build-dependencies legitimately do (see ADR-008 §"build-time
only") — BUT every function is also split so the *pure* transformation
(`parse_toml → validate → emit_rust`) is unit-testable against in-memory
strings, and only a thin outer layer touches `std::fs`.

**C. Error type = `CodegenError` (typed, `#[non_exhaustive]`, `thiserror`).**
No `Box<dyn Error>` (vector 27), no `String` errors in the public API.
Internal helpers may still return `Result<T, String>` during migration to
keep the diff mechanical; the public surface is typed.

**D. Schema version constants are public.** `pub const
MCP_SERVERS_SCHEMA: &str = "nika/mcp-servers@1.0"` — downstream tests +
`nika-catalog-verify` read them without re-declaring strings.

---

## 2. Layer justification — why L0

| L0 rule (per `crate-layer-registry.md` + `forward-compat-invariants.md`) | Satisfied? |
|---|---|
| No async (no `tokio`, no `async fn`) | ✅ — pure sync code |
| No network / no filesystem effects at RUNTIME | ✅ — all FS access happens inside cargo's build phase; the crate itself is a library with no `main` |
| No `nika-*` dependencies | ✅ — zero `nika-*` dep. `nika-catalog` depends on THIS crate, not the other way round |
| No `Box<dyn Error>` in public API (vector 27) | ✅ — typed `CodegenError` |
| `#[non_exhaustive]` on all public structs + enums (FCI-002) | ✅ — every `CodegenError` variant + every exposed TOML schema type |
| ≤15k LOC crate, ≤1,500 LOC / file, ≤100 lines / fn | ✅ — 2,800 LOC budget, split across ~8 files |

The crate runs **at build time only** from a downstream crate's `build.rs`.
At runtime it is absent from the compiled binary. This is the canonical L0
build-tool shape — same layer class as `nika-types`, `nika-error`,
`nika-catalog`, `nika-schema`.

The layer registry (`docs/architecture/crate-layer-registry.md`) already
lists `nika-catalog-codegen [build-dep+lib]` in the L0 row (8 crates).
This spec is the prose-side lock of that entry.

---

## 3. Public API surface

Every public type is `#[non_exhaustive]` with a `pub fn new(...)`
constructor per invariant #19. Errors implement `NikaErrorCode` from
`nika-error` when wired (pending Round 3 — the crate can also stay in
"String errors, no `nika-error` dep" for the initial admission and gain
`NikaErrorCode` in a follow-up if the dep graph bikeshed goes that way).

```rust
// ── Top-level entry point (build.rs one-liner) ────────────────────────

/// Parse every TOML in `data_dir`, validate, and emit Rust source into
/// `out_dir`. Honors `CARGO_FEATURE_*` environment variables to gate
/// which catalogs are emitted — same matrix as the legacy build.rs.
///
/// Called from `nika-catalog/build.rs` as:
/// ```no_run
/// fn main() -> Result<(), nika_catalog_codegen::CodegenError> {
///     let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
///     let out_dir = std::env::var("OUT_DIR")?;
///     let data_dir = std::path::PathBuf::from(manifest_dir).join("data");
///     nika_catalog_codegen::generate(
///         &data_dir,
///         std::path::Path::new(&out_dir),
///         nika_catalog_codegen::FeatureSet::from_env(),
///     )
/// }
/// ```
pub fn generate(
    data_dir: &Path,
    out_dir: &Path,
    features: FeatureSet,
) -> Result<Emitted, CodegenError>;

/// Enabled catalog features, typically built from cargo
/// `CARGO_FEATURE_*` env vars via `FeatureSet::from_env()`. Public
/// constructor exists so unit tests can drive the function without
/// going through env vars.
#[non_exhaustive]
pub struct FeatureSet {
    pub mcp: bool,
    pub providers: bool,
    pub embeddings: bool,
    pub capabilities: bool,
    pub pricing: bool,
}

impl FeatureSet {
    pub fn new() -> Self;                                  // all false
    pub fn from_env() -> Self;                             // read CARGO_FEATURE_*
    pub fn all() -> Self;                                  // every flag on
}

/// Summary of what `generate` actually wrote. Used by build.rs for
/// `cargo:rerun-if-changed` + by tests to assert emission.
#[non_exhaustive]
pub struct Emitted {
    pub files: Vec<PathBuf>,   // e.g. OUT_DIR/mcp_servers.rs
    pub rerun_paths: Vec<PathBuf>, // TOML files read; build.rs emits cargo:rerun-if-changed=
}

// ── Per-catalog pure-transform functions (unit-testable) ──────────────
// Each takes validated TOML bytes (or a Path for FS-reading variants)
// and returns emitted Rust source as a `String`.

pub fn codegen_mcp_servers(toml_bytes: &[u8]) -> Result<String, CodegenError>;
pub fn codegen_providers(toml_bytes: &[u8]) -> Result<String, CodegenError>;
pub fn codegen_embeddings(
    toml_bytes: &[u8],
    providers: &[ProviderEntry],   // FK check input
) -> Result<String, CodegenError>;
pub fn codegen_capabilities(
    toml_bytes: &[u8],
    providers: &[ProviderEntry],
) -> Result<String, CodegenError>;
pub fn codegen_pricing(toml_bytes: &[u8]) -> Result<String, CodegenError>;

// ── TOML schema types (re-exported for nika-catalog-verify + tests) ───

#[non_exhaustive]
pub struct McpServerEntry { /* pub fields, matches current build.rs */ }
#[non_exhaustive]
pub struct ProviderEntry { /* pub fields, matches current build.rs */ }
#[non_exhaustive]
pub struct EmbeddingEntry { /* pub fields, matches current build.rs */ }
#[non_exhaustive]
pub struct PricingEntry { /* pub fields, matches current build.rs */ }
// (capabilities types stay pub(crate) — they are authoring-time only,
//  not consumed by verify)

// ── Schema version constants (public, stable) ─────────────────────────

pub const MCP_SERVERS_SCHEMA:  &str = "nika/mcp-servers@1.0";
pub const LLM_PROVIDERS_SCHEMA:&str = "nika/llm-providers@1.0";
pub const EMBEDDINGS_SCHEMA:   &str = "nika/embeddings@1.0";
pub const CAPABILITIES_SCHEMA: &str = "nika/model-capabilities@1.0";
pub const PRICING_SCHEMA:      &str = "nika/model-pricing@1.0";

// ── Error ─────────────────────────────────────────────────────────────

#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum CodegenError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },

    #[error("{path}: TOML parse error: {source}")]
    TomlParse { path: PathBuf, source: toml::de::Error },

    #[error("{path}: schema mismatch — file declares {got:?}, expected {expected:?}")]
    SchemaMismatch { path: PathBuf, expected: &'static str, got: String },

    #[error("{context}: {reason}")]
    SchemaValidation { context: String, reason: String },

    #[error("{context}: foreign-key violation — {reason}")]
    ForeignKey { context: String, reason: String },

    #[error("cargo env var {name} missing: {source}")]
    EnvVar { name: &'static str, source: std::env::VarError },
}

// No `Box<dyn Error>`, no `impl From<String>` — every path into
// `CodegenError` is explicit.
```

No traits are exposed. L0 + pure function API only. The internal TOML
schema structs that are `Deserialize`-only (capabilities' `RuleEntry`,
`ScopeEntry`, `MatchEntry`, `CapsPatchEntry`, `TokenizerFamilyToml`,
`JsonModeToml`) stay `pub(crate)` — they do not need re-export.

---

## 4. File layout

```
crates/nika-catalog-codegen/
├── Cargo.toml
├── src/
│   ├── lib.rs                 (~200 LOC — pub API, generate() orchestrator,
│   │                                      FeatureSet, Emitted, CodegenError)
│   ├── schema.rs              (~120 LOC — schema version consts +
│   │                                      assert_schema helper)
│   ├── mcp_servers.rs         (~500 LOC — MCP TOML schema structs +
│   │                                      parse + validate + emit. Carries
│   │                                      the MCP safety-tag XOR check +
│   │                                      pypi-runner pairing.)
│   ├── providers.rs           (~350 LOC — provider TOML schema + api_dialect
│   │                                      validation + token-limit checks +
│   │                                      emit_provider.)
│   ├── embeddings.rs          (~250 LOC — embeddings TOML schema + FK check
│   │                                      against providers + emit.)
│   ├── capabilities.rs        (~900 LOC — capabilities rule table parse +
│   │                                      validate + emit. Largest module;
│   │                                      must stay ≤1,000 to keep headroom
│   │                                      under the 1,500-LOC file cap for
│   │                                      Session 2b field additions.)
│   ├── pricing.rs             (~180 LOC — pricing TOML schema + rate
│   │                                      bounds + 7-axis emit.)
│   ├── tags.rs                (~160 LOC — tag_variant() map +
│   │                                      ALL_KNOWN_TAGS + TAG_VARIANT_COUNT
│   │                                      + validate_tags + validate_mcp_
│   │                                      safety_tag. Split out so the 42-
│   │                                      variant table has a clear home.)
│   └── emit.rs                (~100 LOC — shared emission helpers:
│   │                                      rstr/opt_rstr/str_slice_expr,
│   │                                      write_str_slice.)
│   └── error.rs               (~80 LOC  — CodegenError + From conversions.)
└── tests/                     — inline #[cfg(test)] only, NO tests/ dir.
                                  (Matching house style — all other admitted
                                   L0 crates keep tests inline.)
```

**LOC totals (estimate vs migrated source)**:

- Legacy sum: `build.rs` 1,386 + `build/capabilities.rs` 978 + `build/pricing.rs`
  200 = **2,564 LOC**.
- Diamond target: **~2,840 LOC** (estimate) — modest growth (~+10%) from
  typed errors, doc comments on every `pub` item (Gate 8), and splitting
  the current giant `build.rs` into 8 files none of which exceeds 900 LOC.
- No file exceeds **900 LOC** at admission. 1,500 cap has ≥600 LOC of
  headroom per file for Session 2b / Session 3 / Phase E2 growth.

Evidence for each budget line:

| File | Budget | Migrated from (line-range in legacy) |
|---|---|---|
| `lib.rs` | ~200 | build.rs L220–L310 (`main` + `run` + feature gating) |
| `schema.rs` | ~120 | build.rs L59–L71 (`*_SCHEMA` consts) + `assert_schema` L311–L319 |
| `mcp_servers.rs` | ~500 | build.rs L75–L145 (schema) + L323–L399 (parse) + L654–L803 (emit) |
| `providers.rs` | ~350 | build.rs L146–L215 (schema) + L968–L1196 (parse + emit) |
| `embeddings.rs` | ~250 | build.rs L154–L177 (schema) + L1198–L1361 (parse + emit) |
| `capabilities.rs` | ~900 | build/capabilities.rs (978 LOC, shrinks post-typed-error cleanup) |
| `pricing.rs` | ~180 | build/pricing.rs (200 LOC) |
| `tags.rs` | ~160 | build.rs L478–L650 (tag validation + variant map) |
| `emit.rs` | ~100 | build.rs L862–L895 + L1363–L1390 (shared helpers) |
| `error.rs` | ~80 | new (typed enum replaces `String` errors) |

---

## 5. Dependencies

```toml
[dependencies]
toml        = "1.1"                             # TOML parse, matches current build
phf_codegen = "0.13"                            # Emit phf::Map builders
phf_shared  = { version = "0.13", features = ["unicase"] }
serde       = { workspace = true, features = ["derive"] }
unicase     = { workspace = true }              # UniCase::ascii for phf keys
thiserror   = { workspace = true }              # CodegenError derive

[dev-dependencies]
insta       = { workspace = true }              # snapshot tests on emitted Rust
proptest    = { workspace = true }              # invariants on emit
rstest      = { workspace = true }
syn         = { workspace = true, features = ["full", "parsing"] }  # Gate 6 proptest parse-check
```

### Allowed (L0 build-time universe)

- `toml`, `serde`, `phf_codegen`, `phf_shared`, `unicase` — the minimal
  set needed to parse TOML and emit `phf` maps. Identical set to the
  current `nika-catalog/build.rs` build-dependencies, MINUS the ones this
  crate now owns on behalf of `nika-catalog`.
- `thiserror` — typed errors (FCI-019). `miette` deliberately NOT pulled
  in: build-time errors surface via cargo's stderr, diagnostic rendering
  is overkill and doubles the dep graph.
- `syn` (dev-only) — Gate 6 proptest asserts emitted Rust parses as a
  valid TokenStream. Runtime crate never sees it.

### Forbidden

| Crate | Why excluded |
|---|---|
| `tokio` / `async-std` / `futures` | L0 = zero async |
| `nika-kernel` / `nika-catalog` / any `nika-*` | L0-tier-0 = no `nika-*` deps |
| `reqwest` / `hyper` / `ureq` | No network at build time |
| `quote` + `proc-macro2` | **Intentionally skipped.** The legacy build.rs builds Rust source via `write!(String, ...)`. Switching to `quote!` is an orthogonal change that belongs in a later refactor, not in the admission commit. The admission is an **extraction**, not a rewrite. (Decision point — see §12 "Open questions".) |
| `anyhow` / `eyre` / `Box<dyn Error>` | Vector 27 (`no-box-dyn-error`) — typed `CodegenError` only |
| `log` / `tracing` | Build scripts use `println!("cargo::warning=...")` for author-visible diagnostics. No framework needed. |

### Dep delta on `nika-catalog`

Post-extraction, `nika-catalog/Cargo.toml` loses these build-dependencies
(they move to `nika-catalog-codegen`):

```diff
 [build-dependencies]
-phf_codegen = "0.13"
-phf_shared  = { version = "0.13", features = ["unicase"] }
-toml        = "1.1"
-serde       = { workspace = true, features = ["derive"] }
-unicase     = { workspace = true }
+nika-catalog-codegen = { path = "../nika-catalog-codegen", version = "0.90.0" }
```

Net reduction: 5 direct build-dependencies → 1. The transitive graph is
the same size, but `nika-catalog`'s manifest no longer advertises TOML /
phf as something it "uses" — which matches reality.

---

## 6. TDD test plan (Gate 2 outline)

All tests inline `#[cfg(test)] mod tests` per file. Run with
`cargo test -p nika-catalog-codegen --lib`.

| Test location | Scope | Count (target) |
|---|---|---|
| `src/schema.rs` `#[cfg(test)]` | `assert_schema` matches + mismatches, every `*_SCHEMA` const value | ~6 |
| `src/mcp_servers.rs` `#[cfg(test)]` | golden TOML → expected Rust (insta snap), duplicate-id error, missing-package error, pypi-without-runner error, safety-tag XOR error, unknown category/pricing/transport/auth/registry_type | ~18 |
| `src/providers.rs` `#[cfg(test)]` | parse every canonical provider, missing-`api_dialect` warn path, unknown `api_dialect` error, missing-token-limit error, duplicate-alias error | ~14 |
| `src/embeddings.rs` `#[cfg(test)]` | FK check against provider list (success + fail), unknown `similarity`, negative `input_per_million` error | ~8 |
| `src/capabilities.rs` `#[cfg(test)]` | each `MatchEntry` variant (Any/Exact/ExactAny/PrefixAny/ContainsAny) round-trips, `deny_unknown_fields` catches typo, `Any`-must-be-last invariant, region-scope rejection (Phase E2 guard), FK check on `scope.providers`, `deny_unknown_fields` on `TokenizerFamilyToml` | ~22 |
| `src/pricing.rs` `#[cfg(test)]` | all 7 axes emitted correctly (`cache_write`, `cache_read`, `image`, `reasoning`), rate bounds (≥0 ≤10_000 per million) | ~10 |
| `src/tags.rs` `#[cfg(test)]` | `TAG_VARIANT_COUNT == 42`, `ALL_KNOWN_TAGS.len() == 42`, sorted, dedup, alphabetical order enforced | ~6 |
| `src/emit.rs` `#[cfg(test)]` | `rstr` escapes `\n` `"` `\\`, `opt_rstr` handles None, `str_slice_expr` empty → `&[]` | ~6 |
| `src/error.rs` `#[cfg(test)]` | Display format per variant, thiserror source() chain preserved | ~6 |
| `src/lib.rs` `#[cfg(test)]` | `FeatureSet::from_env()` reads `CARGO_FEATURE_*`, `generate()` writes expected files under `FeatureSet::all()` with tempdir, `Emitted.rerun_paths` lists every TOML read | ~6 |
| **Gate 6 proptest** (in `capabilities.rs` + `providers.rs`) | for 10_000 random valid TOML inputs: emitted Rust parses via `syn::parse_str::<syn::File>` WITHOUT error | ~3 |
| **Insta snapshots** | 1 golden per catalog (`mcp_servers`, `providers`, `embeddings`, `capabilities`, `pricing`) — small deterministic fixture, easy to eyeball diff | ~5 |
| **Total** | | **~110** |

### Gate 6 (property testing) targets

- **`codegen_*` output parses as valid Rust** — `syn::parse_str::<syn::File>(&emitted)`
  must succeed for 10,000 random valid TOML inputs per catalog. This is
  the load-bearing invariant: the whole point of this crate is to emit
  Rust source, and "it compiles" is the minimum contract.
- **Idempotence** — `codegen_X(toml) == codegen_X(toml)` for the same bytes
  (deterministic output; no time-, HashMap-, or random-seeded variation).
  Required for reproducible builds.
- **Structural round-trip** — for every `McpServerEntry` / `ProviderEntry`
  / `EmbeddingEntry` / `PricingEntry` proptest instance: serialize to
  TOML, parse back, assert structural equality. Guards the schema types
  against accidental asymmetry.

### Insta snapshot strategy

1 golden TOML fixture per catalog, deliberately small (1–3 entries) to
keep the `.snap` file reviewable. Snapshots live next to the code in
`src/<catalog>/snapshots/`. Large-corpus parity (all 105 MCP servers,
all 21 providers, all 62 pricing rules) is covered by Gate 10 against
the current `nika-catalog` build output — not inside this crate's
snapshots.

---

## 7. Gate 5 — mutation target

`cargo mutants -p nika-catalog-codegen` with **≥90% killed**.

Pure-function codegen is a high-mutation-score context: every emit arm
has a corresponding assert in the `insta` snapshot or the `syn` parse
check. Expected weak spots (where manual `#[mutants::skip]` is allowed,
documented inline):

- Path-display strings inside error `Display` impls — mutating
  `path.display()` → `path.to_string_lossy()` does not change user-visible
  behavior in any mechanically testable way. Skip with inline comment.
- `writeln!` vs `write!` on the trailing newline of emitted headers —
  caught by the `syn` parse check (Rust is newline-insensitive there).

Target = **≥90%**. If the first run lands lower, iterate on tests —
**do not skip mutants to meet the threshold**.

---

## 8. Gate 7 — benchmarks

**Exempt — N/A.**

Justification: this crate runs inside `cargo build`, at most once per
catalog edit (cargo caches build-script output until a TOML dep changes).
The cold-build cost of the full five-catalog generation is **~8 seconds
today** (per ADR-008 §Consequences), amortised over hundreds of
subsequent compiles. It is not a runtime hot path — no user workflow is
ever blocked on `codegen_capabilities` execution time.

Criterion would add signal on "did we regress by 40% in build time
between commits" but the existing cargo-timings vector (#11 in the
engine's 16-vector suite) already catches that at the workspace level,
without the noise of an ad-hoc micro-benchmark.

If Session 2b's capability rule table grows past 50 entries AND the
observed cold-build time exceeds 20s, this exemption will be revisited
and a `benches/codegen.rs` added to lock a ceiling.

---

## 9. Gate 8 — docs

- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p nika-catalog-codegen`
  passes with **zero warnings**.
- Every `pub fn`, `pub struct`, `pub enum`, `pub const` has a doc comment
  covering: what it does, what it accepts, what it returns, how it fails
  (if fallible).
- `lib.rs` top-level `//!` doc explains the extract-from-`nika-catalog`
  rationale, the input/output contract, and links to ADR-008.
- Every `CodegenError` variant carries an author-facing `//!` example
  in the top-level module doc showing the exact TOML that triggers it.

---

## 10. Gate 9 — canary E2E

**Exempt — N/A.**

Justification: this crate has **no runtime surface**. There is no verb
to canary, no workflow shape to exercise, no YAML that ever touches
`nika-catalog-codegen`. The functional equivalent of a canary for a
build-time codegen crate is Gate 10 (parity) — which we run — plus the
downstream `nika-catalog` crate's own 385 tests which implicitly consume
the generated Rust.

The L0 build-tool canary exemption is already granted to `nika-types`,
`nika-error`, and `nika-catalog` by precedent; this crate matches that
profile exactly.

---

## 11. Gate 10 — parity vs legacy `nika-catalog/build.rs`

**Mandatory, mechanical.** The admission commit MUST demonstrate
byte-for-byte (or structural-AST-equivalent) identical output with the
pre-extraction `nika-catalog/build.rs`.

Procedure:

1. Pre-admission: capture the current emitted Rust for all five catalogs
   into `crates/nika-catalog-codegen/tests/fixtures/golden/<name>.rs`.
   (Run `cargo build -p nika-catalog --all-features`, copy from
   `target/debug/build/nika-catalog-*/out/*.rs`.)
2. In `tests/parity.rs` (`#[cfg(test)]` inline in `lib.rs`, actually —
   this crate does NOT have a `tests/` dir, staying consistent with
   every other admitted L0 crate):
   - Read the captured fixture input TOMLs from `nika-catalog/data/`.
   - Call each `codegen_*` function.
   - Assert **byte-equality** with the golden. If byte-equality fails,
     fall back to AST-equality via `syn::parse_str::<syn::File>` and
     compare tokens pretty-printed via `prettyplease` (dev-dep only).
3. Any diff MUST be explicitly justified in the admission commit body
   (e.g., "new header comment, no semantic change" — acceptable; "phf
   key normalisation changed" — blocker, fix before admission).

This gate transitively covers the 105 MCP servers, 21 providers, 13
embeddings, 49 capability rules, 62 pricing rules. The TOMLs stay
unchanged; only the build script that consumes them moves.

---

## 12. Migration plan — exhaustive

The admission lands in two atomic commits. The spec locks the shape;
the impl is Round 3.

### Commit A — `feat(nika-catalog-codegen): admit to workspace — all 12 gates passed`

Creates `crates/nika-catalog-codegen/` with the file layout above.
**Does not touch `crates/nika-catalog/`.**  All parity verified via
the captured fixtures in §11.

### Commit B — `refactor(nika-catalog): delegate build.rs to nika-catalog-codegen`

Shrinks `crates/nika-catalog/build.rs` from 1,386 LOC to ~15 LOC:

```rust
// crates/nika-catalog/build.rs (post-extraction)
// SPDX-License-Identifier: AGPL-3.0-or-later
fn main() -> Result<(), nika_catalog_codegen::CodegenError> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    let out_dir = std::env::var("OUT_DIR")?;
    let data_dir = std::path::PathBuf::from(&manifest_dir).join("data");
    let emitted = nika_catalog_codegen::generate(
        &data_dir,
        std::path::Path::new(&out_dir),
        nika_catalog_codegen::FeatureSet::from_env(),
    )?;
    for p in emitted.rerun_paths {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}
```

Removes `crates/nika-catalog/build/capabilities.rs` (978 LOC) and
`crates/nika-catalog/build/pricing.rs` (200 LOC). Logic moves to
`crates/nika-catalog-codegen/src/capabilities.rs` + `.../pricing.rs`.

All 385 `nika-catalog` tests MUST pass unchanged. Workspace clippy 0.

### Exhaustive function/module migration table

| Legacy `nika-catalog` location | → | Diamond `nika-catalog-codegen` location |
|---|---|---|
| `build.rs` `main()` + `run()` L221–L309 | → | `src/lib.rs` `generate()` |
| `build.rs` `*_SCHEMA` consts L66–L71 | → | `src/schema.rs` (`pub const`) |
| `build.rs` `assert_schema` L311–L319 | → | `src/schema.rs::assert_schema` |
| `build.rs` `McpServersFile`/`McpServerEntry`/`PackageEntry`/`RemoteEntry`/`EnvVarEntry` L75–L145 | → | `src/mcp_servers.rs` (pub schema types) |
| `build.rs` `parse_mcp_servers` L323–L399 | → | `src/mcp_servers.rs::parse` |
| `build.rs` `validate_category`/`validate_pricing`/`validate_registry_type`/`validate_transport`/`validate_auth`/`validate_py_runner` L416–L476 | → | `src/mcp_servers.rs` (pub(crate) helpers) |
| `build.rs` `validate_mcp_safety_tag` L482–L496 | → | `src/mcp_servers.rs` (keeps the XOR check local to MCP) |
| `build.rs` `validate_tags`/`tag_variant`/`ALL_KNOWN_TAGS`/`TAG_VARIANT_COUNT` L504–L650 | → | `src/tags.rs` (consolidated tag module) |
| `build.rs` `generate_mcp_servers_rs`/`emit_server`/`write_*` L654–L858 | → | `src/mcp_servers.rs::emit` |
| `build.rs` `LlmProvidersFile`/`ProviderEntry`/`ProviderModelEntry` L148–L215 | → | `src/providers.rs` (pub schema types) |
| `build.rs` `parse_llm_providers` L968–L1065 | → | `src/providers.rs::parse` |
| `build.rs` `validate_api_dialect` L1256–L1266 | → | `src/providers.rs` (pub(crate)) |
| `build.rs` `generate_providers_rs`/`emit_provider`/`write_models` L1067–L1196 | → | `src/providers.rs::emit` |
| `build.rs` `EmbeddingsFile`/`EmbeddingEntry` L156–L177 | → | `src/embeddings.rs` (pub schema types) |
| `build.rs` `parse_embeddings` L1198–L1254 | → | `src/embeddings.rs::parse` |
| `build.rs` `validate_similarity`/`similarity_variant` L1268–L1282 | → | `src/embeddings.rs` (pub(crate)) |
| `build.rs` `generate_embeddings_rs` L1284–L1361 | → | `src/embeddings.rs::emit` |
| `build.rs` `rstr`/`opt_rstr`/`str_slice_expr`/`tags_slice_expr`/`category_variant`/`pricing_variant`/`registry_variant`/`transport_variant`/`auth_variant`/`py_runner_variant` L862–L966, L1363–L1390 | → | `src/emit.rs` (shared) + per-catalog variant mappers in their own modules |
| `build/capabilities.rs` entire file (978 LOC) | → | `src/capabilities.rs` |
| `build/pricing.rs` entire file (200 LOC) | → | `src/pricing.rs` |

Every numbered line range was grep-verified against HEAD `9ebaf05ca`.

---

## 13. Non-goals

- **No runtime parsing.** Runtime catalog lookups stay in `nika-catalog`
  (`find_provider`, `find_mcp_server`, etc.). This crate emits the
  `&'static` slices + `phf::Map`s those lookups read; it never parses
  TOML at runtime.
- **No schema validation of `.nika.yaml` workflows.** That is
  `nika-schema`'s job (L0 sibling). The two crates share ZERO code and
  ZERO types.
- **No network / FS verification of MCP servers.** That is
  `nika-catalog-verify`'s job (L4 online checker). Static FK checks
  against the TOML are in scope; reachability checks are not.
- **No proc macros.** Q1 decision locks "no proc macros in L0". All
  codegen is `write!(String, ...)` based, same as legacy. `quote!`
  adoption is explicitly deferred (see §5 Forbidden + §14 Open
  questions).
- **No catalog DATA changes.** Every `data/*.toml` under
  `crates/nika-catalog/data/` stays at `nika-catalog/data/`. The TOMLs
  do NOT move into this crate. Data lives with the runtime crate; this
  crate only knows how to parse + emit.

---

## 14. Reserved / future

- **The Connectome (L1 satellite admissions).** No direct interaction.
  The Connectome satellites have no catalog TOML at L0 foundation scope.
  If `nika/memory-adapters@1.0` ever becomes a TOML schema (it will not
  under the 1+10 Connectome cluster), a sibling codegen crate would be
  added — NOT retrofitted here.
- **WASM host (L3 admission).** N/A. This crate does not run at runtime, never gets
  compiled into a WASM binary, never crosses the plugin boundary. Pure
  build-time tool.
- **Phase E2 full-pricing TOML migration.** Drops the residual Rust
  `ALL_PRICING` slice currently in `nika-catalog/src/data/models.rs` and
  makes `data/model-pricing.toml` the sole source of truth. That migration
  adds ≤300 LOC to `src/pricing.rs` (still well under the 1,500-LOC
  file cap) — no structural change to THIS crate's public API.
- **Session 2b capability field additions.** +5 `Option<T>` fields on
  `CapsPatchEntry`, +19 rules in `data/model-capabilities.toml`. Adds
  ≤400 LOC to `src/capabilities.rs`. The 1,500-LOC file cap is the
  forcing function for a `capabilities/` sub-module split if growth
  continues past that; split plan is "one module per matcher kind". Not
  done at admission; not needed until growth demands it.

---

## 15. Related ADRs + Q-decisions

- **ADR-008** — TOML-driven catalog, build-time codegen, perfect-hash lookup. Authority for the crate's existence.
- **ADR-007** — Forward-compat invariants (`#[non_exhaustive]`, FCI-002, FCI-019).
- **ADR-023** — File-modularity discipline (1,500-LOC cap, drives the extraction).
- **ADR-024** — SOTA Rust patterns + L0 sub-tiers (L0-tier-0 = no `nika-*` deps).
- **ADR-027** — Strict L0 tiers + cargo-timings.
- **Q1** (2026-04-16) — No proc macros in L0 → manual `write!(String, ...)` codegen stays.
- **Q5** (2026-04-16) — `nika-catalog` split into runtime + codegen crates (this crate).
- **Invariant #19** — `pub fn new()` on every `#[non_exhaustive]` public type.
- **Vector 27** — No `Box<dyn Error>` → typed `CodegenError`.

---

## 16. Gate status (pre-admission snapshot)

| Gate | Status | Note |
|---|---|---|
| 1. SPEC | 🟡 Pending review | this document |
| 2. TDD | ⏳ | RED tests to be written first (Round 3) |
| 3. IMPL | ⏳ | extraction from `nika-catalog/build.rs` + `build/*.rs` |
| 4. CLIPPY 0 | ⏳ | `--workspace --all-targets -D warnings` |
| 5. MUTATION ≥ 90% | ✅ | **measured 2026-06-11** `cargo mutants -p nika-catalog-codegen -- --lib` · 372 mutants · **360/364 viable caught = 98.9 %** · 8 unviable · 4 documented equivalent-mutant exemptions (`GATE5-EXEMPT` below) — the 47-survivor sweep added 19 targeted tests (declared-dialect + region scope gates · max_out==ctx boundary · exhaustive variant/sort-order pins across capabilities/mcp/embeddings · serde defaults stdio/none · alias-collision + separator placement · generate round-trip on scratch fixtures killing read_file/write_file/feature-gate `\|\|` mutants) |
| 6. PROPERTY | ⏳ | `syn` parse + idempotence + TOML round-trip |
| 7. BENCHMARKS | ⚠️ Exempt | build-time, see §8 |
| 8. DOCS | ⏳ | 0 doc warnings, every pub item documented |
| 9. CANARY E2E | ⚠️ Exempt | no runtime, see §10 |
| 10. PARITY LEGACY | ⏳ | golden byte/AST-diff vs current `nika-catalog/build.rs` (see §11) |
| 11. REVIEW SWARM | ⏳ | 3-agent parallel review before admission |
| 12. ATOMIC COMMIT | ⏳ | 2 commits: admit + delegate (see §12) |

### Gate 5 equivalent-mutant budget (ADR-003 Rule 2 · verified 2026-06-11)

<!-- GATE5-EXEMPT: 4 -->

The four surviving mutants are TRUE equivalents (behavior-identical by
construction · each verified by reading the guarded path):

1. `capabilities.rs check_any_last_in_scope` `skip(i + 1)` → `skip(i)` — the
   only extra element is `rules[i]` itself, which IS `Matcher::Any` (let-else
   guard) and is excluded by the `!matches!(later, Any)` test. (cargo-mutants
   27 spells this mutant `+` → `*`: `skip(i * 1)` ≡ `skip(i)` — the same
   equivalence, re-measured 2026-08-18.)
2. `pricing.rs validate_pricing` `(i + 1)..` → `i..` — (spelled `+` → `*` by
   cargo-mutants 27: `(i * 1)..` ≡ `i..`, same equivalence) the extra self-pair is
   excluded by the `patterns[j] != patterns[i]` condition.
3. `tags.rs validate_tags` `w[0] > w[1]` → `>=` — the equality case returns at
   the duplicate-tag check in the SAME loop iteration, so the sorted-order
   comparison never observes equal pairs.
4. `lib.rs FeatureSet::from_env` → `Default::default()` — `CARGO_FEATURE_*`
   env vars exist only during build-script execution, never in the test
   harness, so both sides read all-false; killing it would need
   `std::env::set_var` (unsafe · forbidden by `unsafe_code = forbid`). The
   projection is exercised for real by `nika-catalog/build.rs`.

---

## 17. Audit trail

| Date | Author | Change |
|---|---|---|
| 2026-04-17 | Round 3 Gate 1 | Initial spec — extract codegen from `nika-catalog`. |
| 2026-08-18 | stabilization sweep | Gate 5 re-measured (`cargo mutants -p nika-catalog-codegen -- --lib` · cargo-mutants 27 · 450 mutants · 8 unviable): 8 survivors = the 4 documented equivalents (two of them now spelled `+`→`*` by the newer operator set) + 4 REAL gaps in `is_iso_date` / `is_iso_month` (`\|\|`→`&&` survived because every refused probe broke two clauses at once). Single-broken-clause probes added (one wrong separator · one non-digit segment · a leading `+` that `u8::from_str` accepts · one out-of-range field) → 21/21 caught on the targeted run · budget stays 4 (nightly RED since 08-15 on this crate closes). |

🦋
