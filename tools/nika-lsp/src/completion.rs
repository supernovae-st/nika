// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MCP completion fallback.
//!
//! Main completion delegates to nika-lsp-core. This module handles
//! MCP-specific context detection and server name extraction.

use tower_lsp_server::ls_types::Position;

use crate::document::DocumentState;

/// MCP-specific completion context.
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionContext {
    McpServer,
    McpTool { server: String },
    Other,
}

/// Detect if cursor is in an MCP-relevant context.
pub fn get_completion_context(doc: &DocumentState, position: Position) -> CompletionContext {
    let content = doc.content();
    let offset = crate::position::position_to_offset(&content, position.line, position.character)
        .map(|o| o.0)
        .unwrap_or(0);
    let ctx = nika_lsp_core::analysis::context::detect_context(&content, offset, None);

    match &ctx {
        nika_lsp_core::analysis::context::CursorContext::McpConfig { .. } => {
            CompletionContext::McpServer
        }
        nika_lsp_core::analysis::context::CursorContext::InvokeBlock {
            mcp_server: Some(server),
            ..
        } => CompletionContext::McpTool {
            server: server.clone(),
        },
        nika_lsp_core::analysis::context::CursorContext::InvokeBlock {
            mcp_server: None, ..
        } => CompletionContext::McpServer,
        _ => {
            // Fallback: line-based MCP detection
            let lines: Vec<&str> = content.lines().collect();
            if (position.line as usize) < lines.len() {
                let trimmed = lines[position.line as usize].trim();
                if trimmed.starts_with("mcp:") || trimmed.starts_with("server:") {
                    return CompletionContext::McpServer;
                }
            }
            CompletionContext::Other
        }
    }
}

/// Extract MCP server names from the document content.
pub fn extract_mcp_servers(content: &str) -> Vec<String> {
    let mut servers = Vec::new();
    let mut in_mcp_block = false;
    let mut mcp_indent = 0;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("mcp:") {
            in_mcp_block = true;
            mcp_indent = line.len() - line.trim_start().len();
            continue;
        }

        if in_mcp_block {
            let current_indent = line.len() - line.trim_start().len();

            if !trimmed.is_empty() && current_indent <= mcp_indent {
                in_mcp_block = false;
                continue;
            }

            if current_indent == mcp_indent + 2 && trimmed.ends_with(':') && !trimmed.contains(' ')
            {
                let server_name = trimmed.trim_end_matches(':').to_string();
                if !server_name.is_empty() {
                    servers.push(server_name);
                }
            }
        }
    }

    servers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_mcp_servers_basic() {
        let content = "mcp:\n  novanet:\n    command: node\n  perplexity:\n    command: perp\n";
        let servers = extract_mcp_servers(content);
        assert_eq!(servers, vec!["novanet", "perplexity"]);
    }

    #[test]
    fn extract_mcp_servers_empty() {
        assert!(extract_mcp_servers("tasks:\n  - id: step1\n").is_empty());
    }
}
