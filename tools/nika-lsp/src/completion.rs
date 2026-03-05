//! Completion provider for Nika workflows.
//!
//! Provides intelligent autocompletion for:
//! - Verbs (infer, exec, fetch, invoke, agent)
//! - Task IDs in use: blocks
//! - Schema versions
//! - MCP server names
//! - Common parameters

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionItemLabelDetails, Documentation,
    InsertTextFormat, MarkupContent, MarkupKind, Position,
};

use crate::document::DocumentState;

/// Completion context for determining what to complete.
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionContext {
    /// At the start of a task definition (after `- id:`)
    TaskVerb,
    /// Inside a use: block (task ID reference)
    UseReference { partial: String },
    /// Schema field
    Schema,
    /// MCP server reference
    McpServer,
    /// Provider name
    Provider,
    /// Unknown context
    Unknown,
}

/// Analyze the document position to determine completion context.
pub fn get_completion_context(doc: &DocumentState, position: Position) -> CompletionContext {
    let content = doc.content();
    let lines: Vec<&str> = content.lines().collect();

    if position.line as usize >= lines.len() {
        return CompletionContext::Unknown;
    }

    let line = lines[position.line as usize];
    let col = position.character as usize;
    let before_cursor = &line[..col.min(line.len())];

    // Check for various contexts based on line content
    let trimmed = before_cursor.trim();

    // Schema completion
    if trimmed.starts_with("schema:") {
        return CompletionContext::Schema;
    }

    // Provider completion
    if trimmed.starts_with("provider:") {
        return CompletionContext::Provider;
    }

    // After "- id: xxx" on a new line, suggest verbs
    if trimmed.is_empty() || trimmed == "-" {
        // Check if previous non-empty line has "- id:"
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

    // Use block reference
    if line.contains("use:") || before_cursor.contains(":") {
        // Look for task ID context
        if let Some(colon_pos) = before_cursor.rfind(':') {
            let after_colon = before_cursor[colon_pos + 1..].trim();
            if !after_colon.contains('{') {
                return CompletionContext::UseReference {
                    partial: after_colon.to_string(),
                };
            }
        }
    }

    // MCP server reference (in invoke: or agent:)
    if trimmed.starts_with("mcp:") || trimmed.starts_with("server:") {
        return CompletionContext::McpServer;
    }

    CompletionContext::Unknown
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
        let task_ids = vec!["step1".to_string(), "step2".to_string(), "generate".to_string()];

        let all = task_id_completions(&task_ids, "");
        assert_eq!(all.len(), 3);

        let filtered = task_id_completions(&task_ids, "step");
        assert_eq!(filtered.len(), 2);
    }
}
