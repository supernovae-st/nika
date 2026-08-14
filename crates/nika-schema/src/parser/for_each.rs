// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `for_each:` block parser — the fan-out and the two knobs that have
//! no meaning without it (spec `03-dag.md` §`for_each`). Split out of
//! `tasks.rs` under the ADR-023 1,500-LOC ceiling, the same wall
//! `lift.rs` cleared.

use marked_yaml::types::MarkedMappingNode;

use crate::error::SchemaError;
use crate::raw::ForEachValue;
use crate::source::Spanned;

use super::Cx;
use super::tasks::parse_bool_field;
use super::value::json_value;

/// The parsed `for_each:` block — the collection plus the two knobs that
/// have no meaning without it.
type ForEach = (
    Option<Spanned<ForEachValue>>,
    Option<Spanned<u32>>,
    Option<Spanned<bool>>,
);

/// `for_each:` — ONE block, so the concurrency is visible where the
/// fan-out is declared (spec 03 §`for_each`). `max_parallel` and
/// `fail_fast` live INSIDE it because they mean nothing without it; as
/// task-level siblings they read as general knobs and were silently
/// inert on a task that never fans out.
///
/// The IR keeps the three flat on [`crate::raw::RawTask`]: the GRAMMAR
/// nests them,
/// the internal shape does not need to, and flattening leaves the resume
/// wire keys (`max_parallel` · `fail_fast`) exactly where they are —
/// a wire rename needs its own version bump, not a grammar change.
pub(super) fn parse_for_each(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
) -> Result<ForEach, SchemaError> {
    const KEYS: &[&str] = &["items", "max_parallel", "fail_fast"];
    let Some(node) = mapping.get_node("for_each") else {
        return Ok((None, None, None));
    };
    let Some(block) = node.as_mapping() else {
        return Err(SchemaError::Validation {
            message: "`for_each` must be a block with `items:` (plus optional \
                      `max_parallel:` / `fail_fast:`)"
                .to_owned(),
            span: cx.span(node.span()),
        });
    };
    cx.check_unknown_keys(block, KEYS, "the `for_each:` block")?;
    let Some(items) = block.get_node("items") else {
        return Err(SchemaError::Validation {
            message: "`for_each` must name `items:` — the collection to fan out over".to_owned(),
            span: cx.span(node.span()),
        });
    };
    let span = cx.span_or_zero(items.span());
    let collection = if let Some(scalar) = items.as_scalar() {
        Spanned::new(ForEachValue::Expression(scalar.as_str().to_owned()), span)
    } else if items.as_sequence().is_some() {
        Spanned::new(ForEachValue::List(json_value(cx, items)?), span)
    } else {
        return Err(SchemaError::Validation {
            message: "`for_each.items` must be a `${{ … }}` expression or a literal list"
                .to_owned(),
            span: cx.span(items.span()),
        });
    };
    let max_parallel = parse_max_parallel(cx, block)?;
    let fail_fast = parse_bool_field(cx, block, "fail_fast")?;
    Ok((Some(collection), max_parallel, fail_fast))
}

/// `max_parallel:` — positive integer ≥ 1 (spec 03 §`max_parallel` ·
/// « **Positive integer** · `1` to `n`. `1` = sequential »).
fn parse_max_parallel(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
) -> Result<Option<Spanned<u32>>, SchemaError> {
    let Some(node) = mapping.get_node("max_parallel") else {
        return Ok(None);
    };
    let value = node
        .as_scalar()
        .and_then(marked_yaml::types::MarkedScalarNode::as_u32)
        .ok_or_else(|| SchemaError::Validation {
            message: "`max_parallel` must be a positive integer".to_owned(),
            span: cx.span(node.span()),
        })?;
    if value == 0 {
        return Err(SchemaError::Validation {
            message: "`max_parallel` must be ≥ 1".to_owned(),
            span: cx.span(node.span()),
        });
    }
    Ok(Some(Spanned::new(value, cx.span_or_zero(node.span()))))
}
