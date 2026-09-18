// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Closed edit frontends lower to one constant operation and one guarded assembler.

use serde_json::Value;

use super::types::EditChange;

/// Both frontends carry the same target and literal into the edit path.
pub(super) struct ConstantEdit<'a> {
    pub name: &'a str,
    pub literal_json: Option<&'a str>,
}

pub(super) fn operation(change: &EditChange) -> Option<ConstantEdit<'_>> {
    let (name, literal_json) = match change {
        EditChange::Text(text) => constant_change(text)?,
        EditChange::Constant { name, literal_json } => (name.as_str(), Some(literal_json.trim())),
    };
    if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') {
        return None;
    }
    Some(ConstantEdit { name, literal_json })
}

/// Return only requests whose complete text fits the supported grammar.
fn constant_change(change: &str) -> Option<(&str, Option<&str>)> {
    let change = change.trim();
    let (verb, rest) = change.split_once(' ')?;
    if !verb.eq_ignore_ascii_case("set") {
        return None;
    }
    let rest = rest.trim_start().strip_prefix("const.")?;
    let (name, literal) = match rest.split_once(' ') {
        Some((name, rest)) => (name, Some(rest.trim_start().strip_prefix("to ")?.trim())),
        None => (rest, None),
    };
    Some((name, literal))
}

/// Literal answers cannot insert expression islands or author implicit references.
pub(super) fn has_expression(value: &Value) -> bool {
    match value {
        Value::String(s) => s.contains("${{"),
        Value::Array(values) => values.iter().any(has_expression),
        Value::Object(values) => values
            .iter()
            .any(|(k, v)| k.contains("${{") || has_expression(v)),
        _ => false,
    }
}

/// Locate the existing node only. Typed constants retain their declaration.
pub(super) fn literal_at<'a>(doc: &'a mut Value, path: &str) -> Option<&'a mut Value> {
    let mut node = doc;
    for key in path.split('.') {
        node = node.get_mut(key)?;
    }
    if node.get("type").is_some()
        && (path.starts_with("inputs.")
            || (path.starts_with("const.") && node.get("value").is_some()))
    {
        let key = if path.starts_with("const.") {
            "value"
        } else {
            "default"
        };
        return node.get_mut(key);
    }
    Some(node)
}

/// Fill a marker line without discarding the surrounding prompt or system text.
pub(super) fn fill_slot(node: &mut Value, answer: Value) -> bool {
    let Some(text) = node.as_str() else {
        return false;
    };
    let holes = text
        .lines()
        .filter(|line| {
            let line = line.trim();
            line.starts_with("<SLOT:") && line.ends_with('>')
        })
        .count();
    if holes != 1 {
        // One answer cannot stand in for several independent authoring choices.
        return false;
    }
    if text.trim().starts_with("<SLOT:") && text.trim().ends_with('>') && text.lines().count() == 1
    {
        *node = answer;
        return true;
    }
    let Some(replacement) = answer.as_str() else {
        return false;
    };
    let filled: Vec<_> = text
        .lines()
        .map(|line| {
            let line_trimmed = line.trim();
            if line_trimmed.starts_with("<SLOT:") && line_trimmed.ends_with('>') {
                replacement
            } else {
                line
            }
        })
        .collect();
    let suffix = if text.ends_with('\n') { "\n" } else { "" };
    *node = Value::String(format!("{}{suffix}", filled.join("\n")));
    true
}

/// Prove that the representation decoder and emitter preserve literal semantics.
/// Unsupported YAML presentation (for example a document directive inside the
/// literal projection) fails closed; this is not a second YAML scalar decoder.
pub(super) fn emit_preserving(
    source: &str,
    before: &Value,
    after: &Value,
) -> Result<Option<String>, serde_yaml_bw::Error> {
    if literal_projection(source).as_ref() != Some(before) {
        return Ok(None);
    }
    let emitted = serde_yaml_bw::to_string(after)?;
    Ok((literal_projection(&emitted).as_ref() == Some(after)).then_some(emitted))
}

/// Reuse THE parser's free-form literal decoding on the whole document. The
/// workflow envelope cannot contain `type`/`value`, so this outer constant is
/// necessarily untyped. No Check or execution is performed on this projection.
fn literal_projection(source: &str) -> Option<Value> {
    let indented = source
        .lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let projected = format!("nika: compile-literal-view\nconst:\n  document:\n{indented}\n");
    let wf = super::parse(&projected).ok()?;
    let (_, nika_schema::VarDecl::Untyped(value)) = wf.consts.into_iter().next()? else {
        return None;
    };
    Some(value)
}
