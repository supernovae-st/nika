//! Completion provider for Nika workflows.
//!
//! Provides intelligent autocompletion for:
//! - Verbs (infer, exec, fetch, invoke, agent)
//! - Task IDs in use: blocks
//! - Schema versions
//! - MCP server names
//! - Common parameters

use tower_lsp_server::ls_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, Documentation,
    InsertTextFormat, MarkupContent, MarkupKind, Position,
};

use crate::document::DocumentState;
// MCP discovery functions used by backend.rs
use crate::node_context::{find_context_at_position, AstContext};

/// Completion context for determining what to complete.
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionContext {
    /// At the start of a task definition (after `- id:`)
    TaskVerb,
    /// Inside a use: block (task ID reference)
    UseReference { partial: String },
    /// Workflow schema field (schema: "nika/workflow@0.9")
    Schema,
    /// Structured output JSON Schema (inside output: or structured: blocks)
    StructuredSchema,
    /// MCP server reference (in mcp: config block or invoke: mcp field)
    McpServer,
    /// MCP tool reference (after mcp: server is specified)
    McpTool { server: String },
    /// Provider name
    Provider,
    /// Unknown context
    Unknown,
}

/// Convert AST context to completion context.
///
/// Maps the detailed AST context to the simpler completion context
/// used by the completion provider.
fn ast_context_to_completion(ast_ctx: &AstContext, word: &str) -> CompletionContext {
    match ast_ctx {
        AstContext::TaskVerb { .. } => CompletionContext::TaskVerb,
        AstContext::UseBlock { partial_ref, .. } => CompletionContext::UseReference {
            partial: partial_ref.clone(),
        },
        AstContext::McpConfig { .. } => CompletionContext::McpServer,
        AstContext::InvokeBlock {
            mcp_server,
            partial_tool: _,
        } => {
            // If we have a server, complete tools; otherwise complete server names
            match mcp_server {
                Some(server) => CompletionContext::McpTool {
                    server: server.clone(),
                },
                None => CompletionContext::McpServer,
            }
        }
        AstContext::ProviderContext { .. } => CompletionContext::Provider,
        AstContext::SchemaContext => CompletionContext::StructuredSchema,
        AstContext::ForEachContext => CompletionContext::Unknown, // Could expand later
        AstContext::WorkflowRoot => {
            // At root level, check what we're typing
            if word.starts_with("sch") || word == "schema" {
                CompletionContext::Schema
            } else if word.starts_with("pro") || word == "provider" {
                CompletionContext::Provider
            } else {
                CompletionContext::Unknown
            }
        }
        AstContext::Unknown => CompletionContext::Unknown,
    }
}

/// Analyze the document position to determine completion context.
///
/// This function uses AST-based detection when the YAML is valid,
/// falling back to line-based heuristics for incomplete/malformed YAML.
pub fn get_completion_context(doc: &DocumentState, position: Position) -> CompletionContext {
    let content = doc.content();

    // Use AST-based context detection
    let ast_result = find_context_at_position(&content, position.line, position.character);

    // Convert to CompletionContext
    let completion_ctx = ast_context_to_completion(&ast_result.context, &ast_result.word_at_cursor);

    // If AST detection found something, use it
    if completion_ctx != CompletionContext::Unknown {
        return completion_ctx;
    }

    // Additional line-based heuristics for edge cases not covered by AST
    let lines: Vec<&str> = content.lines().collect();

    if position.line as usize >= lines.len() {
        return CompletionContext::Unknown;
    }

    let line = lines[position.line as usize];
    let col = position.character as usize;
    let before_cursor = &line[..col.min(line.len())];
    let trimmed = before_cursor.trim();

    // Schema completion at "schema: " prompt
    if trimmed.starts_with("schema:") {
        return CompletionContext::Schema;
    }

    // Provider completion at "provider: " prompt
    if trimmed.starts_with("provider:") {
        return CompletionContext::Provider;
    }

    // After "- id: xxx" on a new line, suggest verbs
    if trimmed.is_empty() || trimmed == "-" {
        for i in (0..position.line as usize).rev() {
            let prev_line = lines[i].trim();
            if prev_line.starts_with("- id:") {
                return CompletionContext::TaskVerb;
            }
            if !prev_line.is_empty() {
                break;
            }
        }
    }

    // Use block reference - colon-based detection for partial typing
    if line.contains("use:") || is_in_use_block(&lines, position.line as usize) {
        if let Some(colon_pos) = before_cursor.rfind(':') {
            let after_colon = before_cursor[colon_pos + 1..].trim();
            if !after_colon.contains('{') {
                return CompletionContext::UseReference {
                    partial: after_colon.to_string(),
                };
            }
        }
    }

    // MCP server reference
    if trimmed.starts_with("mcp:") || trimmed.starts_with("server:") {
        return CompletionContext::McpServer;
    }

    CompletionContext::Unknown
}

/// Check if we're inside a use: block based on indentation.
fn is_in_use_block(lines: &[&str], current_line: usize) -> bool {
    for i in (0..current_line).rev() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.starts_with("use:") {
            return true;
        }

        // Hit a task boundary (- id:)
        if trimmed.starts_with("- id:") {
            return false;
        }

        // Hit non-indented line
        if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
            return false;
        }
    }
    false
}

/// Get completion items for the 5 Nika verbs.
pub fn verb_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "infer".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(" LLM generation".to_string()),
                description: None,
            }),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "**infer:** LLM text generation\n\n```yaml\ninfer: \"Generate a headline\"\n# or\ninfer:\n  prompt: \"Generate content\"\n  temperature: 0.7\n  model: claude-sonnet-4-20250514\n```".to_string(),
            })),
            insert_text: Some("infer: \"$1\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "exec".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(" Shell command".to_string()),
                description: None,
            }),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "**exec:** Shell command execution\n\n```yaml\nexec: \"npm run build\"\n# or\nexec:\n  command: \"npm run build\"\n  shell: true  # for pipes/redirects\n```".to_string(),
            })),
            insert_text: Some("exec: \"$1\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "fetch".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(" HTTP request".to_string()),
                description: None,
            }),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "**fetch:** HTTP request\n\n```yaml\nfetch:\n  url: \"https://api.example.com/data\"\n  method: GET\n  headers:\n    Authorization: \"Bearer $TOKEN\"\n```".to_string(),
            })),
            insert_text: Some("fetch:\n  url: \"$1\"\n  method: ${2|GET,POST,PUT,DELETE|}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "invoke".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(" MCP tool call".to_string()),
                description: None,
            }),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "**invoke:** MCP tool call\n\n```yaml\ninvoke:\n  mcp: novanet\n  tool: novanet_generate\n  params:\n    entity: \"qr-code\"\n    locale: \"fr-FR\"\n```".to_string(),
            })),
            insert_text: Some("invoke:\n  mcp: $1\n  tool: $2\n  params:\n    $3".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "agent".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(" Multi-turn agentic loop".to_string()),
                description: None,
            }),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: "**agent:** Multi-turn agentic loop\n\n```yaml\nagent:\n  prompt: \"Research and summarize AI trends\"\n  mcp: [novanet, perplexity]\n  max_turns: 10\n  depth_limit: 3\n```".to_string(),
            })),
            insert_text: Some("agent:\n  prompt: \"$1\"\n  mcp: [$2]\n  max_turns: ${3:10}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
    ]
}

/// Get completion items for schema versions.
pub fn schema_completions() -> Vec<CompletionItem> {
    let versions = [
        ("0.10", "Latest - all features"),
        ("0.9", "context: + include: DAG fusion"),
        ("0.8", "Studio DX (edit history, sessions)"),
        ("0.7", "Full streaming for all providers"),
        ("0.6", "Multi-provider + chat history"),
        ("0.5", "MVP 8: decompose, lazy bindings, spawn_agent"),
        ("0.3", "for_each parallelism"),
        ("0.2", "invoke: + agent: verbs"),
        ("0.1", "Basic: infer, exec, fetch"),
    ];

    versions
        .into_iter()
        .map(|(version, description)| CompletionItem {
            label: format!("nika/workflow@{}", version),
            kind: Some(CompletionItemKind::VALUE),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(format!(" {}", description)),
                description: None,
            }),
            insert_text: Some(format!("\"nika/workflow@{}\"", version)),
            ..Default::default()
        })
        .collect()
}

/// Get completion items for LLM providers.
pub fn provider_completions() -> Vec<CompletionItem> {
    let providers = [
        ("claude", "Anthropic Claude (ANTHROPIC_API_KEY)"),
        ("openai", "OpenAI GPT (OPENAI_API_KEY)"),
        ("mistral", "Mistral AI (MISTRAL_API_KEY)"),
        ("gemini", "Google Gemini (GEMINI_API_KEY)"),
        ("groq", "Groq (GROQ_API_KEY)"),
        ("deepseek", "DeepSeek (DEEPSEEK_API_KEY)"),
        ("ollama", "Ollama local (OLLAMA_API_BASE_URL)"),
    ];

    providers
        .into_iter()
        .map(|(name, description)| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(format!(" {}", description)),
                description: None,
            }),
            ..Default::default()
        })
        .collect()
}

/// Get completion items for task ID references.
pub fn task_id_completions(task_ids: &[String], partial: &str) -> Vec<CompletionItem> {
    task_ids
        .iter()
        .filter(|id| partial.is_empty() || id.starts_with(partial))
        .map(|id| CompletionItem {
            label: id.clone(),
            kind: Some(CompletionItemKind::REFERENCE),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(" task reference".to_string()),
                description: None,
            }),
            ..Default::default()
        })
        .collect()
}

/// Get completion items for structured output JSON Schema.
///
/// Provides completions for JSON Schema types and common patterns
/// used in the `structured:` or `output: { schema: ... }` blocks.
pub fn structured_output_completions() -> Vec<CompletionItem> {
    let mut items = vec![];

    // JSON Schema type completions
    let types = [
        ("string", "String type", "Validates string values"),
        (
            "number",
            "Number type",
            "Validates numeric values (integers and floats)",
        ),
        ("integer", "Integer type", "Validates integer values only"),
        ("boolean", "Boolean type", "Validates true/false values"),
        ("array", "Array type", "Validates array values"),
        ("object", "Object type", "Validates object values"),
        ("null", "Null type", "Validates null values only"),
    ];

    for (type_name, label_detail, description) in types {
        items.push(CompletionItem {
            label: type_name.to_string(),
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(format!(" {}", label_detail)),
                description: None,
            }),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**{}**\n\n{}", type_name, description),
            })),
            ..Default::default()
        });
    }

    // Common JSON Schema snippets
    items.push(CompletionItem {
        label: "object-template".to_string(),
        kind: Some(CompletionItemKind::SNIPPET),
        label_details: Some(CompletionItemLabelDetails {
            detail: Some(" Object schema".to_string()),
            description: None,
        }),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "**Object Schema Template**\n\nCreates a complete object schema with required properties.".to_string(),
        })),
        insert_text: Some(
            r#"type: object
required:
  - $1
properties:
  $1:
    type: ${2|string,number,integer,boolean,array,object|}
    description: "$3"$0"#.to_string()
        ),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    });

    items.push(CompletionItem {
        label: "array-template".to_string(),
        kind: Some(CompletionItemKind::SNIPPET),
        label_details: Some(CompletionItemLabelDetails {
            detail: Some(" Array schema".to_string()),
            description: None,
        }),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value:
                "**Array Schema Template**\n\nCreates an array schema with item type definition."
                    .to_string(),
        })),
        insert_text: Some(
            r#"type: array
items:
  type: ${1|string,number,integer,boolean,object|}$0"#
                .to_string(),
        ),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    });

    items.push(CompletionItem {
        label: "enum-template".to_string(),
        kind: Some(CompletionItemKind::SNIPPET),
        label_details: Some(CompletionItemLabelDetails {
            detail: Some(" String enum".to_string()),
            description: None,
        }),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "**String Enum Template**\n\nCreates a string type with allowed values."
                .to_string(),
        })),
        insert_text: Some(
            r#"type: string
enum:
  - "$1"
  - "$2"$0"#
                .to_string(),
        ),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    });

    items.push(CompletionItem {
        label: "nested-object-template".to_string(),
        kind: Some(CompletionItemKind::SNIPPET),
        label_details: Some(CompletionItemLabelDetails {
            detail: Some(" Nested object".to_string()),
            description: None,
        }),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value:
                "**Nested Object Template**\n\nCreates an object property with nested properties."
                    .to_string(),
        })),
        insert_text: Some(
            r#"$1:
  type: object
  properties:
    $2:
      type: ${3|string,number,integer,boolean|}$0"#
                .to_string(),
        ),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    });

    items.push(CompletionItem {
        label: "string-with-constraints".to_string(),
        kind: Some(CompletionItemKind::SNIPPET),
        label_details: Some(CompletionItemLabelDetails {
            detail: Some(" Constrained string".to_string()),
            description: None,
        }),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "**String with Constraints**\n\nCreates a string type with length constraints."
                .to_string(),
        })),
        insert_text: Some(
            r#"type: string
minLength: ${1:1}
maxLength: ${2:100}$0"#
                .to_string(),
        ),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    });

    items.push(CompletionItem {
        label: "number-with-range".to_string(),
        kind: Some(CompletionItemKind::SNIPPET),
        label_details: Some(CompletionItemLabelDetails {
            detail: Some(" Ranged number".to_string()),
            description: None,
        }),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: "**Number with Range**\n\nCreates a number type with min/max constraints."
                .to_string(),
        })),
        insert_text: Some(
            r#"type: number
minimum: ${1:0}
maximum: ${2:100}$0"#
                .to_string(),
        ),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..Default::default()
    });

    items
}

/// Extract MCP server names from the document content.
///
/// Parses the YAML looking for `mcp:` block with server definitions.
pub fn extract_mcp_servers(content: &str) -> Vec<String> {
    let mut servers = Vec::new();

    // Simple line-based extraction for mcp: block
    let mut in_mcp_block = false;
    let mut mcp_indent = 0;

    for line in content.lines() {
        let trimmed = line.trim();

        // Detect mcp: block start
        if trimmed.starts_with("mcp:") {
            in_mcp_block = true;
            // Calculate indent of mcp: line
            mcp_indent = line.len() - line.trim_start().len();
            continue;
        }

        if in_mcp_block {
            let current_indent = line.len() - line.trim_start().len();

            // If we're back at same indent or less, we've left the mcp block
            if !trimmed.is_empty() && current_indent <= mcp_indent {
                in_mcp_block = false;
                continue;
            }

            // Look for server name definitions (indented under mcp:)
            // Pattern: "  servername:" at mcp_indent + 2
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
    fn test_verb_completions() {
        let completions = verb_completions();
        assert_eq!(completions.len(), 5);

        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"infer"));
        assert!(labels.contains(&"exec"));
        assert!(labels.contains(&"fetch"));
        assert!(labels.contains(&"invoke"));
        assert!(labels.contains(&"agent"));
    }

    #[test]
    fn test_schema_completions() {
        let completions = schema_completions();
        assert!(!completions.is_empty());
        assert!(completions[0].label.contains("nika/workflow@"));
    }

    #[test]
    fn test_provider_completions() {
        let completions = provider_completions();
        assert_eq!(completions.len(), 7);

        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"claude"));
        assert!(labels.contains(&"gemini"));
    }

    #[test]
    fn test_task_id_completions() {
        let task_ids = vec![
            "step1".to_string(),
            "step2".to_string(),
            "generate".to_string(),
        ];

        let all = task_id_completions(&task_ids, "");
        assert_eq!(all.len(), 3);

        let filtered = task_id_completions(&task_ids, "step");
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_extract_mcp_servers_basic() {
        let content = r#"
schema: "nika/workflow@0.10"
mcp:
  novanet:
    command: novanet-mcp
  filesystem:
    command: npx
    args: ["-y", "@anthropic/mcp-filesystem"]
tasks:
  - id: test
    infer: "Hello"
"#;
        let servers = extract_mcp_servers(content);
        assert_eq!(servers.len(), 2);
        assert!(servers.contains(&"novanet".to_string()));
        assert!(servers.contains(&"filesystem".to_string()));
    }

    #[test]
    fn test_extract_mcp_servers_empty() {
        let content = r#"
schema: "nika/workflow@0.10"
tasks:
  - id: test
    infer: "Hello"
"#;
        let servers = extract_mcp_servers(content);
        assert!(servers.is_empty());
    }

    #[test]
    fn test_extract_mcp_servers_single() {
        let content = r#"
mcp:
  custom_server:
    command: my-server
"#;
        let servers = extract_mcp_servers(content);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0], "custom_server");
    }

    #[test]
    fn test_structured_output_completions() {
        let completions = structured_output_completions();

        // 7 types + 6 snippets = 13 items
        assert_eq!(completions.len(), 13);

        // Verify all JSON Schema types are present
        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"string"));
        assert!(labels.contains(&"number"));
        assert!(labels.contains(&"integer"));
        assert!(labels.contains(&"boolean"));
        assert!(labels.contains(&"array"));
        assert!(labels.contains(&"object"));
        assert!(labels.contains(&"null"));

        // Verify snippet templates are present
        assert!(labels.contains(&"object-template"));
        assert!(labels.contains(&"array-template"));
        assert!(labels.contains(&"enum-template"));
        assert!(labels.contains(&"nested-object-template"));
        assert!(labels.contains(&"string-with-constraints"));
        assert!(labels.contains(&"number-with-range"));
    }

    #[test]
    fn test_structured_output_completions_snippets_have_correct_format() {
        let completions = structured_output_completions();

        // Find snippet completions
        let snippets: Vec<_> = completions
            .iter()
            .filter(|c| c.kind == Some(CompletionItemKind::SNIPPET))
            .collect();

        // Should have 6 snippets
        assert_eq!(snippets.len(), 6);

        // All snippets should have InsertTextFormat::SNIPPET
        for snippet in &snippets {
            assert_eq!(
                snippet.insert_text_format,
                Some(InsertTextFormat::SNIPPET),
                "Snippet '{}' should have SNIPPET insert_text_format",
                snippet.label
            );
            assert!(
                snippet.insert_text.is_some(),
                "Snippet '{}' should have insert_text",
                snippet.label
            );
        }
    }

    #[test]
    fn test_structured_output_completions_types_are_enum_members() {
        let completions = structured_output_completions();

        // Find type completions (non-snippets)
        let types: Vec<_> = completions
            .iter()
            .filter(|c| c.kind == Some(CompletionItemKind::ENUM_MEMBER))
            .collect();

        // Should have 7 types
        assert_eq!(types.len(), 7);

        // All types should have documentation
        for type_item in &types {
            assert!(
                type_item.documentation.is_some(),
                "Type '{}' should have documentation",
                type_item.label
            );
        }
    }
}
