//! Code Action Handler
//!
//! Provides quick fixes and refactoring actions for Nika workflows:
//! - Fix unknown task references
//! - Add missing required fields
//! - Convert between shorthand and full form
//! - Fix schema version

#[cfg(feature = "lsp")]
use tower_lsp::lsp_types::*;

#[cfg(feature = "lsp")]
use super::super::utils::extract_task_ids;

/// Compute code actions for a range
#[cfg(feature = "lsp")]
pub fn compute_code_actions(
    text: &str,
    range: Range,
    diagnostics: &[Diagnostic],
    uri: Url,
) -> CodeActionResponse {
    let mut actions = Vec::new();

    // Process diagnostics to generate quick fixes
    for diagnostic in diagnostics {
        if let Some(action) = create_quickfix_for_diagnostic(text, diagnostic, &uri) {
            actions.push(action);
        }
    }

    // Add refactoring actions based on context
    actions.extend(create_refactoring_actions(text, range, &uri));

    actions
}

/// Create a quick fix for a diagnostic
#[cfg(feature = "lsp")]
fn create_quickfix_for_diagnostic(
    text: &str,
    diagnostic: &Diagnostic,
    uri: &Url,
) -> Option<CodeActionOrCommand> {
    let code = diagnostic.code.as_ref()?;
    let code_str = match code {
        NumberOrString::String(s) => s.as_str(),
        NumberOrString::Number(n) => return create_fix_for_code(*n, text, diagnostic, uri),
    };

    match code_str {
        "NIKA-140" => create_unknown_task_fix(text, diagnostic, uri),
        "NIKA-141" => create_duplicate_task_fix(diagnostic, uri),
        "NIKA-142" => create_invalid_schema_fix(diagnostic, uri),
        "NIKA-145" => create_missing_field_fix(text, diagnostic, uri),
        _ => None,
    }
}

/// Create fix for numeric error codes
#[cfg(feature = "lsp")]
fn create_fix_for_code(
    _code: i32,
    _text: &str,
    _diagnostic: &Diagnostic,
    _uri: &Url,
) -> Option<CodeActionOrCommand> {
    // Handle numeric codes if needed
    None
}

/// Fix for unknown task reference (NIKA-140)
#[cfg(feature = "lsp")]
fn create_unknown_task_fix(
    text: &str,
    diagnostic: &Diagnostic,
    uri: &Url,
) -> Option<CodeActionOrCommand> {
    // Extract the unknown task name from the diagnostic message
    let message = &diagnostic.message;
    if !message.contains("Unknown task") {
        return None;
    }

    // Find similar task IDs for suggestions
    let task_ids = extract_task_ids(text);
    if task_ids.is_empty() {
        return None;
    }

    // Create a suggestion to use an existing task
    // For now, suggest the first available task
    let suggested_task = &task_ids[0];

    let edit = TextEdit {
        range: diagnostic.range,
        new_text: suggested_task.clone(),
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Did you mean '{}'?", suggested_task),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    }))
}

/// Fix for duplicate task ID (NIKA-141)
#[cfg(feature = "lsp")]
fn create_duplicate_task_fix(diagnostic: &Diagnostic, uri: &Url) -> Option<CodeActionOrCommand> {
    // Suggest renaming with a suffix
    let line_num = diagnostic.range.start.line;
    let suggested_id = format!("task_{}", line_num);

    let edit = TextEdit {
        range: diagnostic.range,
        new_text: format!("id: {}", suggested_id),
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Rename to '{}'", suggested_id),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    }))
}

/// Fix for invalid schema version (NIKA-142)
#[cfg(feature = "lsp")]
fn create_invalid_schema_fix(diagnostic: &Diagnostic, uri: &Url) -> Option<CodeActionOrCommand> {
    let edit = TextEdit {
        range: diagnostic.range,
        new_text: "schema: nika/workflow@0.10".to_string(),
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: "Update to schema version @0.10".to_string(),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    }))
}

/// Fix for missing required field (NIKA-145)
#[cfg(feature = "lsp")]
fn create_missing_field_fix(
    _text: &str,
    diagnostic: &Diagnostic,
    uri: &Url,
) -> Option<CodeActionOrCommand> {
    let message = &diagnostic.message;

    // Extract the missing field name from message
    let field = if message.contains("'id'") {
        "id: new_task"
    } else if message.contains("'schema'") {
        "schema: nika/workflow@0.10"
    } else if message.contains("'tasks'") {
        "tasks:\n  - id: step1\n    infer: \"TODO\""
    } else {
        return None;
    };

    let edit = TextEdit {
        range: Range {
            start: Position {
                line: diagnostic.range.start.line,
                character: 0,
            },
            end: Position {
                line: diagnostic.range.start.line,
                character: 0,
            },
        },
        new_text: format!("{}\n", field),
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: format!(
            "Add missing field: {}",
            field.split(':').next().unwrap_or("field")
        ),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    }))
}

/// Create refactoring actions based on context
#[cfg(feature = "lsp")]
fn create_refactoring_actions(text: &str, range: Range, uri: &Url) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    let lines: Vec<&str> = text.lines().collect();
    let line_idx = range.start.line as usize;

    if line_idx >= lines.len() {
        return actions;
    }

    let line = lines[line_idx];
    let trimmed = line.trim();

    // Convert shorthand infer to full form
    if trimmed.starts_with("infer: \"") || trimmed.starts_with("infer: '") {
        if let Some(action) = create_expand_infer_action(line, range.start.line, uri) {
            actions.push(action);
        }
    }

    // Convert shorthand exec to full form
    if trimmed.starts_with("exec: \"") || trimmed.starts_with("exec: '") {
        if let Some(action) = create_expand_exec_action(line, range.start.line, uri) {
            actions.push(action);
        }
    }

    // Add use: block refactoring
    if trimmed.starts_with("- id:") {
        actions.push(create_add_use_block_action(range.start.line, uri));
    }

    actions
}

/// Create action to expand shorthand infer to full form
#[cfg(feature = "lsp")]
fn create_expand_infer_action(line: &str, line_num: u32, uri: &Url) -> Option<CodeActionOrCommand> {
    let trimmed = line.trim();
    let indent = line.len() - trimmed.len();

    // Extract the prompt
    let prompt_start = trimmed.find('"').or_else(|| trimmed.find('\''))?;
    let quote_char = trimmed.chars().nth(prompt_start)?;
    let prompt_end = trimmed[prompt_start + 1..].find(quote_char)? + prompt_start + 1;
    let prompt = &trimmed[prompt_start + 1..prompt_end];

    let indent_str = " ".repeat(indent);
    let new_text = format!(
        "{}infer:\n{}  prompt: \"{}\"\n{}  model: claude-sonnet-4-6",
        indent_str, indent_str, prompt, indent_str
    );

    let edit = TextEdit {
        range: Range {
            start: Position {
                line: line_num,
                character: 0,
            },
            end: Position {
                line: line_num,
                character: line.len() as u32,
            },
        },
        new_text,
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: "Expand to full infer form".to_string(),
        kind: Some(CodeActionKind::REFACTOR),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    }))
}

/// Create action to expand shorthand exec to full form
#[cfg(feature = "lsp")]
fn create_expand_exec_action(line: &str, line_num: u32, uri: &Url) -> Option<CodeActionOrCommand> {
    let trimmed = line.trim();
    let indent = line.len() - trimmed.len();

    // Extract the command
    let cmd_start = trimmed.find('"').or_else(|| trimmed.find('\''))?;
    let quote_char = trimmed.chars().nth(cmd_start)?;
    let cmd_end = trimmed[cmd_start + 1..].find(quote_char)? + cmd_start + 1;
    let command = &trimmed[cmd_start + 1..cmd_end];

    let indent_str = " ".repeat(indent);
    let new_text = format!(
        "{}exec:\n{}  command: \"{}\"\n{}  shell: false",
        indent_str, indent_str, command, indent_str
    );

    let edit = TextEdit {
        range: Range {
            start: Position {
                line: line_num,
                character: 0,
            },
            end: Position {
                line: line_num,
                character: line.len() as u32,
            },
        },
        new_text,
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: "Expand to full exec form".to_string(),
        kind: Some(CodeActionKind::REFACTOR),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    }))
}

/// Create action to add use: block
#[cfg(feature = "lsp")]
fn create_add_use_block_action(line_num: u32, uri: &Url) -> CodeActionOrCommand {
    let edit = TextEdit {
        range: Range {
            start: Position {
                line: line_num + 1,
                character: 0,
            },
            end: Position {
                line: line_num + 1,
                character: 0,
            },
        },
        new_text: "    use:\n      input: $previous_task\n".to_string(),
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    CodeActionOrCommand::CodeAction(CodeAction {
        title: "Add use: block for data binding".to_string(),
        kind: Some(CodeActionKind::REFACTOR),
        diagnostics: None,
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(false),
        disabled: None,
        data: None,
    })
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
    exec: "echo"
  - id: "step3"
    fetch:
      url: https://example.com
"#;
        let ids = extract_task_ids(text);
        assert_eq!(ids, vec!["step1", "step2", "step3"]);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_expand_infer_action() {
        let line = "    infer: \"Generate a headline\"";
        let uri = Url::parse("file:///test.nika.yaml").unwrap();
        let action = create_expand_infer_action(line, 5, &uri);

        assert!(action.is_some());
        if let Some(CodeActionOrCommand::CodeAction(ca)) = action {
            assert_eq!(ca.title, "Expand to full infer form");
            assert_eq!(ca.kind, Some(CodeActionKind::REFACTOR));
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_expand_exec_action() {
        let line = "    exec: \"npm run build\"";
        let uri = Url::parse("file:///test.nika.yaml").unwrap();
        let action = create_expand_exec_action(line, 3, &uri);

        assert!(action.is_some());
        if let Some(CodeActionOrCommand::CodeAction(ca)) = action {
            assert_eq!(ca.title, "Expand to full exec form");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_code_actions_for_diagnostics() {
        let text = "schema: nika/workflow@0.9\ntasks:\n  - id: step1\n    infer: \"test\"";
        let uri = Url::parse("file:///test.nika.yaml").unwrap();

        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 25,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String("NIKA-142".to_string())),
            source: Some("nika".to_string()),
            message: "Invalid schema version".to_string(),
            related_information: None,
            tags: None,
            code_description: None,
            data: None,
        };

        let actions = compute_code_actions(
            text,
            Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 25,
                },
            },
            &[diagnostic],
            uri,
        );

        assert!(!actions.is_empty());
    }
}
