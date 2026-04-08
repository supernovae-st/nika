// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Code Action Handler
//!
//! Provides quick fixes and refactoring actions for Nika workflows:
//! - Fix unknown task references
//! - Add missing required fields
//! - Convert between shorthand and full form
//! - Fix schema version
//!
//! ## Phase 2 Architecture
//!
//! The code action handler can use AstIndex for semantic-aware fixes:
//! - Use `compute_code_actions_with_ast` for better task suggestions
//! - TaskTable provides O(1) task lookup for fuzzy matching
//! - Context-aware refactoring based on actual workflow structure

#[cfg(feature = "lsp")]
use tower_lsp_server::ls_types::*;

#[cfg(feature = "lsp")]
pub use tower_lsp_server::ls_types::Uri;

#[cfg(feature = "lsp")]
use super::super::ast_index::AstIndex;
#[cfg(feature = "lsp")]
use nika_lsp_core::analysis::context::extract_task_ids;

/// Compute code actions using AST-aware semantic analysis
///
/// This function uses the AstIndex to provide better code actions:
/// - For NIKA-140 (unknown task), uses TaskTable for fuzzy matching
/// - Context-aware refactoring based on actual workflow structure
/// - Falls back to text-based analysis if AST is unavailable
#[cfg(feature = "lsp")]
pub fn compute_code_actions_with_ast(
    ast_index: &AstIndex,
    uri: &Uri,
    text: &str,
    range: Range,
    diagnostics: &[Diagnostic],
) -> CodeActionResponse {
    let mut actions = Vec::new();

    // Try to get task IDs from AST for better suggestions
    let ast_task_ids: Vec<String> = if let Some(cached) = ast_index.get(uri) {
        if let Some(ref analyzed) = cached.analyzed {
            analyzed
                .task_table
                .iter()
                .map(|(_, name)| name.to_string())
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Process diagnostics to generate quick fixes
    for diagnostic in diagnostics {
        if let Some(action) =
            create_quickfix_for_diagnostic_with_ast(text, diagnostic, uri, &ast_task_ids)
        {
            actions.push(action);
        }
    }

    // Add refactoring actions based on context (still text-based for now)
    actions.extend(create_refactoring_actions(text, range, uri));

    actions
}

/// Create a quick fix for a diagnostic using AST information
#[cfg(feature = "lsp")]
fn create_quickfix_for_diagnostic_with_ast(
    text: &str,
    diagnostic: &Diagnostic,
    uri: &Uri,
    ast_task_ids: &[String],
) -> Option<CodeActionOrCommand> {
    let code = diagnostic.code.as_ref()?;
    let code_str = match code {
        NumberOrString::String(s) => s.as_str(),
        NumberOrString::Number(n) => return create_fix_for_code(*n, text, diagnostic, uri),
    };

    match code_str {
        "NIKA-140" => create_unknown_task_fix_with_ast(text, diagnostic, uri, ast_task_ids),
        "NIKA-141" => create_duplicate_task_fix(diagnostic, uri),
        "NIKA-142" => create_invalid_schema_fix(diagnostic, uri),
        "NIKA-145" => create_missing_field_fix(text, diagnostic, uri),
        _ => None,
    }
}

/// Fix for unknown task reference using AST-aware fuzzy matching (NIKA-140)
#[cfg(feature = "lsp")]
fn create_unknown_task_fix_with_ast(
    text: &str,
    diagnostic: &Diagnostic,
    uri: &Uri,
    ast_task_ids: &[String],
) -> Option<CodeActionOrCommand> {
    let message = &diagnostic.message;
    if !message.contains("Unknown task") {
        return None;
    }

    // Extract the unknown task name from diagnostic message
    // Format: "Unknown task 'xxx' referenced"
    let unknown_task = extract_unknown_task_name(message)?;

    // Use AST task IDs if available, otherwise fall back to text extraction
    let task_ids = if !ast_task_ids.is_empty() {
        ast_task_ids.to_vec()
    } else {
        extract_task_ids(text)
    };

    if task_ids.is_empty() {
        return None;
    }

    // Find the best match using fuzzy similarity
    let best_match = find_best_match(&unknown_task, &task_ids)?;

    let edit = TextEdit {
        range: diagnostic.range,
        new_text: best_match.clone(),
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Did you mean '{}'?", best_match),
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

/// Extract the unknown task name from a diagnostic message
#[cfg(feature = "lsp")]
fn extract_unknown_task_name(message: &str) -> Option<String> {
    // Pattern: "Unknown task 'xxx' referenced" or "Unknown task 'xxx'"
    let start = message.find('\'')?;
    let end = message[start + 1..].find('\'')?;
    Some(message[start + 1..start + 1 + end].to_string())
}

/// Find the best matching task ID using fuzzy similarity
#[cfg(feature = "lsp")]
fn find_best_match(target: &str, candidates: &[String]) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }

    let target_lower = target.to_lowercase();
    let mut best_score = 0.0;
    let mut best_match = &candidates[0];

    for candidate in candidates {
        let score = fuzzy_similarity(&target_lower, &candidate.to_lowercase());
        if score > best_score {
            best_score = score;
            best_match = candidate;
        }
    }

    // Only return if similarity is above threshold
    if best_score >= 0.3 {
        Some(best_match.clone())
    } else {
        // Fall back to first candidate if no good match
        Some(candidates[0].clone())
    }
}

/// Calculate fuzzy similarity score between two strings (0.0 to 1.0)
///
/// Uses a combination of:
/// - Longest common subsequence ratio
/// - Character overlap ratio
/// - Prefix match bonus
#[cfg(feature = "lsp")]
fn fuzzy_similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let lcs_len = longest_common_subsequence_length(a, b);
    let max_len = a.len().max(b.len()) as f64;
    let lcs_ratio = lcs_len as f64 / max_len;

    // Character overlap
    let a_chars: std::collections::HashSet<char> = a.chars().collect();
    let b_chars: std::collections::HashSet<char> = b.chars().collect();
    let overlap = a_chars.intersection(&b_chars).count() as f64;
    let overlap_ratio = overlap / a_chars.len().max(b_chars.len()) as f64;

    // Prefix bonus (if one starts with the other)
    let prefix_bonus = if a.starts_with(b) || b.starts_with(a) {
        0.2
    } else {
        0.0
    };

    // Weighted combination
    (lcs_ratio * 0.5 + overlap_ratio * 0.3 + prefix_bonus).min(1.0)
}

/// Calculate the length of the longest common subsequence
#[cfg(feature = "lsp")]
fn longest_common_subsequence_length(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    // Use two rows for space efficiency
    let mut prev = vec![0; n + 1];
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        for j in 1..=n {
            if a_chars[i - 1] == b_chars[j - 1] {
                curr[j] = prev[j - 1] + 1;
            } else {
                curr[j] = prev[j].max(curr[j - 1]);
            }
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.fill(0);
    }

    prev[n]
}

/// Compute code actions for a range
#[cfg(feature = "lsp")]
pub fn compute_code_actions(
    text: &str,
    range: Range,
    diagnostics: &[Diagnostic],
    uri: Uri,
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
    uri: &Uri,
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
    _uri: &Uri,
) -> Option<CodeActionOrCommand> {
    // Handle numeric codes if needed
    None
}

/// Fix for unknown task reference (NIKA-140)
#[cfg(feature = "lsp")]
fn create_unknown_task_fix(
    text: &str,
    diagnostic: &Diagnostic,
    uri: &Uri,
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
fn create_duplicate_task_fix(diagnostic: &Diagnostic, uri: &Uri) -> Option<CodeActionOrCommand> {
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
fn create_invalid_schema_fix(diagnostic: &Diagnostic, uri: &Uri) -> Option<CodeActionOrCommand> {
    let edit = TextEdit {
        range: diagnostic.range,
        new_text: "schema: nika/workflow@0.12".to_string(),
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: "Update to schema version @0.12".to_string(),
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
    uri: &Uri,
) -> Option<CodeActionOrCommand> {
    let message = &diagnostic.message;

    // Extract the missing field name from message
    let field = if message.contains("'id'") {
        "id: new_task"
    } else if message.contains("'schema'") {
        "schema: nika/workflow@0.12"
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
fn create_refactoring_actions(text: &str, range: Range, uri: &Uri) -> Vec<CodeActionOrCommand> {
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

    // Add with: block refactoring
    if trimmed.starts_with("- id:") {
        actions.push(create_add_with_block_action(range.start.line, uri));
    }

    actions
}

/// Create action to expand shorthand infer to full form
#[cfg(feature = "lsp")]
fn create_expand_infer_action(line: &str, line_num: u32, uri: &Uri) -> Option<CodeActionOrCommand> {
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
fn create_expand_exec_action(line: &str, line_num: u32, uri: &Uri) -> Option<CodeActionOrCommand> {
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

/// Create action to add with: block for data binding
#[cfg(feature = "lsp")]
fn create_add_with_block_action(line_num: u32, uri: &Uri) -> CodeActionOrCommand {
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
        new_text: "    with:\n      input: $previous_task\n".to_string(),
    };

    let mut changes = std::collections::HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    CodeActionOrCommand::CodeAction(CodeAction {
        title: "Add with: block for data binding".to_string(),
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
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
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
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let action = create_expand_exec_action(line, 3, &uri);

        assert!(action.is_some());
        if let Some(CodeActionOrCommand::CodeAction(ca)) = action {
            assert_eq!(ca.title, "Expand to full exec form");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_code_actions_for_diagnostics() {
        let text = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    infer: \"test\"";
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();

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

    // === Phase 2: AST-aware code action tests ===

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_unknown_task_name() {
        assert_eq!(
            extract_unknown_task_name("Unknown task 'step1' referenced"),
            Some("step1".to_string())
        );
        assert_eq!(
            extract_unknown_task_name("Unknown task 'generate_content' in depends_on"),
            Some("generate_content".to_string())
        );
        assert_eq!(extract_unknown_task_name("No task mentioned"), None);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_fuzzy_similarity_exact_match() {
        assert_eq!(fuzzy_similarity("step1", "step1"), 1.0);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_fuzzy_similarity_similar() {
        // step1 vs step2 - should be reasonably similar (4/5 chars match)
        let score = fuzzy_similarity("step1", "step2");
        assert!(score > 0.5, "Expected reasonable similarity, got {}", score);

        // generate_content vs generate_contnt (typo) - should be high
        let score2 = fuzzy_similarity("generate_content", "generate_contnt");
        assert!(score2 > 0.7, "Expected high similarity, got {}", score2);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_fuzzy_similarity_different() {
        // Completely different strings
        let score = fuzzy_similarity("alpha", "omega");
        assert!(score < 0.5, "Expected low similarity, got {}", score);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_fuzzy_similarity_prefix() {
        // Prefix match should get bonus
        let score = fuzzy_similarity("step", "step1");
        assert!(score > 0.6, "Expected prefix bonus, got {}", score);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_best_match() {
        let candidates = vec![
            "step1".to_string(),
            "step2".to_string(),
            "generate_content".to_string(),
        ];

        // Typo in step1
        let result = find_best_match("setp1", &candidates);
        assert_eq!(result, Some("step1".to_string()));

        // Similar to generate_content
        let result2 = find_best_match("generate_contnt", &candidates);
        assert_eq!(result2, Some("generate_content".to_string()));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_best_match_empty_candidates() {
        let candidates: Vec<String> = vec![];
        let result = find_best_match("step1", &candidates);
        assert_eq!(result, None);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_code_actions_with_ast_unknown_task() {
        let text = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    with:
      input: $setp1
    infer: "World"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();
        ast_index.parse_document(&uri, text, 0);

        // Create diagnostic for unknown task 'setp1' (typo)
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 7,
                    character: 13,
                },
                end: Position {
                    line: 7,
                    character: 18,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String("NIKA-140".to_string())),
            source: Some("nika".to_string()),
            message: "Unknown task 'setp1' referenced".to_string(),
            related_information: None,
            tags: None,
            code_description: None,
            data: None,
        };

        let actions = compute_code_actions_with_ast(
            &ast_index,
            &uri,
            text,
            Range {
                start: Position {
                    line: 7,
                    character: 13,
                },
                end: Position {
                    line: 7,
                    character: 18,
                },
            },
            &[diagnostic],
        );

        // Should suggest 'step1' as a fix
        assert!(!actions.is_empty());
        if let CodeActionOrCommand::CodeAction(ca) = &actions[0] {
            assert!(
                ca.title.contains("step1"),
                "Expected 'step1' suggestion, got: {}",
                ca.title
            );
            assert_eq!(ca.kind, Some(CodeActionKind::QUICKFIX));
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_code_actions_with_ast_fallback() {
        // Test that it falls back gracefully when AST is not available
        let text = "schema: nika/workflow@0.12\ntasks:\n  - id: step1\n    infer: \"test\"";
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();
        // Don't parse - simulate AST not being available

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

        let actions = compute_code_actions_with_ast(
            &ast_index,
            &uri,
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
        );

        // Should still work with text-based fallback
        assert!(!actions.is_empty());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_duplicate_task_fix() {
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 5,
                    character: 4,
                },
                end: Position {
                    line: 5,
                    character: 14,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String("NIKA-141".to_string())),
            source: Some("nika".to_string()),
            message: "Duplicate task ID 'step1'".to_string(),
            related_information: None,
            tags: None,
            code_description: None,
            data: None,
        };

        let action = create_duplicate_task_fix(&diagnostic, &uri);
        assert!(action.is_some());
        if let Some(CodeActionOrCommand::CodeAction(ca)) = action {
            assert!(ca.title.contains("Rename to"));
            assert_eq!(ca.kind, Some(CodeActionKind::QUICKFIX));
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_missing_field_fix_id() {
        let text = "schema: nika/workflow@0.12\ntasks:\n  - infer: \"test\"";
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 2,
                    character: 0,
                },
                end: Position {
                    line: 2,
                    character: 10,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String("NIKA-145".to_string())),
            source: Some("nika".to_string()),
            message: "Missing required field 'id'".to_string(),
            related_information: None,
            tags: None,
            code_description: None,
            data: None,
        };

        let action = create_missing_field_fix(text, &diagnostic, &uri);
        assert!(action.is_some());
        if let Some(CodeActionOrCommand::CodeAction(ca)) = action {
            assert!(ca.title.contains("id"), "Should suggest adding 'id' field");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_missing_field_fix_schema() {
        let text = "workflow: test\ntasks:\n  - id: step1\n    infer: \"test\"";
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 10,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String("NIKA-145".to_string())),
            source: Some("nika".to_string()),
            message: "Missing required field 'schema'".to_string(),
            related_information: None,
            tags: None,
            code_description: None,
            data: None,
        };

        let action = create_missing_field_fix(text, &diagnostic, &uri);
        assert!(action.is_some());
        if let Some(CodeActionOrCommand::CodeAction(ca)) = action {
            assert!(ca.title.contains("schema"));
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_refactoring_add_with_block() {
        let text = "tasks:\n  - id: step1\n    infer: \"test\"";
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let range = Range {
            start: Position {
                line: 1,
                character: 0,
            },
            end: Position {
                line: 1,
                character: 14,
            },
        };

        let actions = create_refactoring_actions(text, range, &uri);
        assert!(
            actions.iter().any(|a| {
                if let CodeActionOrCommand::CodeAction(ca) = a {
                    ca.title.contains("with:")
                } else {
                    false
                }
            }),
            "Should offer 'Add with: block' refactoring on '- id:' lines"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_no_actions_for_unknown_diagnostic_code() {
        let text = "schema: nika/workflow@0.12";
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 10,
                },
            },
            severity: Some(DiagnosticSeverity::WARNING),
            code: Some(NumberOrString::String("NIKA-999".to_string())),
            source: Some("nika".to_string()),
            message: "Some unknown warning".to_string(),
            related_information: None,
            tags: None,
            code_description: None,
            data: None,
        };

        let action = create_quickfix_for_diagnostic(text, &diagnostic, &uri);
        assert!(
            action.is_none(),
            "Unknown NIKA codes should not produce quick fixes"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_code_actions_no_diagnostics_still_has_refactoring() {
        let text = "tasks:\n  - id: step1\n    infer: \"test\"";
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let range = Range {
            start: Position {
                line: 2,
                character: 4,
            },
            end: Position {
                line: 2,
                character: 20,
            },
        };

        let actions = compute_code_actions(text, range, &[], uri);
        // Even without diagnostics, refactoring actions should be available
        assert!(
            actions.iter().any(|a| {
                if let CodeActionOrCommand::CodeAction(ca) = a {
                    ca.kind == Some(CodeActionKind::REFACTOR)
                } else {
                    false
                }
            }),
            "Should still offer refactoring actions without diagnostics"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_longest_common_subsequence() {
        assert_eq!(longest_common_subsequence_length("step1", "step1"), 5);
        assert_eq!(longest_common_subsequence_length("step1", "step2"), 4); // "step"
        assert_eq!(longest_common_subsequence_length("abc", "xyz"), 0);
        assert_eq!(
            longest_common_subsequence_length("generate", "generate_content"),
            8
        );
    }
}
