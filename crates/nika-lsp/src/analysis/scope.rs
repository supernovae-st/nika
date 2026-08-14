// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Upward block scanning — the completion lanes that depend on WHERE the
//! cursor sits (which task · which `tool:` · inside `args:`) share these
//! line-walk primitives. Same v0.1 register as the rest of the analysis:
//! robust line heuristics over a full AST, silence beats noise.

/// The id of the task whose block encloses `offset` — the nearest
/// preceding task map key (W1 « the map »: a bare `name:` at indent 2).
/// `None` above the first task.
pub(super) fn current_task_id(text: &str, offset: usize) -> Option<String> {
    for line in lines_upward(text, offset) {
        if is_task_boundary(line) {
            let head = line.trim_start().split('#').next().unwrap_or("").trim_end();
            return head.strip_suffix(':').map(str::to_owned);
        }
        // A column-0 key above us means we left the tasks block — a
        // workflow-level `outputs:`/`vars:` island is NOT inside the
        // last task that happens to sit above it.
        let t = line.trim_start();
        if !t.is_empty() && line.len() == t.len() {
            return None;
        }
    }
    None
}

/// Whether the cursor's line is a `recover:` value — the DAG-003
/// carve-out (spec 05 §recover): refs there are NOT edges, only the
/// DAG-004 no-downstream law binds. Line-based like the rest of v0.1
/// (a multi-line block-scalar recover value is out of this heuristic's
/// reach — silence there, never a wrong set).
/// Whether `offset` sits inside the ENCLOSING task-level block named
/// `key` (`with:` · `after:` · `on_finally:`) — an upward line walk
/// from the cursor: the nearest shallower-indented block head before
/// the task boundary decides. Value positions (islands · flow scalars)
/// resolve correctly where a bare-key detector cannot (the lane-router
/// fix the 199-test migration pinned).
pub(super) fn in_task_block(text: &str, offset: usize, key: &str) -> bool {
    let upto = text.get(..offset).unwrap_or("");
    let cursor_line_start = upto.rfind('\n').map_or(0, |i| i + 1);
    let cursor_indent = {
        let line = upto.get(cursor_line_start..).unwrap_or("");
        let full = text.get(cursor_line_start..).unwrap_or("");
        let l = full.split('\n').next().unwrap_or(line);
        l.len() - l.trim_start().len()
    };
    // same-line head (`with: ${{ tasks. …` · flow forms)
    let cursor_line = text
        .get(cursor_line_start..)
        .unwrap_or("")
        .split('\n')
        .next()
        .unwrap_or("");
    if cursor_line
        .trim_start()
        .strip_prefix(key)
        .is_some_and(|rest| rest.starts_with(':'))
    {
        return true;
    }
    let mut min_indent = cursor_indent;
    for line in upto[..cursor_line_start].split('\n').rev() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent >= min_indent && min_indent > 0 {
            continue; // deeper or sibling content — keep climbing
        }
        min_indent = indent;
        if is_task_boundary(line) || indent == 0 {
            return false; // reached the task key (or top level) first
        }
        let head = line.trim_start().split('#').next().unwrap_or("").trim_end();
        if let Some(name) = head.strip_suffix(':')
            && !name.is_empty()
            && name == key
        {
            return true;
        }
        // else: an intermediate block head (an `on_finally:` mini-task key ·
        // a nested map) or a key with an inline value — climb past it.
    }
    false
}

pub(super) fn in_recover_value(text: &str, offset: usize) -> bool {
    let upto = text.get(..offset).unwrap_or("");
    let line_start = upto.rfind('\n').map_or(0, |i| i + 1);
    upto.get(line_start..)
        .unwrap_or("")
        .trim_start()
        .starts_with("recover:")
}

/// W1 « the map »: a task boundary is the task's MAP KEY — a bare
/// `name:` block key at indent 2 (the `- id:` row died with the list
/// form). Two-indent entries that carry inline values (vars · outputs)
/// never match; a typed-var block HEAD does — an acceptable stop for
/// every upward predicate here (nothing task-scoped lives above it
/// either way).
fn is_task_boundary(line: &str) -> bool {
    let indent = line.len() - line.trim_start().len();
    if indent != 2 {
        return false;
    }
    let t = line.trim_start();
    let head = t.split('#').next().unwrap_or("").trim_end();
    let Some(name) = head.strip_suffix(':') else {
        return false;
    };
    !name.is_empty()
        && name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// The producers this task attaches to as an UNWIND — every `X` in an
/// `after: { X: unwind }` of the enclosing task. Declaration order. An
/// unwind task is the one shape whose readable set does NOT follow the
/// surface: it reads its producer from a verb body too, because the
/// cleanup runs after that producer settled either way (spec 03 §the
/// rest of the contract). Empty for every other task.
pub(super) fn unwind_producers(text: &str, offset: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen_boundary = false;
    for line in lines_upward(text, offset) {
        if seen_boundary {
            break;
        }
        if is_task_boundary(line) {
            seen_boundary = true;
        }
        let body = line.split('#').next().unwrap_or("");
        if !body.contains("unwind") {
            continue;
        }
        for chunk in body.split(',') {
            let Some((lhs, rhs)) = chunk.rsplit_once(':') else {
                continue;
            };
            if rhs.trim().trim_end_matches(&['}', ' '][..]) != "unwind" {
                continue;
            }
            let id = lhs
                .trim()
                .trim_start_matches(&['a', 'f', 't', 'e', 'r', ' ', '{'][..]);
            let id = id.rsplit('{').next().unwrap_or(id).trim();
            if !id.is_empty() && !out.contains(&id.to_owned()) {
                out.push(id.to_owned());
            }
        }
    }
    out.reverse();
    out
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
        if is_task_boundary(line) {
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
        if is_task_boundary(line) || line_indent == 0 {
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
        if is_task_boundary(line) || line_indent == 0 {
            return false;
        }
        current = line_indent;
    }
    false
}

/// The key of the block ENCLOSING the cursor's key position, plus that
/// block's own parent key — `(block, parent)`. The nearest shallower
/// non-blank line's key is the block; the next shallower one is its
/// parent. `None` when the cursor is not at an indented key position.
pub(super) fn enclosing_block_key(text: &str, offset: usize) -> Option<(String, Option<String>)> {
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
    if indent == 0 {
        return None;
    }
    let mut block: Option<(String, usize)> = None;
    for line in lines_upward(text, offset) {
        if line.trim().is_empty() {
            continue;
        }
        let t = line.trim_start();
        let line_indent = line.len() - t.len();
        let key = t
            .trim_start_matches("- ")
            .split(':')
            .next()
            .unwrap_or("")
            .to_owned();
        match &block {
            None if line_indent < indent && t.contains(':') => {
                block = Some((key, line_indent));
            }
            Some((_, block_indent)) if line_indent < *block_indent && t.contains(':') => {
                return block.map(|(b, _)| (b, Some(key)));
            }
            _ => {}
        }
    }
    block.map(|(b, _)| (b, None))
}

/// Whether the task enclosing `offset` declares `for_each:` — the gate
/// for the loop-scoped roots (`item` · `index` are alive only inside a
/// fan-out task · spec 04 §loop-scoped locals). Line-walk like the rest
/// of this module: scan up to the task boundary looking for the key.
pub(super) fn in_for_each_task(text: &str, offset: usize) -> bool {
    for line in lines_upward(text, offset) {
        let t = line.trim_start();
        if t.starts_with("for_each:") {
            return true;
        }
        if is_task_boundary(line) {
            return false;
        }
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

/// Whether the cursor sits on a block-list ITEM line (`- ` · `- "…`
/// partial) whose nearest shallower non-blank ancestor is `<key>:` —
/// the block-form twin of the flow-list detectors.
pub(super) fn in_block_list_item(text: &str, offset: usize, key: &str) -> bool {
    let upto = text.get(..offset).unwrap_or("");
    let line_start = upto.rfind('\n').map_or(0, |i| i + 1);
    let prefix = &upto[line_start..];
    let typed = prefix.trim_start();
    if !typed.starts_with('-') {
        return false;
    }
    let indent = prefix.len() - typed.len();
    if indent == 0 {
        return false; // a top-level list is never a task-field register
    }
    for line in lines_upward(text, offset) {
        if line.trim().is_empty() {
            continue;
        }
        let line_indent = line.len() - line.trim_start().len();
        if line_indent < indent {
            return line.trim_start().starts_with(key);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "nika: w\ntasks:\n  fetch_article:\n    invoke:\n      tool: nika:fetch\n      args:\n        url: \"https://x\"\n        \n  second:\n    exec:\n      command: [\"ls\"]\n";

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
        let doc = "tasks:\n  a:\n    invoke:\n      tool: \"nika:jq\"  # data\n      args:\n        x: 1\n";
        let in_args = doc.find("x: 1").expect("x line");
        assert_eq!(enclosing_tool(doc, in_args).as_deref(), Some("nika:jq"));
    }
}
