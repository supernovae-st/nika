// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # nika-catalog-codegen
//!
//! Build-time TOML → Rust source generator for [`nika-catalog`].
//! Lifted from `nika-catalog/build.rs` per ADR-008 + Q5 (Foundation v0.81
//! lock) so the codegen concern lives in a unit-testable library crate
//! rather than a build-script monolith.
//!
//! ## Layer
//!
//! L0 (sub-tier 0) — pure transformation, build-time only, zero
//! `nika-*` dependencies. Never compiled into a runtime binary.
//!
//! **Wired (D-2026-06-10-N4 · nika 10/10 B3)** · `nika-catalog`
//! consumes this crate from its `build.rs` thin adapter
//! (`[build-dependencies]`). The in-tree twin (~2.5k LOC build.rs +
//! build/) was deleted after the swap was proven **byte-identical** on
//! the live catalog data (5/5 emitted files · pre/post `OUT_DIR` diff).
//!
//! ## Public API surface
//!
//! Two layers :
//!
//! 1. **Top-level orchestrator** — [`generate`] takes a `data_dir` +
//!    `out_dir` + [`FeatureSet`] and writes one `.rs` file per enabled
//!    catalog. This is the one-line `build.rs` adapter.
//! 2. **Per-catalog pure-transform** — [`codegen_mcp_servers`],
//!    [`codegen_providers`], [`codegen_embeddings`],
//!    [`codegen_capabilities`], [`codegen_pricing`] take TOML bytes and
//!    return Rust source as `String`. Used by unit tests and parity
//!    fixtures without going through the filesystem.
//!
//! ## Errors
//!
//! Every fallible path returns [`CodegenError`] (`#[non_exhaustive]`,
//! `thiserror`-derived). No `Box<dyn Error>` per vector 27.
//!
//! ## Example (from `nika-catalog/build.rs`)
//!
//! ```no_run
//! fn main() -> Result<(), nika_catalog_codegen::CodegenError> {
//!     let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|source| {
//!         nika_catalog_codegen::CodegenError::EnvVar {
//!             name: "CARGO_MANIFEST_DIR",
//!             source,
//!         }
//!     })?;
//!     let out_dir = std::env::var("OUT_DIR").map_err(|source| {
//!         nika_catalog_codegen::CodegenError::EnvVar {
//!             name: "OUT_DIR",
//!             source,
//!         }
//!     })?;
//!     let data_dir = std::path::PathBuf::from(&manifest_dir).join("data");
//!     let emitted = nika_catalog_codegen::generate(
//!         &data_dir,
//!         std::path::Path::new(&out_dir),
//!         nika_catalog_codegen::FeatureSet::from_env(),
//!     )?;
//!     for p in &emitted.rerun_paths {
//!         println!("cargo:rerun-if-changed={}", p.display());
//!     }
//!     println!("cargo:rerun-if-changed=build.rs");
//!     Ok(())
//! }
//! ```
//!
//! [`nika-catalog`]: https://github.com/supernovae-st/nika
//!
//! ## Build-tool lint discipline
//!
//! This crate is build-time only (cargo `[build-dependencies]`). It is
//! never compiled into a runtime binary, never reaches `nika-runtime`,
//! and never crosses the kernel boundary. The workspace lints designed
//! for runtime `src/` (no `unwrap_used`, no `disallowed_methods` on
//! `std::env::var`, no raw `std::collections::HashSet` for taint-tracked
//! data, no `panic`) are explicitly carved out here with rationale per
//! lint :
//!
//! - `disallowed_methods` (`std::env::var`) — legitimately reads
//!   `CARGO_FEATURE_*` env vars (cargo's documented feature-detection API)
//! - `disallowed_types` (`std::collections::HashSet`) — used for
//!   build-time uniqueness checks on TOML data ; no runtime / no taint
//! - `expect_used` / `unwrap_used` — `expect()` on writeln-to-`String`
//!   is provably infallible ; `validated by upstream` panic markers cover
//!   variant-mapping branches that already passed validation
//! - `panic` — `panic_validated` is structurally unreachable ; the
//!   only path through it indicates a bug in the validator (i.e. caught
//!   in dev, never in user TOML)
//! - `too_many_bools` — `FeatureSet` mirrors `cargo`'s 5-feature matrix
//!   1:1 (no semantic-collapse possible without breaking the API contract)
//! - `missing_errors_doc` — every `pub fn` returning `Result` already
//!   documents the `CodegenError` variants in the function-level doc comment
//!
//! These mirror the analogous allow block in `nika-catalog/build.rs`
//! L11-20 (pre-extraction), which carried the same set with the same
//! rationale.

#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::struct_excessive_bools,
    clippy::missing_errors_doc,
    reason = "build-tool lint carve-out — see lib.rs top-level doc"
)]

mod capabilities;
mod embeddings;
mod emit;
mod error;
mod mcp_servers;
mod pricing;
mod providers;
mod schema;
mod tags;

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

// ── Re-exports — public API surface ─────────────────────────────────────

pub use crate::capabilities::codegen_capabilities;
pub use crate::embeddings::{
    EmbeddingEntry, EmbeddingsFile, codegen_embeddings, parse_embeddings_bytes,
};
pub use crate::error::CodegenError;
pub use crate::mcp_servers::{
    EnvVarEntry, McpServerEntry, McpServersFile, PackageEntry, RemoteEntry, codegen_mcp_servers,
    parse_mcp_servers_bytes,
};
pub use crate::pricing::{PricingEntry, PricingFile, codegen_pricing, parse_pricing_bytes};
pub use crate::providers::{
    LlmProvidersFile, ProviderEntry, ProviderModelEntry, codegen_providers,
    parse_llm_providers_bytes,
};
pub use crate::schema::{
    CAPABILITIES_SCHEMA, EMBEDDINGS_SCHEMA, LLM_PROVIDERS_SCHEMA, MCP_SERVERS_SCHEMA,
    PRICING_SCHEMA,
};
pub use crate::tags::TAG_VARIANT_COUNT;

// ── FeatureSet — which catalogs to emit ─────────────────────────────────

/// Enabled catalog features, typically built from cargo `CARGO_FEATURE_*`
/// env vars via [`FeatureSet::from_env`]. Public constructor exists so
/// unit tests can drive [`generate`] without going through env vars.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct FeatureSet {
    /// Emit `mcp_servers.rs` from `data/mcp-servers.toml`.
    pub mcp: bool,
    /// Emit `providers.rs` from `data/llm-providers.toml`.
    pub providers: bool,
    /// Emit `embeddings.rs` from `data/embeddings.toml`.
    /// Note: even when `providers = false`, providers are parsed
    /// internally for the embeddings FK check.
    pub embeddings: bool,
    /// Emit `model_capabilities.rs` from `data/model-capabilities.toml`.
    pub capabilities: bool,
    /// Emit `model_pricing.rs` from `data/model-pricing.toml`.
    pub pricing: bool,
}

impl FeatureSet {
    /// Empty set — all flags false. Use this in tests when you want to
    /// drive [`generate`] without emitting anything (smoke check).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build from cargo `CARGO_FEATURE_*` env vars. The matrix matches
    /// the legacy `nika-catalog/build.rs` exactly — `CARGO_FEATURE_MCP`
    /// → `mcp` field, etc.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            mcp: std::env::var("CARGO_FEATURE_MCP").is_ok(),
            providers: std::env::var("CARGO_FEATURE_PROVIDERS").is_ok(),
            embeddings: std::env::var("CARGO_FEATURE_EMBEDDINGS").is_ok(),
            capabilities: std::env::var("CARGO_FEATURE_CAPABILITIES").is_ok(),
            pricing: std::env::var("CARGO_FEATURE_PRICING").is_ok(),
        }
    }

    /// All flags on — useful for tests that want full-coverage emission
    /// from a single fixture directory.
    #[must_use]
    pub fn all() -> Self {
        Self {
            mcp: true,
            providers: true,
            embeddings: true,
            capabilities: true,
            pricing: true,
        }
    }
}

// ── Emitted — what `generate` produced ──────────────────────────────────

/// Summary of what [`generate`] actually wrote. Used by `build.rs` for
/// `cargo:rerun-if-changed` registration + by tests to assert emission.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Emitted {
    /// Generated Rust files written to `out_dir` (one per enabled feature).
    pub files: Vec<PathBuf>,
    /// Every TOML file read during emission. `build.rs` should emit
    /// `cargo:rerun-if-changed=<path>` for each entry so cargo
    /// re-invokes the build script when any catalog source changes.
    pub rerun_paths: Vec<PathBuf>,
}

impl Emitted {
    /// Empty result — no files emitted, no paths read.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

// ── Top-level orchestrator ──────────────────────────────────────────────

/// Parse every enabled TOML in `data_dir`, validate, and emit Rust source
/// into `out_dir`. Equivalent to the pre-extraction `nika-catalog/build.rs`
/// `run()` function.
///
/// # Errors
///
/// Returns [`CodegenError`] on the first failure encountered :
/// - `Io` — `data_dir` not readable, or `out_dir` not writable
/// - `TomlParse` — a `.toml` file failed to deserialise
/// - `SchemaMismatch` — file's `schema = "..."` doesn't match the canonical version
/// - `SchemaValidation` — a structural invariant failed (uniqueness,
///   vocab, sort-order, etc.)
/// - `ForeignKey` — embedding's `provider` or capability rule's
///   `scope.providers` references an undeclared id
pub fn generate(
    data_dir: &Path,
    out_dir: &Path,
    features: FeatureSet,
) -> Result<Emitted, CodegenError> {
    let mut emitted = Emitted::new();

    // Walk the data dir to register `cargo:rerun-if-changed` paths even
    // for files we don't end up emitting from. Matches legacy behavior :
    // a TOML edit triggers rebuild regardless of feature gate, so the
    // gate-flip itself produces a fresh emission.
    let read_dir = fs::read_dir(data_dir).map_err(|source| CodegenError::Io {
        path: data_dir.to_path_buf(),
        source,
    })?;
    for entry in read_dir {
        let entry = entry.map_err(|source| CodegenError::Io {
            path: data_dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.extension() == Some(OsStr::new("toml")) {
            emitted.rerun_paths.push(path);
        }
    }

    // Providers are parsed unconditionally when `providers` OR
    // `embeddings` OR `capabilities` is on — both downstream catalogs
    // need the canonical provider list for FK validation.
    let providers_path = data_dir.join("llm-providers.toml");
    let providers = if features.providers || features.embeddings || features.capabilities {
        let raw = read_file(&providers_path)?;
        parse_llm_providers_bytes(&raw, &providers_path)?
    } else {
        Vec::new()
    };

    if features.providers {
        let raw = read_file(&providers_path)?;
        let rust_src = codegen_providers(&raw)?;
        let out_path = out_dir.join("providers.rs");
        write_file(&out_path, &rust_src)?;
        emitted.files.push(out_path);
    }

    if features.mcp {
        let path = data_dir.join("mcp-servers.toml");
        let raw = read_file(&path)?;
        let rust_src = codegen_mcp_servers(&raw)?;
        let out_path = out_dir.join("mcp_servers.rs");
        write_file(&out_path, &rust_src)?;
        emitted.files.push(out_path);
    }

    if features.embeddings {
        let path = data_dir.join("embeddings.toml");
        let raw = read_file(&path)?;
        let rust_src = codegen_embeddings(&raw, &providers)?;
        let out_path = out_dir.join("embeddings.rs");
        write_file(&out_path, &rust_src)?;
        emitted.files.push(out_path);
    }

    if features.capabilities {
        let path = data_dir.join("model-capabilities.toml");
        let raw = read_file(&path)?;
        let rust_src = codegen_capabilities(&raw, &providers)?;
        let out_path = out_dir.join("model_capabilities.rs");
        write_file(&out_path, &rust_src)?;
        emitted.files.push(out_path);
    }

    if features.pricing {
        let path = data_dir.join("model-pricing.toml");
        let raw = read_file(&path)?;
        let rust_src = codegen_pricing(&raw)?;
        let out_path = out_dir.join("model_pricing.rs");
        write_file(&out_path, &rust_src)?;
        emitted.files.push(out_path);
    }

    Ok(emitted)
}

fn read_file(path: &Path) -> Result<Vec<u8>, CodegenError> {
    fs::read(path).map_err(|source| CodegenError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_file(path: &Path, contents: &str) -> Result<(), CodegenError> {
    fs::write(path, contents).map_err(|source| CodegenError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Gate 10 — parity vs legacy nika-catalog/build.rs output ────────
    //
    // These fixtures are byte-equal copies of the canonical TOML data
    // shipped in nika-catalog/data/, captured at the moment of codegen
    // extraction. Golden Rust outputs were captured from
    // `target/debug/build/nika-catalog-*/out/*.rs` after a clean
    // `cargo build -p nika-catalog --all-features` against HEAD
    // 611ccdf7e (pre-extraction baseline).
    //
    // Any drift here means the codegen output diverged from the legacy
    // build script — which is a Gate 10 BLOCKER.

    const FIXTURE_MCP: &[u8] = include_bytes!("../tests/fixtures/mcp-servers.toml");
    const FIXTURE_PROVIDERS: &[u8] = include_bytes!("../tests/fixtures/llm-providers.toml");
    const FIXTURE_EMBEDDINGS: &[u8] = include_bytes!("../tests/fixtures/embeddings.toml");
    const FIXTURE_CAPABILITIES: &[u8] = include_bytes!("../tests/fixtures/model-capabilities.toml");
    const FIXTURE_PRICING: &[u8] = include_bytes!("../tests/fixtures/model-pricing.toml");

    const GOLDEN_MCP: &str = include_str!("../tests/golden/mcp_servers.golden");
    const GOLDEN_PROVIDERS: &str = include_str!("../tests/golden/providers.golden");
    const GOLDEN_EMBEDDINGS: &str = include_str!("../tests/golden/embeddings.golden");
    const GOLDEN_CAPABILITIES: &str = include_str!("../tests/golden/model_capabilities.golden");
    const GOLDEN_PRICING: &str = include_str!("../tests/golden/model_pricing.golden");

    #[test]
    fn parity_mcp_servers_byte_equal() {
        let emitted = codegen_mcp_servers(FIXTURE_MCP).expect("mcp codegen failed");
        assert_eq!(
            emitted, GOLDEN_MCP,
            "mcp_servers codegen drifted from legacy build.rs output"
        );
    }

    #[test]
    fn parity_providers_byte_equal() {
        let emitted = codegen_providers(FIXTURE_PROVIDERS).expect("providers codegen failed");
        assert_eq!(
            emitted, GOLDEN_PROVIDERS,
            "providers codegen drifted from legacy build.rs output"
        );
    }

    #[test]
    fn parity_embeddings_byte_equal() {
        let providers = parse_llm_providers_bytes(
            FIXTURE_PROVIDERS,
            std::path::Path::new("<fixture>:llm-providers.toml"),
        )
        .expect("providers parse failed");
        let emitted =
            codegen_embeddings(FIXTURE_EMBEDDINGS, &providers).expect("embeddings codegen failed");
        assert_eq!(
            emitted, GOLDEN_EMBEDDINGS,
            "embeddings codegen drifted from legacy build.rs output"
        );
    }

    #[test]
    fn parity_capabilities_byte_equal() {
        let providers = parse_llm_providers_bytes(
            FIXTURE_PROVIDERS,
            std::path::Path::new("<fixture>:llm-providers.toml"),
        )
        .expect("providers parse failed");
        let emitted = codegen_capabilities(FIXTURE_CAPABILITIES, &providers)
            .expect("capabilities codegen failed");
        assert_eq!(
            emitted, GOLDEN_CAPABILITIES,
            "capabilities codegen drifted from legacy build.rs output"
        );
    }

    #[test]
    fn parity_pricing_byte_equal() {
        let emitted = codegen_pricing(FIXTURE_PRICING).expect("pricing codegen failed");
        assert_eq!(
            emitted, GOLDEN_PRICING,
            "pricing codegen drifted from legacy build.rs output"
        );
    }

    // ─── Gate 6 — property-based parse check ────────────────────────────
    //
    // Every emitted Rust source MUST be parseable as a valid `syn::File`.
    // We can't run name-resolution (the `crate::types::*` paths only
    // resolve when included from nika-catalog), but syntactic
    // well-formedness IS the load-bearing contract — if the emitted
    // source doesn't even parse, every downstream `include!()` site
    // fails to build.

    #[test]
    fn syn_parses_emitted_mcp_servers() {
        let emitted = codegen_mcp_servers(FIXTURE_MCP).expect("codegen failed");
        syn::parse_file(&emitted).expect("emitted mcp_servers.rs must parse as a valid syn::File");
    }

    #[test]
    fn syn_parses_emitted_providers() {
        let emitted = codegen_providers(FIXTURE_PROVIDERS).expect("codegen failed");
        syn::parse_file(&emitted).expect("emitted providers.rs must parse as a valid syn::File");
    }

    #[test]
    fn syn_parses_emitted_embeddings() {
        let providers = parse_llm_providers_bytes(
            FIXTURE_PROVIDERS,
            std::path::Path::new("<fixture>:llm-providers.toml"),
        )
        .expect("providers parse failed");
        let emitted = codegen_embeddings(FIXTURE_EMBEDDINGS, &providers).expect("codegen failed");
        syn::parse_file(&emitted).expect("emitted embeddings.rs must parse as a valid syn::File");
    }

    #[test]
    fn syn_parses_emitted_capabilities() {
        let providers = parse_llm_providers_bytes(
            FIXTURE_PROVIDERS,
            std::path::Path::new("<fixture>:llm-providers.toml"),
        )
        .expect("providers parse failed");
        let emitted =
            codegen_capabilities(FIXTURE_CAPABILITIES, &providers).expect("codegen failed");
        syn::parse_file(&emitted)
            .expect("emitted model_capabilities.rs must parse as a valid syn::File");
    }

    #[test]
    fn syn_parses_emitted_pricing() {
        let emitted = codegen_pricing(FIXTURE_PRICING).expect("codegen failed");
        syn::parse_file(&emitted)
            .expect("emitted model_pricing.rs must parse as a valid syn::File");
    }

    // ─── Idempotence (Gate 6 invariant) ────────────────────────────────
    //
    // Same TOML input + same arguments → byte-equal output. Required for
    // reproducible builds — any HashMap-derived ordering or random seed
    // would break this.

    #[test]
    fn idempotent_full_run_mcp() {
        let a = codegen_mcp_servers(FIXTURE_MCP).unwrap();
        let b = codegen_mcp_servers(FIXTURE_MCP).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn idempotent_full_run_capabilities() {
        let providers =
            parse_llm_providers_bytes(FIXTURE_PROVIDERS, std::path::Path::new("llm-providers"))
                .unwrap();
        let a = codegen_capabilities(FIXTURE_CAPABILITIES, &providers).unwrap();
        let b = codegen_capabilities(FIXTURE_CAPABILITIES, &providers).unwrap();
        assert_eq!(a, b);
    }

    // ─── Gate 6 — proptest invariants ──────────────────────────────────
    //
    // For random valid TOML inputs (string `id`s + ASCII chars only):
    // - codegen succeeds → output must be parseable as syn::File
    // - same input → same output (idempotence)

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config { cases: 200, .. proptest::test_runner::Config::default() })]

        #[test]
        fn proptest_emit_pricing_is_idempotent(
            provider in "[a-z]{3,8}",
            model in "[a-z]{3,8}-[a-z0-9]{2,6}",
            input_rate in 0.0_f64..1000.0,
            output_rate in 0.0_f64..1000.0,
        ) {
            let toml = format!(
                "schema = \"nika/model-pricing@1.0\"\n\n[[rules]]\nprovider = \"{provider}\"\nmodel_pattern = \"{model}\"\ninput_per_million = {input_rate}\noutput_per_million = {output_rate}\n"
            );
            let a = codegen_pricing(toml.as_bytes());
            let b = codegen_pricing(toml.as_bytes());
            proptest::prop_assert_eq!(&a.is_ok(), &b.is_ok());
            if let (Ok(a_src), Ok(b_src)) = (a, b) {
                proptest::prop_assert_eq!(&a_src, &b_src);
                // emitted source must parse as valid Rust
                proptest::prop_assert!(syn::parse_file(&a_src).is_ok());
            }
        }
    }

    #[test]
    fn parity_real_data_no_validation_errors() {
        // Smoke check : every real catalog parses + emits cleanly.
        let providers =
            parse_llm_providers_bytes(FIXTURE_PROVIDERS, std::path::Path::new("llm-providers"))
                .expect("providers must parse");
        assert!(!providers.is_empty(), "expected ≥1 provider in fixture");

        codegen_mcp_servers(FIXTURE_MCP).expect("mcp must emit");
        codegen_providers(FIXTURE_PROVIDERS).expect("providers must emit");
        codegen_embeddings(FIXTURE_EMBEDDINGS, &providers).expect("embeddings must emit");
        codegen_capabilities(FIXTURE_CAPABILITIES, &providers).expect("capabilities must emit");
        codegen_pricing(FIXTURE_PRICING).expect("pricing must emit");
    }

    #[test]
    fn feature_set_default_is_empty() {
        let f = FeatureSet::new();
        assert!(!f.mcp);
        assert!(!f.providers);
        assert!(!f.embeddings);
        assert!(!f.capabilities);
        assert!(!f.pricing);
    }

    #[test]
    fn feature_set_all_is_full() {
        let f = FeatureSet::all();
        assert!(f.mcp);
        assert!(f.providers);
        assert!(f.embeddings);
        assert!(f.capabilities);
        assert!(f.pricing);
    }

    #[test]
    fn emitted_default_is_empty() {
        let e = Emitted::new();
        assert!(e.files.is_empty());
        assert!(e.rerun_paths.is_empty());
    }

    #[test]
    fn generate_with_no_features_emits_nothing() {
        let tmp = tempdir();
        let data_dir = tmp.join("data");
        let out_dir = tmp.join("out");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&out_dir).unwrap();
        // Drop a stub TOML so read_dir succeeds.
        std::fs::write(data_dir.join("placeholder.toml"), "").unwrap();

        let emitted = generate(&data_dir, &out_dir, FeatureSet::new()).unwrap();
        assert!(emitted.files.is_empty(), "no features → no output files");
        assert_eq!(emitted.rerun_paths.len(), 1);
    }

    #[test]
    fn generate_returns_io_error_for_missing_dir() {
        let missing = std::path::PathBuf::from("/nonexistent-dir-xyz");
        let out = tempdir().join("out");
        std::fs::create_dir_all(&out).unwrap();
        let err = generate(&missing, &out, FeatureSet::new()).unwrap_err();
        assert!(matches!(err, CodegenError::Io { .. }));
    }

    /// Tiny tempdir helper — avoids pulling in the `tempfile` dev-dep
    /// for one test path. Cleaned up by the test's own end-of-scope
    /// (we're not enforcing strict cleanup; tests run in isolated
    /// processes with cargo's target dir).
    fn tempdir() -> std::path::PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = std::env::temp_dir().join(format!("nika-codegen-test-{pid}-{nanos}"));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
