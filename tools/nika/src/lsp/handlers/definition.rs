//! Go to Definition Handler
//!
//! Provides definition lookup for Nika workflow elements:
//! - Task references in `use:` bindings → task definition
//! - Template variables → binding declaration
//! - Include paths → included file

#[cfg(feature = "lsp")]
use tower_lsp::lsp_types::*;

#[cfg(feature = "lsp")]
use crate::lsp::conversion::position_to_offset;

/// Find definition location for element at position
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

/// Find the location of a task definition by ID
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
}
