// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Upward block scanning — the completion lanes that depend on WHERE the
//! cursor sits (which task · which `tool:` · inside `args:`) share these
//! line-walk primitives. Same v0.1 register as the rest of the analysis:
//! robust line heuristics over a full AST, silence beats noise.

/// The id of the task whose block encloses `offset` — the nearest
/// preceding `- id:` line. `None` above the first task.
pub(super) fn current_task_id(text: &str, offset: usize) -> Option<String> {
    for line in lines_upward(text, offset) {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("- id:") {
            return Some(unquote(rest).to_owned());
        }
    }
    None
}

/// The `tool:` value of the invoke block enclosing `offset`, when the
/// enclosing task declares one — the scan stops at the task boundary
/// (`- id:`) so a PREVIOUS task's tool never leaks into this one.
pub(super) fn enclosing_tool(text: &str, offset: usize) -> Option<String> {
    for line in lines_upward(text, offset) {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("tool:") {
            return Some(unquote(rest).to_owned());
        }
        if t.starts_with("- id:") {
            return None;
        }
    }
    None
}

/// The indent of the cursor's KEY position — `Some(indent)` when the
/// line so far is an indented bare word (no `:` typed yet), the shared
/// precondition of every key-completion lane.
fn key_position_indent(text: &str, offset: usize) -> Option<usize> {
    let upto = text.get(..offset).unwrap_or("");
    let line_start = upto.rfind('\n').map_or(0, |i| i + 1);
    let prefix = &upto[line_start..];
    let typed = prefix.trim_start();
    if !typed.is_empty()
        && !typed
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    let indent = prefix.len() - typed.len();
    (indent > 0).then_some(indent)
}

/// Whether the cursor sits at a KEY position directly inside an `args:`
/// block — an indented bare word (no `:` typed yet) whose nearest
/// shallower non-blank ancestor line is `args:`. Nested maps inside an
/// argument value (a deeper ancestor that is not `args:`) stay silent.
pub(super) fn in_args_key_position(text: &str, offset: usize) -> bool {
    let Some(indent) = key_position_indent(text, offset) else {
        return false;
    };
    for line in lines_upward(text, offset) {
        if line.trim().is_empty() {
            continue;
        }
        let line_indent = line.len() - line.trim_start().len();
        if line_indent < indent {
            return line.trim_start().starts_with("args:");
        }
    }
    false
}

/// Whether the cursor sits at a KEY position whose IMMEDIATE ancestor
/// opens a `schema:` (or a nested `items:` inside one) — the JSON-Schema
/// vocabulary position. Direct children of `properties:` stay silent:
/// those names belong to the author, not the spec.
pub(super) fn in_schema_key_position(text: &str, offset: usize) -> bool {
    let Some(indent) = key_position_indent(text, offset) else {
        return false;
    };
    let mut current = indent;
    let mut immediate = true;
    for line in lines_upward(text, offset) {
        if line.trim().is_empty() {
            continue;
        }
        let line_indent = line.len() - line.trim_start().len();
        if line_indent >= current {
            continue;
        }
        let key = line.trim_start();
        if immediate {
            if key.starts_with("schema:") {
                return true;
            }
            if key.starts_with("items:") {
                // `items:` counts only when IT sits inside a schema —
                // keep walking the chain to find out.
                immediate = false;
                current = line_indent;
                continue;
            }
            return false;
        }
        if key.starts_with("schema:") {
            return true;
        }
        if key.starts_with("- id:") || line_indent == 0 {
            return false;
        }
        current = line_indent;
    }
    false
}

/// Whether the ancestor CHAIN above the cursor crosses a `schema:` key
/// before the task boundary — anywhere inside the block, any depth. The
/// task-field lane stays silent here (a `schema:` block speaks
/// JSON-Schema, never `depends_on`).
pub(super) fn in_schema_scope(text: &str, offset: usize) -> bool {
    let upto = text.get(..offset).unwrap_or("");
    let line_start = upto.rfind('\n').map_or(0, |i| i + 1);
    let prefix = &upto[line_start..];
    let mut current = prefix.len() - prefix.trim_start().len();
    if current == 0 {
        return false;
    }
    for line in lines_upward(text, offset) {
        if line.trim().is_empty() {
            continue;
        }
        let line_indent = line.len() - line.trim_start().len();
        if line_indent >= current {
            continue;
        }
        let key = line.trim_start();
        if key.starts_with("schema:") {
            return true;
        }
        if key.starts_with("- id:") || line_indent == 0 {
            return false;
        }
        current = line_indent;
    }
    false
}

/// The lines strictly above `offset`'s line, nearest first.
fn lines_upward(text: &str, offset: usize) -> impl Iterator<Item = &str> {
    let upto = text.get(..offset).unwrap_or("");
    let line_start = upto.rfind('\n').map_or(0, |i| i + 1);
    text.get(..line_start)
        .unwrap_or("")
        .lines()
        .rev()
        .collect::<Vec<_>>()
        .into_iter()
}

/// A scalar value with optional YAML quotes and trailing comment shed.
fn unquote(rest: &str) -> &str {
    let v = rest.split('#').next().unwrap_or("").trim();
    v.trim_matches('"').trim_matches('\'')
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "nika: v1\nworkflow: w\ntasks:\n  - id: fetch_article\n    invoke:\n      tool: nika:fetch\n      args:\n        url: \"https://x\"\n        \n  - id: second\n    exec:\n      command: ls\n";

    #[test]
    fn current_task_is_the_nearest_id_above() {
        let in_args = DOC.find("url:").expect("url line");
        assert_eq!(
            current_task_id(DOC, in_args).as_deref(),
            Some("fetch_article")
        );
        let in_second = DOC.find("command:").expect("command line");
        assert_eq!(current_task_id(DOC, in_second).as_deref(), Some("second"));
        assert_eq!(current_task_id(DOC, 0), None);
    }

    #[test]
    fn enclosing_tool_stops_at_the_task_boundary() {
        let in_args = DOC.find("url:").expect("url line");
        assert_eq!(enclosing_tool(DOC, in_args).as_deref(), Some("nika:fetch"));
        // the second task declares no tool — the first task's must not leak
        let in_second = DOC.find("command:").expect("command line");
        assert_eq!(enclosing_tool(DOC, in_second), None);
    }

    #[test]
    fn args_key_position_wants_the_args_ancestor() {
        // the blank-indented line inside args: IS a key position
        let blank = DOC.find("\n        \n").expect("blank args line") + 9;
        assert!(in_args_key_position(DOC, blank));
        // a value position (after `url: `) is not
        let after_url = DOC.find("url:").expect("url") + "url: ".len();
        assert!(!in_args_key_position(DOC, after_url));
        // a task-field key position (ancestor is the task item, not args:)
        let field = DOC.find("invoke:").expect("invoke line");
        assert!(!in_args_key_position(DOC, field));
    }

    #[test]
    fn quotes_and_comments_shed_from_scalar_values() {
        let doc = "tasks:\n  - id: a\n    invoke:\n      tool: \"nika:jq\"  # data\n      args:\n        x: 1\n";
        let in_args = doc.find("x: 1").expect("x line");
        assert_eq!(enclosing_tool(doc, in_args).as_deref(), Some("nika:jq"));
    }
}
