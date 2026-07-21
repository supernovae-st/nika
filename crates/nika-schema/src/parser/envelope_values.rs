// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The four value authorities — `inputs:` · `config:` · `const:` ·
//! `secrets:` (spec `01-envelope.md` · post-C2 the E-split family).
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
use crate::types::{EgressRule, SecretRef, SecretSource, VarDecl, VarType};

use super::envelope::{
    CONFIG_KEYS, CONST_TYPED_KEYS, EGRESS_KEYS, INPUT_KEYS, SECRET_KEYS, require_mapping,
};
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

/// Parse `config:` — typed declarations (`type:` required · `default:` the
/// declared fallback the deployment may override · spec 01 §config).
pub(super) fn parse_config(
    cx: &Cx<'_>,
    workflow: &MarkedMappingNode,
) -> Result<Vec<(Spanned<String>, VarDecl)>, SchemaError> {
    parse_typed_block(cx, workflow, "config", CONFIG_KEYS)
}

/// The shared typed-declaration block reader (`inputs:` · `config:`) —
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
            return Err(SchemaError::BadTypedVar {
                name: name.value.clone(),
                reason: format!(
                    "`{key}` entries are typed declarations — `type:` required (a fixed value is a `const:` entry)"
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
                    r#type: parse_var_type(cx, &name.value, t)?,
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

/// The `type:` scalar of a typed declaration, parsed into the closed
/// vocabulary (spec 01 · string·number·integer·boolean·array·object).
fn parse_var_type(
    cx: &Cx<'_>,
    name: &str,
    mapping: &MarkedMappingNode,
) -> Result<VarType, SchemaError> {
    let type_scalar = mapping
        .get_scalar("type")
        .ok_or_else(|| SchemaError::BadTypedVar {
            name: name.to_owned(),
            reason: "`type` must be a scalar".to_owned(),
            span: cx.span(mapping.span()),
        })?;
    VarType::from_str_opt(type_scalar.as_str()).ok_or_else(|| SchemaError::BadTypedVar {
        name: name.to_owned(),
        reason: format!(
            "unknown type `{}` (string·number·integer·boolean·array·object)",
            type_scalar.as_str()
        ),
        span: cx.span(type_scalar.span()),
    })
}

/// Parse the typed-declaration form (`{ type, required?, default?,
/// description? }` — the pre-C2 typed `vars:` form, now shared by
/// `inputs:` and `config:` with per-authority closed key sets).
fn parse_typed_var(
    cx: &Cx<'_>,
    name: &str,
    mapping: &MarkedMappingNode,
    keys: &[&str],
) -> Result<VarDecl, SchemaError> {
    cx.check_unknown_keys(mapping, keys, &format!("typed declaration `{name}`"))?;

    let r#type = parse_var_type(cx, name, mapping)?;

    let required = match mapping.get_node("required") {
        None => false,
        Some(node) => node
            .as_scalar()
            .and_then(marked_yaml::types::MarkedScalarNode::as_bool)
            .ok_or_else(|| SchemaError::BadTypedVar {
                name: name.to_owned(),
                reason: "`required` must be a boolean".to_owned(),
                span: cx.span(node.span()),
            })?,
    };

    let default = mapping
        .get_node("default")
        .map(|n| json_value(cx, n))
        .transpose()?;

    let description = cx
        .opt_scalar(mapping, "description")
        .map_err(|_| SchemaError::BadTypedVar {
            name: name.to_owned(),
            reason: "`description` must be a scalar string".to_owned(),
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
                reason: format!(
                    "secret `{}` is an inline literal — a secret is a reference to a store \
                     (`{{ source, key }}`), never a value",
                    name.value
                ),
                span: cx.span(value.span()),
            });
        };
        cx.check_unknown_keys(entry, SECRET_KEYS, &format!("secret `{}`", name.value))?;

        // `source` is REQUIRED (R8 · the flipped dialect — provenance is
        // explicit, never defaulted · the conformance guard fixture).
        let source = match entry.get_node("source") {
            None => {
                return Err(SchemaError::BadSecretRef {
                    reason: format!(
                        "secret `{}` has no `source:` — the provenance is required explicitly \
                         (vault · env · file · R8)",
                        name.value
                    ),
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
                        reason: format!(
                            "secret `{}` has unknown source `{}` (vault·env·file)",
                            name.value,
                            scalar.as_str()
                        ),
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
                reason: format!(
                    "secret `{}` with `source: {source}` takes `{want}:`, not `{reject}:` \
                     (spec 01 §secrets · the shape is discriminated by source)",
                    name.value
                ),
                span: cx.span(entry.span()),
            });
        }
        let reference = entry
            .get_scalar(want)
            .ok_or_else(|| SchemaError::BadSecretRef {
                reason: format!(
                    "secret `{}` with `source: {source}` is missing its `{want}:`",
                    name.value
                ),
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
            reason: format!(
                "secret `{secret_name}` `egress:` must be a list of sanctioned destinations"
            ),
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
            format!(
                "secret `{secret_name}` `egress:` entry must be a mapping \
                 `{{ to, host, host_from_self }}`"
            ),
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
                format!(
                    "secret `{secret_name}` egress entry is missing `to:` \
                     (the sanctioned sink · a tool id like `nika:fetch` or `exec`)"
                ),
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
            format!(
                "secret `{secret_name}` egress `to: \"{to}\"` names no sink — the set: \
                 a tool id (`nika:<tool>` · `mcp:<server>/<tool>`) · `exec` · `infer` · \
                 `agent` · `outputs` (a destination host goes in `host:`, not `to:`)"
            ),
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
            format!(
                "secret `{secret_name}` egress entry sets BOTH `host:` and `host_from_self:` \
                 — a host is a literal OR the secret itself, not both"
            ),
            mapping.span(),
        ));
    }

    Ok(EgressRule {
        to,
        host,
        host_from_self,
    })
}
