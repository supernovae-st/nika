// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Schema version constants for every TOML catalog parsed by this crate.
//!
//! Every `data/<name>.toml` carries a top-level `schema = "nika/<name>@<version>"`
//! string. `assert_schema` hard-fails on mismatch so a backward-incompatible
//! TOML edit produces a clear error at build time rather than silently
//! deserialising into stale shapes.
//!
//! Bumping a version is how we signal a breaking change in the on-disk
//! format (fields removed, semantics changed). Additive changes do NOT
//! need a bump (FCI invariant — `#[non_exhaustive]` schema types).

use std::path::Path;

use crate::error::CodegenError;

/// Schema version for `data/mcp-servers.toml`.
pub const MCP_SERVERS_SCHEMA: &str = "nika/mcp-servers@1.0";

/// Schema version for `data/llm-providers.toml`.
pub const LLM_PROVIDERS_SCHEMA: &str = "nika/llm-providers@1.0";

/// Schema version for `data/embeddings.toml`.
pub const EMBEDDINGS_SCHEMA: &str = "nika/embeddings@1.0";

/// Schema version for `data/model-capabilities.toml`.
pub const CAPABILITIES_SCHEMA: &str = "nika/model-capabilities@1.0";

/// Schema version for `data/model-pricing.toml`.
///
/// `@1.1` (2026-07-07): required `[meta]` table — snapshot provenance
/// (`source` · `as_of` · `source_sha256_16`) promoted from TOML comments to
/// machine-readable fields, emitted as `PRICING_SNAPSHOT`.
pub const PRICING_SCHEMA: &str = "nika/model-pricing@1.1";

/// Hard-fail when the file's `schema = "..."` field doesn't match `expected`.
///
/// Returns the typed `CodegenError::SchemaMismatch` with the offending path,
/// expected canonical version, and the version the file declared.
pub(crate) fn assert_schema(
    expected: &'static str,
    got: &str,
    path: &Path,
) -> Result<(), CodegenError> {
    if got != expected {
        return Err(CodegenError::SchemaMismatch {
            path: path.to_path_buf(),
            expected,
            got: got.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn assert_schema_match() {
        let p = PathBuf::from("/tmp/mcp.toml");
        assert_schema(MCP_SERVERS_SCHEMA, "nika/mcp-servers@1.0", &p).unwrap();
    }

    #[test]
    fn assert_schema_mismatch_returns_typed_error() {
        let p = PathBuf::from("/tmp/mcp.toml");
        let e = assert_schema(MCP_SERVERS_SCHEMA, "nika/mcp-servers@0.9", &p).unwrap_err();
        match e {
            CodegenError::SchemaMismatch {
                path,
                expected,
                got,
            } => {
                assert_eq!(path, p);
                assert_eq!(expected, MCP_SERVERS_SCHEMA);
                assert_eq!(got, "nika/mcp-servers@0.9");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn schema_constants_have_canonical_shape() {
        for s in [
            MCP_SERVERS_SCHEMA,
            LLM_PROVIDERS_SCHEMA,
            EMBEDDINGS_SCHEMA,
            CAPABILITIES_SCHEMA,
            PRICING_SCHEMA,
        ] {
            assert!(
                s.starts_with("nika/"),
                "schema must start with 'nika/': {s}"
            );
            assert!(
                s.contains('@'),
                "schema must contain version separator '@': {s}"
            );
        }
    }

    #[test]
    fn mcp_schema_value_locked() {
        // Lock the exact string — any drift here breaks parity with
        // legacy nika-catalog/build.rs output (Gate 10).
        assert_eq!(MCP_SERVERS_SCHEMA, "nika/mcp-servers@1.0");
        assert_eq!(LLM_PROVIDERS_SCHEMA, "nika/llm-providers@1.0");
        assert_eq!(EMBEDDINGS_SCHEMA, "nika/embeddings@1.0");
        assert_eq!(CAPABILITIES_SCHEMA, "nika/model-capabilities@1.0");
        assert_eq!(PRICING_SCHEMA, "nika/model-pricing@1.1");
    }
}
