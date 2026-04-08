// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Go to Definition Handler
//!
//! Provides definition lookup for Nika workflow elements:
//! - Task references in `with:` bindings → task definition
//! - Template variables → binding declaration
//! - Include paths → included file
//!
//! ## Phase 2 Architecture
//!
//! The definition handler can use AstIndex for accurate span-based locations:
//! - Use `find_definition_with_ast` when AstIndex is available
//! - TaskTable provides O(1) task lookup by name
//! - Spans from AST give precise definition locations

#[cfg(feature = "lsp")]
use tower_lsp_server::ls_types::*;

#[cfg(feature = "lsp")]
pub use tower_lsp_server::ls_types::Uri;

#[cfg(feature = "lsp")]
use super::super::ast_index::AstIndex;
#[cfg(feature = "lsp")]
use crate::lsp::conversion::{position_to_offset, span_to_range};

/// Find definition location using AstIndex for accurate positions
///
/// Uses the parsed AST to find task definitions with precise span locations.
/// Falls back to text-based search if AST lookup fails.
#[cfg(feature = "lsp")]
pub fn find_definition_with_ast(
    ast_index: &AstIndex,
    uri: &Uri,
    text: &str,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    let offset = position_to_offset(position, text);
    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;

    if line_idx >= lines.len() {
        return None;
    }

    let line = lines[line_idx];

    // Try depends_on definition (cursor on a task ID inside depends_on array)
    if let Some(response) =
        find_depends_on_definition_with_ast(text, line, position.character as usize, ast_index, uri)
    {
        return Some(response);
    }

    // Try AST-aware task reference definition first
    if let Some(response) = find_task_reference_definition_with_ast(text, line, ast_index, uri) {
        return Some(response);
    }

    // Try AST-aware template definition
    if let Some(response) =
        find_template_definition_with_ast(text, line, position.character as usize, ast_index, uri)
    {
        return Some(response);
    }

    // Fall back to text-based include definition (no AST support yet)
    if let Some(response) = find_include_definition(line, offset, uri) {
        return Some(response);
    }

    None
}

/// Find definition location for element at position
#[cfg(feature = "lsp")]
pub fn find_definition(text: &str, position: Position, uri: Uri) -> Option<GotoDefinitionResponse> {
    let offset = position_to_offset(position, text);
    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;

    if line_idx >= lines.len() {
        return None;
    }

    let line = lines[line_idx];

    // Check for depends_on task reference
    if let Some(response) =
        find_depends_on_definition(text, line, position.character as usize, &uri)
    {
        return Some(response);
    }

    // Check for task reference in with: block
    if let Some(response) = find_task_reference_definition(text, line, &uri) {
        return Some(response);
    }

    // Check for template variable reference
    if let Some(response) = find_template_definition(text, line, position.character as usize, &uri)
    {
        return Some(response);
    }

    // Check for include path
    if let Some(response) = find_include_definition(line, offset, &uri) {
        return Some(response);
    }

    None
}

/// Find definition for task reference in with: block
#[cfg(feature = "lsp")]
fn find_task_reference_definition(
    text: &str,
    line: &str,
    uri: &Uri,
) -> Option<GotoDefinitionResponse> {
    let trimmed = line.trim();

    // Pattern: "alias: task_id" in with: block
    if let Some(colon_pos) = trimmed.find(':') {
        let value = trimmed[colon_pos + 1..].trim();

        // Skip if value starts with special characters (not a task ref)
        if value.starts_with('{')
            || value.starts_with('[')
            || value.starts_with('"')
            || value.starts_with('\'')
        {
            return None;
        }

        // Strip $ prefix if present (implicit output syntax)
        let task_id = value.trim_start_matches('$').trim();

        // Handle path like "task_id.field"
        let task_id = task_id.split('.').next().unwrap_or(task_id);

        if task_id.is_empty() {
            return None;
        }

        // Find task definition
        if let Some(location) = find_task_location(text, task_id, uri) {
            return Some(GotoDefinitionResponse::Scalar(location));
        }
    }

    None
}

/// Find definition for template variable like {{with.alias}}
#[cfg(feature = "lsp")]
fn find_template_definition(
    text: &str,
    line: &str,
    col: usize,
    uri: &Uri,
) -> Option<GotoDefinitionResponse> {
    // Find template at cursor position
    let mut search_start = 0;
    while let Some(start) = line[search_start..].find("{{") {
        let abs_start = search_start + start;
        if let Some(end) = line[abs_start..].find("}}") {
            let abs_end = abs_start + end + 2;
            if col >= abs_start && col <= abs_end {
                let template = &line[abs_start + 2..abs_start + end];
                let template = template.trim();

                // Handle {{with.alias}} → find with: block
                if let Some(alias) = template.strip_prefix("with.") {
                    if let Some(location) = find_with_binding_location(text, alias, uri) {
                        return Some(GotoDefinitionResponse::Scalar(location));
                    }
                }

                // Handle {{context.files.name}} → find context: block
                if let Some(name) = template.strip_prefix("context.files.") {
                    if let Some(location) = find_context_file_location(text, name, uri) {
                        return Some(GotoDefinitionResponse::Scalar(location));
                    }
                }
            }
            search_start = abs_end;
        } else {
            break;
        }
    }

    None
}

/// Extract a file path from an `include:` block's `path:` field.
///
/// Handles various YAML formats:
/// - `path: ./lib/seo-tasks.nika.yaml`
/// - `- path: ./lib/seo-tasks.nika.yaml`
/// - `path: "./lib/seo-tasks.nika.yaml"` (quoted)
#[cfg(feature = "lsp")]
fn extract_include_path(line: &str) -> Option<&str> {
    let trimmed = line.trim();

    // Strip leading list marker `- `
    let trimmed = trimmed.strip_prefix("- ").unwrap_or(trimmed);

    // Match `path:` field
    let value = if let Some(rest) = trimmed.strip_prefix("path:") {
        rest.trim()
    } else {
        return None;
    };

    if value.is_empty() {
        return None;
    }

    // Strip surrounding quotes (single or double)
    let value = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(value);

    if value.is_empty() {
        return None;
    }

    Some(value)
}

/// Find include path definition (returns file URI)
///
/// Resolves relative paths from `path:` fields in `include:` blocks against
/// the document URI's parent directory. Returns a definition location pointing
/// to the beginning of the resolved file.
#[cfg(feature = "lsp")]
fn find_include_definition(
    line: &str,
    _offset: usize,
    doc_uri: &Uri,
) -> Option<GotoDefinitionResponse> {
    let path_str = extract_include_path(line)?;

    // Resolve the document URI to a filesystem path
    let doc_path = doc_uri.to_file_path()?;
    let parent_dir = doc_path.parent()?;

    // Resolve the include path relative to the document's parent directory
    let resolved = parent_dir.join(path_str);

    // Canonicalize to resolve `./` and `../` components.
    // This also verifies the file exists on disk.
    let canonical = resolved.canonicalize().ok()?;

    let target_uri = Uri::from_file_path(canonical)?;

    Some(GotoDefinitionResponse::Scalar(Location::new(
        target_uri,
        Range::default(),
    )))
}

// ============================================================================
// AST-Aware Helper Functions (Phase 2)
// ============================================================================

/// Find definition for task reference using AstIndex
#[cfg(feature = "lsp")]
fn find_task_reference_definition_with_ast(
    text: &str,
    line: &str,
    ast_index: &AstIndex,
    uri: &Uri,
) -> Option<GotoDefinitionResponse> {
    let trimmed = line.trim();

    // Pattern: "alias: task_id" in with: block
    if let Some(colon_pos) = trimmed.find(':') {
        let value = trimmed[colon_pos + 1..].trim();

        // Skip if value starts with special characters (not a task ref)
        if value.starts_with('{')
            || value.starts_with('[')
            || value.starts_with('"')
            || value.starts_with('\'')
        {
            return None;
        }

        // Strip $ prefix if present (implicit output syntax)
        let task_id = value.trim_start_matches('$').trim();

        // Handle path like "task_id.field"
        let task_id = task_id.split('.').next().unwrap_or(task_id);

        if task_id.is_empty() {
            return None;
        }

        // Try AST-aware task location first
        if let Some(location) = find_task_location_with_ast(text, task_id, ast_index, uri) {
            return Some(GotoDefinitionResponse::Scalar(location));
        }

        // Fall back to text-based search
        if let Some(location) = find_task_location(text, task_id, uri) {
            return Some(GotoDefinitionResponse::Scalar(location));
        }
    }

    None
}

/// Find definition for template variable using AstIndex
#[cfg(feature = "lsp")]
fn find_template_definition_with_ast(
    text: &str,
    line: &str,
    col: usize,
    ast_index: &AstIndex,
    uri: &Uri,
) -> Option<GotoDefinitionResponse> {
    // Find template at cursor position
    let mut search_start = 0;
    while let Some(start) = line[search_start..].find("{{") {
        let abs_start = search_start + start;
        if let Some(end) = line[abs_start..].find("}}") {
            let abs_end = abs_start + end + 2;
            if col >= abs_start && col <= abs_end {
                let template = &line[abs_start + 2..abs_start + end];
                let template = template.trim();

                // Handle {{with.alias}} → find task referenced by binding
                if let Some(alias) = template.strip_prefix("with.") {
                    // Try to find the task that this binding references via AST
                    if let Some(location) = find_with_binding_location(text, alias, uri) {
                        return Some(GotoDefinitionResponse::Scalar(location));
                    }
                }

                // Handle {{context.files.name}} → find context: block
                if let Some(name) = template.strip_prefix("context.files.") {
                    if let Some(location) = find_context_file_location(text, name, uri) {
                        return Some(GotoDefinitionResponse::Scalar(location));
                    }
                }

                // Handle {{$task}} → find task definition
                if let Some(task_id) = template.strip_prefix('$') {
                    if let Some(location) =
                        find_task_location_with_ast(text, task_id, ast_index, uri)
                    {
                        return Some(GotoDefinitionResponse::Scalar(location));
                    }
                }
            }
            search_start = abs_end;
        } else {
            break;
        }
    }

    None
}

/// Find task location using AstIndex for precise span positions
#[cfg(feature = "lsp")]
fn find_task_location_with_ast(
    text: &str,
    task_id: &str,
    ast_index: &AstIndex,
    uri: &Uri,
) -> Option<Location> {
    // Try to get task from AST cache
    if let Some(cached) = ast_index.get(uri) {
        if let Some(ref analyzed) = cached.analyzed {
            for task in &analyzed.tasks {
                if task.name == task_id {
                    // Use span from AST for accurate location
                    return Some(Location {
                        uri: uri.clone(),
                        range: span_to_range(&task.span, text),
                    });
                }
            }
        }
    }

    None
}

// ============================================================================
// depends_on Definition Helpers
// ============================================================================

/// Extract the task ID at the cursor position from a depends_on line.
///
/// Handles both inline array `depends_on: [step1, step2]` and scalar
/// `depends_on: step1` forms. Returns `None` when the cursor is not on
/// a recognisable task identifier.
#[cfg(feature = "lsp")]
fn extract_depends_on_task_at_cursor(line: &str, col: usize) -> Option<String> {
    let trimmed = line.trim();

    // Must be a depends_on line
    let stripped = trimmed.strip_prefix("depends_on:")?;
    let value = stripped.trim();

    if value.is_empty() {
        return None;
    }

    // Scalar form: depends_on: step1
    if !value.starts_with('[') {
        let task_id = value.trim_matches('"').trim_matches('\'');
        if !task_id.is_empty() {
            // Check cursor is on the value
            if let Some(val_start) = line.find(task_id) {
                let val_end = val_start + task_id.len();
                if col >= val_start && col < val_end {
                    return Some(task_id.to_string());
                }
            }
        }
        return None;
    }

    // Array form: depends_on: [step1, step2]
    // Walk through the items and find which one the cursor is over
    let bracket_start = line.find('[')? + 1;
    let bracket_end = line.find(']')?;
    let inner = &line[bracket_start..bracket_end];

    let mut offset = bracket_start;
    for part in inner.split(',') {
        let ref_name = part.trim().trim_matches('"').trim_matches('\'');
        if ref_name.is_empty() {
            offset += part.len() + 1; // +1 for the comma
            continue;
        }

        // Find the ref_name within this part (skip leading whitespace)
        let ref_start_in_part = part.find(ref_name).unwrap_or(0);
        let abs_start = offset + ref_start_in_part;
        let abs_end = abs_start + ref_name.len();

        if col >= abs_start && col < abs_end {
            return Some(ref_name.to_string());
        }

        offset += part.len() + 1; // +1 for the comma
    }

    None
}

/// Find definition for a task ID inside `depends_on:` (text-based)
#[cfg(feature = "lsp")]
fn find_depends_on_definition(
    text: &str,
    line: &str,
    col: usize,
    uri: &Uri,
) -> Option<GotoDefinitionResponse> {
    let task_id = extract_depends_on_task_at_cursor(line, col)?;
    let location = find_task_location(text, &task_id, uri)?;
    Some(GotoDefinitionResponse::Scalar(location))
}

/// Find definition for a task ID inside `depends_on:` (AST-aware)
#[cfg(feature = "lsp")]
fn find_depends_on_definition_with_ast(
    text: &str,
    line: &str,
    col: usize,
    ast_index: &AstIndex,
    uri: &Uri,
) -> Option<GotoDefinitionResponse> {
    let task_id = extract_depends_on_task_at_cursor(line, col)?;

    // Try AST-aware location first
    if let Some(location) = find_task_location_with_ast(text, &task_id, ast_index, uri) {
        return Some(GotoDefinitionResponse::Scalar(location));
    }

    // Fall back to text-based search
    let location = find_task_location(text, &task_id, uri)?;
    Some(GotoDefinitionResponse::Scalar(location))
}

// ============================================================================
// Text-Based Helper Functions
// ============================================================================

/// Find the location of a task definition by ID (text-based)
#[cfg(feature = "lsp")]
fn find_task_location(text: &str, task_id: &str, uri: &Uri) -> Option<Location> {
    let pattern = format!("id: {}", task_id);
    let list_pattern = format!("- id: {}", task_id);

    for (line_num, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        // Match both "id: task_id" and "- id: task_id" (list item syntax)
        let is_match = trimmed.starts_with(&pattern)
            || trimmed.starts_with(&list_pattern)
            || trimmed == format!("id: \"{}\"", task_id)
            || trimmed == format!("- id: \"{}\"", task_id);

        if is_match {
            let indent = line.len() - trimmed.len();
            return Some(Location {
                uri: uri.clone(),
                range: Range {
                    start: Position {
                        line: line_num as u32,
                        character: indent as u32,
                    },
                    end: Position {
                        line: line_num as u32,
                        character: line.len() as u32,
                    },
                },
            });
        }
    }

    None
}

/// Find the location of a with: binding by alias
#[cfg(feature = "lsp")]
fn find_with_binding_location(text: &str, alias: &str, uri: &Uri) -> Option<Location> {
    let pattern = format!("{}:", alias);
    let mut in_with_block = false;

    for (line_num, line) in text.lines().enumerate() {
        let trimmed = line.trim();

        // Track when we enter a with: block
        if trimmed == "with:" || trimmed.starts_with("with:") {
            in_with_block = true;
            continue;
        }

        // Exit with block when we see a non-indented line or new field
        if in_with_block && !line.starts_with("  ") && !line.starts_with("\t") && !line.is_empty() {
            in_with_block = false;
        }

        // Check for alias in with block
        if in_with_block && trimmed.starts_with(&pattern) {
            let indent = line.len() - trimmed.len();
            return Some(Location {
                uri: uri.clone(),
                range: Range {
                    start: Position {
                        line: line_num as u32,
                        character: indent as u32,
                    },
                    end: Position {
                        line: line_num as u32,
                        character: (indent + alias.len()) as u32,
                    },
                },
            });
        }
    }

    None
}

/// Find the location of a context file by name
#[cfg(feature = "lsp")]
fn find_context_file_location(text: &str, name: &str, uri: &Uri) -> Option<Location> {
    let pattern = format!("{}:", name);
    let mut in_context_files = false;

    for (line_num, line) in text.lines().enumerate() {
        let trimmed = line.trim();

        // Track when we enter context.files block
        if trimmed == "files:" {
            in_context_files = true;
            continue;
        }

        // Exit when we see a less-indented line
        if in_context_files && !line.starts_with("    ") && !line.is_empty() {
            in_context_files = false;
        }

        // Check for file name
        if in_context_files && trimmed.starts_with(&pattern) {
            let indent = line.len() - trimmed.len();
            return Some(Location {
                uri: uri.clone(),
                range: Range {
                    start: Position {
                        line: line_num as u32,
                        character: indent as u32,
                    },
                    end: Position {
                        line: line_num as u32,
                        character: line.len() as u32,
                    },
                },
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_task_location() {
        let text = r#"
tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    with:
      input: $step1
    infer: "World"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let location = find_task_location(text, "step1", &uri);

        assert!(location.is_some());
        let loc = location.unwrap();
        assert_eq!(loc.range.start.line, 2); // Line with "id: step1"
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_with_binding_location() {
        let text = r#"
- id: step2
  with:
    input: step1
    data: other_task
  infer: "Process {{with.input}}"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let location = find_with_binding_location(text, "input", &uri);

        assert!(location.is_some());
        let loc = location.unwrap();
        assert_eq!(loc.range.start.line, 3); // Line with "input: step1"
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_task_reference_with_dollar() {
        let text = r#"
tasks:
  - id: generate
    infer: "Generate"
  - id: process
    with:
      input: $generate
    infer: "Process"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let response = find_task_reference_definition(text, "      input: $generate", &uri);

        assert!(response.is_some());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_context_file_location() {
        let text = r#"
context:
  files:
    brand: ./brand.md
    data: ./data.json
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let location = find_context_file_location(text, "brand", &uri);

        assert!(location.is_some());
        let loc = location.unwrap();
        assert_eq!(loc.range.start.line, 3); // Line with "brand: ./brand.md"
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_no_definition_for_unknown_task() {
        let text = r#"
tasks:
  - id: step1
    infer: "Hello"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let location = find_task_location(text, "nonexistent", &uri);

        assert!(location.is_none());
    }

    // ============================================================================
    // AST-Aware Definition Tests (Phase 2)
    // ============================================================================

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_definition_with_ast_task_reference() {
        let text = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    with:
      input: $step1
    infer: "World"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();

        // Parse document into AST index
        ast_index.parse_document(&uri, text, 0);

        // Position on "step1" in the with: block (line 7, after "input: ")
        let position = Position {
            line: 7,
            character: 12,
        };
        let response = find_definition_with_ast(&ast_index, &uri, text, position);

        assert!(response.is_some());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_task_location_with_ast_uses_spans() {
        let text = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: generate
    model: gpt-4o-mini
    infer: "Generate something"
  - id: process
    model: gpt-4o-mini
    infer: "Process"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();

        // Parse document into AST index
        ast_index.parse_document(&uri, text, 0);

        // Look up task location via AST
        let location = find_task_location_with_ast(text, "generate", &ast_index, &uri);

        assert!(location.is_some());
        let loc = location.unwrap();
        // Should point to the task definition
        assert!(loc.range.start.line >= 3);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_task_location_with_ast_fallback_to_text() {
        // When AST has no cached document, falls back to text search
        let text = r#"
tasks:
  - id: step1
    infer: "Hello"
"#;
        let uri = "file:///uncached.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();
        // Deliberately NOT parsing into AST index

        // AST lookup should return None (no cache)
        let location = find_task_location_with_ast(text, "step1", &ast_index, &uri);
        assert!(location.is_none());

        // But text-based lookup should still work
        let text_location = find_task_location(text, "step1", &uri);
        assert!(text_location.is_some());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_template_definition_with_ast_dollar_syntax() {
        let text = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: data
    model: gpt-4o-mini
    infer: "Get data"
  - id: process
    model: gpt-4o-mini
    infer: "Process {{$data}}"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();

        // Parse document into AST index
        ast_index.parse_document(&uri, text, 0);

        // Position inside {{$data}} template (line 6, character within the template)
        let line = r#"    infer: "Process {{$data}}""#;
        let col = 23; // Inside {{$data}}

        let response = find_template_definition_with_ast(text, line, col, &ast_index, &uri);

        assert!(response.is_some());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_task_location_quoted_id() {
        let text = r#"
tasks:
  - id: "quoted_task"
    infer: "Hello"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let location = find_task_location(text, "quoted_task", &uri);
        assert!(location.is_some(), "Should find quoted task ID");
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_task_reference_with_path_syntax() {
        let text = r#"
tasks:
  - id: api_call
    fetch:
      url: "https://example.com"
  - id: process
    with:
      data: api_call.body.json
    infer: "Process"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        // The path "api_call.body.json" should resolve to task "api_call"
        let response = find_task_reference_definition(text, "      data: api_call.body.json", &uri);
        assert!(
            response.is_some(),
            "Should resolve path-style task reference"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_definition_skips_json_values() {
        let text = "tasks:\n  - id: step1\n    infer: \"test\"";
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        // A line with a JSON object value should not be treated as a task ref
        let response = find_task_reference_definition(text, "      data: {\"key\": \"val\"}", &uri);
        assert!(response.is_none());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_definition_skips_array_values() {
        let text = "tasks:\n  - id: step1\n    infer: \"test\"";
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let response = find_task_reference_definition(text, "      items: [\"a\", \"b\"]", &uri);
        assert!(response.is_none());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_definition_skips_string_values() {
        let text = "tasks:\n  - id: step1\n    infer: \"test\"";
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let response = find_task_reference_definition(text, "      prompt: \"hello world\"", &uri);
        assert!(response.is_none());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_with_binding_location_multiple_bindings() {
        let text = r#"
- id: step3
  with:
    first: step1
    second: step2
  infer: "{{with.first}} and {{with.second}}"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();

        let loc1 = find_with_binding_location(text, "first", &uri);
        let loc2 = find_with_binding_location(text, "second", &uri);

        assert!(loc1.is_some());
        assert!(loc2.is_some());
        assert_ne!(
            loc1.unwrap().range.start.line,
            loc2.unwrap().range.start.line
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_definition_empty_text() {
        let text = "";
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let result = find_definition(
            text,
            Position {
                line: 0,
                character: 0,
            },
            uri,
        );
        assert!(result.is_none());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_definition_with_ast_no_match() {
        let text = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: step1
    infer: "Hello"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();

        // Parse document into AST index
        ast_index.parse_document(&uri, text, 0);

        // Position on a line with no definition target (line 0, schema)
        let position = Position {
            line: 0,
            character: 5,
        };
        let response = find_definition_with_ast(&ast_index, &uri, text, position);

        assert!(response.is_none());
    }

    // ============================================================================
    // Include Path Definition Tests (FEAT-001)
    // ============================================================================

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_include_path_basic() {
        let line = "    - path: ./lib/seo-tasks.nika.yaml";
        let result = extract_include_path(line);
        assert_eq!(result, Some("./lib/seo-tasks.nika.yaml"));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_include_path_without_list_marker() {
        let line = "    path: ./lib/seo-tasks.nika.yaml";
        let result = extract_include_path(line);
        assert_eq!(result, Some("./lib/seo-tasks.nika.yaml"));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_include_path_double_quoted() {
        let line = r#"    - path: "./lib/seo-tasks.nika.yaml""#;
        let result = extract_include_path(line);
        assert_eq!(result, Some("./lib/seo-tasks.nika.yaml"));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_include_path_single_quoted() {
        let line = "    - path: './lib/seo-tasks.nika.yaml'";
        let result = extract_include_path(line);
        assert_eq!(result, Some("./lib/seo-tasks.nika.yaml"));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_include_path_no_match() {
        let line = "    prefix: seo_";
        let result = extract_include_path(line);
        assert!(result.is_none());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_include_path_empty_value() {
        let line = "    path:";
        let result = extract_include_path(line);
        assert!(result.is_none());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_include_path_non_path_line() {
        let line = "    infer: \"Generate a title\"";
        let result = extract_include_path(line);
        assert!(result.is_none());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_include_definition_with_existing_file() {
        // Create a temp directory with a workflow file
        let dir = std::env::temp_dir().join("nika_lsp_test_include");
        let _ = std::fs::create_dir_all(dir.join("lib"));
        let target_file = dir.join("lib").join("seo-tasks.nika.yaml");
        std::fs::write(
            &target_file,
            "schema: nika/workflow@0.12\nworkflow: seo\ntasks: []",
        )
        .unwrap();

        let main_file = dir.join("main.nika.yaml");
        let doc_uri = Uri::from_file_path(&main_file).unwrap();

        let line = "    - path: ./lib/seo-tasks.nika.yaml";
        let result = find_include_definition(line, 0, &doc_uri);

        assert!(
            result.is_some(),
            "Should resolve include path to existing file"
        );

        // Verify the target URI points to the right file
        if let Some(GotoDefinitionResponse::Scalar(location)) = result {
            // canonicalize() resolves symlinks (e.g. /var -> /private/var on macOS)
            let canonical_target = target_file.canonicalize().unwrap();
            let target_uri = Uri::from_file_path(&canonical_target).unwrap();
            assert_eq!(location.uri, target_uri);
            // Range should be at the start of the file
            assert_eq!(location.range.start.line, 0);
            assert_eq!(location.range.start.character, 0);
        } else {
            panic!("Expected GotoDefinitionResponse::Scalar");
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_include_definition_nonexistent_file() {
        let doc_uri = "file:///project/main.nika.yaml".parse::<Uri>().unwrap();
        let line = "    - path: ./lib/nonexistent.nika.yaml";
        let result = find_include_definition(line, 0, &doc_uri);
        assert!(result.is_none(), "Should return None for non-existent file");
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_include_definition_non_path_line() {
        let doc_uri = "file:///project/main.nika.yaml".parse::<Uri>().unwrap();
        let line = "    prefix: seo_";
        let result = find_include_definition(line, 0, &doc_uri);
        assert!(result.is_none(), "Should return None for non-path line");
    }

    // ========================================================================
    // depends_on Go-to-Definition Tests
    // ========================================================================

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_depends_on_task_scalar() {
        let line = "    depends_on: step1";
        // Cursor on 's' of step1 (col 16)
        let result = extract_depends_on_task_at_cursor(line, 16);
        assert_eq!(result, Some("step1".to_string()));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_depends_on_task_array_first() {
        let line = "    depends_on: [step1, step2]";
        // Find exact position of 'step1' after [
        let step1_start = line.find('[').unwrap() + 1;
        let result = extract_depends_on_task_at_cursor(line, step1_start);
        assert_eq!(result, Some("step1".to_string()));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_depends_on_task_array_second() {
        let line = "    depends_on: [step1, step2]";
        // Find position of 'step2'
        let step2_start = line.find("step2").unwrap();
        let result = extract_depends_on_task_at_cursor(line, step2_start);
        assert_eq!(result, Some("step2".to_string()));
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_depends_on_task_not_on_id() {
        let line = "    depends_on: [step1, step2]";
        // Cursor on the comma between items
        let comma_pos = line.find(',').unwrap();
        let result = extract_depends_on_task_at_cursor(line, comma_pos);
        assert!(result.is_none());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_depends_on_non_depends_line() {
        let line = "    infer: \"test\"";
        let result = extract_depends_on_task_at_cursor(line, 4);
        assert!(result.is_none());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_depends_on_definition_text_based() {
        let text = r#"
tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    depends_on: [step1]
    infer: "World"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let line = "    depends_on: [step1]";
        let step1_pos = line.find("step1").unwrap();
        let response = find_depends_on_definition(text, line, step1_pos, &uri);

        assert!(
            response.is_some(),
            "Should find definition for task in depends_on"
        );
        if let Some(GotoDefinitionResponse::Scalar(location)) = response {
            assert_eq!(
                location.range.start.line, 2,
                "Should point to step1 id line"
            );
        } else {
            panic!("Expected GotoDefinitionResponse::Scalar");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_depends_on_definition_with_ast() {
        let text = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: generate
    infer: "Generate content"
  - id: process
    depends_on: [generate]
    infer: "Process"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let ast_index = AstIndex::new();
        ast_index.parse_document(&uri, text, 0);

        // Position on "generate" inside depends_on (line 6)
        let position = Position {
            line: 6,
            character: 17, // inside "generate" within [generate]
        };
        let response = find_definition_with_ast(&ast_index, &uri, text, position);

        assert!(
            response.is_some(),
            "Should find definition for task in depends_on via AST"
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_depends_on_definition_multiple_deps() {
        let text = r#"schema: nika/workflow@0.12
workflow: test
tasks:
  - id: fetch_data
    exec: "curl http://example.com"
  - id: transform
    exec: "jq '.data'"
  - id: publish
    depends_on: [fetch_data, transform]
    infer: "Summarize"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();

        // Test navigating to second dependency (transform)
        let line = "    depends_on: [fetch_data, transform]";
        let transform_pos = line.find("transform").unwrap();
        let response = find_depends_on_definition(text, line, transform_pos, &uri);

        assert!(
            response.is_some(),
            "Should find definition for second dependency"
        );
        if let Some(GotoDefinitionResponse::Scalar(location)) = response {
            assert_eq!(
                location.range.start.line, 5,
                "Should point to transform id line"
            );
        } else {
            panic!("Expected GotoDefinitionResponse::Scalar");
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_depends_on_definition_unknown_task() {
        let text = r#"
tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    depends_on: [nonexistent]
    infer: "World"
"#;
        let uri = "file:///test.nika.yaml".parse::<Uri>().unwrap();
        let line = "    depends_on: [nonexistent]";
        let pos = line.find("nonexistent").unwrap();
        let response = find_depends_on_definition(text, line, pos, &uri);
        assert!(
            response.is_none(),
            "Should return None for unknown task in depends_on"
        );
    }
}
