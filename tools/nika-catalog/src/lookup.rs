// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Public lookup API — thin wrappers over data module functions.
//!
//! All lookups return `Option`, not `NikaError`. The catalog answers
//! "is this known?", the caller decides if "unknown" is an error.

pub use crate::data::builtins::{find_builtin, is_known_builtin};
pub use crate::data::mcp_aliases::{find_mcp_alias, is_known_mcp_alias};
pub use crate::data::models::{estimate_cost, find_pricing, model_capabilities};
pub use crate::data::providers::{find_provider, validate_key_format};
pub use crate::data::transforms::{find_transform, is_known_transform};

/// Resolve a name that might be a provider alias or a package path.
///
/// Used by the MCP subsystem to resolve `nika mcp add <name>`.
/// If the name is a known alias, returns the npm package.
/// If it looks like a package name (contains `/` or starts with `@`), returns as-is.
/// Otherwise returns `None`.
#[must_use]
pub fn resolve_mcp_name(name: &str) -> Option<String> {
    if name.starts_with('@') || name.contains('/') {
        return Some(name.to_string());
    }
    find_mcp_alias(name).map(|a| a.package.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_mcp_name_alias() {
        let result = resolve_mcp_name("neo4j");
        assert_eq!(result, Some("@neo4j/mcp-neo4j".to_string()));
    }

    #[test]
    fn resolve_mcp_name_package_passthrough_at_prefix() {
        // Starts with @ but no / — should still pass through
        let result = resolve_mcp_name("@scoped-pkg");
        assert_eq!(result, Some("@scoped-pkg".to_string()));
    }

    #[test]
    fn resolve_mcp_name_package_passthrough_slash() {
        // Contains / but no @ — should still pass through
        let result = resolve_mcp_name("some-org/server");
        assert_eq!(result, Some("some-org/server".to_string()));
    }

    #[test]
    fn resolve_mcp_name_package_passthrough_both() {
        let result = resolve_mcp_name("@custom/server");
        assert_eq!(result, Some("@custom/server".to_string()));
    }

    #[test]
    fn resolve_mcp_name_unknown() {
        assert!(resolve_mcp_name("unknown-thing").is_none());
    }

    #[test]
    fn lookup_integration() {
        // Provider (case-insensitive)
        assert!(find_provider("Anthropic").is_some());
        assert!(find_provider("claude").is_some());

        // Builtin (case-sensitive)
        assert!(find_builtin("sleep").is_some());
        assert!(find_builtin("Sleep").is_none());

        // Transform (case-sensitive)
        assert!(find_transform("upper").is_some());
        assert!(find_transform("Upper").is_none());

        // MCP alias (case-insensitive)
        assert!(find_mcp_alias("Neo4j").is_some());

        // Pricing (2-pass)
        assert!(find_pricing("claude-sonnet-4-20250514").is_some());

        // Unknown → None for all
        assert!(find_provider("ollama").is_none());
        assert!(find_builtin("nonexistent").is_none());
        assert!(find_transform("pad").is_none());
        assert!(find_mcp_alias("nonexistent").is_none());
        assert!(find_pricing("random-model").is_none());
    }
}
