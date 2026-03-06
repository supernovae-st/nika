//! LSP Utility Functions
//!
//! Shared helpers for LSP handlers.

/// Extract task IDs from workflow text
///
/// Scans YAML workflow text and extracts all task IDs.
/// Handles both list syntax (`- id: step1`) and nested syntax (`id: step1`).
#[cfg(feature = "lsp")]
pub fn extract_task_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();

        // Handle list item syntax: "- id: task_id" or "-id:task_id"
        let id_value = trimmed
            .strip_prefix("- id:")
            .or_else(|| trimmed.strip_prefix("-id:"));

        if let Some(id) = id_value {
            let id = id.trim().trim_matches('\"').trim_matches('\'');
            if !id.is_empty() {
                ids.push(id.to_string());
            }
        } else if let Some(stripped) = trimmed.strip_prefix("id:") {
            // Handle nested id: (only if indented, not at root level)
            let indent = line.len() - trimmed.len();
            if indent > 0 {
                let id = stripped.trim().trim_matches('\"').trim_matches('\'');
                if !id.is_empty() {
                    ids.push(id.to_string());
                }
            }
        }
    }

    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_task_ids_list_syntax() {
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
    fn test_extract_task_ids_nested_syntax() {
        let text = r#"
tasks:
  -
    id: step1
    infer: "Hello"
  -
    id: step2
    exec: "echo"
"#;
        let ids = extract_task_ids(text);
        assert_eq!(ids, vec!["step1", "step2"]);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_task_ids_empty() {
        let ids = extract_task_ids("");
        assert!(ids.is_empty());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_extract_task_ids_no_tasks() {
        let text = "schema: nika/workflow@0.10\nworkflow: test\n";
        let ids = extract_task_ids(text);
        assert!(ids.is_empty());
    }
}
