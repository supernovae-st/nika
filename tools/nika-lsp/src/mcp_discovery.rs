//! MCP Tool Discovery for LSP Completions
//!
//! Provides intelligent completions for MCP server names and tool names
//! based on workflow definitions and static knowledge of common MCP servers.

use tower_lsp_server::ls_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
};

/// Known MCP tool definition with metadata for completions
#[derive(Debug, Clone)]
pub struct McpToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub params_snippet: &'static str,
}

/// NovaNet MCP tools (12 tools from novanet-mcp server)
pub const NOVANET_TOOLS: &[McpToolDef] = &[
    McpToolDef {
        name: "novanet_query",
        description: "Execute a raw Cypher query against the knowledge graph",
        params_snippet: r#"cypher: "$1"
params: {$2}"#,
    },
    McpToolDef {
        name: "novanet_describe",
        description: "Get detailed description of a node or arc class from the schema",
        params_snippet: r#"class: "$1"
type: "${2|node,arc|}"#,
    },
    McpToolDef {
        name: "novanet_search",
        description: "Full-text search across the knowledge graph",
        params_snippet: r#"query: "$1"
limit: ${2:10}"#,
    },
    McpToolDef {
        name: "novanet_traverse",
        description: "Traverse the graph from a starting node following arc patterns",
        params_snippet: r#"start: "$1"
arc: "${2:HAS_NATIVE}"
depth: ${3:1}"#,
    },
    McpToolDef {
        name: "novanet_assemble",
        description: "Assemble context for an entity with all related content",
        params_snippet: r#"entity: "$1"
locale: "${2:en-US}"
include: [${3:"native", "terms", "expressions"}]"#,
    },
    McpToolDef {
        name: "novanet_atoms",
        description: "Retrieve knowledge atoms (terms, expressions, patterns) for an entity",
        params_snippet: r#"entity: "$1"
locale: "${2:en-US}"
types: [${3:"terms", "expressions"}]"#,
    },
    McpToolDef {
        name: "novanet_generate",
        description: "Generate denomination forms and LLM context for an entity",
        params_snippet: r#"entity: "$1"
locale: "${2:en-US}"
forms: [${3:"text", "title", "abbrev"}]"#,
    },
    McpToolDef {
        name: "novanet_introspect",
        description: "Introspect the schema to discover node classes, arc classes, and properties",
        params_snippet: r#"node_class: "$1"
include: [${2:"arcs", "properties", "constraints"}]"#,
    },
    McpToolDef {
        name: "novanet_batch",
        description: "Execute multiple MCP tool calls in a single request",
        params_snippet: r#"calls:
  - tool: "$1"
    params: {$2}"#,
    },
    McpToolDef {
        name: "novanet_cache_stats",
        description: "Get cache statistics for the MCP server",
        params_snippet: "",
    },
    McpToolDef {
        name: "novanet_cache_invalidate",
        description: "Invalidate cache entries matching a pattern",
        params_snippet: r#"pattern: "$1""#,
    },
    McpToolDef {
        name: "novanet_write",
        description: "Write or update nodes and relationships in the knowledge graph",
        params_snippet: r#"operation: "${1|create,update,delete|}"
node_class: "$2"
properties: {$3}"#,
    },
];

/// Get completions for MCP server names defined in a workflow
pub fn mcp_server_completions(server_names: &[String]) -> Vec<CompletionItem> {
    server_names
        .iter()
        .map(|name| CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some("MCP Server".to_string()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("MCP server `{}` defined in this workflow", name),
            })),
            ..Default::default()
        })
        .collect()
}

/// Get completions for MCP tools based on server name
pub fn mcp_tool_completions(server_name: &str) -> Vec<CompletionItem> {
    // For now, we only have static knowledge of NovaNet tools
    // In the future, this could connect to MCP servers dynamically
    match server_name {
        "novanet" | "nova" | "novanet-mcp" => novanet_tool_completions(),
        _ => {
            // Unknown server - return generic completion hint
            vec![CompletionItem {
                label: "<tool_name>".to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(format!("Tool from {} server", server_name)),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!(
                        "Enter the name of an MCP tool from the `{}` server.\n\n\
                         Use `invoke:` with `mcp:` and `tool:` to call MCP tools.",
                        server_name
                    ),
                })),
                ..Default::default()
            }]
        }
    }
}

/// Get completions for all NovaNet MCP tools
pub fn novanet_tool_completions() -> Vec<CompletionItem> {
    NOVANET_TOOLS
        .iter()
        .map(|tool| {
            let insert_text = if tool.params_snippet.is_empty() {
                tool.name.to_string()
            } else {
                format!("{}\n    params:\n      {}", tool.name, tool.params_snippet.replace('\n', "\n      "))
            };

            CompletionItem {
                label: tool.name.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some("NovaNet MCP Tool".to_string()),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!(
                        "**{}**\n\n{}\n\n```yaml\ninvoke:\n  mcp: novanet\n  tool: {}\n  params:\n    ...\n```",
                        tool.name, tool.description, tool.name
                    ),
                })),
                insert_text: Some(insert_text),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            }
        })
        .collect()
}

/// Get completions for invoke: block based on context
pub fn invoke_completions(
    server_names: &[String],
    partial_server: Option<&str>,
) -> Vec<CompletionItem> {
    let mut completions = Vec::new();

    match partial_server {
        // User is typing server name (mcp: <cursor>)
        None => {
            completions.extend(mcp_server_completions(server_names));
        }
        // User has selected a server, now show tools (tool: <cursor>)
        Some(server) => {
            completions.extend(mcp_tool_completions(server));
        }
    }

    completions
}

/// Check if a string looks like a NovaNet server name.
///
/// Used for determining which tool completions to provide.
#[allow(dead_code)] // Public API for future use
pub fn is_novanet_server(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "novanet" | "nova" | "novanet-mcp" | "novanet_mcp"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_novanet_tools_count() {
        assert_eq!(NOVANET_TOOLS.len(), 12);
    }

    #[test]
    fn test_novanet_tool_completions() {
        let completions = novanet_tool_completions();
        assert_eq!(completions.len(), 12);

        // Check first tool
        let first = &completions[0];
        assert_eq!(first.label, "novanet_query");
        assert_eq!(first.kind, Some(CompletionItemKind::FUNCTION));
    }

    #[test]
    fn test_mcp_server_completions() {
        let servers = vec!["novanet".to_string(), "custom".to_string()];
        let completions = mcp_server_completions(&servers);
        assert_eq!(completions.len(), 2);
        assert_eq!(completions[0].label, "novanet");
        assert_eq!(completions[1].label, "custom");
    }

    #[test]
    fn test_mcp_tool_completions_novanet() {
        let completions = mcp_tool_completions("novanet");
        assert_eq!(completions.len(), 12);
    }

    #[test]
    fn test_mcp_tool_completions_unknown() {
        let completions = mcp_tool_completions("unknown_server");
        assert_eq!(completions.len(), 1);
        assert!(completions[0].label.contains("tool_name"));
    }

    #[test]
    fn test_is_novanet_server() {
        assert!(is_novanet_server("novanet"));
        assert!(is_novanet_server("NovaNet"));
        assert!(is_novanet_server("novanet-mcp"));
        assert!(!is_novanet_server("custom"));
    }

    #[test]
    fn test_invoke_completions_no_server() {
        let servers = vec!["novanet".to_string()];
        let completions = invoke_completions(&servers, None);
        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].label, "novanet");
    }

    #[test]
    fn test_invoke_completions_with_server() {
        let servers = vec!["novanet".to_string()];
        let completions = invoke_completions(&servers, Some("novanet"));
        assert_eq!(completions.len(), 12);
    }
}
