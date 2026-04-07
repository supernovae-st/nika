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

/// NovaNet MCP tools (7 tools from novanet-mcp server v0.22.0)
pub const NOVANET_TOOLS: &[McpToolDef] = &[
    McpToolDef {
        name: "novanet_describe",
        description: "Bootstrap understanding of the graph (schema, entity, category, relations, locales, stats)",
        params_snippet: r#"describe: "${1|schema,entity,category,relations,locales,stats|}"#,
    },
    McpToolDef {
        name: "novanet_search",
        description: "Find nodes via 5 modes: fulltext, property, hybrid, walk (graph traversal), triggers",
        params_snippet: r#"mode: "${1|fulltext,property,hybrid,walk,triggers|}"
query: "$2"
limit: ${3:20}"#,
    },
    McpToolDef {
        name: "novanet_introspect",
        description: "Query NodeClasses and ArcClasses with their relationships and properties",
        params_snippet: r#"target: "${1|classes,class,arcs,arc|}"
name: "$2"
include_arcs: ${3:true}"#,
    },
    McpToolDef {
        name: "novanet_context",
        description: "Unified context assembly for LLM content generation (page, block, knowledge, assemble modes)",
        params_snippet: r#"mode: "${1|page,block,knowledge,assemble|}"
focus_key: "$2"
locale: "${3:fr-FR}"
token_budget: ${4:4000}"#,
    },
    McpToolDef {
        name: "novanet_write",
        description: "Write or update nodes and arcs with schema validation (dry_run=true to preview)",
        params_snippet: r#"operation: "${1|upsert_node,create_arc,update_props|}"
class: "$2"
key: "$3"
properties: {$4}"#,
    },
    McpToolDef {
        name: "novanet_audit",
        description: "Post-write quality checks with CSR metrics (coverage, orphans, integrity, freshness)",
        params_snippet: r#"target: "${1|coverage,orphans,integrity,freshness,provenance,all|}"#,
    },
    McpToolDef {
        name: "novanet_batch",
        description: "Execute multiple MCP tool calls in a single request with parallel support",
        params_snippet: r#"operations:
  - tool: "$1"
    params: {$2}
parallel: ${3:true}"#,
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_novanet_tools_count() {
        assert_eq!(NOVANET_TOOLS.len(), 7);
    }

    #[test]
    fn test_novanet_tool_completions() {
        let completions = novanet_tool_completions();
        assert_eq!(completions.len(), 7);

        // Check first tool
        let first = &completions[0];
        assert_eq!(first.label, "novanet_describe");
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
        assert_eq!(completions.len(), 7);
    }

    #[test]
    fn test_mcp_tool_completions_unknown() {
        let completions = mcp_tool_completions("unknown_server");
        assert_eq!(completions.len(), 1);
        assert!(completions[0].label.contains("tool_name"));
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
        assert_eq!(completions.len(), 7);
    }
}
