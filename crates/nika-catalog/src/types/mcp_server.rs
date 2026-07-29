// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP server catalog entry.
//!
//! An `McpServer` is either local-install (one or more [`super::McpPackage`])
//! or remote (one or more [`super::McpRemote`]) — or both. The build-time
//! invariant `packages.len() + remotes.len() >= 1` is enforced in `build.rs`.

use super::{Category, EnvVarSpec, McpPackage, McpRemote};

/// Pricing model for an MCP server's underlying service.
///
/// Mirrors the small, closed set used by CLI UX (`nika mcp list` colouring,
/// `nika mcp add` prompts). `#[non_exhaustive]` for additive variants.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum McpPricing {
    /// Fully free / open source (no API key needed or free forever).
    Free,
    /// Free tier available, paid for higher usage.
    Freemium,
    /// Requires paid subscription / API key with charges.
    Paid,
}

/// Per-tool effect hints, vendored from a server's manifest (schema `@1.1`).
///
/// The MCP spec lets a server annotate each tool with `readOnlyHint` and
/// `destructiveHint`. Server-level tags (`read-only` XOR `destructive` on
/// [`McpServer::tags`]) classify the WHOLE server — one write tool makes
/// the server destructive — but irreversibility reasoning needs the
/// per-tool grain: a server can expose ninety read tools and one delete.
///
/// Each hint is `Option<bool>`: `None` = the manifest did not annotate,
/// which MUST NOT be read as "safe" (the spec's own defaults assume the
/// worst, and so should a consumer). These are HINTS from the server
/// author, not measurements — trust them for UX and check-time
/// reasoning, never as a security boundary.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ToolEffectHints {
    /// Tool name as the server declares it (e.g. `"query"`).
    pub tool: &'static str,
    /// MCP `readOnlyHint` — the tool claims to mutate nothing.
    /// `None` = not annotated in the manifest.
    pub read_only: Option<bool>,
    /// MCP `destructiveHint` — the tool may destroy or irreversibly
    /// change data. `None` = not annotated in the manifest.
    pub destructive: Option<bool>,
}

impl ToolEffectHints {
    /// Explicit constructor — required because [`ToolEffectHints`] is
    /// `#[non_exhaustive]` (invariant #19) and the generated static is
    /// const-constructed.
    #[must_use]
    pub const fn new(
        tool: &'static str,
        read_only: Option<bool>,
        destructive: Option<bool>,
    ) -> Self {
        Self {
            tool,
            read_only,
            destructive,
        }
    }
}

/// Rich MCP server catalog entry.
///
/// Generated into `$OUT_DIR/mcp_servers.rs` by the crate's `build.rs`
/// from `data/mcp-servers.toml`. All fields are `&'static` — every entry is
/// embedded in the binary at compile time.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Typed tags describing MCP server capabilities and permissioning.
    /// Every MCP server MUST carry `Tag::ReadOnly` XOR `Tag::Destructive`.
    /// Sorted + deduplicated (enforced by build.rs assertion).
    pub tags: &'static [crate::types::Tag],
    /// Community / vendor-specific tags not in the core [`Tag`](crate::types::Tag) enum.
    pub extra_tags: &'static [&'static str],
    /// Per-tool effect hints vendored from the server manifest (schema
    /// `@1.1`), sorted by tool name. Empty = no manifest annotations
    /// vendored — which says nothing about what the tools can do.
    pub effect_hints: &'static [ToolEffectHints],
}

impl McpServer {
    /// Explicit constructor — required because [`McpServer`] is
    /// `#[non_exhaustive]` (invariant #19).
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    // REASON: 14 fields reflect the real MCP-registry wire surface
    // (id + aliases + title + description + packages + remotes +
    // env_vars + homepage + category + pricing + last_verified + tags +
    // extra_tags + effect_hints).
    // A builder would fragment one catalog row for zero readability win.
    pub const fn new(
        id: &'static str,
        aliases: &'static [&'static str],
        title: &'static str,
        description: &'static str,
        packages: &'static [McpPackage],
        remotes: &'static [McpRemote],
        env_vars: &'static [EnvVarSpec],
        homepage: Option<&'static str>,
        category: Category,
        pricing: McpPricing,
        last_verified: &'static str,
        tags: &'static [crate::types::Tag],
        extra_tags: &'static [&'static str],
        effect_hints: &'static [ToolEffectHints],
    ) -> Self {
        Self {
            id,
            aliases,
            title,
            description,
            packages,
            remotes,
            env_vars,
            homepage,
            category,
            pricing,
            last_verified,
            tags,
            extra_tags,
            effect_hints,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const _: () = {
        const fn assert_send_sync<T: Send + Sync>() {}
        // McpServer is intentionally not Copy — 11 fields with slices
        // (~200 bytes on 64-bit). Always pass by reference.
        assert_send_sync::<McpServer>();
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
            tags: &[],
            extra_tags: &[],
            effect_hints: &[],
        };
        assert_eq!(s.id, "filesystem");
        assert_eq!(s.category, Category::Anthropic);
        assert!(s.effect_hints.is_empty(), "no manifest vendored");
    }

    #[test]
    fn tool_effect_hints_new_builds_all_fields() {
        const H: ToolEffectHints = ToolEffectHints::new("query", Some(true), None);
        assert_eq!(H.tool, "query");
        assert_eq!(H.read_only, Some(true));
        assert_eq!(
            H.destructive, None,
            "unannotated must stay None, never false"
        );
    }
}
