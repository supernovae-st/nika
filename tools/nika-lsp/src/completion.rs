//! Completion context detection for MCP discovery fallback.
//!
//! The main completion pipeline delegates to `nika-lsp-core`. This module
//! is retained ONLY for two functions used by backend.rs:
//! - `get_completion_context()` — determines MCP-specific context
//! - `extract_mcp_servers()` — extracts server names from mcp: blocks

use tower_lsp_server::ls_types::Position;

use crate::document::DocumentState;
use crate::node_context::{find_context_at_position, AstContext};

/// Completion context for MCP-specific completion.
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionContext {
    /// MCP server reference (in mcp: config block or invoke: mcp field)
    McpServer,
    /// MCP tool reference (after mcp: server is specified)
    McpTool { server: String },
    /// Any other context (handled by nika-lsp-core)
    Other,
}

/// Analyze the document position to determine if MCP completion is needed.
///
/// Returns `McpServer` or `McpTool` for MCP-specific contexts,
/// `Other` for everything else (delegated to nika-lsp-core).
pub fn get_completion_context(doc: &DocumentState, position: Position) -> CompletionContext {
    let content = doc.content();
    let ast_result = find_context_at_position(&content, position.line, position.character);

    match &ast_result.context {
        AstContext::McpConfig { .. } => CompletionContext::McpServer,
        AstContext::InvokeBlock {
            mcp_server: Some(server),
            ..
        } => CompletionContext::McpTool {
            server: server.clone(),
        },
        AstContext::InvokeBlock {
            mcp_server: None, ..
        } => CompletionContext::McpServer,
        _ => {
            // Fallback: line-based detection for mcp: context
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
///
/// Parses the YAML looking for `mcp:` block with server definitions.
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
        let servers = extract_mcp_servers("tasks:\n  - id: step1\n");
        assert!(servers.is_empty());
    }

    #[test]
    fn extract_mcp_servers_no_mcp_block() {
        let servers = extract_mcp_servers("schema: v0.12\nworkflow: test\n");
        assert!(servers.is_empty());
    }
}
