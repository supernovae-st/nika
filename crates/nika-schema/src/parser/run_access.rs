// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `run.access:` and `run.reasoning:` — the authored access selection
//! (route · protocol · fallback · native effort), SHAPE only.
//!
//! Every key set is closed in both parse modes (a typo'd `protocl:` would
//! let the run reach its model another way), every value is a literal
//! string resolved before task 1 (no `${{ }}`), and an empty block selects
//! nothing, so it is refused rather than read as a declaration. Whether a
//! route, model or effort is AVAILABLE is never judged here: the access
//! resolver refuses before task 1, and a live session refuses an unoffered
//! model or effort before its first prompt.

use marked_yaml::Node;
use marked_yaml::types::MarkedMappingNode;
use nika_types::access::{AccessFallback, AccessProtocol};
use nika_vocab::keys::{RUN_ACCESS_KEYS, RUN_REASONING_KEYS};

use crate::error::SchemaError;
use crate::types::{RunAccess, RunReasoning};

use super::Cx;
use super::envelope::require_mapping;

/// `run.access:` — `None` when the block is absent.
pub(super) fn parse_access(
    cx: &Cx<'_>,
    run: &MarkedMappingNode,
) -> Result<Option<RunAccess>, SchemaError> {
    let Some(node) = run.get_node("access") else {
        return Ok(None);
    };
    let mapping = require_mapping(cx, node, "run.access")?;
    cx.check_unknown_keys_always(mapping, RUN_ACCESS_KEYS, "`run.access`")?;
    if mapping.is_empty() {
        return Err(invalid(
            cx,
            node,
            "`run.access` selects nothing — declare `via:`, `protocol:` or `fallback:`, \
             or remove the block (absent keeps today's access resolution)"
                .to_owned(),
        ));
    }
    let via = match literal(cx, mapping, "via", "run.access.via")? {
        Some((via, node)) if !super::is_kebab_id(&via) => {
            return Err(invalid(
                cx,
                node,
                format!(
                    "`run.access.via` names one route id in lowercase kebab-case (an agent \
                     application such as `codex` or `claude-code`, or a provider such as \
                     `openai`) — got `{via}`"
                ),
            ));
        }
        Some((via, _)) => Some(via),
        None => None,
    };
    let protocol = match literal(cx, mapping, "protocol", "run.access.protocol")? {
        Some((word, node)) => Some(AccessProtocol::parse(&word).ok_or_else(|| {
            invalid(
                cx,
                node,
                format!(
                    "`run.access.protocol` must be `api` or `acp` — got `{word}` (a direct CLI \
                     is an adapter detail, never a protocol)"
                ),
            )
        })?),
        None => None,
    };
    let fallback = match literal(cx, mapping, "fallback", "run.access.fallback")? {
        Some((word, node)) => Some(AccessFallback::parse(&word).ok_or_else(|| {
            invalid(
                cx,
                node,
                format!(
                    "`run.access.fallback` admits only `none` — got `{word}` (declared \
                     alternatives are not part of the language; an explicit selection is exact)"
                ),
            )
        })?),
        None => None,
    };
    Ok(Some(RunAccess::new(via, protocol, fallback)))
}

/// `run.reasoning:` — `None` when absent; `effort:` is required inside.
pub(super) fn parse_reasoning(
    cx: &Cx<'_>,
    run: &MarkedMappingNode,
) -> Result<Option<RunReasoning>, SchemaError> {
    let Some(node) = run.get_node("reasoning") else {
        return Ok(None);
    };
    let mapping = require_mapping(cx, node, "run.reasoning")?;
    cx.check_unknown_keys_always(mapping, RUN_REASONING_KEYS, "`run.reasoning`")?;
    let Some((effort, _)) = literal(cx, mapping, "effort", "run.reasoning.effort")? else {
        return Err(invalid(
            cx,
            node,
            "`run.reasoning` needs `effort:` — the route's own native value, verbatim, or \
             remove the block (absent keeps each route's default)"
                .to_owned(),
        ));
    };
    Ok(Some(RunReasoning::new(Some(effort))))
}

/// One literal string value with its node: a non-null, non-empty string
/// scalar used verbatim (no surrounding whitespace), never a template and
/// never a plain number or boolean.
fn literal<'n>(
    cx: &Cx<'_>,
    mapping: &'n MarkedMappingNode,
    key: &str,
    field: &str,
) -> Result<Option<(String, &'n Node)>, SchemaError> {
    let Some(node) = mapping.get_node(key) else {
        return Ok(None);
    };
    let Some(scalar) = node.as_scalar() else {
        return Err(invalid(cx, node, format!("`{field}` must be a string")));
    };
    let text = scalar.as_str();
    if scalar.may_coerce() && matches!(text, "" | "~" | "null" | "Null" | "NULL") {
        return Err(invalid(
            cx,
            node,
            format!("`{field}` must be a non-empty string — got null"),
        ));
    }
    super::refuse_ambiguous_plain_scalar(scalar, field, cx.span_or_zero(node.span()))?;
    if text.trim().is_empty() {
        return Err(invalid(
            cx,
            node,
            format!("`{field}` must be a non-empty string"),
        ));
    }
    if text.trim() != text {
        return Err(invalid(
            cx,
            node,
            format!("`{field}` is used verbatim — remove the surrounding whitespace from `{text}`"),
        ));
    }
    if text.contains("${{") {
        return Err(invalid(
            cx,
            node,
            format!(
                "`{field}` is resolved before task 1 and takes a literal value, never a \
                 template — got `{text}`"
            ),
        ));
    }
    Ok(Some((text.to_owned(), node)))
}

fn invalid(cx: &Cx<'_>, node: &Node, message: String) -> SchemaError {
    SchemaError::Validation {
        message,
        span: cx.span(node.span()),
    }
}

#[cfg(test)]
mod tests;
