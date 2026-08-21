// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The three value authorities — `inputs:` · `const:` · `secrets:`
//! (spec `01-envelope.md` · `config:` died with the nine-key envelope).
//!
//! Split out of `parser/envelope.rs` at the C2 wall (the 1500-LOC file
//! ratchet — the value authorities are ONE coherent unit: the typed
//! declaration machinery + the governed store references). `envelope.rs`
//! re-exports every `parse_*` door, so the parser's call paths are
//! unchanged.

use marked_yaml::Node;
use marked_yaml::types::MarkedMappingNode;

use crate::error::SchemaError;
use crate::source::Spanned;
use crate::types::secret;
use crate::types::{EgressRule, SecretRef, SecretSource, VarDecl};

use super::envelope::{CONST_TYPED_KEYS, EGRESS_KEYS, INPUT_KEYS, SECRET_KEYS, require_mapping};
use super::{Cx, value::json_value};

/// Parse `inputs:` — every entry a TYPED declaration (`type:` required ·
/// spec 01 §inputs). An untyped entry is refused (inputs are the typed
/// half of the callable contract — a fixed value is a `const:` entry).
pub(super) fn parse_inputs(
    cx: &Cx<'_>,
    workflow: &MarkedMappingNode,
) -> Result<Vec<(Spanned<String>, VarDecl)>, SchemaError> {
    parse_typed_block(cx, workflow, "inputs", INPUT_KEYS)
}

/// The shared typed-declaration block reader (`inputs:`) —
/// same shape as the pre-C2 typed `vars:` form.
fn parse_typed_block(
    cx: &Cx<'_>,
    workflow: &MarkedMappingNode,
    key: &str,
    keys: &[&str],
) -> Result<Vec<(Spanned<String>, VarDecl)>, SchemaError> {
    let Some(node) = workflow.get_node(key) else {
        return Ok(Vec::new());
    };
    let mapping = require_mapping(cx, node, key)?;
    let mut out = Vec::with_capacity(mapping.len());
    for (k, value) in mapping.iter() {
        let name = Spanned::new(k.as_str().to_owned(), cx.span_or_zero(k.span()));
        let Some(typed) = value.as_mapping().filter(|m| m.get_node("type").is_some()) else {
            return Err(SchemaError::Validation {
                message: format!(
                    "`{key}` entry `{}` is not a typed declaration — `type:` required \
                     (a fixed value is a `const:` entry)",
                    name.value
                ),
                span: cx.span(value.span()),
            });
        };
        out.push((name.clone(), parse_typed_var(cx, &name.value, typed, keys)?));
    }
    Ok(out)
}

/// Parse `const:` — bare literals OR typed constants `{type, value}`
/// (spec 01 §const · the discriminator: an object carrying BOTH `type`
/// and `value` keys is the typed constant; anything else — scalar ·
/// sequence · any other mapping — is a bare literal).
pub(super) fn parse_const(
    cx: &Cx<'_>,
    workflow: &MarkedMappingNode,
) -> Result<Vec<(Spanned<String>, VarDecl)>, SchemaError> {
    let Some(node) = workflow.get_node("const") else {
        return Ok(Vec::new());
    };
    let mapping = require_mapping(cx, node, "const")?;
    let mut out = Vec::with_capacity(mapping.len());
    for (k, value) in mapping.iter() {
        let name = Spanned::new(k.as_str().to_owned(), cx.span_or_zero(k.span()));
        // The discriminator (normative): an object carrying BOTH `type`
        // and `value` keys is the typed constant; anything else is a bare
        // literal object constant.
        let typed = value
            .as_mapping()
            .filter(|m| m.get_node("type").is_some() && m.get_node("value").is_some());
        let decl = match typed.and_then(|t| t.get_node("value").map(|vn| (t, vn))) {
            Some((t, value_node)) => {
                cx.check_unknown_keys(
                    t,
                    CONST_TYPED_KEYS,
                    &format!("typed const `{}`", name.value),
                )?;
                VarDecl::Typed {
                    r#type: parse_type_expr(cx, &name.value, t)?,
                    required: false,
                    default: Some(json_value(cx, value_node)?),
                    description: None,
                }
            }
            None => VarDecl::Untyped(json_value(cx, value)?),
        };
        out.push((name, decl));
    }
    Ok(out)
}

/// The `type:` of a typed declaration, read SHAPE-ONLY into the raw
/// `TypeExpr` (spec 09 §grammar · R3b — the field speaks the full
/// `TypeExpr`: primitives · named references · constructors). The grammar
/// judgment (`NIKA-TYPE-001/006`) and the default-conformance judgment
/// (`NIKA-DEFAULT-001`) are the analyzer's via the one type core — the
/// `types:` block precedent: one truth, never re-implemented here.
fn parse_type_expr(
    cx: &Cx<'_>,
    name: &str,
    mapping: &MarkedMappingNode,
) -> Result<Spanned<serde_json::Value>, SchemaError> {
    let node = mapping
        .get_node("type")
        .ok_or_else(|| SchemaError::Validation {
            message: format!("typed declaration `{name}` — `type:` required"),
            span: cx.span(mapping.span()),
        })?;
    Ok(Spanned::new(
        json_value(cx, node)?,
        cx.span_or_zero(node.span()),
    ))
}

/// Parse the typed-declaration form (`{ type, required?, default?,
/// description? }` — the pre-C2 typed `vars:` form, now `inputs:`).
fn parse_typed_var(
    cx: &Cx<'_>,
    name: &str,
    mapping: &MarkedMappingNode,
    keys: &[&str],
) -> Result<VarDecl, SchemaError> {
    cx.check_unknown_keys(mapping, keys, &format!("typed declaration `{name}`"))?;

    let r#type = parse_type_expr(cx, name, mapping)?;

    let required = match mapping.get_node("required") {
        None => false,
        Some(node) => node
            .as_scalar()
            .and_then(marked_yaml::types::MarkedScalarNode::as_bool)
            .ok_or_else(|| SchemaError::Validation {
                message: format!("typed declaration `{name}` — `required` must be a boolean"),
                span: cx.span(node.span()),
            })?,
    };

    let default = mapping
        .get_node("default")
        .map(|n| json_value(cx, n))
        .transpose()?;

    let description = cx
        .opt_scalar(mapping, "description")
        .map_err(|_| SchemaError::Validation {
            message: format!("typed declaration `{name}` — `description` must be a scalar string"),
            span: cx.span(mapping.span()),
        })?
        .map(|s| s.value);

    Ok(VarDecl::Typed {
        r#type,
        required,
        default,
        description,
    })
}

/// Parse `secrets:` — each entry MUST be a reference to a store,
/// **discriminated by `source`** · `vault`/`env` require `key:` ·
/// `file` requires `path:` (spec 01 §secrets).
///
/// « A secret is always a **reference to a store** — never an inline
/// literal. » A scalar value is therefore a parse error, and so is the
/// wrong field for the source (`file` + `key:` · `vault` + `path:`).
pub(super) fn parse_secrets(
    cx: &Cx<'_>,
    workflow: &MarkedMappingNode,
) -> Result<super::SpannedEntries<SecretRef>, SchemaError> {
    let Some(node) = workflow.get_node("secrets") else {
        return Ok(Vec::new());
    };
    let mapping = require_mapping(cx, node, "secrets")?;
    let mut out = Vec::with_capacity(mapping.len());
    for (key, value) in mapping.iter() {
        let name = Spanned::new(key.as_str().to_owned(), cx.span_or_zero(key.span()));
        let Some(entry) = value.as_mapping() else {
            return Err(SchemaError::BadSecretRef {
                reason: secret::inline_literal_teaching(&name.value),
                span: cx.span(value.span()),
            });
        };
        cx.check_unknown_keys(entry, SECRET_KEYS, &format!("secret `{}`", name.value))?;

        // `source` is REQUIRED (R8 · the flipped dialect — provenance is
        // explicit, never defaulted · the conformance guard fixture).
        let source = match entry.get_node("source") {
            None => {
                return Err(SchemaError::BadSecretRef {
                    reason: secret::missing_source_teaching(&name.value),
                    span: cx.span(value.span()),
                });
            }
            Some(source_node) => {
                let scalar = source_node
                    .as_scalar()
                    .ok_or_else(|| SchemaError::BadSecretRef {
                        reason: format!("secret `{}` `source` must be a scalar", name.value),
                        span: cx.span(source_node.span()),
                    })?;
                SecretSource::from_str_opt(scalar.as_str()).ok_or_else(|| {
                    SchemaError::BadSecretRef {
                        reason: secret::unknown_source_teaching(&name.value, scalar.as_str()),
                        span: cx.span(scalar.span()),
                    }
                })?
            }
        };

        // The reference field is discriminated by `source` (spec 01) ·
        // vault/env read `key:` · file reads `path:` · the OTHER field
        // present is a shape error (never silently accepted).
        let (want, reject) = match source {
            SecretSource::File => ("path", "key"),
            SecretSource::Vault | SecretSource::Env => ("key", "path"),
        };
        if entry.get_node(reject).is_some() {
            return Err(SchemaError::BadSecretRef {
                reason: secret::wrong_field_teaching(&name.value, source, want, reject),
                span: cx.span(entry.span()),
            });
        }
        let reference = entry
            .get_scalar(want)
            .ok_or_else(|| SchemaError::BadSecretRef {
                reason: secret::missing_reference_teaching(&name.value, source, want),
                span: cx.span(entry.span()),
            })?;

        // Optional `egress:` declassification list (default-deny · absent
        // = no sanctioned egress · the current blocking-leak behavior).
        let egress = parse_egress(cx, entry, &name.value)?;

        let span = cx.span_or_zero(entry.span());
        out.push((
            name,
            Spanned::new(
                SecretRef::new(source, reference.as_str()).with_egress(egress),
                span,
            ),
        ));
    }
    Ok(out)
}

/// Parse one secret's optional `egress:` list (spec 01 §secrets ·
/// declassification). Each entry sanctions ONE sink ·
///
/// ```yaml
/// egress:
///   - to: "nika:fetch"        # the SPECIFIC sink (tool id or "exec")
///     host: "api.stripe.com"  # a static-literal destination host
///   - to: "nika:notify"
///     host_from_self: true    # the secret value IS the URL
/// ```
///
/// `to:` is required. `host:` and `host_from_self:` are mutually exclusive
/// (a host is either a literal OR the secret itself, never both). Both are
/// optional — a sink with no addressable host (`exec`) carries neither.
fn parse_egress(
    cx: &Cx<'_>,
    entry: &MarkedMappingNode,
    secret_name: &str,
) -> Result<Vec<EgressRule>, SchemaError> {
    let Some(node) = entry.get_node("egress") else {
        return Ok(Vec::new());
    };
    let Some(seq) = node.as_sequence() else {
        return Err(SchemaError::BadSecretRef {
            reason: secret::egress_not_a_list_teaching(secret_name),
            span: cx.span(node.span()),
        });
    };
    let mut rules = Vec::with_capacity(seq.len());
    for item in seq.iter() {
        rules.push(parse_egress_rule(cx, item, secret_name)?);
    }
    Ok(rules)
}

/// Parse one `egress[]` entry into an [`EgressRule`].
fn parse_egress_rule(
    cx: &Cx<'_>,
    item: &Node,
    secret_name: &str,
) -> Result<EgressRule, SchemaError> {
    let bad = |reason: String, span: &marked_yaml::Span| SchemaError::BadSecretRef {
        reason,
        span: cx.span(span),
    };
    let Some(mapping) = item.as_mapping() else {
        return Err(bad(
            secret::egress_entry_shape_teaching(secret_name),
            item.span(),
        ));
    };
    cx.check_unknown_keys_always(
        mapping,
        EGRESS_KEYS,
        &format!("secret `{secret_name}` egress entry"),
    )?;

    // `to:` — REQUIRED · the SPECIFIC sink (tool id or "exec").
    let to = mapping
        .get_scalar("to")
        .ok_or_else(|| {
            bad(
                secret::egress_missing_to_teaching(secret_name),
                mapping.span(),
            )
        })?
        .as_str()
        .to_owned();

    // `to:` names a SINK — the vocabulary is closed (spec 01 §egress ①):
    // a tool id (`nika:<tool>` · `mcp:<server>/<tool>`), `exec`, the
    // provider sinks `infer` / `agent`, or the workflow boundary
    // `outputs`. Anything else can never match, so the sanction would be
    // silently DEAD — reading as declassified while nothing is. The classic
    // slip is a destination HOST in `to:` (the use-case battery's own
    // authoring error, 2026-07-11): `host:` is its own field.
    let to_is_sink = matches!(to.as_str(), "exec" | "infer" | "agent" | "outputs")
        || to.starts_with("nika:")
        || to.starts_with("mcp:");
    if !to_is_sink {
        return Err(bad(
            secret::egress_not_a_sink_teaching(secret_name, &to),
            mapping.span(),
        ));
    }

    // `host_from_self:` — the secret value IS the URL.
    let host_from_self = match mapping.get_node("host_from_self") {
        None => false,
        Some(n) => n
            .as_scalar()
            .and_then(marked_yaml::types::MarkedScalarNode::as_bool)
            .ok_or_else(|| {
                bad(
                    format!("secret `{secret_name}` egress `host_from_self:` must be a boolean"),
                    n.span(),
                )
            })?,
    };

    // `host:` — a static-literal destination host.
    let host = mapping.get_scalar("host").map(|s| s.as_str().to_owned());

    // `host:` and `host_from_self:` are mutually exclusive — a host is
    // either a literal we can check statically OR the secret itself, never
    // both (the two clauses sanction by different rules · §L2).
    if host.is_some() && host_from_self {
        return Err(bad(
            secret::egress_host_and_self_teaching(secret_name),
            mapping.span(),
        ));
    }

    Ok(EgressRule {
        to,
        host,
        host_from_self,
    })
}
