// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Typed error surface for `nika-catalog-codegen`.
//!
//! No `Box<dyn Error>` (vector 27 / FCI-019) — every fallible path through
//! the public API surfaces a `CodegenError` variant. Internal helpers may
//! still return `Result<T, String>` during the lift from
//! `nika-catalog/build.rs` ; the public boundary maps `String` →
//! `CodegenError::SchemaValidation { context, reason }` at the seam.

use std::path::PathBuf;

/// Codegen failure surface.
///
/// `#[non_exhaustive]` per FCI-002 — adding a variant in a future session
/// (e.g. a region-resolver wiring at Phase E2) is additive, not breaking.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum CodegenError {
    /// Filesystem I/O failure (read or write).
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    /// TOML parse failure on a known catalog file.
    #[error("{path}: TOML parse error: {source}")]
    TomlParse {
        path: PathBuf,
        source: toml::de::Error,
    },

    /// `schema = "..."` field doesn't match the expected canonical version.
    #[error("{path}: schema mismatch — file declares {got:?}, expected {expected:?}")]
    SchemaMismatch {
        path: PathBuf,
        expected: &'static str,
        got: String,
    },

    /// Generic schema validation failure (uniqueness, FK violation, vocab,
    /// invariant — anything caught after parse but before emit).
    #[error("{context}: {reason}")]
    SchemaValidation { context: String, reason: String },

    /// Foreign-key violation (e.g. embedding's `provider` not declared in
    /// llm-providers.toml). Distinct variant for tooling that wants to
    /// surface this class specifically.
    #[error("{context}: foreign-key violation — {reason}")]
    ForeignKey { context: String, reason: String },

    /// Cargo `CARGO_MANIFEST_DIR` / `OUT_DIR` env var missing — typically
    /// only happens when a caller invokes `generate()` outside a `build.rs`
    /// context.
    #[error("cargo env var {name} missing: {source}")]
    EnvVar {
        name: &'static str,
        source: std::env::VarError,
    },
}

impl CodegenError {
    /// Convenience for the common `String → SchemaValidation` lift used at
    /// the public boundary of every per-catalog `codegen_*` function.
    #[must_use]
    pub fn schema_validation(context: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::SchemaValidation {
            context: context.into(),
            reason: reason.into(),
        }
    }

    /// Convenience for the common `String → ForeignKey` lift.
    #[must_use]
    pub fn foreign_key(context: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::ForeignKey {
            context: context.into(),
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_schema_mismatch() {
        let e = CodegenError::SchemaMismatch {
            path: PathBuf::from("/tmp/x.toml"),
            expected: "nika/mcp-servers@1.0",
            got: "nika/mcp-servers@0.9".to_string(),
        };
        let s = format!("{e}");
        assert!(s.contains("schema mismatch"));
        assert!(s.contains("nika/mcp-servers@1.0"));
        assert!(s.contains("nika/mcp-servers@0.9"));
    }

    #[test]
    fn display_io_chain() {
        let e = CodegenError::Io {
            path: PathBuf::from("/tmp/missing.toml"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"),
        };
        let s = format!("{e}");
        assert!(s.contains("/tmp/missing.toml"));
        assert!(s.contains("no such file"));
    }

    #[test]
    fn schema_validation_constructor() {
        let e = CodegenError::schema_validation("rule \"x\"", "field y missing");
        match e {
            CodegenError::SchemaValidation { context, reason } => {
                assert_eq!(context, "rule \"x\"");
                assert_eq!(reason, "field y missing");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn foreign_key_constructor() {
        let e = CodegenError::foreign_key("embedding \"a\"", "provider \"b\" not declared");
        match e {
            CodegenError::ForeignKey { context, reason } => {
                assert_eq!(context, "embedding \"a\"");
                assert_eq!(reason, "provider \"b\" not declared");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn source_chain_preserves_io_error() {
        use std::error::Error as _;

        let io = std::io::Error::other("boom");
        let e = CodegenError::Io {
            path: PathBuf::from("/x"),
            source: io,
        };
        assert!(e.source().is_some());
    }

    #[test]
    fn env_var_variant_carries_name() {
        let e = CodegenError::EnvVar {
            name: "OUT_DIR",
            source: std::env::VarError::NotPresent,
        };
        let s = format!("{e}");
        assert!(s.contains("OUT_DIR"));
    }
}
