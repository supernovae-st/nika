// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP server alias definitions.
//!
//! Decision B (locked): 11 ex-MCP "providers" from legacy migrate here with
//! optional `env_var` and `key_prefix` fields.

/// Pricing model for an MCP server's underlying service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
#[non_exhaustive]
pub enum McpPricing {
    /// Fully free / open source (no API key needed or free forever).
    Free,
    /// Free tier available, paid for higher usage.
    Freemium,
    /// Requires paid subscription / API key with charges.
    Paid,
}

/// A known MCP server alias with metadata.
///
/// Maps short names (e.g. `"neo4j"`) to npm packages (e.g. `"@neo4j/mcp-neo4j"`).
/// Ex-MCP providers gain `env_var` and `key_prefix` for secret management.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct McpAlias {
    /// Short alias name (e.g. `"neo4j"`).
    pub name: &'static str,
    /// npm package name (e.g. `"@neo4j/mcp-neo4j"`).
    pub package: &'static str,
    /// Category for CLI display grouping (e.g. `"databases"`).
    pub category: &'static str,
    /// Pricing model of the underlying service.
    pub pricing: McpPricing,
    /// Environment variable for API key (ex-MCP providers only).
    pub env_var: Option<&'static str>,
    /// Expected key prefix for format validation (ex-MCP providers only).
    pub key_prefix: Option<&'static str>,
}
