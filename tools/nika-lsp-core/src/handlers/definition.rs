// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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
                    .rsplit(['[', ',', ' '])
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

        // Verb block: handle sub-field jumps
        CursorContext::InvokeBlock {
            mcp_server: Some(server),
            ..
        } => find_mcp_server_def(text, server),

        // Verb sub-field: from: agent_name → agents: block definition
        CursorContext::VerbBlock { prefix, verb, .. } if verb == "agent" => {
            let key = prefix.trim().trim_end_matches(':');
            if key == "from" {
                // Extract the value after "from:" on this line
                if let Some(from_val) = extract_field_value(text, _offset, "from") {
                    find_agent_def(text, &from_val)
                } else {
                    None
                }
            } else if key == "skills" {
                find_root_key(text, "skills")
            } else {
                None
            }
        }

        // McpConfig: jump from server reference to its definition
        CursorContext::McpConfig {
            server_name: Some(name),
            ..
        } => find_mcp_server_def(text, name),

        // ForEach: no specific definition target
        CursorContext::ForEach { .. } => None,

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

/// Find an agent definition in the `agents:` block.
fn find_agent_def(text: &str, name: &str) -> Option<DefinitionResult> {
    if name.is_empty() {
        return None;
    }
    let needle = format!("  {}:", name);
    let mut in_agents = false;
    let mut offset = 0u32;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "agents:" {
            in_agents = true;
        } else if in_agents && !trimmed.is_empty() && !line.starts_with(' ') {
            in_agents = false; // Left agents block
        }
        if in_agents && line.starts_with(&needle) {
            return Some(DefinitionResult {
                offset,
                end_offset: offset + line.len() as u32,
                file: None,
            });
        }
        offset += line.len() as u32 + 1;
    }
    None
}

/// Extract the value of a field from the line at the given offset.
fn extract_field_value(text: &str, offset: u32, field: &str) -> Option<String> {
    let start = (offset as usize).min(text.len());
    // Find the line containing the offset
    let line_start = text[..start].rfind('\n').map_or(0, |p| p + 1);
    let line_end = text[start..].find('\n').map_or(text.len(), |p| start + p);
    let line = &text[line_start..line_end];
    let trimmed = line.trim();

    let prefix = format!("{field}:");
    if let Some(rest) = trimmed.strip_prefix(&prefix) {
        let val = rest.trim().trim_matches('"').trim_matches('\'');
        if !val.is_empty() {
            return Some(val.to_string());
        }
    }
    // Also search nearby lines (field might be on adjacent line)
    let raw_start = line_start.saturating_sub(200);
    let search_start = {
        let mut s = raw_start;
        while s > 0 && !text.is_char_boundary(s) {
            s -= 1;
        }
        s
    };
    for search_line in text[search_start..line_end.min(text.len())]
        .lines()
        .take(10)
    {
        let t = search_line.trim();
        if let Some(rest) = t.strip_prefix(&prefix) {
            let val = rest.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                return Some(val.to_string());
            }
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

skills:
  research: ./skills/research.md
  summarize: ./skills/summarize.md

agents:
  researcher:
    system: \"You are a researcher\"
    tools: [perplexity/search]

tasks:
  - id: step1
    infer: \"Generate\"

  - id: step2
    with:
      data: $step1
    infer: \"Process {{with.data}}\"
    depends_on: [step1]

  - id: step3
    agent:
      from: researcher
      prompt: \"Research topic\"
      skills: [research]
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
    fn agent_def_found() {
        let r = find_agent_def(SAMPLE, "researcher").unwrap();
        let slice = &SAMPLE[r.offset as usize..r.end_offset as usize];
        assert!(slice.contains("researcher"));
    }

    #[test]
    fn agent_def_not_found() {
        assert!(find_agent_def(SAMPLE, "nonexistent").is_none());
    }

    #[test]
    fn mcp_config_jumps_to_server() {
        let ctx = CursorContext::McpConfig {
            server_name: Some("novanet".to_string()),
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
