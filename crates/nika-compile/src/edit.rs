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

pub(super) use nika_compile_fidelity::literal::{
    fill_slot, has_expression, inexact_integer, literal_at,
};

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
#[must_use]
pub fn literal_projection(source: &str) -> Option<Value> {
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
