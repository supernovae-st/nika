//! Completion Handler
//!
//! Schema-aware completions for `.nika.yaml` workflow files.

#[cfg(feature = "lsp")]
use tower_lsp::lsp_types::*;

#[cfg(feature = "lsp")]
use super::super::utils::extract_task_ids;

/// Completion context based on cursor position
#[cfg(feature = "lsp")]
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionContext {
    /// Top-level workflow keys (schema, tasks, mcp, etc.)
    TopLevel,
    /// Inside a task definition
    TaskField,
    /// After a verb keyword (infer:, exec:, etc.)
    VerbValue(String),
    /// Inside a use: block (binding references)
    UseBinding,
    /// Inside mcp: block
    McpServer,
    /// Inside a template {{ }}
    Template,
    /// Unknown context
    Unknown,
}

/// Compute completions based on cursor position
#[cfg(feature = "lsp")]
pub fn compute_completions(text: &str, position: Position) -> Vec<CompletionItem> {
    let context = analyze_completion_context(text, position);

    match context {
        CompletionContext::TopLevel => top_level_completions(),
        CompletionContext::TaskField => task_field_completions(),
        CompletionContext::VerbValue(verb) => verb_value_completions(&verb),
        CompletionContext::UseBinding => binding_completions(text),
        CompletionContext::McpServer => mcp_server_completions(),
        CompletionContext::Template => template_completions(text),
        CompletionContext::Unknown => vec![],
    }
}

/// Analyze the text to determine completion context
#[cfg(feature = "lsp")]
fn analyze_completion_context(text: &str, position: Position) -> CompletionContext {
    let lines: Vec<&str> = text.lines().collect();

    if position.line as usize >= lines.len() {
        return CompletionContext::TopLevel;
    }

    let current_line = lines[position.line as usize];
    // Ensure we don't slice past the line length or into a multi-byte char
    let char_pos = (position.character as usize).min(current_line.len());
    // Find valid UTF-8 boundary at or before char_pos
    let prefix_end = current_line
        .char_indices()
        .take_while(|(i, _)| *i < char_pos)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(0);
    let prefix = &current_line[..prefix_end];

    // Check for template context {{ }}
    if prefix.contains("{{") && !prefix.contains("}}") {
        return CompletionContext::Template;
    }

    // Check indentation level
    let indent = current_line.len() - current_line.trim_start().len();

    // Top level (no indentation)
    if indent == 0 {
        return CompletionContext::TopLevel;
    }

    // Look for context in preceding lines
    for i in (0..position.line as usize).rev() {
        let line = lines[i].trim();

        // Inside tasks array
        if line.starts_with("- id:") || line.starts_with("-id:") {
            let task_indent = lines[i].len() - lines[i].trim_start().len();
            if indent > task_indent {
                // Check for specific contexts
                if prefix.trim().starts_with("use") {
                    return CompletionContext::UseBinding;
                }
                if line_contains_verb(prefix) {
                    return CompletionContext::VerbValue(extract_verb(prefix));
                }
                return CompletionContext::TaskField;
            }
        }

        // Inside mcp block
        if line == "mcp:" {
            return CompletionContext::McpServer;
        }

        // At tasks level
        if line == "tasks:" && indent == 2 && prefix.trim().starts_with('-') {
            return CompletionContext::TaskField;
        }
    }

    CompletionContext::Unknown
}

#[cfg(feature = "lsp")]
fn line_contains_verb(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("infer:")
        || trimmed.starts_with("exec:")
        || trimmed.starts_with("fetch:")
        || trimmed.starts_with("invoke:")
        || trimmed.starts_with("agent:")
}

#[cfg(feature = "lsp")]
fn extract_verb(line: &str) -> String {
    let trimmed = line.trim();
    for verb in ["infer", "exec", "fetch", "invoke", "agent"] {
        if trimmed.starts_with(&format!("{}:", verb)) {
            return verb.to_string();
        }
    }
    String::new()
}

/// Top-level workflow completions
#[cfg(feature = "lsp")]
fn top_level_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "schema".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            insert_text: Some("schema: nika/workflow@0.10".to_string()),
            documentation: Some(Documentation::String(
                "Required. Schema version for this workflow.".to_string(),
            )),
            ..Default::default()
        },
        CompletionItem {
            label: "workflow".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            insert_text: Some("workflow: ${1:workflow-name}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Optional. Workflow name/identifier.".to_string(),
            )),
            ..Default::default()
        },
        CompletionItem {
            label: "tasks".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            insert_text: Some(
                "tasks:\n  - id: ${1:task-id}\n    ${2:infer}: ${3:prompt}".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Required. List of tasks to execute.".to_string(),
            )),
            ..Default::default()
        },
        CompletionItem {
            label: "mcp".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            insert_text: Some(
                "mcp:\n  ${1:server-name}:\n    command: ${2:command}\n    args: [${3}]"
                    .to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Optional. MCP server configurations.".to_string(),
            )),
            ..Default::default()
        },
        CompletionItem {
            label: "flows".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            insert_text: Some(
                "flows:\n  - source: ${1:task-id}\n    target: ${2:task-id}".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Optional. Explicit task dependencies.".to_string(),
            )),
            ..Default::default()
        },
        CompletionItem {
            label: "context".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            insert_text: Some(
                "context:\n  files:\n    ${1:alias}: ${2:./path/to/file}".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Optional. Load files at workflow start (v0.14.3+).".to_string(),
            )),
            ..Default::default()
        },
        CompletionItem {
            label: "include".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            insert_text: Some(
                "include:\n  - path: ${1:./partial.nika.yaml}\n    prefix: ${2:partial_}"
                    .to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Optional. Include tasks from other workflows (v0.14.3+).".to_string(),
            )),
            ..Default::default()
        },
    ]
}

/// Task field completions (inside a task definition)
#[cfg(feature = "lsp")]
fn task_field_completions() -> Vec<CompletionItem> {
    let mut items = vec![
        CompletionItem {
            label: "id".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            insert_text: Some("id: ${1:task-id}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Required. Unique task identifier.".to_string(),
            )),
            ..Default::default()
        },
        CompletionItem {
            label: "use".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            insert_text: Some("use:\n  ${1:alias}: ${2:task-id}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Bind outputs from previous tasks.".to_string(),
            )),
            ..Default::default()
        },
        CompletionItem {
            label: "for_each".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            insert_text: Some("for_each: [${1}]\nas: ${2:item}\nconcurrency: ${3:3}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Parallel iteration over an array.".to_string(),
            )),
            ..Default::default()
        },
        CompletionItem {
            label: "retry".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            insert_text: Some("retry:\n  max_attempts: ${1:3}\n  delay: ${2:1s}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Retry configuration for failed tasks.".to_string(),
            )),
            ..Default::default()
        },
        CompletionItem {
            label: "timeout".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            insert_text: Some("timeout: ${1:30s}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Maximum execution time for this task.".to_string(),
            )),
            ..Default::default()
        },
    ];

    // Add verb completions
    items.extend(verb_completions());

    items
}

/// Completions for the 5 semantic verbs
#[cfg(feature = "lsp")]
fn verb_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "infer".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            insert_text: Some("infer: ${1:prompt}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "⚡ LLM text generation. Shorthand accepts a string.".to_string(),
            )),
            detail: Some("Verb".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "exec".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            insert_text: Some("exec: ${1:command}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "📟 Shell command execution. Defaults to shell: false for security.".to_string(),
            )),
            detail: Some("Verb".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "fetch".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            insert_text: Some("fetch:\n  url: ${1:https://}\n  method: ${2:GET}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "🛰️ HTTP request.".to_string(),
            )),
            detail: Some("Verb".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "invoke".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            insert_text: Some("invoke:\n  mcp: ${1:server}\n  tool: ${2:tool-name}\n  params:\n    ${3:key}: ${4:value}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "🔌 MCP tool invocation.".to_string(),
            )),
            detail: Some("Verb".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "agent".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            insert_text: Some("agent:\n  prompt: ${1:goal}\n  mcp: [${2:server}]\n  max_turns: ${3:10}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "🐔 Multi-turn agentic loop with tool calling.".to_string(),
            )),
            detail: Some("Verb".to_string()),
            ..Default::default()
        },
    ]
}

/// Completions for verb values (after infer:, exec:, etc.)
#[cfg(feature = "lsp")]
fn verb_value_completions(verb: &str) -> Vec<CompletionItem> {
    match verb {
        "infer" => vec![
            CompletionItem {
                label: "prompt".to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                insert_text: Some("prompt: ${1}".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
            CompletionItem {
                label: "model".to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                insert_text: Some("model: ${1:claude-sonnet-4-6}".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
            CompletionItem {
                label: "temperature".to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                insert_text: Some("temperature: ${1:0.7}".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
            CompletionItem {
                label: "system".to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                insert_text: Some("system: ${1:You are a helpful assistant.}".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
            CompletionItem {
                label: "max_tokens".to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                insert_text: Some("max_tokens: ${1:1000}".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
        ],
        "exec" => vec![
            CompletionItem {
                label: "command".to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                insert_text: Some("command: ${1}".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
            CompletionItem {
                label: "shell".to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                insert_text: Some("shell: ${1|true,false|}".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                documentation: Some(Documentation::String(
                    "Enable shell mode for pipes/redirects. Default: false (secure).".to_string(),
                )),
                ..Default::default()
            },
        ],
        "agent" => vec![
            CompletionItem {
                label: "prompt".to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                insert_text: Some("prompt: ${1}".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
            CompletionItem {
                label: "mcp".to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                insert_text: Some("mcp: [${1}]".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
            CompletionItem {
                label: "max_turns".to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                insert_text: Some("max_turns: ${1:10}".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            },
            CompletionItem {
                label: "depth_limit".to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                insert_text: Some("depth_limit: ${1:3}".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                documentation: Some(Documentation::String(
                    "Max spawn_agent recursion depth. Default: 3.".to_string(),
                )),
                ..Default::default()
            },
            CompletionItem {
                label: "extended_thinking".to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                insert_text: Some(
                    "extended_thinking: true\nthinking_budget: ${1:8192}".to_string(),
                ),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                documentation: Some(Documentation::String(
                    "Enable Claude's extended thinking mode.".to_string(),
                )),
                ..Default::default()
            },
        ],
        _ => vec![],
    }
}

/// Completions for use: block (binding references)
#[cfg(feature = "lsp")]
fn binding_completions(text: &str) -> Vec<CompletionItem> {
    // Extract task IDs from the text
    let task_ids = extract_task_ids(text);

    task_ids
        .into_iter()
        .map(|id| CompletionItem {
            label: id.clone(),
            kind: Some(CompletionItemKind::REFERENCE),
            insert_text: Some(id.clone()),
            documentation: Some(Documentation::String(format!(
                "Reference output from task '{}'",
                id
            ))),
            ..Default::default()
        })
        .collect()
}

/// Completions for MCP server configuration
#[cfg(feature = "lsp")]
fn mcp_server_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "command".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            insert_text: Some("command: ${1:npx}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Command to start the MCP server.".to_string(),
            )),
            ..Default::default()
        },
        CompletionItem {
            label: "args".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            insert_text: Some("args: [${1}]".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Arguments to pass to the command.".to_string(),
            )),
            ..Default::default()
        },
        CompletionItem {
            label: "env".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            insert_text: Some("env:\n  ${1:KEY}: ${2:value}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Environment variables for the MCP server.".to_string(),
            )),
            ..Default::default()
        },
    ]
}

/// Completions inside templates {{ }}
#[cfg(feature = "lsp")]
fn template_completions(text: &str) -> Vec<CompletionItem> {
    let mut items = vec![
        CompletionItem {
            label: "use.".to_string(),
            kind: Some(CompletionItemKind::VARIABLE),
            insert_text: Some("use.${1:alias}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Reference bound task output.".to_string(),
            )),
            ..Default::default()
        },
        CompletionItem {
            label: "context.files.".to_string(),
            kind: Some(CompletionItemKind::VARIABLE),
            insert_text: Some("context.files.${1:alias}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Reference loaded context file.".to_string(),
            )),
            ..Default::default()
        },
        CompletionItem {
            label: "inputs.".to_string(),
            kind: Some(CompletionItemKind::VARIABLE),
            insert_text: Some("inputs.${1:name}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            documentation: Some(Documentation::String(
                "Reference workflow input parameter.".to_string(),
            )),
            ..Default::default()
        },
    ];

    // Add task IDs for $task shorthand
    for id in extract_task_ids(text) {
        items.push(CompletionItem {
            label: format!("${}", id),
            kind: Some(CompletionItemKind::REFERENCE),
            insert_text: Some(format!("${}", id)),
            documentation: Some(Documentation::String(format!(
                "Implicit output from task '{}' (shorthand)",
                id
            ))),
            ..Default::default()
        });
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_task_ids() {
        let text = r#"
tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    exec: "echo hi"
"#;
        let ids = extract_task_ids(text);
        assert_eq!(ids, vec!["step1", "step2"]);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_top_level_completions() {
        let items = top_level_completions();
        assert!(items.iter().any(|i| i.label == "schema"));
        assert!(items.iter().any(|i| i.label == "tasks"));
        assert!(items.iter().any(|i| i.label == "mcp"));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_task_field_completions_include_verbs() {
        let items = task_field_completions();
        assert!(items.iter().any(|i| i.label == "infer"));
        assert!(items.iter().any(|i| i.label == "exec"));
        assert!(items.iter().any(|i| i.label == "agent"));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_analyze_context_top_level() {
        let text = "";
        let ctx = analyze_completion_context(
            text,
            Position {
                line: 0,
                character: 0,
            },
        );
        assert_eq!(ctx, CompletionContext::TopLevel);
    }
}
