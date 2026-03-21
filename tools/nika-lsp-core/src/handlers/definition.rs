//! Go-to-definition handler — protocol-agnostic.
//!
//! Finds definition locations for:
//! - Task references in `depends_on:` → task `id:` line
//! - Task references in `with:` bindings (`$task_id`) → task definition
//! - Template expressions (`{{with.alias}}`) → binding source
//! - Include paths → file path (returned in `DefinitionResult.file`)

use crate::analysis::context::CursorContext;

/// Protocol-agnostic definition result.
///
/// The tower-lsp shim converts this to `GotoDefinitionResponse`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionResult {
    /// Start byte offset of the definition in the document.
    pub offset: u32,
    /// End byte offset.
    pub end_offset: u32,
    /// If set, the definition is in a different file (for include paths).
    pub file: Option<String>,
}

/// Find the definition for the element at the given cursor context.
pub fn definition(text: &str, _offset: u32, context: &CursorContext) -> Option<DefinitionResult> {
    match context {
        // depends_on: [step1] → jump to `- id: step1`
        CursorContext::DependsOn { prefix, .. } => {
            let name = prefix.trim();
            // Handle cursor on a specific task ID within the array
            if name.contains('[') || name.contains(']') || name.contains(',') {
                // Extract the last word (task ID the cursor is likely on)
                let last = name
                    .rsplit(|c: char| c == '[' || c == ',' || c == ' ')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_end_matches(']');
                find_task_def(text, last)
            } else {
                find_task_def(text, name)
            }
        }

        // with: { alias: $task_id } → jump to task definition
        CursorContext::WithBlock { partial_ref, .. } => {
            let name = partial_ref.trim().trim_start_matches('$');
            find_task_def(text, name)
        }

        // {{with.alias}} → find the binding source, then the task
        // {{context.files.name}} → find the context file declaration
        // {{inputs.param}} → find the inputs declaration
        CursorContext::Template { partial_expr, .. } => {
            let expr = partial_expr.trim();
            if let Some(rest) = expr.strip_prefix("with.") {
                let alias = rest.split('.').next().unwrap_or("");
                find_with_source(text, alias)
            } else if expr.starts_with("context.") || expr.starts_with("inputs.") {
                // Jump to the root-level context: or inputs: block
                let root_key = expr.split('.').next().unwrap_or("");
                find_root_key(text, root_key)
            } else {
                None
            }
        }

        // Verb block: if on the verb keyword, no definition to jump to
        // but if inside invoke: { mcp: server } → could jump to mcp config
        CursorContext::InvokeBlock {
            mcp_server: Some(server),
            ..
        } => find_mcp_server_def(text, server),

        _ => None,
    }
}

/// Find a task definition by `- id: <name>` in the document text.
fn find_task_def(text: &str, name: &str) -> Option<DefinitionResult> {
    if name.is_empty() {
        return None;
    }
    for needle in [
        format!("- id: {name}"),
        format!("- id: \"{name}\""),
        format!("- id: '{name}'"),
    ] {
        if let Some(pos) = text.find(&needle) {
            return Some(DefinitionResult {
                offset: pos as u32,
                end_offset: (pos + needle.len()) as u32,
                file: None,
            });
        }
    }
    None
}

/// Trace a `with:` alias back to its `$task_ref`, then find that task's definition.
fn find_with_source(text: &str, alias: &str) -> Option<DefinitionResult> {
    if alias.is_empty() {
        return None;
    }

    // Search for `alias: $task_ref` pattern in with: blocks
    for pat in [
        format!("{alias}: $"),
        format!("{alias}: \"$"),
        format!("{alias}: '$"),
    ] {
        if let Some(pos) = text.find(&pat) {
            let after = &text[pos + pat.len()..];
            let task_ref: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if !task_ref.is_empty() {
                return find_task_def(text, &task_ref);
            }
        }
    }
    None
}

/// Find a root-level key (context:, inputs:, mcp:, include:) in the document.
fn find_root_key(text: &str, key: &str) -> Option<DefinitionResult> {
    // Root keys start at column 0
    let needle = format!("{}:", key);
    for (offset, line) in text.lines().scan(0u32, |off, line| {
        let start = *off;
        *off += line.len() as u32 + 1;
        Some((start, line))
    }) {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&needle) && line.len() == trimmed.len() {
            return Some(DefinitionResult {
                offset,
                end_offset: offset + line.len() as u32,
                file: None,
            });
        }
    }
    None
}

/// Find an MCP server definition in the `mcp:` config section.
fn find_mcp_server_def(text: &str, server: &str) -> Option<DefinitionResult> {
    // Look for `  server_name:` under `mcp:` block
    let needle = format!("  {}:", server);
    for (offset, line) in text.lines().scan(0u32, |off, line| {
        let start = *off;
        *off += line.len() as u32 + 1;
        Some((start, line))
    }) {
        if line.trim_start() == format!("{}:", server) || line == needle {
            return Some(DefinitionResult {
                offset,
                end_offset: offset + line.len() as u32,
                file: None,
            });
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
schema: \"@0.12\"
workflow: test
provider: anthropic

mcp:
  novanet:
    command: node

context:
  files:
    brand: ./brand.md

inputs:
  topic:
    type: string

tasks:
  - id: step1
    infer: \"Generate\"

  - id: step2
    with:
      data: $step1
    infer: \"Process {{with.data}}\"
    depends_on: [step1]
";

    #[test]
    fn find_task_by_id() {
        let r = find_task_def(SAMPLE, "step1").unwrap();
        assert!(r.offset > 0);
        assert!(r.file.is_none());
    }

    #[test]
    fn find_task_quoted() {
        let text = "tasks:\n  - id: \"my-task\"\n    exec: \"echo\"";
        assert!(find_task_def(text, "my-task").is_some());
    }

    #[test]
    fn find_task_not_found() {
        assert!(find_task_def(SAMPLE, "nonexistent").is_none());
    }

    #[test]
    fn find_task_empty_name() {
        assert!(find_task_def(SAMPLE, "").is_none());
    }

    #[test]
    fn depends_on_context() {
        let ctx = CursorContext::DependsOn {
            task_id: None,
            existing_deps: vec![],
            prefix: "step1".into(),
        };
        let r = definition(SAMPLE, 0, &ctx).unwrap();
        assert!(r.file.is_none());
    }

    #[test]
    fn depends_on_array_format() {
        let ctx = CursorContext::DependsOn {
            task_id: None,
            existing_deps: vec![],
            prefix: "[step1, step2".into(),
        };
        // Should find step2 (last element)
        let r = definition(SAMPLE, 0, &ctx).unwrap();
        assert!(r.offset > 0);
    }

    #[test]
    fn with_ref_context() {
        let ctx = CursorContext::WithBlock {
            task_id: None,
            alias: None,
            partial_ref: "$step1".into(),
        };
        assert!(definition(SAMPLE, 0, &ctx).is_some());
    }

    #[test]
    fn template_with_binding() {
        let ctx = CursorContext::Template {
            task_id: None,
            available_bindings: vec![],
            partial_expr: "with.data".into(),
            in_transform_chain: false,
        };
        // Traces data → $step1 → finds step1 definition
        let r = definition(SAMPLE, 0, &ctx);
        assert!(r.is_some());
    }

    #[test]
    fn template_context_files() {
        let ctx = CursorContext::Template {
            task_id: None,
            available_bindings: vec![],
            partial_expr: "context.files.brand".into(),
            in_transform_chain: false,
        };
        let r = definition(SAMPLE, 0, &ctx).unwrap();
        // Should find the `context:` root key
        let slice = &SAMPLE[r.offset as usize..r.end_offset as usize];
        assert!(slice.starts_with("context:"));
    }

    #[test]
    fn template_inputs() {
        let ctx = CursorContext::Template {
            task_id: None,
            available_bindings: vec![],
            partial_expr: "inputs.topic".into(),
            in_transform_chain: false,
        };
        let r = definition(SAMPLE, 0, &ctx).unwrap();
        let slice = &SAMPLE[r.offset as usize..r.end_offset as usize];
        assert!(slice.starts_with("inputs:"));
    }

    #[test]
    fn with_source_traces_to_task() {
        let r = find_with_source(SAMPLE, "data").unwrap();
        // Should resolve data → $step1 → step1 definition
        let slice = &SAMPLE[r.offset as usize..r.end_offset as usize];
        assert!(slice.contains("step1"));
    }

    #[test]
    fn root_key_found() {
        let r = find_root_key(SAMPLE, "mcp").unwrap();
        let slice = &SAMPLE[r.offset as usize..r.end_offset as usize];
        assert_eq!(slice, "mcp:");
    }

    #[test]
    fn root_key_not_found() {
        assert!(find_root_key(SAMPLE, "nonexistent").is_none());
    }

    #[test]
    fn mcp_server_def() {
        let r = find_mcp_server_def(SAMPLE, "novanet").unwrap();
        assert!(r.offset > 0);
    }

    #[test]
    fn invoke_mcp_context() {
        let ctx = CursorContext::InvokeBlock {
            task_id: None,
            mcp_server: Some("novanet".to_string()),
            tool_name: None,
            focus: crate::analysis::context::InvokeFocus::McpServer,
            prefix: String::new(),
        };
        assert!(definition(SAMPLE, 0, &ctx).is_some());
    }

    #[test]
    fn unknown_context_returns_none() {
        let ctx = CursorContext::Unknown {
            prefix: String::new(),
        };
        assert!(definition(SAMPLE, 0, &ctx).is_none());
    }
}
