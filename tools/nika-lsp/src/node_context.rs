//! AST-based node context detection for LSP completions.
//!
//! This module replaces string-based context detection with proper AST traversal.
//! It parses the YAML workflow using nika's two-phase AST and determines the
//! cursor context for intelligent completions.

use nika::ast::raw::{parse, RawTaskAction};
use nika::source::{ByteOffset, FileId, Span};

use crate::position::position_to_offset;

/// Convert position module's ByteOffset to nika's ByteOffset.
fn to_nika_offset(offset: crate::position::ByteOffset) -> ByteOffset {
    ByteOffset(offset.0)
}

/// The AST context at a cursor position.
///
/// This enum represents what kind of element the cursor is within,
/// enabling the LSP to provide context-aware completions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AstContext {
    /// Cursor is at the task level, possibly entering a verb (infer, exec, etc.)
    TaskVerb {
        /// The task ID if we're inside a task block
        task_id: Option<String>,
    },

    /// Cursor is in a `with:` block, referencing other tasks
    WithBlock {
        /// The alias being typed (if any)
        alias: Option<String>,
        /// Partial task reference being typed
        partial_ref: String,
    },

    /// Cursor is in MCP configuration section
    McpConfig {
        /// Server name if inside a server block
        server_name: Option<String>,
    },

    /// Cursor is in an invoke block, might need MCP tool completion
    InvokeBlock {
        /// MCP server being used
        mcp_server: Option<String>,
        /// Partial tool name being typed
        partial_tool: String,
    },

    /// Cursor is in an infer/agent block where provider is relevant
    ProviderContext {
        /// Current provider if specified
        current_provider: Option<String>,
    },

    /// Cursor is in a schema/output section
    SchemaContext,

    /// Cursor is in for_each items
    ForEachContext,

    /// Cursor is at workflow root level (schema, provider, mcp, tasks, etc.)
    WorkflowRoot,

    /// Unknown or unsupported context
    Unknown,
}

/// Result of context detection with additional metadata.
#[derive(Debug)]
pub struct ContextResult {
    /// The detected context
    pub context: AstContext,
    /// The span of the containing element (if any).
    /// Note: Currently unused but retained for future diagnostics and hover info.
    #[allow(dead_code)]
    pub span: Option<Span>,
    /// The word at cursor position
    pub word_at_cursor: String,
}

/// Check if a byte offset is within a span.
fn offset_in_span(offset: ByteOffset, span: &Span) -> bool {
    offset.0 >= span.start.0 && offset.0 <= span.end.0
}

/// Find the AST context at a given position in the workflow YAML.
///
/// # Arguments
/// * `content` - The YAML source text
/// * `line` - 0-indexed line number (LSP format)
/// * `character` - 0-indexed character position (LSP format)
///
/// # Returns
/// A `ContextResult` describing what kind of element the cursor is within.
pub fn find_context_at_position(content: &str, line: u32, character: u32) -> ContextResult {
    // Convert LSP position to byte offset
    let pos_offset = match position_to_offset(content, line, character) {
        Some(o) => o,
        None => {
            return ContextResult {
                context: AstContext::Unknown,
                span: None,
                word_at_cursor: String::new(),
            };
        }
    };

    // Get word at cursor for partial completions (uses position module's ByteOffset)
    let word_at_cursor = crate::position::word_at_offset(content, pos_offset).to_string();

    // Convert to nika's ByteOffset for AST span comparisons
    let offset = to_nika_offset(pos_offset);

    // Try to parse the YAML
    let workflow = match parse(content, FileId(0)) {
        Ok(w) => w,
        Err(_) => {
            // If parsing fails, fall back to line-based heuristics
            return fallback_context_detection(content, line, character, word_at_cursor);
        }
    };

    // Check if we're in the MCP config section
    if let Some(ref mcp) = workflow.mcp {
        if offset_in_span(offset, &mcp.span) {
            // We're in the MCP section
            // Check if we're in a specific server (use first server's name if available)
            if let Some((server_name, _server)) = mcp.value.servers.iter().next() {
                // Note: RawMcpServer doesn't have span, so we use the overall MCP span
                // In a real implementation, we'd need spans on server configs too
                return ContextResult {
                    context: AstContext::McpConfig {
                        server_name: Some(server_name.value.clone()),
                    },
                    span: Some(mcp.span),
                    word_at_cursor,
                };
            }
            return ContextResult {
                context: AstContext::McpConfig { server_name: None },
                span: Some(mcp.span),
                word_at_cursor,
            };
        }
    }

    // Check if we're in a task
    if offset_in_span(offset, &workflow.tasks.span) {
        for task in &workflow.tasks.value {
            if offset_in_span(offset, &task.span) {
                let task_id = Some(task.value.id.value.clone());

                // Check if we're in the with: block
                if let Some(ref with_refs) = task.value.with_refs {
                    if offset_in_span(offset, &with_refs.span) {
                        return ContextResult {
                            context: AstContext::WithBlock {
                                alias: None, // Would need finer span tracking
                                partial_ref: word_at_cursor.clone(),
                            },
                            span: Some(with_refs.span),
                            word_at_cursor,
                        };
                    }
                }

                // Check if we're in the action (verb) block
                if let Some(ref action) = task.value.action {
                    let action_span = action.span();
                    if offset_in_span(offset, &action_span) {
                        return match action {
                            RawTaskAction::Invoke(invoke) => ContextResult {
                                context: AstContext::InvokeBlock {
                                    mcp_server: invoke.value.mcp.as_ref().map(|m| m.value.clone()),
                                    partial_tool: word_at_cursor.clone(),
                                },
                                span: Some(action_span),
                                word_at_cursor,
                            },
                            RawTaskAction::Infer(_) | RawTaskAction::Agent(_) => ContextResult {
                                context: AstContext::ProviderContext {
                                    current_provider: task
                                        .value
                                        .provider
                                        .as_ref()
                                        .map(|p| p.value.clone()),
                                },
                                span: Some(action_span),
                                word_at_cursor,
                            },
                            _ => ContextResult {
                                context: AstContext::TaskVerb { task_id },
                                span: Some(action_span),
                                word_at_cursor,
                            },
                        };
                    }
                }

                // Check for_each
                if let Some(ref for_each) = task.value.for_each {
                    if offset_in_span(offset, &for_each.span) {
                        return ContextResult {
                            context: AstContext::ForEachContext,
                            span: Some(for_each.span),
                            word_at_cursor,
                        };
                    }
                }

                // We're in a task but not in a specific sub-element
                // Likely entering a verb or field name
                return ContextResult {
                    context: AstContext::TaskVerb { task_id },
                    span: Some(task.span),
                    word_at_cursor,
                };
            }
        }
    }

    // Check if we're at workflow root level
    // The parser's workflow.span may not start at byte 0 (it starts after the schema: key)
    // so we also check if offset is before the span start (still root-level editing)
    if offset_in_span(offset, &workflow.span) || offset.0 < workflow.span.start.0 {
        return ContextResult {
            context: AstContext::WorkflowRoot,
            span: Some(workflow.span),
            word_at_cursor,
        };
    }

    ContextResult {
        context: AstContext::Unknown,
        span: None,
        word_at_cursor,
    }
}

/// Fallback context detection using line-based heuristics.
///
/// Used when the YAML is malformed or incomplete (common during editing).
fn fallback_context_detection(
    content: &str,
    line: u32,
    _character: u32,
    word_at_cursor: String,
) -> ContextResult {
    // Empty content is unknown
    if content.is_empty() {
        return ContextResult {
            context: AstContext::Unknown,
            span: None,
            word_at_cursor,
        };
    }

    let lines: Vec<&str> = content.lines().collect();

    let current_line = lines.get(line as usize).unwrap_or(&"");
    let trimmed = current_line.trim();

    // Check for common patterns
    if trimmed.starts_with("with:") || trimmed.starts_with("- ") && current_line.contains("with:") {
        return ContextResult {
            context: AstContext::WithBlock {
                alias: None,
                partial_ref: word_at_cursor.clone(),
            },
            span: None,
            word_at_cursor,
        };
    }

    if trimmed.starts_with("mcp:") || is_indented_under(lines.as_slice(), line as usize, "mcp:") {
        return ContextResult {
            context: AstContext::McpConfig { server_name: None },
            span: None,
            word_at_cursor,
        };
    }

    if trimmed.starts_with("invoke:")
        || is_indented_under(lines.as_slice(), line as usize, "invoke:")
    {
        return ContextResult {
            context: AstContext::InvokeBlock {
                mcp_server: None,
                partial_tool: word_at_cursor.clone(),
            },
            span: None,
            word_at_cursor,
        };
    }

    if trimmed.starts_with("infer:") || trimmed.starts_with("agent:") {
        return ContextResult {
            context: AstContext::ProviderContext {
                current_provider: None,
            },
            span: None,
            word_at_cursor,
        };
    }

    // output: context (JSON Schema for structured output)
    // Note: schema: at root level is WorkflowRoot, not SchemaContext
    if trimmed.starts_with("output:")
        || is_indented_under(lines.as_slice(), line as usize, "output:")
    {
        return ContextResult {
            context: AstContext::SchemaContext,
            span: None,
            word_at_cursor,
        };
    }

    if trimmed.starts_with("for_each:") {
        return ContextResult {
            context: AstContext::ForEachContext,
            span: None,
            word_at_cursor,
        };
    }

    // Check if we're in a task block (under tasks:)
    if is_indented_under(lines.as_slice(), line as usize, "tasks:") {
        // Check if this looks like a verb position (after - id: line)
        if trimmed.starts_with("- id:") || current_line.starts_with("    ") {
            return ContextResult {
                context: AstContext::TaskVerb { task_id: None },
                span: None,
                word_at_cursor,
            };
        }
    }

    // Check if at root level
    if !current_line.starts_with(' ') && !current_line.starts_with('\t') {
        return ContextResult {
            context: AstContext::WorkflowRoot,
            span: None,
            word_at_cursor,
        };
    }

    ContextResult {
        context: AstContext::Unknown,
        span: None,
        word_at_cursor,
    }
}

/// Check if a line is indented under a specific parent key.
fn is_indented_under(lines: &[&str], current_idx: usize, parent_key: &str) -> bool {
    // Look backwards to find the parent
    for i in (0..current_idx).rev() {
        let line = lines[i];
        let trimmed = line.trim();

        // Found the parent key
        if trimmed.starts_with(parent_key) {
            return true;
        }

        // Hit a non-indented line that's not our parent - stop
        if !line.starts_with(' ') && !line.starts_with('\t') && !trimmed.is_empty() {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_root_context() {
        let yaml = r#"schema: "nika/workflow@0.12"
provider: claude

tasks:
  - id: test
    infer: "Hello"
"#;
        let result = find_context_at_position(yaml, 0, 0);
        assert!(
            matches!(result.context, AstContext::WorkflowRoot),
            "Expected WorkflowRoot but got {:?}",
            result.context
        );
    }

    #[test]
    fn test_task_verb_context() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer: "Hello"
"#;
        // Position at line 3 (infer:), character 4
        let result = find_context_at_position(yaml, 3, 4);
        // Should detect we're in a task or action context
        assert!(matches!(
            result.context,
            AstContext::TaskVerb { .. } | AstContext::ProviderContext { .. }
        ));
    }

    #[test]
    fn test_with_block_context() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: first
    infer: "Step 1"
  - id: second
    with:
      data: $first
    infer: "Step 2"
"#;
        // Position at line 6 (with:), after "data: "
        let result = find_context_at_position(yaml, 6, 10);
        // Check it's some kind of with or task context
        // The exact detection depends on span boundaries
        assert!(
            matches!(result.context, AstContext::WithBlock { .. })
                || matches!(result.context, AstContext::TaskVerb { .. })
        );
    }

    #[test]
    fn test_mcp_config_context() {
        let yaml = r#"schema: "nika/workflow@0.12"
mcp:
  servers:
    novanet:
      command: novanet-mcp
tasks:
  - id: test
    infer: "Hello"
"#;
        // Position inside mcp block
        let result = find_context_at_position(yaml, 3, 4);
        assert!(matches!(result.context, AstContext::McpConfig { .. }));
    }

    #[test]
    fn test_invoke_context() {
        let yaml = r#"schema: "nika/workflow@0.12"
mcp:
  servers:
    novanet:
      command: novanet-mcp
tasks:
  - id: test
    invoke:
      tool: novanet_search
      mcp: novanet
"#;
        // Position inside invoke block
        let result = find_context_at_position(yaml, 8, 10);
        assert!(
            matches!(result.context, AstContext::InvokeBlock { .. })
                || matches!(result.context, AstContext::TaskVerb { .. })
        );
    }

    #[test]
    fn test_fallback_on_malformed_yaml() {
        // Incomplete YAML that won't parse
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: test
    with:
      data:
"#;
        // Should still detect with: context via fallback
        let result = find_context_at_position(yaml, 4, 10);
        // Fallback should detect something useful
        assert!(!matches!(result.context, AstContext::Unknown));
    }

    #[test]
    fn test_word_at_cursor() {
        let yaml = r#"schema: "nika/workflow@0.12"
provider: claude
"#;
        let result = find_context_at_position(yaml, 1, 11);
        assert_eq!(result.word_at_cursor, "claude");
    }

    #[test]
    fn test_for_each_context() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: parallel
    for_each: ["a", "b", "c"]
    infer: "Process {{with.item}}"
"#;
        let result = find_context_at_position(yaml, 3, 14);
        assert!(
            matches!(result.context, AstContext::ForEachContext)
                || matches!(result.context, AstContext::TaskVerb { .. })
        );
    }

    #[test]
    fn test_schema_context() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer: "Hello"
    output:
      format: json
      schema:
        type: object
"#;
        // Fallback detection for schema context
        let result = find_context_at_position(yaml, 6, 6);
        // May detect as schema context or task context
        assert!(
            matches!(result.context, AstContext::SchemaContext)
                || matches!(result.context, AstContext::TaskVerb { .. })
        );
    }

    #[test]
    fn test_is_indented_under() {
        let lines = vec!["mcp:", "  servers:", "    novanet:", "      command: test"];

        assert!(is_indented_under(&lines, 3, "mcp:"));
        assert!(is_indented_under(&lines, 2, "mcp:"));
        assert!(!is_indented_under(&lines, 0, "tasks:"));
    }

    #[test]
    fn test_empty_content() {
        let result = find_context_at_position("", 0, 0);
        assert!(matches!(result.context, AstContext::Unknown));
    }

    #[test]
    fn test_position_out_of_bounds() {
        let yaml = "schema: test\n";
        let result = find_context_at_position(yaml, 100, 0);
        assert!(matches!(result.context, AstContext::Unknown));
    }

    #[test]
    fn test_provider_context_in_infer() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: test
    provider: openai
    infer:
      prompt: "Hello"
      model: gpt-4o
"#;
        // Inside infer block
        let result = find_context_at_position(yaml, 5, 6);
        // Should detect provider context or task context
        assert!(
            matches!(result.context, AstContext::ProviderContext { .. })
                || matches!(result.context, AstContext::TaskVerb { .. })
        );
    }

    #[test]
    fn test_agent_context() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: research
    agent:
      prompt: "Research AI papers"
      max_turns: 10
"#;
        let result = find_context_at_position(yaml, 4, 6);
        assert!(
            matches!(result.context, AstContext::ProviderContext { .. })
                || matches!(result.context, AstContext::TaskVerb { .. })
        );
    }
}
