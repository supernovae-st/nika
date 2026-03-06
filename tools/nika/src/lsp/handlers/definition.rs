//! Go to Definition Handler
//!
//! Provides definition lookup for Nika workflow elements:
//! - Task references in `use:` bindings → task definition
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
use tower_lsp::lsp_types::*;

#[cfg(feature = "lsp")]
pub use tower_lsp::lsp_types::Url;

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
    uri: &Url,
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
    if let Some(response) = find_include_definition(line, offset) {
        return Some(response);
    }

    None
}

/// Find definition location for element at position (legacy text-based)
#[cfg(feature = "lsp")]
pub fn find_definition(text: &str, position: Position, uri: Url) -> Option<GotoDefinitionResponse> {
    let offset = position_to_offset(position, text);
    let lines: Vec<&str> = text.lines().collect();
    let line_idx = position.line as usize;

    if line_idx >= lines.len() {
        return None;
    }

    let line = lines[line_idx];

    // Check for task reference in use: block
    if let Some(response) = find_task_reference_definition(text, line, &uri) {
        return Some(response);
    }

    // Check for template variable reference
    if let Some(response) = find_template_definition(text, line, position.character as usize, &uri)
    {
        return Some(response);
    }

    // Check for include path
    if let Some(response) = find_include_definition(line, offset) {
        return Some(response);
    }

    None
}

/// Find definition for task reference in use: block
#[cfg(feature = "lsp")]
fn find_task_reference_definition(
    text: &str,
    line: &str,
    uri: &Url,
) -> Option<GotoDefinitionResponse> {
    let trimmed = line.trim();

    // Pattern: "alias: task_id" in use: block
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

/// Find definition for template variable like {{use.alias}}
#[cfg(feature = "lsp")]
fn find_template_definition(
    text: &str,
    line: &str,
    col: usize,
    uri: &Url,
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

                // Handle {{use.alias}} → find use: block
                if let Some(alias) = template.strip_prefix("use.") {
                    if let Some(location) = find_use_binding_location(text, alias, uri) {
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

/// Find include path definition (returns file URI)
#[cfg(feature = "lsp")]
fn find_include_definition(_line: &str, _offset: usize) -> Option<GotoDefinitionResponse> {
    // TODO: Implement include path resolution
    // This would need to resolve relative paths to absolute file URIs
    None
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
    uri: &Url,
) -> Option<GotoDefinitionResponse> {
    let trimmed = line.trim();

    // Pattern: "alias: task_id" in use: block
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
    uri: &Url,
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

                // Handle {{use.alias}} → find task referenced by binding
                if let Some(alias) = template.strip_prefix("use.") {
                    // Try to find the task that this binding references via AST
                    if let Some(location) = find_use_binding_location(text, alias, uri) {
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
    uri: &Url,
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
// Text-Based Helper Functions (Legacy)
// ============================================================================

/// Find the location of a task definition by ID (text-based)
#[cfg(feature = "lsp")]
fn find_task_location(text: &str, task_id: &str, uri: &Url) -> Option<Location> {
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

/// Find the location of a use: binding by alias
#[cfg(feature = "lsp")]
fn find_use_binding_location(text: &str, alias: &str, uri: &Url) -> Option<Location> {
    let pattern = format!("{}:", alias);
    let mut in_use_block = false;

    for (line_num, line) in text.lines().enumerate() {
        let trimmed = line.trim();

        // Track when we enter a use: block
        if trimmed == "use:" || trimmed.starts_with("use:") {
            in_use_block = true;
            continue;
        }

        // Exit use block when we see a non-indented line or new field
        if in_use_block && !line.starts_with("  ") && !line.starts_with("\t") && !line.is_empty() {
            in_use_block = false;
        }

        // Check for alias in use block
        if in_use_block && trimmed.starts_with(&pattern) {
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
fn find_context_file_location(text: &str, name: &str, uri: &Url) -> Option<Location> {
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
    use:
      input: step1
    infer: "World"
"#;
        let uri = Url::parse("file:///test.nika.yaml").unwrap();
        let location = find_task_location(text, "step1", &uri);

        assert!(location.is_some());
        let loc = location.unwrap();
        assert_eq!(loc.range.start.line, 2); // Line with "id: step1"
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_use_binding_location() {
        let text = r#"
- id: step2
  use:
    input: step1
    data: other_task
  infer: "Process {{use.input}}"
"#;
        let uri = Url::parse("file:///test.nika.yaml").unwrap();
        let location = find_use_binding_location(text, "input", &uri);

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
    use:
      input: $generate
    infer: "Process"
"#;
        let uri = Url::parse("file:///test.nika.yaml").unwrap();
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
        let uri = Url::parse("file:///test.nika.yaml").unwrap();
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
        let uri = Url::parse("file:///test.nika.yaml").unwrap();
        let location = find_task_location(text, "nonexistent", &uri);

        assert!(location.is_none());
    }

    // ============================================================================
    // AST-Aware Definition Tests (Phase 2)
    // ============================================================================

    #[test]
    #[cfg(feature = "lsp")]
    fn test_find_definition_with_ast_task_reference() {
        let text = r#"schema: nika/workflow@0.9
workflow: test
tasks:
  - id: step1
    infer: "Hello"
  - id: step2
    use:
      input: step1
    infer: "World"
"#;
        let uri = Url::parse("file:///test.nika.yaml").unwrap();
        let ast_index = AstIndex::new();

        // Parse document into AST index
        ast_index.parse_document(&uri, text, 0);

        // Position on "step1" in the use: block (line 7, after "input: ")
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
        let text = r#"schema: nika/workflow@0.9
workflow: test
tasks:
  - id: generate
    infer: "Generate something"
  - id: process
    infer: "Process"
"#;
        let uri = Url::parse("file:///test.nika.yaml").unwrap();
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
        let uri = Url::parse("file:///uncached.nika.yaml").unwrap();
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
        let text = r#"schema: nika/workflow@0.9
workflow: test
tasks:
  - id: data
    infer: "Get data"
  - id: process
    infer: "Process {{$data}}"
"#;
        let uri = Url::parse("file:///test.nika.yaml").unwrap();
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
    fn test_find_definition_with_ast_no_match() {
        let text = r#"schema: nika/workflow@0.9
workflow: test
tasks:
  - id: step1
    infer: "Hello"
"#;
        let uri = Url::parse("file:///test.nika.yaml").unwrap();
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
}
