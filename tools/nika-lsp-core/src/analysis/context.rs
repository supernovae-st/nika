// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cursor context detection for completion and other LSP features.
//!
//! [`CursorContext`] is the merged 16-variant enum combining the embedded LSP's
//! text-based `CompletionContext` with the standalone LSP's AST-based `AstContext`.
//!
//! The initial implementation uses **text-based detection only** (matching the
//! embedded LSP's approach). AST-based detection via `PositionIndex` will be
//! wired in a follow-up.

use nika_core::ast::analyzed::AnalyzedWorkflow;

use crate::parse::PartialWorkflow;

// ---------------------------------------------------------------------------
// InvokeFocus / ContentFocus
// ---------------------------------------------------------------------------

/// Sub-focus within an `invoke:` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvokeFocus {
    /// Cursor is on the `mcp:` field.
    McpServer,
    /// Cursor is on the `tool:` field.
    Tool,
    /// Cursor is on `params:` or inside it.
    Params,
    /// Cursor is on `resource:`.
    Resource,
    /// General invoke block (not on a specific sub-field).
    General,
}

/// Sub-focus within a `content:` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentFocus {
    /// At the content list level (suggesting `- type: ...`).
    PartType,
    /// Inside a `type: image` part, on the `detail:` field.
    ImageDetail,
    /// Inside a `type: image_url` part.
    ImageUrl,
    /// Inside a content part's fields.
    PartField,
}

// ---------------------------------------------------------------------------
// CursorContext
// ---------------------------------------------------------------------------

/// The merged cursor context -- 16 variants covering all completion scenarios.
///
/// Produced by [`detect_context`] from text analysis (and optionally AST data).
/// Consumed by the completion handler to decide what items to offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorContext {
    /// Cursor at workflow root level (indent 0).
    WorkflowRoot { prefix: String },

    /// Inside a task definition -- suggesting task-level fields.
    TaskField {
        task_id: Option<String>,
        existing_fields: Vec<String>,
        prefix: String,
    },

    /// Inside a verb's sub-block (e.g. `infer:`, `exec:`, `fetch:`).
    VerbBlock {
        task_id: Option<String>,
        verb: String,
        existing_subfields: Vec<String>,
        prefix: String,
    },

    /// Inside a `with:` block -- suggesting task references.
    WithBlock {
        task_id: Option<String>,
        alias: Option<String>,
        partial_ref: String,
    },

    /// Inside a `{{ }}` template expression.
    Template {
        task_id: Option<String>,
        available_bindings: Vec<String>,
        partial_expr: String,
        in_transform_chain: bool,
    },

    /// Inside an `invoke:` block with sub-field focus.
    InvokeBlock {
        task_id: Option<String>,
        mcp_server: Option<String>,
        tool_name: Option<String>,
        focus: InvokeFocus,
        prefix: String,
    },

    /// Inside the `mcp:` configuration section.
    McpConfig {
        server_name: Option<String>,
        prefix: String,
    },

    /// Cursor on a provider/model field.
    ProviderContext {
        task_id: Option<String>,
        verb: String,
        current_provider: Option<String>,
        current_model: Option<String>,
        prefix: String,
    },

    /// Inside a `content:` block (multimodal vision support).
    ContentPart {
        task_id: Option<String>,
        focus: ContentFocus,
        part_type: Option<String>,
        prefix: String,
    },

    /// Inside a `for_each:` block.
    ForEach {
        task_id: Option<String>,
        loop_var: Option<String>,
        prefix: String,
    },

    /// Inside a `structured:` / `schema:` output block.
    SchemaBlock {
        task_id: Option<String>,
        prefix: String,
    },

    /// Inside `depends_on:` -- suggesting other task IDs.
    DependsOn {
        task_id: Option<String>,
        existing_deps: Vec<String>,
        prefix: String,
    },

    /// Inside a `guardrails:` block.
    Guardrails {
        task_id: Option<String>,
        guardrail_type: Option<String>,
        prefix: String,
    },

    /// Inside a `retry:` block.
    RetryBlock {
        task_id: Option<String>,
        prefix: String,
    },

    /// Inside a `limits:` / `timeout:` block.
    LimitsBlock {
        task_id: Option<String>,
        prefix: String,
    },

    /// Cannot determine context.
    Unknown { prefix: String },
}

// ---------------------------------------------------------------------------
// Text-based context detection
// ---------------------------------------------------------------------------

/// Detect cursor context with error-recovery parsing.
///
/// Runs tree-sitter recovery parser first, then uses the resulting
/// `PartialWorkflow` to augment text-based detection. This enables
/// completions, hover, and diagnostics even on broken YAML.
pub fn detect_context_with_recovery(text: &str, offset: u32) -> CursorContext {
    let partial = crate::parse::parse_and_extract(text);
    detect_context_with_partial(text, offset, &partial)
}

/// Detect cursor context using a pre-parsed `PartialWorkflow`.
///
/// Use this when you already have a `PartialWorkflow` (avoids double-parsing).
pub fn detect_context_with_partial(
    text: &str,
    offset: u32,
    partial: &PartialWorkflow,
) -> CursorContext {
    // Use partial workflow for task-aware context when inside a task span
    let offset_usize = offset as usize;
    for task in &partial.tasks {
        if task.span.contains(offset) {
            // We're inside a task — use structural info for better detection
            let existing_fields = task.existing_keys.clone();
            let task_id = task.id.clone();

            // If we have a verb, detect verb sub-field context
            if let Some(ref verb) = task.verb {
                let before = &text[..offset_usize.min(text.len())];
                let last_line_start = before.rfind('\n').map_or(0, |p| p + 1);
                let current_line = &text[last_line_start
                    ..text[last_line_start..]
                        .find('\n')
                        .map_or(text.len(), |p| last_line_start + p)];
                let trimmed = current_line.trim();
                let prefix = trimmed.to_string();

                // Check if cursor is inside the verb block (deeper indent)
                if let Some(verb_span) = &task.verb_span {
                    if offset > verb_span.start && !trimmed.is_empty() {
                        // Inside verb block — offer verb sub-fields
                        return CursorContext::VerbBlock {
                            task_id,
                            verb: verb.clone(),
                            existing_subfields: existing_fields,
                            prefix,
                        };
                    }
                }
            }
        }
    }

    // Fall back to text-based detection
    detect_context(text, offset, None)
}

/// Detect the cursor context from document text and byte offset.
///
/// When `analyzed` is `Some`, AST data can augment the detection.
/// For the initial implementation this parameter is unused -- pure text-based.
pub fn detect_context(
    text: &str,
    offset: u32,
    _analyzed: Option<&AnalyzedWorkflow>,
) -> CursorContext {
    let offset = offset as usize;
    if offset > text.len() {
        return CursorContext::Unknown {
            prefix: String::new(),
        };
    }

    // Convert byte offset to line/character (handles \n, \r\n, \r).
    let before = &text[..offset];
    let mut line_idx = 0usize;
    let mut last_line_start = 0usize;
    let bytes = before.as_bytes();
    let mut bi = 0;
    while bi < before.len() {
        if bytes[bi] == b'\r' {
            line_idx += 1;
            bi += 1;
            if bi < before.len() && bytes[bi] == b'\n' {
                bi += 1; // \r\n = single line break
            }
            last_line_start = bi;
        } else if bytes[bi] == b'\n' {
            line_idx += 1;
            bi += 1;
            last_line_start = bi;
        } else {
            bi += 1;
        }
    }
    let char_idx = offset - last_line_start;

    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return CursorContext::WorkflowRoot {
            prefix: String::new(),
        };
    }

    let current_line = lines.get(line_idx).copied().unwrap_or("");
    // Clamp to line length for the prefix slice.
    let safe_end = char_idx.min(current_line.len());
    let prefix = &current_line[..safe_end];
    let trimmed = prefix.trim();

    // -----------------------------------------------------------------------
    // Template context: inside {{ ... }}
    // -----------------------------------------------------------------------
    if prefix.contains("{{") && !prefix.contains("}}") {
        let partial_expr = prefix.rsplit("{{").next().unwrap_or("").trim().to_string();
        let in_transform_chain = partial_expr.contains('|');
        let task_id = find_enclosing_task_id(&lines, line_idx);
        let available_bindings = extract_with_bindings(&lines, line_idx);
        return CursorContext::Template {
            task_id,
            available_bindings,
            partial_expr,
            in_transform_chain,
        };
    }

    // -----------------------------------------------------------------------
    // Indentation level determines scope
    // -----------------------------------------------------------------------
    let indent = current_line.len() - current_line.trim_start().len();

    // Root level (indent 0)
    if indent == 0 {
        return CursorContext::WorkflowRoot {
            prefix: trimmed.to_string(),
        };
    }

    // -----------------------------------------------------------------------
    // Look backward for context
    // -----------------------------------------------------------------------

    // Check if inside mcp: block
    if is_under_key(&lines, line_idx, "mcp:") {
        let server_name = find_mcp_server_name(&lines, line_idx);
        return CursorContext::McpConfig {
            server_name,
            prefix: trimmed.to_string(),
        };
    }

    // Check if inside tasks: block
    if !is_under_key(&lines, line_idx, "tasks:") {
        // Not under tasks: and not at root -- unknown.
        return CursorContext::Unknown {
            prefix: trimmed.to_string(),
        };
    }

    // We are under tasks:. Determine the specific sub-context.
    let task_id = find_enclosing_task_id(&lines, line_idx);

    // Check for specific sub-block contexts by scanning ALL ancestors,
    // not just the immediate parent. This handles deeply nested structures
    // like `infer: > content: > - type: > detail:`.
    let ancestors = find_ancestor_keys(&lines, line_idx);

    for ancestor in &ancestors {
        match ancestor.as_str() {
            "with:" | "with" => {
                let alias = if trimmed.contains(':') {
                    trimmed
                        .split(':')
                        .next()
                        .map(|s| s.trim_start_matches("- ").to_string())
                } else {
                    None
                };
                let partial_ref = trimmed.rsplit(':').next().unwrap_or("").trim().to_string();
                return CursorContext::WithBlock {
                    task_id,
                    alias,
                    partial_ref,
                };
            }
            "depends_on:" | "depends_on" => {
                let existing_deps = extract_depends_on(&lines, line_idx);
                return CursorContext::DependsOn {
                    task_id,
                    existing_deps,
                    prefix: trimmed.to_string(),
                };
            }
            "content:" | "content" => {
                let focus = detect_content_focus(trimmed);
                let part_type = detect_content_part_type(&lines, line_idx);
                return CursorContext::ContentPart {
                    task_id,
                    focus,
                    part_type,
                    prefix: trimmed.to_string(),
                };
            }
            "for_each:" | "for_each" => {
                let loop_var = find_loop_var(&lines, line_idx);
                return CursorContext::ForEach {
                    task_id,
                    loop_var,
                    prefix: trimmed.to_string(),
                };
            }
            "structured:" | "structured" | "schema:" => {
                return CursorContext::SchemaBlock {
                    task_id,
                    prefix: trimmed.to_string(),
                };
            }
            "guardrails:" | "guardrails" => {
                let guardrail_type = if trimmed.contains(':') {
                    trimmed.split(':').next().map(|s| s.trim().to_string())
                } else {
                    None
                };
                return CursorContext::Guardrails {
                    task_id,
                    guardrail_type,
                    prefix: trimmed.to_string(),
                };
            }
            "retry:" | "retry" => {
                return CursorContext::RetryBlock {
                    task_id,
                    prefix: trimmed.to_string(),
                };
            }
            "limits:" | "limits" | "timeout:" => {
                return CursorContext::LimitsBlock {
                    task_id,
                    prefix: trimmed.to_string(),
                };
            }
            "invoke:" | "invoke" => {
                let (focus, mcp_server, tool_name) = detect_invoke_focus(&lines, line_idx, trimmed);
                return CursorContext::InvokeBlock {
                    task_id,
                    mcp_server,
                    tool_name,
                    focus,
                    prefix: trimmed.to_string(),
                };
            }
            _ => {}
        }
    }

    // Check if cursor is on a verb line or inside a verb block.
    if let Some(verb) = detect_verb_context(&lines, line_idx, indent) {
        // Check if we're on a provider/model field inside a verb.
        if trimmed.starts_with("provider:") || trimmed.starts_with("model:") {
            let current_provider = find_field_value(&lines, line_idx, "provider:");
            let current_model = find_field_value(&lines, line_idx, "model:");
            return CursorContext::ProviderContext {
                task_id,
                verb: verb.clone(),
                current_provider,
                current_model,
                prefix: trimmed.to_string(),
            };
        }

        // Inside invoke: specifically
        if verb == "invoke" {
            let (focus, mcp_server, tool_name) = detect_invoke_focus(&lines, line_idx, trimmed);
            return CursorContext::InvokeBlock {
                task_id,
                mcp_server,
                tool_name,
                focus,
                prefix: trimmed.to_string(),
            };
        }

        let existing_subfields = extract_sibling_fields(&lines, line_idx);
        return CursorContext::VerbBlock {
            task_id,
            verb,
            existing_subfields,
            prefix: trimmed.to_string(),
        };
    }

    // Check if the current line itself starts with a verb.
    if line_starts_with_verb(trimmed) {
        let verb = extract_verb_name(trimmed);
        return CursorContext::VerbBlock {
            task_id,
            verb,
            existing_subfields: vec![],
            prefix: trimmed.to_string(),
        };
    }

    // Check if line starts with "with:"
    if trimmed.starts_with("with:") {
        return CursorContext::WithBlock {
            task_id,
            alias: None,
            partial_ref: String::new(),
        };
    }

    // Check for provider/model at task level.
    if trimmed.starts_with("provider:") || trimmed.starts_with("model:") {
        let verb = find_verb_for_task(&lines, line_idx).unwrap_or_default();
        let current_provider = find_field_value(&lines, line_idx, "provider:");
        let current_model = find_field_value(&lines, line_idx, "model:");
        return CursorContext::ProviderContext {
            task_id,
            verb,
            current_provider,
            current_model,
            prefix: trimmed.to_string(),
        };
    }

    // Default: inside a task block at field level.
    let existing_fields = extract_sibling_fields(&lines, line_idx);
    CursorContext::TaskField {
        task_id,
        existing_fields,
        prefix: trimmed.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Check if `line_idx` is indented under a top-level key.
fn is_under_key(lines: &[&str], line_idx: usize, key: &str) -> bool {
    for i in (0..line_idx).rev() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let line_indent = line.len() - trimmed.len();
        if line_indent == 0 {
            return trimmed.starts_with(key);
        }
    }
    false
}

/// Find the task ID of the enclosing task block.
fn find_enclosing_task_id(lines: &[&str], line_idx: usize) -> Option<String> {
    for i in (0..=line_idx).rev() {
        let trimmed = lines[i].trim();
        if let Some(rest) = trimmed.strip_prefix("- id:") {
            return Some(rest.trim().trim_matches('"').trim_matches('\'').to_string());
        }
        // Note: "-id:" (no space) is invalid YAML, removed dead branch
        // Handle multiline task def:  `- \n    id: foo`
        if trimmed.starts_with("id:") {
            let line_indent = lines[i].len() - trimmed.len();
            if line_indent > 0 {
                let rest = trimmed.strip_prefix("id:").unwrap_or("");
                let id = rest.trim().trim_matches('"').trim_matches('\'');
                if !id.is_empty() {
                    return Some(id.to_string());
                }
            }
        }
    }
    None
}

/// Collect all ancestor keys (from nearest to farthest) by walking up
/// through decreasing indentation levels.
///
/// For a cursor at indent 10 inside `infer: > content: > - type: > detail:`,
/// this returns `["- type:", "content:", "infer:"]` (or similar), letting
/// callers check if any ancestor is a known block.
fn find_ancestor_keys(lines: &[&str], line_idx: usize) -> Vec<String> {
    let current_indent = lines
        .get(line_idx)
        .map(|l| l.len() - l.trim_start().len())
        .unwrap_or(0);

    let mut ancestors = Vec::new();
    let mut search_indent = current_indent;

    for i in (0..line_idx).rev() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let line_indent = line.len() - trimmed.len();

        if line_indent < search_indent {
            // This is an ancestor. Extract the key.
            let key = if trimmed.contains(':') {
                trimmed.split(':').next().unwrap_or(trimmed).trim()
            } else {
                trimmed
            };
            let key = key.strip_prefix("- ").unwrap_or(key);
            ancestors.push(format!("{key}:"));
            search_indent = line_indent;

            // Stop when we reach root level.
            if line_indent == 0 {
                break;
            }
        }
    }
    ancestors
}

/// Detect if cursor is inside a verb's sub-block. Returns the verb name.
fn detect_verb_context(lines: &[&str], line_idx: usize, current_indent: usize) -> Option<String> {
    for i in (0..line_idx).rev() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let line_indent = line.len() - trimmed.len();

        // If we hit a line at lesser indent, check if it's a verb.
        if line_indent < current_indent {
            if line_starts_with_verb(trimmed) {
                return Some(extract_verb_name(trimmed));
            }
            // Not a verb -- stop searching.
            return None;
        }
    }
    None
}

/// Check if a trimmed line starts with one of the 5 verbs.
fn line_starts_with_verb(trimmed: &str) -> bool {
    trimmed.starts_with("infer:")
        || trimmed.starts_with("exec:")
        || trimmed.starts_with("fetch:")
        || trimmed.starts_with("invoke:")
        || trimmed.starts_with("agent:")
}

/// Extract the verb name from a line like "infer: ..." or "invoke:".
fn extract_verb_name(trimmed: &str) -> String {
    for verb in &["infer", "exec", "fetch", "invoke", "agent"] {
        if trimmed.starts_with(&format!("{verb}:")) {
            return (*verb).to_string();
        }
    }
    String::new()
}

/// Find the MCP server name when inside an mcp: block.
fn find_mcp_server_name(lines: &[&str], line_idx: usize) -> Option<String> {
    let current_indent = lines
        .get(line_idx)
        .map(|l| l.len() - l.trim_start().len())
        .unwrap_or(0);

    for i in (0..line_idx).rev() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let line_indent = line.len() - trimmed.len();

        // A server name is a key at indent 2 (directly under mcp:).
        if line_indent < current_indent && line_indent > 0 && trimmed.ends_with(':') {
            return Some(trimmed.trim_end_matches(':').to_string());
        }
        if line_indent == 0 {
            break;
        }
    }
    None
}

/// Extract with: bindings from the current task.
fn extract_with_bindings(lines: &[&str], line_idx: usize) -> Vec<String> {
    let mut bindings = Vec::new();
    let mut in_with = false;
    let mut with_indent = 0;

    // Scan backward to find the task start, then forward through with:.
    let task_start = find_task_start_line(lines, line_idx);

    for line in lines
        .iter()
        .take(find_next_task_line(lines, task_start))
        .skip(task_start)
    {
        let trimmed = line.trim();
        let cur_indent = line.len() - trimmed.len();

        if trimmed.starts_with("with:") {
            in_with = true;
            with_indent = cur_indent;
            continue;
        }

        if in_with {
            if cur_indent <= with_indent && !trimmed.is_empty() {
                in_with = false;
                continue;
            }
            if trimmed.contains(':') {
                if let Some(alias) = trimmed.split(':').next() {
                    let alias = alias.trim();
                    if !alias.is_empty() && !alias.starts_with('-') {
                        bindings.push(alias.to_string());
                    }
                }
            }
        }
    }
    bindings
}

/// Find the line index where the current task starts.
fn find_task_start_line(lines: &[&str], line_idx: usize) -> usize {
    for i in (0..=line_idx).rev() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("- id:") || trimmed.starts_with("-id:") {
            return i;
        }
    }
    0
}

/// Find the line index where the next task starts (or end of file).
fn find_next_task_line(lines: &[&str], from_line: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(from_line + 1)
        .find(|(_, l)| l.trim().starts_with("- id:"))
        .map_or(lines.len(), |(i, _)| i)
}

/// Extract depends_on entries from the current context.
fn extract_depends_on(lines: &[&str], line_idx: usize) -> Vec<String> {
    let mut deps = Vec::new();
    // Find the depends_on: line and parse its value.
    for i in (0..=line_idx).rev() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("depends_on:") {
            let value = trimmed.strip_prefix("depends_on:").unwrap_or("").trim();
            // Inline array format: [a, b, c]
            let value = value.trim_start_matches('[').trim_end_matches(']');
            for dep in value.split(',') {
                let dep = dep.trim().trim_matches('"').trim_matches('\'');
                if !dep.is_empty() {
                    deps.push(dep.to_string());
                }
            }
            break;
        }
    }
    deps
}

/// Detect content focus from the trimmed prefix.
fn detect_content_focus(trimmed: &str) -> ContentFocus {
    if trimmed.starts_with("- type:") || trimmed.starts_with("-type:") || trimmed == "-" {
        ContentFocus::PartType
    } else if trimmed.starts_with("detail:") {
        ContentFocus::ImageDetail
    } else if trimmed.starts_with("url:") || trimmed.starts_with("source:") {
        ContentFocus::ImageUrl
    } else {
        ContentFocus::PartField
    }
}

/// Detect the part type of the current content entry.
fn detect_content_part_type(lines: &[&str], line_idx: usize) -> Option<String> {
    for i in (0..=line_idx).rev() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("- type:") || trimmed.starts_with("-type:") {
            let value = trimmed
                .strip_prefix("- type:")
                .or_else(|| trimmed.strip_prefix("-type:"))
                .unwrap_or("")
                .trim()
                .trim_matches('"')
                .trim_matches('\'');
            return if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
        }
        // Stop if we hit a different list item.
        if trimmed.starts_with("- ") && !trimmed.starts_with("- type:") {
            break;
        }
    }
    None
}

/// Find the loop variable (`as:` field) in a for_each block.
fn find_loop_var(lines: &[&str], line_idx: usize) -> Option<String> {
    for i in (0..=line_idx).rev() {
        let trimmed = lines[i].trim();
        if trimmed.starts_with("as:") {
            let val = trimmed.strip_prefix("as:")?.trim().trim_matches('"');
            return if val.is_empty() {
                None
            } else {
                Some(val.to_string())
            };
        }
        // Stop if we exit the task.
        if trimmed.starts_with("- id:") {
            break;
        }
    }
    None
}

/// Detect invoke sub-focus and extract mcp/tool fields.
fn detect_invoke_focus(
    lines: &[&str],
    line_idx: usize,
    trimmed: &str,
) -> (InvokeFocus, Option<String>, Option<String>) {
    let focus = if trimmed.starts_with("mcp:") {
        InvokeFocus::McpServer
    } else if trimmed.starts_with("tool:") {
        InvokeFocus::Tool
    } else if trimmed.starts_with("params:") || is_under_sibling(lines, line_idx, "params:") {
        InvokeFocus::Params
    } else if trimmed.starts_with("resource:") {
        InvokeFocus::Resource
    } else {
        InvokeFocus::General
    };

    let mcp_server = find_field_value(lines, line_idx, "mcp:");
    let tool_name = find_field_value(lines, line_idx, "tool:");

    (focus, mcp_server, tool_name)
}

/// Check if the current line is under a sibling key (at same indent level).
fn is_under_sibling(lines: &[&str], line_idx: usize, key: &str) -> bool {
    let current_indent = lines
        .get(line_idx)
        .map(|l| l.len() - l.trim_start().len())
        .unwrap_or(0);

    for i in (0..line_idx).rev() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let line_indent = line.len() - trimmed.len();
        if line_indent < current_indent && trimmed.starts_with(key) {
            return true;
        }
        if line_indent < current_indent {
            return false;
        }
    }
    false
}

/// Find the value of a field (e.g. `mcp: novanet`) near the current line.
fn find_field_value(lines: &[&str], line_idx: usize, field: &str) -> Option<String> {
    // Search in the enclosing block (backward then forward).
    let start = line_idx.saturating_sub(15);
    let end = (line_idx + 10).min(lines.len());
    for line in lines.iter().take(end).skip(start) {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(field) {
            let val = rest.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                return Some(val.to_string());
            }
        }
    }
    None
}

/// Find the verb used in the current task.
fn find_verb_for_task(lines: &[&str], line_idx: usize) -> Option<String> {
    let task_start = find_task_start_line(lines, line_idx);
    for line in lines
        .iter()
        .take(find_next_task_line(lines, task_start))
        .skip(task_start)
    {
        let trimmed = line.trim();
        if line_starts_with_verb(trimmed) {
            return Some(extract_verb_name(trimmed));
        }
    }
    None
}

/// Extract sibling field names at the same indent level.
fn extract_sibling_fields(lines: &[&str], line_idx: usize) -> Vec<String> {
    let current_indent = lines
        .get(line_idx)
        .map(|l| l.len() - l.trim_start().len())
        .unwrap_or(0);

    let mut fields = Vec::new();
    // Scan backward.
    for i in (0..line_idx).rev() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cur_indent = line.len() - trimmed.len();
        if cur_indent < current_indent {
            break;
        }
        if cur_indent == current_indent && trimmed.contains(':') {
            if let Some(key) = trimmed.split(':').next() {
                let key = key.trim_start_matches("- ").trim();
                if !key.is_empty() {
                    fields.push(key.to_string());
                }
            }
        }
    }
    // Scan forward.
    for line in lines.iter().skip(line_idx + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cur_indent = line.len() - trimmed.len();
        if cur_indent < current_indent {
            break;
        }
        if cur_indent == current_indent && trimmed.contains(':') {
            if let Some(key) = trimmed.split(':').next() {
                let key = key.trim_start_matches("- ").trim();
                if !key.is_empty() {
                    fields.push(key.to_string());
                }
            }
        }
    }
    fields
}

/// Extract all task IDs from the document text.
pub fn extract_task_ids(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let id_value = trimmed
            .strip_prefix("- id:")
            .or_else(|| trimmed.strip_prefix("-id:"));
        if let Some(id) = id_value {
            let id = id.trim().trim_matches('"').trim_matches('\'');
            if !id.is_empty() {
                ids.push(id.to_string());
            }
        } else if let Some(stripped) = trimmed.strip_prefix("id:") {
            let line_indent = line.len() - trimmed.len();
            if line_indent > 0 {
                let id = stripped.trim().trim_matches('"').trim_matches('\'');
                if !id.is_empty() {
                    ids.push(id.to_string());
                }
            }
        }
    }
    ids
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: detect context at a given line/character position.
    fn ctx(text: &str, line: usize, character: usize) -> CursorContext {
        let offset = text_offset(text, line, character);
        detect_context(text, offset as u32, None)
    }

    /// Convert line/character to byte offset.
    fn text_offset(text: &str, line: usize, character: usize) -> usize {
        let mut offset = 0;
        for (i, l) in text.lines().enumerate() {
            if i == line {
                return offset + character.min(l.len());
            }
            offset += l.len() + 1; // +1 for '\n'
        }
        text.len()
    }

    #[test]
    fn empty_text_is_workflow_root() {
        let c = detect_context("", 0, None);
        assert!(matches!(c, CursorContext::WorkflowRoot { .. }));
    }

    #[test]
    fn root_level_indent_zero() {
        let c = ctx("schema: nika/workflow@0.12\n", 0, 5);
        assert!(matches!(c, CursorContext::WorkflowRoot { .. }));
    }

    #[test]
    fn task_field_context() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    ";
        let c = ctx(yaml, 3, 4);
        assert!(matches!(c, CursorContext::TaskField { .. }));
    }

    #[test]
    fn verb_block_infer() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      prompt: hello
      ";
        let c = ctx(yaml, 5, 6);
        match &c {
            CursorContext::VerbBlock { verb, .. } => assert_eq!(verb, "infer"),
            other => panic!("Expected VerbBlock, got {other:?}"),
        }
    }

    #[test]
    fn with_block_context() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
  - id: step2
    with:
      data: step1
      ";
        // On the line "      data: step1"
        let c = ctx(yaml, 6, 10);
        assert!(
            matches!(c, CursorContext::WithBlock { .. }),
            "Expected WithBlock, got {c:?}"
        );
    }

    #[test]
    fn template_context() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: \"Hello {{with.data}}\"
";
        // Position inside the {{ }}
        let offset = yaml.find("with.").unwrap() + 3;
        let c = detect_context(yaml, offset as u32, None);
        match &c {
            CursorContext::Template {
                partial_expr,
                in_transform_chain,
                ..
            } => {
                assert!(partial_expr.starts_with("wit"));
                assert!(!in_transform_chain);
            }
            other => panic!("Expected Template, got {other:?}"),
        }
    }

    #[test]
    fn mcp_config_context() {
        let yaml = "\
schema: nika/workflow@0.12
mcp:
  novanet:
    command: cargo run
    ";
        let c = ctx(yaml, 3, 4);
        assert!(
            matches!(c, CursorContext::McpConfig { .. }),
            "Expected McpConfig, got {c:?}"
        );
    }

    #[test]
    fn invoke_block_context() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    invoke:
      mcp: novanet
      tool: query
      ";
        let c = ctx(yaml, 5, 6);
        assert!(
            matches!(c, CursorContext::InvokeBlock { .. }),
            "Expected InvokeBlock, got {c:?}"
        );
    }

    #[test]
    fn depends_on_context() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
  - id: step2
    depends_on:
      - step1
      ";
        // Inside depends_on block
        let c = ctx(yaml, 6, 6);
        assert!(
            matches!(c, CursorContext::DependsOn { .. }),
            "Expected DependsOn, got {c:?}"
        );
    }

    #[test]
    fn content_part_context() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      content:
        - type: text
          text: hello
        ";
        let c = ctx(yaml, 6, 10);
        assert!(
            matches!(c, CursorContext::ContentPart { .. }),
            "Expected ContentPart, got {c:?}"
        );
    }

    #[test]
    fn for_each_context() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    for_each:
      items: [a, b]
      ";
        let c = ctx(yaml, 4, 6);
        assert!(
            matches!(c, CursorContext::ForEach { .. }),
            "Expected ForEach, got {c:?}"
        );
    }

    #[test]
    fn retry_block_context() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
    retry:
      max_attempts: 3
      ";
        let c = ctx(yaml, 5, 6);
        assert!(
            matches!(c, CursorContext::RetryBlock { .. }),
            "Expected RetryBlock, got {c:?}"
        );
    }

    #[test]
    fn schema_block_context() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
    structured:
      schema:
        type: object
        ";
        let c = ctx(yaml, 6, 8);
        assert!(
            matches!(c, CursorContext::SchemaBlock { .. }),
            "Expected SchemaBlock, got {c:?}"
        );
    }

    #[test]
    fn guardrails_context() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: hello
    guardrails:
      input: safety
      ";
        let c = ctx(yaml, 5, 6);
        assert!(
            matches!(c, CursorContext::Guardrails { .. }),
            "Expected Guardrails, got {c:?}"
        );
    }

    #[test]
    fn unknown_context_outside_blocks() {
        let yaml = "\
schema: nika/workflow@0.12
  orphan-indented: true
";
        let c = ctx(yaml, 1, 10);
        assert!(
            matches!(c, CursorContext::Unknown { .. }),
            "Expected Unknown, got {c:?}"
        );
    }

    #[test]
    fn extract_task_ids_basic() {
        let text = "\
tasks:
  - id: step1
    infer: hello
  - id: step2
";
        let ids = extract_task_ids(text);
        assert_eq!(ids, vec!["step1", "step2"]);
    }

    #[test]
    fn extract_task_ids_empty() {
        assert!(extract_task_ids("").is_empty());
    }

    #[test]
    fn offset_past_text_returns_unknown() {
        let c = detect_context("short", 9999, None);
        assert!(matches!(c, CursorContext::Unknown { .. }));
    }

    #[test]
    fn template_with_transform_chain() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer: \"{{with.data | upper }}\"
";
        let offset = yaml.find("upper").unwrap() + 2;
        let c = detect_context(yaml, offset as u32, None);
        match &c {
            CursorContext::Template {
                in_transform_chain, ..
            } => assert!(in_transform_chain),
            other => panic!("Expected Template, got {other:?}"),
        }
    }

    #[test]
    fn task_field_has_task_id() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: my-task
    ";
        let c = ctx(yaml, 3, 4);
        match &c {
            CursorContext::TaskField { task_id, .. } => {
                assert_eq!(task_id.as_deref(), Some("my-task"));
            }
            other => panic!("Expected TaskField, got {other:?}"),
        }
    }

    #[test]
    fn verb_block_exec() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    exec:
      command: echo hi
      ";
        let c = ctx(yaml, 4, 6);
        match &c {
            CursorContext::VerbBlock { verb, .. } => assert_eq!(verb, "exec"),
            other => panic!("Expected VerbBlock(exec), got {other:?}"),
        }
    }

    #[test]
    fn verb_block_fetch() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    fetch:
      url: https://api.example.com
      ";
        let c = ctx(yaml, 4, 6);
        match &c {
            CursorContext::VerbBlock { verb, .. } => assert_eq!(verb, "fetch"),
            other => panic!("Expected VerbBlock(fetch), got {other:?}"),
        }
    }

    #[test]
    fn verb_block_agent() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    agent:
      prompt: research AI
      ";
        let c = ctx(yaml, 4, 6);
        match &c {
            CursorContext::VerbBlock { verb, .. } => assert_eq!(verb, "agent"),
            other => panic!("Expected VerbBlock(agent), got {other:?}"),
        }
    }

    #[test]
    fn invoke_block_with_mcp_server() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    invoke:
      mcp: novanet
      tool: query
      params:
        q: test
";
        // Cursor on the `mcp:` field (char 10 captures "mcp: ")
        let c = ctx(yaml, 4, 10);
        match &c {
            CursorContext::InvokeBlock {
                mcp_server,
                focus: InvokeFocus::McpServer,
                ..
            } => {
                assert_eq!(mcp_server.as_deref(), Some("novanet"));
            }
            other => panic!("Expected InvokeBlock/McpServer, got {other:?}"),
        }

        // Cursor at indent (char 6) gives General focus (no field typed yet)
        let c2 = ctx(yaml, 4, 6);
        match &c2 {
            CursorContext::InvokeBlock {
                mcp_server,
                focus: InvokeFocus::General,
                ..
            } => {
                assert_eq!(mcp_server.as_deref(), Some("novanet"));
            }
            other => panic!("Expected InvokeBlock/General, got {other:?}"),
        }
    }

    #[test]
    fn content_focus_image_detail() {
        let yaml = "\
schema: nika/workflow@0.12
tasks:
  - id: step1
    infer:
      content:
        - type: image
          source: hash123
          detail: high
";
        // Cursor at char 17 captures "detail:" in the prefix
        let c = ctx(yaml, 7, 17);
        match &c {
            CursorContext::ContentPart {
                focus: ContentFocus::ImageDetail,
                ..
            } => {}
            other => panic!("Expected ContentPart/ImageDetail, got {other:?}"),
        }
    }

    #[test]
    fn mcp_config_with_server_name() {
        let yaml = "\
schema: nika/workflow@0.12
mcp:
  novanet:
    command: cargo
    args: [run]
";
        let c = ctx(yaml, 3, 6);
        match &c {
            CursorContext::McpConfig { server_name, .. } => {
                assert_eq!(server_name.as_deref(), Some("novanet"));
            }
            other => panic!("Expected McpConfig, got {other:?}"),
        }
    }
}
