// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Public lookup API — thin wrappers over data module functions.
//!
//! All lookups return `Option`, not `NikaError`. The catalog answers
//! "is this known?", the caller decides if "unknown" is an error.

pub use crate::data::builtins::{find_builtin, is_known_builtin};
pub use crate::data::generated::{find_mcp_server, find_provider};
pub use crate::data::models::{estimate_cost, find_pricing, find_pricing_scoped, model_capabilities};
pub use crate::data::transforms::{find_transform, is_known_transform};
pub use crate::types::provider::validate_key_format;

/// Resolve a name that might be an MCP server alias or a package path.
///
/// If the name is a known MCP server id/alias, returns the npm package
/// identifier. If the server is remote-only or non-npm, returns `None`
/// (callers should use `find_mcp_server` for richer introspection).
/// If it looks like a package name (contains `/` or starts with `@`),
/// returns as-is (passthrough).
#[must_use]
pub fn resolve_mcp_name(name: &str) -> Option<String> {
    if name.starts_with('@') || name.contains('/') {
        return Some(name.to_string());
    }
    let server = find_mcp_server(name)?;
    server
        .packages
        .iter()
        .find(|p| matches!(p.registry_type, crate::types::RegistryType::Npm))
        .map(|p| p.identifier.to_string())
}

/// Check whether `name` resolves to a known MCP server id or alias
/// (case-insensitive).
#[must_use]
pub fn is_known_mcp_server(name: &str) -> bool {
    find_mcp_server(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_mcp_name_alias() {
        let result = resolve_mcp_name("neo4j");
        assert_eq!(result, Some("@johnymontana/neo4j-mcp".to_string()));
    }

    #[test]
    fn resolve_mcp_name_package_passthrough_at_prefix() {
        let result = resolve_mcp_name("@scoped-pkg");
        assert_eq!(result, Some("@scoped-pkg".to_string()));
    }

    #[test]
    fn resolve_mcp_name_package_passthrough_slash() {
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
        let result = resolve_mcp_name("nonexistent-server-xyz");
        assert_eq!(result, None);
    }

    #[test]
    fn provider_lookup() {
        assert!(find_provider("Anthropic").is_some());
        assert!(find_provider("claude").is_some());
    }

    #[test]
    fn provider_unknown() {
        assert!(find_provider("ollama").is_none());
    }

    #[test]
    fn remote_only_server_has_no_npm_resolution() {
        // intercom is remote-only (streamable-http + OAuth, no npm package).
        let result = resolve_mcp_name("intercom");
        assert_eq!(result, None);
    }

    #[test]
    fn is_known_mcp_server_positive() {
        assert!(is_known_mcp_server("neo4j"));
        assert!(is_known_mcp_server("filesystem"));
    }

    #[test]
    fn is_known_mcp_server_negative() {
        assert!(!is_known_mcp_server("nonexistent"));
        assert!(!is_known_mcp_server(""));
    }
}
