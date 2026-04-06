//! AST integration module for LSP features.
//!
//! This module provides AST-based utilities for LSP operations,
//! replacing string-based pattern matching with proper parsing.
//!
//! The module uses Nika's two-phase IR:
//! 1. Raw AST (RawWorkflow) - from YAML parsing with spans
//! 2. Analyzed AST (AnalyzedWorkflow) - after validation and reference resolution

use nika_core::ast::raw::{parse, RawWorkflow};
use nika_core::source::{FileId, Span};

/// Result of parsing a workflow, containing both raw AST and any errors.
#[derive(Debug)]
pub struct ParsedWorkflow {
    /// The raw workflow AST (if parsing succeeded).
    pub workflow: Option<RawWorkflow>,
    /// Whether parsing was successful.
    /// Note: Kept for future diagnostic features.
    #[allow(dead_code)]
    pub success: bool,
}

/// Task definition with its source location.
#[derive(Debug, Clone)]
pub struct TaskDefinition {
    /// Task ID.
    pub id: String,
    /// Source span of the entire task.
    /// Note: Kept for future features (multi-file navigation, refactoring).
    #[allow(dead_code)]
    pub span: Span,
    /// Line number of the task definition (0-indexed).
    pub line: u32,
    /// Column number of the task definition (0-indexed).
    pub column: u32,
}

/// Parse workflow YAML into raw AST.
///
/// Returns a ParsedWorkflow containing the AST if successful.
/// This function is tolerant of partial/incomplete YAML that may
/// occur during editing.
pub fn parse_workflow(content: &str) -> ParsedWorkflow {
    match parse(content, FileId(0)) {
        Ok(workflow) => ParsedWorkflow {
            workflow: Some(workflow),
            success: true,
        },
        Err(_) => ParsedWorkflow {
            workflow: None,
            success: false,
        },
    }
}

/// Extract all task IDs from the workflow AST.
///
/// This replaces the string-based `extract_task_ids()` in backend.rs
/// with proper AST traversal.
#[allow(dead_code)]
pub fn extract_task_ids_from_ast(content: &str) -> Vec<String> {
    let parsed = parse_workflow(content);

    match parsed.workflow {
        Some(workflow) => workflow
            .tasks
            .value
            .iter()
            .map(|task| task.value.id.value.clone())
            .collect(),
        None => {
            // Fallback to string-based extraction for incomplete YAML
            extract_task_ids_fallback(content)
        }
    }
}

/// Fallback task ID extraction using string patterns.
///
/// Used when AST parsing fails (e.g., incomplete YAML during editing).
#[allow(dead_code)]
fn extract_task_ids_fallback(content: &str) -> Vec<String> {
    let mut task_ids = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("- id:") || trimmed.starts_with("-id:") {
            if let Some(id_part) = trimmed
                .strip_prefix("- id:")
                .or_else(|| trimmed.strip_prefix("-id:"))
            {
                let id = id_part.trim().trim_matches('"').trim_matches('\'');
                if !id.is_empty() {
                    task_ids.push(id.to_string());
                }
            }
        }
    }

    task_ids
}

/// Find all task definitions with their spans.
///
/// Returns a list of TaskDefinition structs containing task IDs
/// and their source locations for goto-definition support.
pub fn find_task_definitions(content: &str) -> Vec<TaskDefinition> {
    let parsed = parse_workflow(content);

    match parsed.workflow {
        Some(workflow) => {
            let mut definitions = Vec::new();

            for task in &workflow.tasks.value {
                // Convert span to line/column
                let (line, column) = span_to_line_col(content, &task.span);

                definitions.push(TaskDefinition {
                    id: task.value.id.value.clone(),
                    span: task.span,
                    line,
                    column,
                });
            }

            definitions
        }
        None => {
            // Fallback: find task definitions using string patterns
            find_task_definitions_fallback(content)
        }
    }
}

/// Fallback task definition finding using string patterns.
fn find_task_definitions_fallback(content: &str) -> Vec<TaskDefinition> {
    let mut definitions = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("- id:") {
            if let Some(id_part) = trimmed.strip_prefix("- id:") {
                let id = id_part.trim().trim_matches('"').trim_matches('\'');
                if !id.is_empty() {
                    // Calculate byte offset for the span
                    let line_start = content
                        .lines()
                        .take(line_num)
                        .map(|l| l.len() + 1) // +1 for newline
                        .sum::<usize>();
                    let line_end = line_start + line.len();

                    definitions.push(TaskDefinition {
                        id: id.to_string(),
                        span: Span::new(FileId(0), line_start as u32, line_end as u32),
                        line: line_num as u32,
                        column: 0,
                    });
                }
            }
        }
    }

    definitions
}

/// Find a specific task definition by ID.
///
/// Returns the TaskDefinition if found, None otherwise.
pub fn find_task_by_id(content: &str, task_id: &str) -> Option<TaskDefinition> {
    find_task_definitions(content)
        .into_iter()
        .find(|def| def.id == task_id)
}

/// Convert a byte span to line and column numbers.
///
/// Returns (line, column) as 0-indexed values.
fn span_to_line_col(content: &str, span: &Span) -> (u32, u32) {
    let mut line = 0u32;
    let mut col = 0u32;
    let mut current_offset = 0usize;

    for ch in content.chars() {
        if current_offset >= span.start.0 as usize {
            return (line, col);
        }

        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        current_offset += ch.len_utf8();
    }

    (line, col)
}

/// Get the line content and range for a task definition.
///
/// Returns (line_content, start_col, end_col) for the task's id line.
/// Note: Kept for future features (range selection, rename refactoring).
#[allow(dead_code)]
pub fn get_task_line_range(content: &str, task_def: &TaskDefinition) -> Option<(String, u32, u32)> {
    let lines: Vec<&str> = content.lines().collect();
    let line_idx = task_def.line as usize;

    if line_idx < lines.len() {
        let line = lines[line_idx];
        Some((line.to_string(), 0, line.len() as u32))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_task_ids_from_ast() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    exec: "echo test"
  - id: step3
    use:
      data: step1
    infer: "World"
"#;
        let ids = extract_task_ids_from_ast(yaml);
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&"step1".to_string()));
        assert!(ids.contains(&"step2".to_string()));
        assert!(ids.contains(&"step3".to_string()));
    }

    #[test]
    fn test_extract_task_ids_fallback() {
        // Incomplete YAML that won't parse
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: step1
    infer:
  - id: step2
"#;
        let ids = extract_task_ids_from_ast(yaml);
        // Should fall back to string-based extraction
        assert!(ids.len() >= 2);
    }

    #[test]
    fn test_find_task_definitions() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: first
    infer: "Hello"
  - id: second
    exec: "echo test"
"#;
        let defs = find_task_definitions(yaml);
        assert_eq!(defs.len(), 2);

        let first = defs.iter().find(|d| d.id == "first").unwrap();
        assert!(first.line >= 2); // After schema: and tasks:

        let second = defs.iter().find(|d| d.id == "second").unwrap();
        assert!(second.line > first.line);
    }

    #[test]
    fn test_find_task_by_id() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: target_task
    infer: "Find me"
"#;
        let result = find_task_by_id(yaml, "target_task");
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "target_task");

        let not_found = find_task_by_id(yaml, "nonexistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_span_to_line_col() {
        let content = "line1\nline2\nline3";
        // line1 is bytes 0-5, line2 starts at 6
        let span = Span::new(FileId(0), 6, 11); // "line2"
        let (line, col) = span_to_line_col(content, &span);
        assert_eq!(line, 1); // 0-indexed, second line
        assert_eq!(col, 0);
    }

    #[test]
    fn test_parse_workflow_success() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer: "Hello"
"#;
        let result = parse_workflow(yaml);
        assert!(result.success);
        assert!(result.workflow.is_some());
    }

    #[test]
    fn test_parse_workflow_failure() {
        let yaml = "not: valid: yaml: at: all:";
        let result = parse_workflow(yaml);
        assert!(!result.success);
        assert!(result.workflow.is_none());
    }

    #[test]
    fn test_get_task_line_range() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: my_task
    infer: "Hello"
"#;
        let defs = find_task_definitions(yaml);
        let task = defs.iter().find(|d| d.id == "my_task").unwrap();

        let result = get_task_line_range(yaml, task);
        assert!(result.is_some());
        let (line_content, start, _end) = result.unwrap();
        assert!(line_content.contains("id:"));
        assert!(line_content.contains("my_task"));
        assert_eq!(start, 0);
    }

    #[test]
    fn test_empty_content() {
        let ids = extract_task_ids_from_ast("");
        assert!(ids.is_empty());

        let defs = find_task_definitions("");
        assert!(defs.is_empty());
    }

    #[test]
    fn test_quoted_task_ids() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: "quoted-id"
    infer: "Hello"
  - id: 'single-quoted'
    exec: "test"
"#;
        let ids = extract_task_ids_from_ast(yaml);
        // AST parser should handle quoted IDs
        assert!(ids.len() >= 2);
    }
}
