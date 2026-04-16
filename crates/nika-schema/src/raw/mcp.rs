// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Raw MCP server configuration — the `mcp:` section at workflow root.

use crate::source::Spanned;

/// Raw MCP configuration — a map of server alias → server config.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawMcpConfig {
    /// MCP server entries keyed by alias.
    pub servers: Vec<RawMcpServer>,
}

impl RawMcpConfig {
    /// Create a new empty MCP config.
    #[must_use]
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
        }
    }
}

impl Default for RawMcpConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// A single MCP server entry in the workflow config.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawMcpServer {
    /// Server alias (used in `invoke` tasks via `mcp: alias`).
    pub alias: Spanned<String>,
    /// Server command (for stdio transport).
    pub command: Option<Spanned<String>>,
    /// Command arguments.
    pub args: Vec<Spanned<String>>,
    /// Environment variables for the server process.
    pub env: Vec<(Spanned<String>, Spanned<String>)>,
    /// URL (for SSE/HTTP transport).
    pub url: Option<Spanned<String>>,
    /// Working directory for the server.
    pub cwd: Option<Spanned<String>>,
}

impl RawMcpServer {
    /// Create a new MCP server with the given alias.
    #[must_use]
    pub fn new(alias: Spanned<String>) -> Self {
        Self {
            alias,
            command: None,
            args: Vec::new(),
            env: Vec::new(),
            url: None,
            cwd: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Span;

    #[test]
    fn mcp_config_empty() {
        let cfg = RawMcpConfig::new();
        assert!(cfg.servers.is_empty());
    }

    #[test]
    fn mcp_server_new() {
        let alias = Spanned {
            value: "github".into(),
            span: Span::default(),
        };
        let server = RawMcpServer::new(alias);
        assert_eq!(server.alias.value, "github");
        assert!(server.command.is_none());
        assert!(server.args.is_empty());
        assert!(server.url.is_none());
    }
}
