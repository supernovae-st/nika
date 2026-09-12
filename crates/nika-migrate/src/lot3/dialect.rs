// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Contextual spellings inside task verbs. Payload keys are never grammar.
use super::{Edits, TaskSpan, indent_of, key_of, nested_end, unquote, value_of};

#[derive(Clone, Copy)]
enum Context {
    Task,
    Invoke,
    Exec,
}

impl Context {
    fn rename(self) -> Option<(&'static str, &'static str, &'static str)> {
        match self {
            Self::Task => None,
            Self::Invoke => Some(("params", "args", "invoke-args")),
            Self::Exec => Some(("argv", "command", "exec-command")),
        }
    }
}

pub(super) fn migrate(lines: &[&str], span: &TaskSpan, edits: &mut Edits) {
    let header = lines[span.header];
    if value_of(header).starts_with('{') {
        rewrite_line(span.header, header, Context::Task, edits);
        return;
    }
    for index in span.header + 1..span.end {
        let line = lines[index];
        if indent_of(line) != span.body_indent {
            continue;
        }
        let context = match key_of(line) {
            Some("invoke") => Context::Invoke,
            Some("exec") => Context::Exec,
            _ => continue,
        };
        if value_of(line).starts_with('{') {
            rewrite_line(index, line, context, edits);
        } else if value_of(line).is_empty() {
            block_verb(
                lines,
                index,
                nested_end(lines, index, span.end),
                context,
                edits,
            );
        }
    }
}

fn rewrite_line(index: usize, line: &str, context: Context, edits: &mut Edits) {
    let Some(colon) = line.find(':') else { return };
    if let Some(value) = flow_map(&line[colon + 1..], context, edits) {
        edits
            .replace
            .push((index, format!("{}{value}", &line[..=colon])));
    }
}

pub(super) fn field(line: &str) -> Option<(&str, &str, usize)> {
    let colon = line.find(':')?;
    let key = unquote(line[..colon].trim());
    (!key.is_empty() && !key.starts_with('#')).then_some((key, &line[colon + 1..], colon))
}

pub(super) fn simple_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn collision(keys: &[&str], context: Context, edits: &mut Edits) -> bool {
    let Some((old, new, _)) = context.rename() else {
        return false;
    };
    let count = keys.iter().filter(|key| **key == old).count();
    if count > 0 && keys.iter().any(|key| !simple_key(key)) {
        edits.notes.push(format!("`{old}:` shares a mapping with a key whose YAML spelling is not provably plain — normalize the keys by hand before renaming to `{new}:`"));
        return true;
    }
    if count > 1
        || (count == 1
            && (keys.contains(&new) || matches!(context, Context::Exec) && keys.contains(&"shell")))
    {
        edits.notes.push(format!("`{old}:` conflicts with another verb field — choose one `{new}:` declaration by hand; no value was discarded"));
        true
    } else {
        false
    }
}

fn value_shape(value: &str, context: Context) -> bool {
    let (body, _) = crate::split_flow_comment(value);
    let body = body.trim();
    match context {
        Context::Exec => {
            body.starts_with('[') && body.ends_with(']') && crate::flow_scan(body).balanced
        }
        Context::Invoke => {
            body.starts_with('{') && body.ends_with('}') && crate::flow_scan(body).balanced
        }
        Context::Task => {
            (body.starts_with('[') && body.ends_with(']') || unquote(body).starts_with("${{"))
                && crate::flow_scan(body).balanced
        }
    }
}

fn shape_stop(context: Context, edits: &mut Edits) {
    let (field, shape) = match context {
        Context::Exec => (
            "exec.argv",
            "a sequence of arguments; choose command: [...] or shell: explicitly",
        ),
        Context::Invoke => (
            "invoke.params",
            "an argument mapping; write args: explicitly",
        ),
        Context::Task => return,
    };
    edits.notes.push(format!(
        "`{field}` is not provably {shape} — migrate by hand"
    ));
}

fn block_verb(lines: &[&str], header: usize, end: usize, context: Context, edits: &mut Edits) {
    let entries: Vec<_> = (header + 1..end)
        .filter(|i| !lines[*i].trim().is_empty() && !lines[*i].trim_start().starts_with('#'))
        .collect();
    let Some(indent) = entries.iter().map(|i| indent_of(lines[*i])).min() else {
        return;
    };
    let fields: Vec<_> = entries
        .iter()
        .copied()
        .filter(|i| indent_of(lines[*i]) == indent)
        .filter_map(|i| field(lines[i]).map(|(key, value, colon)| (i, key, value, colon)))
        .collect();
    let keys: Vec<_> = entries
        .iter()
        .filter(|i| indent_of(lines[**i]) == indent)
        .map(|i| field(lines[*i]).map_or("", |f| f.0))
        .collect();
    if collision(&keys, context, edits) {
        return;
    }
    let Some((old, new, rung)) = context.rename() else {
        return;
    };
    for (i, key, value, colon) in fields {
        if key != old {
            continue;
        }
        let (body, _) = crate::split_flow_comment(value);
        let shape = if body.trim().is_empty() {
            lines[i + 1..nested_end(lines, i, end)]
                .iter()
                .find(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
                .is_some_and(|line| match context {
                    Context::Exec => line.trim_start().starts_with("- "),
                    Context::Invoke => key_of(line).is_some(),
                    Context::Task => false,
                })
        } else {
            value_shape(value, context)
        };
        if !shape {
            shape_stop(context, edits);
            continue;
        }
        let prefix = lines[i][..colon].replacen(old, new, 1);
        edits
            .replace
            .push((i, format!("{prefix}:{}", &lines[i][colon + 1..])));
        edits.fired(rung);
    }
}

fn flow_map(value: &str, context: Context, edits: &mut Edits) -> Option<String> {
    let (body, _) = crate::split_flow_comment(value);
    let map = body.trim();
    let inner = map.strip_prefix('{')?.strip_suffix('}')?;
    let scan = crate::flow_scan(inner);
    if !scan.balanced {
        return None;
    }
    let offset = value.find('{')? + 1;
    let mut ranges = Vec::new();
    let mut start = 0;
    for end in scan.commas.into_iter().chain(std::iter::once(inner.len())) {
        ranges.push((start, end));
        start = end + 1;
    }
    // A final comma is valid YAML, not an opaque mapping key.
    if ranges
        .last()
        .is_some_and(|(start, end)| inner[*start..*end].trim().is_empty())
    {
        ranges.pop();
    }
    let keys: Vec<_> = ranges
        .iter()
        .map(|(a, b)| field(&inner[*a..*b]).map_or("", |f| f.0))
        .collect();
    if collision(&keys, context, edits) {
        return None;
    }
    let mut replacements = Vec::new();
    for (start, end) in ranges {
        let entry = &inner[start..end];
        let Some((key, old_value, colon)) = field(entry) else {
            continue;
        };
        if let Some((old, new, rung)) = context.rename() {
            if key != old {
                continue;
            }
            if !value_shape(old_value, context) {
                shape_stop(context, edits);
                continue;
            }
            replacements.push((
                offset + start,
                offset + start + colon,
                entry[..colon].replacen(old, new, 1),
            ));
            edits.fired(rung);
        } else {
            let changed = match key {
                "exec" => flow_map(old_value, Context::Exec, edits),
                "invoke" => flow_map(old_value, Context::Invoke, edits),
                "for_each" if value_shape(old_value, Context::Task) => {
                    if keys.contains(&"max_parallel") || keys.contains(&"fail_fast") {
                        edits.notes.push("a flow task with retired fan-out knobs needs a block task before migration".to_owned());
                        None
                    } else {
                        edits.fired("for-each-items");
                        Some(format!(" {{ items:{old_value} }}"))
                    }
                }
                _ => None,
            };
            if let Some(changed) = changed {
                replacements.push((offset + start + colon + 1, offset + end, changed));
            }
        }
    }
    if replacements.is_empty() {
        return None;
    }
    let mut result = value.to_owned();
    for (start, end, replacement) in replacements.into_iter().rev() {
        result.replace_range(start..end, &replacement);
    }
    Some(result)
}
