// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Pure prepass transforms moved unchanged from the host repair ladder.

fn indent_of(line: &str) -> &str {
    line.split_at(line.len() - line.trim_start().len()).0
}

/// Wrap supported bare exec text without changing unrelated lines.
#[must_use]
pub fn wrap_bare_exec(source: &str) -> Option<String> {
    let mut changed = false;
    let mut out = String::new();
    for line in source.lines() {
        if let Some(wrapped) = wrap_one_bare_exec(line) {
            out.push_str(&wrapped);
            changed = true;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !source.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    changed.then_some(out)
}

fn wrap_one_bare_exec(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("exec:")?.trim();
    if rest.is_empty()
        || rest.starts_with('{')
        || rest.starts_with('[')
        || rest.starts_with('|')
        || rest.starts_with('>')
        || rest == "true"
        || rest == "false"
    {
        return None;
    }
    let indent = indent_of(line);
    let unquoted = rest.trim().trim_matches(|c| c == '"' || c == '\'');
    if unquoted.is_empty() {
        return None;
    }
    // Live dialect: argv for inert tokens, `shell:` for metacharacters.
    // Writing both `command:` and `shell: true` is the 0.102 form and
    // PARSE-019s (P08 · C13).
    if unquoted
        .chars()
        .any(|c| matches!(c, '|' | ';' | '&' | '<' | '>' | '`' | '$' | '(' | ')'))
    {
        let escaped = unquoted.replace('\\', "\\\\").replace('"', "\\\"");
        return Some(format!("{indent}exec:\n{indent}  shell: \"{escaped}\""));
    }
    let args: Vec<String> = unquoted
        .split_whitespace()
        .map(|w| format!("\"{w}\""))
        .collect();
    Some(format!(
        "{indent}exec:\n{indent}  command: [{}]",
        args.join(", ")
    ))
}

/// Whether an unhandled bare exec spelling remains.
#[must_use]
pub fn has_bare_exec(source: &str) -> bool {
    source.lines().any(|line| {
        let t = line.trim_start();
        t.strip_prefix("exec:").is_some_and(|rest| {
            let rest = rest.trim();
            !rest.is_empty()
                && !rest.starts_with('{')
                && !rest.starts_with('[')
                && rest != "true"
                && rest != "false"
        })
    })
}

/// Rewrite supported needs lists only when no sibling after key exists.
#[must_use]
pub fn rewrite_needs(source: &str) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut changed = false;
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(rewritten) = rewrite_one_needs(line) {
            if sibling_has_after(&lines, i) {
                out.push_str(line);
            } else {
                out.push_str(&rewritten);
                changed = true;
            }
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !source.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }
    changed.then_some(out)
}

fn rewrite_one_needs(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("needs:")?.trim();
    let rest = rest.split('#').next().unwrap_or(rest).trim();
    let inner = rest.strip_prefix('[')?.strip_suffix(']')?.trim();
    if inner.is_empty() {
        return None;
    }
    let mut ids = Vec::new();
    for raw in inner.split(',') {
        let id = raw.trim().trim_matches('"').trim_matches('\'').trim();
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return None;
        }
        ids.push(id);
    }
    if ids.is_empty() {
        return None;
    }
    let map = ids
        .iter()
        .map(|id| format!("{id}: success"))
        .collect::<Vec<_>>()
        .join(", ");
    let indent = indent_of(line);
    Some(format!("{indent}after: {{ {map} }}"))
}

fn sibling_has_after(lines: &[&str], idx: usize) -> bool {
    let indent = indent_of(lines[idx]);
    let same_key = |line: &str| {
        let t = line.trim_start();
        indent_of(line) == indent && t.starts_with("after:")
    };
    lines[..idx]
        .iter()
        .rev()
        .take_while(|l| {
            let t = l.trim();
            t.is_empty() || t.starts_with('#') || indent_of(l).len() >= indent.len()
        })
        .any(|l| same_key(l))
        || lines[idx + 1..]
            .iter()
            .take_while(|l| {
                let t = l.trim();
                t.is_empty() || t.starts_with('#') || indent_of(l).len() >= indent.len()
            })
            .any(|l| same_key(l))
}

/// Whether an unhandled needs key remains.
#[must_use]
pub fn has_needs_key(source: &str) -> bool {
    source.lines().any(|line| {
        let t = line.trim_start();
        t.starts_with("needs:") && !t.starts_with("needs: #")
    })
}
