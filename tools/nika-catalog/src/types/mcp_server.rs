// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP server catalog entry (v3 architecture).
//!
//! Target type for the TOML-driven `build.rs` codegen in Phase C. Coexists
//! with the legacy [`super::McpAlias`] during the migration; Phase C step 6
//! flips all consumers over and retires the alias type.
//!
//! An `McpServer` is either local-install (one or more [`super::McpPackage`])
//! or remote (one or more [`super::McpRemote`]) — or both. The build-time
//! invariant `packages.len() + remotes.len() >= 1` is enforced in `build.rs`.

use super::{Category, EnvVarSpec, McpPackage, McpPricing, McpRemote};

/// Rich MCP server catalog entry.
///
/// Generated into `$OUT_DIR/mcp_servers.rs` by the crate's `build.rs`
/// from `data/mcp-servers.toml`. All fields are `&'static` — every entry is
/// embedded in the binary at compile time.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct McpServer {
    /// Canonical identifier (e.g. `"filesystem"`, `"qdrant"`).
    pub id: &'static str,
    /// Additional aliases that resolve to this entry (case-insensitive).
    pub aliases: &'static [&'static str],
    /// Human-readable title (e.g. `"Qdrant"`).
    pub title: &'static str,
    /// One-line description for CLI / `nika list` output.
    pub description: &'static str,
    /// Local-install packages (may be empty when the server is remote-only).
    pub packages: &'static [McpPackage],
    /// Remote HTTP/SSE endpoints (may be empty when the server is local-only).
    pub remotes: &'static [McpRemote],
    /// Environment variables consumed by this server (any distribution).
    pub env_vars: &'static [EnvVarSpec],
    /// Optional homepage URL.
    pub homepage: Option<&'static str>,
    /// Category used for CLI display grouping.
    pub category: Category,
    /// Pricing model of the underlying service.
    pub pricing: McpPricing,
    /// ISO date (YYYY-MM-DD) of the last `xtask verify-catalog` success.
    pub last_verified: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    const _: () = {
        const fn assert_copy_send_sync<T: Copy + Send + Sync>() {}
        assert_copy_send_sync::<McpServer>();
    };

    #[test]
    fn construct_server_with_package() {
        const PKGS: &[McpPackage] = &[];
        const REMOTES: &[McpRemote] = &[];
        const ENV: &[EnvVarSpec] = &[];
        let s = McpServer {
            id: "filesystem",
            aliases: &[],
            title: "Filesystem",
            description: "Read/write local files.",
            packages: PKGS,
            remotes: REMOTES,
            env_vars: ENV,
            homepage: None,
            category: Category::Anthropic,
            pricing: McpPricing::Free,
            last_verified: "2026-04-14",
        };
        assert_eq!(s.id, "filesystem");
        assert_eq!(s.category, Category::Anthropic);
    }
}
