// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Envelope block parsing — `vars:` · `env:` · `secrets:` · `permits:` ·
//! `outputs:` (spec `01-envelope.md`).

use marked_yaml::Node;
use marked_yaml::types::MarkedMappingNode;

use crate::error::SchemaError;
use crate::source::Spanned;
use crate::types::{
    EgressRule, ExecPermit, FsPermits, NetPermits, OutputDecl, Permits, Policy, SecretRef,
    SecretSource, VarDecl, VarType,
};

use super::{Cx, value::json_value};

/// Keys of the typed `vars:` form (spec 01 §vars).
pub(crate) const TYPED_VAR_KEYS: &[&str] = &["type", "required", "default", "description"];

/// Keys of a `secrets:` entry (spec 01 §secrets · `key` XOR `path` ·
/// discriminated by `source` · plus the optional `egress:` declass list).
pub(crate) const SECRET_KEYS: &[&str] = &["source", "key", "path", "egress"];

/// Keys of one `secrets.<name>.egress[]` entry (spec 01 §secrets · the
/// sanctioned-egress declaration · closed).
const EGRESS_KEYS: &[&str] = &["to", "host", "host_from_self"];

/// Keys of the typed `outputs:` form (spec 01 §outputs).
const TYPED_OUTPUT_KEYS: &[&str] = &["value", "type", "description"];

/// Keys of the `permits:` block (spec 01 §permits · closed).
pub(crate) const PERMITS_KEYS: &[&str] = &["fs", "net", "exec", "tools"];

/// Keys of `permits.fs` (closed).
pub(crate) const PERMITS_FS_KEYS: &[&str] = &["read", "write"];

/// Keys of `permits.net` (closed).
pub(crate) const PERMITS_NET_KEYS: &[&str] = &["http"];

/// Parse `vars:` — untyped (`name: value`) OR typed
/// (`name: { type, required, default, description }`).
///
/// Discriminator · a mapping value carrying a `type` key is the typed
/// form; everything else (scalar · sequence · type-less mapping) is an
/// untyped default value.
pub(super) fn parse_vars(
    cx: &Cx<'_>,
    workflow: &MarkedMappingNode,
) -> Result<Vec<(Spanned<String>, VarDecl)>, SchemaError> {
    let Some(node) = workflow.get_node("vars") else {
        return Ok(Vec::new());
    };
    let mapping = require_mapping(cx, node, "vars")?;
    let mut out = Vec::with_capacity(mapping.len());
    for (key, value) in mapping.iter() {
        let name = Spanned::new(key.as_str().to_owned(), cx.span_or_zero(key.span()));
        let decl = if let Some(typed) = value.as_mapping().filter(|m| m.get_node("type").is_some())
        {
            parse_typed_var(cx, &name.value, typed)?
        } else {
            VarDecl::Untyped(json_value(cx, value)?)
        };
        out.push((name, decl));
    }
    Ok(out)
}

/// Parse the typed `vars:` form (spec 01 §vars · « type: string · number
/// · integer · boolean · array · object » · `required` default false).
fn parse_typed_var(
    cx: &Cx<'_>,
    name: &str,
    mapping: &MarkedMappingNode,
) -> Result<VarDecl, SchemaError> {
    cx.check_unknown_keys(mapping, TYPED_VAR_KEYS, &format!("typed var `{name}`"))?;

    let type_scalar = mapping
        .get_scalar("type")
        .ok_or_else(|| SchemaError::BadTypedVar {
            name: name.to_owned(),
            reason: "`type` must be a scalar".to_owned(),
            span: cx.span(mapping.span()),
        })?;
    let r#type =
        VarType::from_str_opt(type_scalar.as_str()).ok_or_else(|| SchemaError::BadTypedVar {
            name: name.to_owned(),
            reason: format!(
                "unknown type `{}` (string·number·integer·boolean·array·object)",
                type_scalar.as_str()
            ),
            span: cx.span(type_scalar.span()),
        })?;

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

/// Parse `env:` — a flat mapping of scalar → scalar (spec 01 §env ·
/// non-sensitive runtime config).
pub(super) fn parse_env(
    cx: &Cx<'_>,
    workflow: &MarkedMappingNode,
) -> Result<super::SpannedEntries<String>, SchemaError> {
    let Some(node) = workflow.get_node("env") else {
        return Ok(Vec::new());
    };
    parse_string_map(cx, node, "env")
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

        // `source` defaults to vault (the sovereign default).
        let source = match entry.get_node("source") {
            None => SecretSource::Vault,
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

/// A parsed `types:` block — declaration name → raw expression, spans kept.
pub(super) type TypeDecls = Vec<(Spanned<String>, Spanned<serde_json::Value>)>;

/// Parse `types:` — named type declarations (spec `09-types.md`) ·
/// `PascalCase` name → RAW type expression.
///
/// Shape-only (the parser's contract): the block is a mapping, each
/// declaration name matches `^[A-Z][A-Za-z0-9]*$` (the published
/// `workflow.schema.json` `propertyNames` rule — engine ≡ schema), and
/// each value converts to a neutral JSON value. The GRAMMAR of the
/// expression (`NIKA-TYPE-001/002/006`) is the analyzer's job via the
/// type core — one truth, never re-implemented here.
pub(super) fn parse_types(
    cx: &Cx<'_>,
    workflow: &MarkedMappingNode,
) -> Result<TypeDecls, SchemaError> {
    let Some(node) = workflow.get_node("types") else {
        return Ok(Vec::new());
    };
    let mapping = require_mapping(cx, node, "types")?;
    let mut out = Vec::with_capacity(mapping.len());
    for (key, value) in mapping.iter() {
        let name = Spanned::new(key.as_str().to_owned(), cx.span_or_zero(key.span()));
        if !is_pascal_case(&name.value) {
            return Err(SchemaError::Validation {
                message: format!(
                    "type name `{}` must be PascalCase (^[A-Z][A-Za-z0-9]*$) — \
                     disjoint from task ids and the lowercase primitives by \
                     construction (09-types.md)",
                    name.value
                ),
                span: Some(name.span),
            });
        }
        out.push((
            name,
            Spanned::new(json_value(cx, value)?, cx.span_or_zero(value.span())),
        ));
    }
    Ok(out)
}

/// A legal declared-type name (spec 09 · `^[A-Z][A-Za-z0-9]*$`).
fn is_pascal_case(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_alphanumeric())
}

/// Parse `outputs:` — untyped (`name: ${{ … }}`) OR typed
/// (`name: { value, type, description }`).
pub(super) fn parse_outputs(
    cx: &Cx<'_>,
    workflow: &MarkedMappingNode,
) -> Result<Vec<(Spanned<String>, OutputDecl)>, SchemaError> {
    let Some(node) = workflow.get_node("outputs") else {
        return Ok(Vec::new());
    };
    let mapping = require_mapping(cx, node, "outputs")?;
    let mut out = Vec::with_capacity(mapping.len());
    for (key, value) in mapping.iter() {
        let name = Spanned::new(key.as_str().to_owned(), cx.span_or_zero(key.span()));
        let decl = match value {
            Node::Scalar(scalar) => OutputDecl::Untyped(Spanned::new(
                scalar.as_str().to_owned(),
                cx.span_or_zero(scalar.span()),
            )),
            Node::Mapping(typed) => {
                cx.check_unknown_keys(
                    typed,
                    TYPED_OUTPUT_KEYS,
                    &format!("typed output `{}`", name.value),
                )?;
                let value_scalar = cx.require_scalar(typed, "value", "outputs")?;
                let r#type = match typed.get_scalar("type") {
                    None => None,
                    Some(t) => Some(VarType::from_str_opt(t.as_str()).ok_or_else(|| {
                        SchemaError::Validation {
                            message: format!(
                                "output `{}` has unknown type `{}` \
                                 (string·number·integer·boolean·array·object)",
                                name.value,
                                t.as_str()
                            ),
                            span: cx.span(t.span()),
                        }
                    })?),
                };
                let description = cx.opt_scalar(typed, "description")?.map(|s| s.value);
                OutputDecl::Typed {
                    value: value_scalar,
                    r#type,
                    description,
                }
            }
            Node::Sequence(seq) => {
                return Err(SchemaError::Validation {
                    message: format!(
                        "output `{}` must be a `${{{{ }}}}` reference or `{{ value, type, \
                         description }}`",
                        name.value
                    ),
                    span: cx.span(seq.span()),
                });
            }
        };
        out.push((name, decl));
    }
    Ok(out)
}

// ── Shared shape helpers ────────────────────────────────────────────

/// Require a mapping node (for `vars:` / `secrets:` / `outputs:` blocks).
pub(super) fn require_mapping<'n>(
    cx: &Cx<'_>,
    node: &'n Node,
    key: &str,
) -> Result<&'n MarkedMappingNode, SchemaError> {
    node.as_mapping().ok_or_else(|| SchemaError::Validation {
        message: format!("`{key}` must be a YAML mapping"),
        span: cx.span(node.span()),
    })
}

/// Parse a flat scalar→scalar mapping (envelope `env:` · `exec.env:`).
pub(super) fn parse_string_map(
    cx: &Cx<'_>,
    node: &Node,
    key: &str,
) -> Result<super::SpannedEntries<String>, SchemaError> {
    let mapping = require_mapping(cx, node, key)?;
    let mut out = Vec::with_capacity(mapping.len());
    for (k, v) in mapping.iter() {
        let Some(scalar) = v.as_scalar() else {
            return Err(SchemaError::Validation {
                message: format!("`{key}.{}` must be a scalar string", k.as_str()),
                span: cx.span(v.span()),
            });
        };
        out.push((
            Spanned::new(k.as_str().to_owned(), cx.span_or_zero(k.span())),
            Spanned::new(scalar.as_str().to_owned(), cx.span_or_zero(scalar.span())),
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use crate::error::SchemaError;
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;
    use crate::types::{OutputDecl, SecretSource, VarDecl, VarType};

    fn parse_strict(yaml: &str) -> Result<crate::raw::RawWorkflow, SchemaError> {
        parse(yaml, FileId::new(0), ParseMode::Strict)
    }

    /// T1 (use-case battery 2026-07-11) · an unknown secret field with no
    /// near-miss teaches the WHOLE closed set — `env` is nobody's typo for
    /// `key`, the author needs the vocabulary (the chart-semantics
    /// precedent applied to parse).
    #[test]
    fn unknown_secret_field_teaches_the_accepted_set() {
        let yaml = "\
secrets:
  api_key:
    env: MY_KEY
";
        let err = parse_strict(yaml).expect_err("rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("source") && msg.contains("key") && msg.contains("egress"),
            "the accepted fields ride the refusal: {msg}"
        );
    }

    /// E3 (use-case battery 2026-07-11) · a sanction whose `to:` names no
    /// sink is DEAD — it can never match, and the author believes they
    /// declassified. The classic slip is a HOST in `to:` (host: is its own
    /// field): refuse at parse, list the sink vocabulary.
    #[test]
    fn egress_to_outside_the_sink_vocabulary_is_refused() {
        let yaml = "\
secrets:
  api_key:
    source: env
    key: MY_KEY
    egress:
      - to: \"nika.sh\"
";
        let err = parse_strict(yaml).expect_err("rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("names no sink") && msg.contains("host:"),
            "the dead sanction teaches the vocabulary + the host slip: {msg}"
        );
        // The legitimate forms all still parse.
        for to in ["exec", "infer", "agent", "nika:fetch", "mcp:srv/tool"] {
            let ok = format!(
                "secrets:\n  api_key:\n    source: env\n    key: MY_KEY\n    egress:\n      - to: \"{to}\"\n"
            );
            parse_strict(&ok).expect("a real sink form parses");
        }
    }

    /// T3 (use-case battery 2026-07-11) · the non-mapping egress refusal
    /// names ALL the entry's fields — the `…` used to hide `host` /
    /// `host_from_self`.
    #[test]
    fn egress_non_mapping_refusal_names_every_field() {
        let yaml = "\
secrets:
  api_key:
    source: env
    key: MY_KEY
    egress: [\"nika.sh\"]
";
        let err = parse_strict(yaml).expect_err("rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("host_from_self"),
            "the entry shape is spelled out: {msg}"
        );
    }

    #[test]
    fn vars_untyped_and_typed() {
        // Spec 01 §vars · both forms side-by-side.
        let yaml = "\
vars:
  output_dir: \"./output\"
  topic:
    type: string
    required: true
    default: \"Rust async 2026\"
    description: \"Subject to research\"
";
        let wf = parse_strict(yaml).expect("parse");
        assert_eq!(wf.vars.len(), 2);
        assert_eq!(wf.vars[0].0.value, "output_dir");
        assert!(matches!(
            &wf.vars[0].1,
            VarDecl::Untyped(v) if v == "./output"
        ));
        assert_eq!(wf.vars[1].0.value, "topic");
        let VarDecl::Typed {
            r#type,
            required,
            default,
            description,
        } = &wf.vars[1].1
        else {
            panic!("expected Typed");
        };
        assert_eq!(*r#type, VarType::String);
        assert!(required);
        assert_eq!(default.as_ref().expect("default"), "Rust async 2026");
        assert_eq!(description.as_deref(), Some("Subject to research"));
    }

    #[test]
    fn vars_untyped_list_value() {
        let yaml = "\
vars:
  locales: [\"fr\", \"es\"]
";
        let wf = parse_strict(yaml).expect("parse");
        assert!(matches!(
            &wf.vars[0].1,
            VarDecl::Untyped(v) if v.as_array().is_some_and(|a| a.len() == 2)
        ));
    }

    #[test]
    fn vars_typed_unknown_type_errors() {
        let yaml = "\
vars:
  x:
    type: str
";
        let err = parse_strict(yaml).expect_err("bad type");
        assert!(matches!(err, SchemaError::BadTypedVar { .. }), "{err:?}");
    }

    #[test]
    fn vars_typed_unknown_key_strict_errors() {
        let yaml = "\
vars:
  x:
    type: string
    requierd: true
";
        let err = parse_strict(yaml).expect_err("typo key");
        assert!(matches!(err, SchemaError::UnknownField { .. }), "{err:?}");
    }

    #[test]
    fn env_flat_map() {
        let yaml = "\
env:
  LOG_LEVEL: info
  REGION: eu-west
";
        let wf = parse_strict(yaml).expect("parse");
        assert_eq!(wf.env.len(), 2);
        assert_eq!(wf.env[0].0.value, "LOG_LEVEL");
        assert_eq!(wf.env[0].1.value, "info");
    }

    #[test]
    fn secrets_full_forms() {
        // Spec 01 §secrets · vault default + explicit env source.
        let yaml = "\
secrets:
  api_key:
    source: vault
    key: prod/anthropic/api-key
  github_token:
    source: env
    key: GITHUB_TOKEN
  implicit:
    key: some/path
";
        let wf = parse_strict(yaml).expect("parse");
        assert_eq!(wf.secrets.len(), 3);
        assert_eq!(wf.secrets[0].1.value.source, SecretSource::Vault);
        assert_eq!(wf.secrets[0].1.value.key, "prod/anthropic/api-key");
        assert_eq!(wf.secrets[1].1.value.source, SecretSource::Env);
        // `source` defaults to vault (the sovereign default).
        assert_eq!(wf.secrets[2].1.value.source, SecretSource::Vault);
    }

    #[test]
    fn secrets_inline_literal_errors() {
        // Spec 01 · « never an inline literal ».
        let yaml = "\
secrets:
  api_key: \"sk-12345\"
";
        let err = parse_strict(yaml).expect_err("inline literal");
        assert!(matches!(err, SchemaError::BadSecretRef { .. }), "{err:?}");
    }

    #[test]
    fn secrets_unknown_source_errors() {
        let yaml = "\
secrets:
  api_key:
    source: aws
    key: x
";
        let err = parse_strict(yaml).expect_err("unknown source");
        assert!(matches!(err, SchemaError::BadSecretRef { .. }), "{err:?}");
    }

    #[test]
    fn secrets_file_source_takes_path() {
        // Spec 01 §secrets · the shape is discriminated by source ·
        // `file` requires `path:` (k8s/Docker mounted secrets).
        let yaml = "\
secrets:
  signing_pem:
    source: file
    path: ~/.keys/signing.pem
";
        let wf = parse_strict(yaml).expect("parse");
        assert_eq!(wf.secrets[0].1.value.source, SecretSource::File);
        assert_eq!(wf.secrets[0].1.value.key, "~/.keys/signing.pem");
    }

    #[test]
    fn secrets_wrong_field_for_source_errors() {
        // `file` + `key:` and `vault` + `path:` are both shape errors —
        // the wrong field is never silently accepted.
        let file_with_key = "\
secrets:
  pem:
    source: file
    key: ~/.keys/signing.pem
";
        let err = parse_strict(file_with_key).expect_err("file takes path");
        assert!(
            matches!(&err, SchemaError::BadSecretRef { reason, .. } if reason.contains("`path:`")),
            "{err:?}"
        );

        let vault_with_path = "\
secrets:
  api_key:
    source: vault
    path: prod/key
";
        let err = parse_strict(vault_with_path).expect_err("vault takes key");
        assert!(
            matches!(&err, SchemaError::BadSecretRef { reason, .. } if reason.contains("`key:`")),
            "{err:?}"
        );
    }

    #[test]
    fn secrets_missing_key_errors() {
        let yaml = "\
secrets:
  api_key:
    source: vault
";
        let err = parse_strict(yaml).expect_err("missing key");
        assert!(matches!(err, SchemaError::BadSecretRef { .. }), "{err:?}");
    }

    #[test]
    fn secret_without_egress_has_empty_list() {
        // backward-compat: a plain secret carries no sanctioned egress.
        let yaml = "secrets:\n  k:\n    source: env\n    key: K\n";
        let wf = parse_strict(yaml).expect("parse");
        assert!(wf.secrets[0].1.value.egress.is_empty());
    }

    #[test]
    fn egress_literal_host_parses() {
        let yaml = "\
secrets:
  stripe:
    source: env
    key: STRIPE_KEY
    egress:
      - to: \"nika:fetch\"
        host: \"api.stripe.com\"
";
        let wf = parse_strict(yaml).expect("parse");
        let egress = &wf.secrets[0].1.value.egress;
        assert_eq!(egress.len(), 1);
        assert_eq!(egress[0].to, "nika:fetch");
        assert_eq!(egress[0].host.as_deref(), Some("api.stripe.com"));
        assert!(!egress[0].host_from_self);
    }

    #[test]
    fn egress_host_from_self_parses() {
        let yaml = "\
secrets:
  hook:
    source: env
    key: WEBHOOK
    egress:
      - to: \"nika:notify\"
        host_from_self: true
";
        let wf = parse_strict(yaml).expect("parse");
        let egress = &wf.secrets[0].1.value.egress;
        assert_eq!(egress.len(), 1);
        assert_eq!(egress[0].to, "nika:notify");
        assert!(egress[0].host_from_self);
        assert_eq!(egress[0].host, None);
    }

    #[test]
    fn egress_sink_only_exec_parses() {
        let yaml = "\
secrets:
  tok:
    source: env
    key: TOK
    egress:
      - to: exec
";
        let wf = parse_strict(yaml).expect("parse");
        let egress = &wf.secrets[0].1.value.egress;
        assert_eq!(egress[0].to, "exec");
        assert_eq!(egress[0].host, None);
        assert!(!egress[0].host_from_self);
    }

    #[test]
    fn egress_missing_to_errors() {
        let yaml = "\
secrets:
  k:
    source: env
    key: K
    egress:
      - host: \"api.x.com\"
";
        let err = parse_strict(yaml).expect_err("missing to");
        assert!(matches!(err, SchemaError::BadSecretRef { .. }), "{err:?}");
    }

    #[test]
    fn egress_host_and_host_from_self_are_mutually_exclusive() {
        let yaml = "\
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"nika:fetch\"
        host: \"api.x.com\"
        host_from_self: true
";
        let err = parse_strict(yaml).expect_err("both host clauses");
        assert!(matches!(err, SchemaError::BadSecretRef { .. }), "{err:?}");
    }

    #[test]
    fn egress_unknown_key_errors() {
        // strict: a typo in an egress entry must not silently widen.
        let yaml = "\
secrets:
  k:
    source: env
    key: K
    egress:
      - to: \"nika:fetch\"
        hsot: \"api.x.com\"
";
        let err = parse_strict(yaml).expect_err("unknown egress key");
        assert!(err.to_string().contains("unknown"), "{err}");
    }

    #[test]
    fn egress_non_list_errors() {
        let yaml = "\
secrets:
  k:
    source: env
    key: K
    egress: \"nika:fetch\"
";
        let err = parse_strict(yaml).expect_err("egress not a list");
        assert!(matches!(err, SchemaError::BadSecretRef { .. }), "{err:?}");
    }

    #[test]
    fn outputs_untyped_and_typed() {
        // Spec 01 §outputs · both forms.
        let yaml = "\
outputs:
  summary: ${{ tasks.synthesize.output }}
  report:
    value: ${{ tasks.write_report.output }}
    type: string
    description: \"The final markdown brief\"
";
        let wf = parse_strict(yaml).expect("parse");
        assert_eq!(wf.outputs.len(), 2);
        assert!(matches!(
            &wf.outputs[0].1,
            OutputDecl::Untyped(v) if v.value == "${{ tasks.synthesize.output }}"
        ));
        let OutputDecl::Typed { value, r#type, .. } = &wf.outputs[1].1 else {
            panic!("expected Typed");
        };
        assert_eq!(value.value, "${{ tasks.write_report.output }}");
        assert_eq!(*r#type, Some(VarType::String));
    }

    #[test]
    fn outputs_typed_missing_value_errors() {
        let yaml = "\
outputs:
  report:
    type: string
";
        let err = parse_strict(yaml).expect_err("missing value");
        assert!(matches!(err, SchemaError::MissingField { .. }), "{err:?}");
    }
}

/// Parse `permits:` — the declared capability boundary (spec 01 §permits).
///
/// `None` = the block is absent (today's behavior · engine floor only).
/// `Some(Permits)` = default-deny: every category not listed is denied.
/// The block and its sub-blocks use CLOSED key sets — an unknown key is
/// always an error here (a typo'd capability silently widening or
/// narrowing the boundary would be a security bug, so `permits:` is
/// strict in BOTH parse modes).
pub(super) fn parse_permits(
    cx: &Cx<'_>,
    workflow: &MarkedMappingNode,
) -> Result<Option<Spanned<Permits>>, SchemaError> {
    let Some(node) = workflow.get_node("permits") else {
        return Ok(None);
    };
    let mapping = require_mapping(cx, node, "permits")?;
    cx.check_unknown_keys_always(mapping, PERMITS_KEYS, "`permits:`")?;

    let mut permits = Permits::new();

    if let Some(fs_node) = mapping.get_node("fs") {
        let fs_map = require_mapping(cx, fs_node, "permits.fs")?;
        cx.check_unknown_keys_always(fs_map, PERMITS_FS_KEYS, "`permits.fs`")?;
        permits.fs = Some(FsPermits::new(
            string_list_values(cx, fs_map, "read")?,
            string_list_values(cx, fs_map, "write")?,
        ));
    }

    if let Some(net_node) = mapping.get_node("net") {
        let net_map = require_mapping(cx, net_node, "permits.net")?;
        cx.check_unknown_keys_always(net_map, PERMITS_NET_KEYS, "`permits.net`")?;
        permits.net = Some(NetPermits::new(string_list_values(cx, net_map, "http")?));
    }

    if let Some(exec_node) = mapping.get_node("exec") {
        permits.exec = Some(parse_exec_permit(cx, exec_node)?);
    }

    if mapping.get_node("tools").is_some() {
        permits.tools = Some(string_list_values(cx, mapping, "tools")?);
    }

    Ok(Some(Spanned::new(permits, cx.span_or_zero(node.span()))))
}

/// `permits.exec` — the closed tri-state · `false` | `true` | `[programs…]`.
fn parse_exec_permit(cx: &Cx<'_>, node: &Node) -> Result<ExecPermit, SchemaError> {
    if let Some(scalar) = node.as_scalar() {
        return match scalar.as_str() {
            "false" => Ok(ExecPermit::No),
            "true" => Ok(ExecPermit::Any),
            other => Err(SchemaError::Validation {
                message: format!(
                    "`permits.exec` must be false, true, or a program list — got `{other}`"
                ),
                span: cx.span(scalar.span()),
            }),
        };
    }
    if let Some(seq) = node.as_sequence() {
        let mut programs = Vec::with_capacity(seq.len());
        for item in seq.iter() {
            let Some(scalar) = item.as_scalar() else {
                return Err(SchemaError::Validation {
                    message: "each `permits.exec` entry must be a program name string".to_owned(),
                    span: cx.span(item.span()),
                });
            };
            programs.push(scalar.as_str().to_owned());
        }
        return Ok(ExecPermit::Programs(programs));
    }
    Err(SchemaError::Validation {
        message: "`permits.exec` must be false, true, or a program list".to_owned(),
        span: cx.span(node.span()),
    })
}

/// A plain string list under `key` (values only · permits globs carry no
/// per-element spans downstream).
fn string_list_values(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
    key: &str,
) -> Result<Vec<String>, SchemaError> {
    Ok(super::tasks::parse_string_list(cx, mapping, key)?
        .into_iter()
        .map(|s| s.value)
        .collect())
}

/// Parse `policy:` — named workflow law (spec `10-authority.md`). The
/// families/rules/values are a CLOSED set at the TYPE level (`nika-cap`
/// serde `deny_unknown_fields`) — an unknown name is a `NIKA-PARSE`-class
/// refusal in BOTH parse modes (the `permits:` precedent: a typo'd law
/// silently not binding would be a security bug).
pub(super) fn parse_policy(
    cx: &Cx<'_>,
    workflow: &MarkedMappingNode,
) -> Result<Option<Spanned<Policy>>, SchemaError> {
    let Some(node) = workflow.get_node("policy") else {
        return Ok(None);
    };
    require_mapping(cx, node, "policy")?;
    let span = cx.span(node.span());
    let policy = Policy::from_value(json_value(cx, node)?)
        .map_err(|message| SchemaError::Validation { message, span })?;
    Ok(Some(Spanned::new(policy, cx.span_or_zero(node.span()))))
}

#[cfg(test)]
mod policy_tests {
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;
    use crate::types::{EffectClass, Objective};

    const BASE: &str = "\
nika: v1
workflow:
  id: demo
tasks:
  t:
    infer: { prompt: \"x\" }
";

    #[test]
    fn absent_policy_is_none() {
        let wf = parse(BASE, FileId::new(0), ParseMode::Strict).expect("parse");
        assert!(wf.policy.is_none(), "absent block = no law bound");
    }

    #[test]
    fn full_policy_block_parses_spec_shape() {
        // The spec 10 §policy example, verbatim families.
        let yaml = format!(
            "{BASE}\
policy:
  require:
    human_gate_before: [exec, write]
  forbid:
    exec_after: [net]
  allow:
    providers: [ollama, mistral]
  limits:
    max_tasks: 50
  prefer:
    providers: [ollama]
  optimize: cost
"
        );
        let wf = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let p = &wf.policy.expect("present").value;
        assert_eq!(
            p.require.as_ref().expect("require").human_gate_before,
            Some(vec![EffectClass::Exec, EffectClass::Write])
        );
        assert_eq!(
            p.forbid.as_ref().expect("forbid").exec_after,
            Some(vec![EffectClass::Net])
        );
        assert_eq!(
            p.allow.as_ref().expect("allow").providers,
            Some(vec!["ollama".to_owned(), "mistral".to_owned()])
        );
        assert_eq!(p.limits.as_ref().expect("limits").max_tasks, Some(50));
        assert!(p.has_soft_families());
        assert_eq!(p.optimize, Some(Objective::Cost));
    }

    #[test]
    fn unknown_rule_is_a_parse_class_refusal_in_both_modes() {
        // Fixture core/policy/009: `write_after` is not a v1 rule — the
        // closed set refuses (never a silent no-op), and the refusal
        // holds in LENIENT mode too (the permits precedent: a law that
        // silently does not bind is a security bug).
        let yaml = format!("{BASE}policy:\n  forbid:\n    write_after: [net]\n");
        for mode in [ParseMode::Strict, ParseMode::Lenient] {
            let err = parse(&yaml, FileId::new(0), mode).expect_err("refused");
            let code = err.spec_code();
            assert_eq!(code.namespace, "PARSE", "{mode:?}");
            assert_eq!(code.category.as_str(), "validation_error", "{mode:?}");
            let msg = err.to_string();
            assert!(
                msg.contains("write_after") && msg.contains("exec_after"),
                "the refusal teaches the closed rule set: {msg}"
            );
        }
    }

    #[test]
    fn unknown_family_and_value_are_refused_with_the_vocabulary() {
        let unknown_family = format!("{BASE}policy:\n  deny:\n    exec_after: [net]\n");
        let err = parse(&unknown_family, FileId::new(0), ParseMode::Strict).expect_err("family");
        assert!(
            err.to_string().contains("require") && err.to_string().contains("optimize"),
            "the family vocabulary rides the refusal: {err}"
        );
        // an effect class outside the closed set (reads are not gateable)
        let bad_class = format!("{BASE}policy:\n  forbid:\n    exec_after: [read]\n");
        let err = parse(&bad_class, FileId::new(0), ParseMode::Strict).expect_err("class");
        assert!(
            err.to_string().contains("exec·write·net·tools"),
            "the class vocabulary rides the refusal: {err}"
        );
    }

    #[test]
    fn max_tasks_zero_is_refused_at_parse() {
        let yaml = format!("{BASE}policy:\n  limits:\n    max_tasks: 0\n");
        let err = parse(&yaml, FileId::new(0), ParseMode::Strict).expect_err("zero");
        assert!(err.to_string().contains("must be ≥ 1"), "{err}");
    }

    #[test]
    fn non_mapping_policy_is_refused() {
        let yaml = format!("{BASE}policy: strict\n");
        let err = parse(&yaml, FileId::new(0), ParseMode::Strict).expect_err("scalar");
        assert!(err.to_string().contains("mapping"), "{err}");
    }

    #[test]
    fn empty_policy_block_parses_as_no_law() {
        let yaml = format!("{BASE}policy: {{}}\n");
        let wf = parse(&yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let p = &wf.policy.expect("present").value;
        assert!(p.require.is_none() && p.forbid.is_none());
        assert!(!p.has_soft_families());
    }
}

#[cfg(test)]
mod permits_tests {
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;
    use crate::types::ExecPermit;

    fn parse_mode(yaml: &str, mode: ParseMode) -> crate::raw::RawWorkflow {
        parse(yaml, FileId::new(0), mode).expect("parse")
    }

    const BASE: &str = "\
nika: v1
workflow:
  id: demo
tasks:
  t:
    exec: { command: [\"true\"] }
";

    #[test]
    fn absent_permits_is_none() {
        let wf = parse_mode(BASE, ParseMode::Strict);
        assert!(wf.permits.is_none(), "absent block = today's behavior");
    }

    #[test]
    fn empty_permits_is_pure_compute() {
        let yaml = format!("{BASE}permits: {{}}\n");
        let wf = parse_mode(&yaml, ParseMode::Strict);
        let p = &wf.permits.expect("present").value;
        assert!(!p.allows_exec());
        assert!(!p.allows_tool("nika:read"));
        assert!(p.fs.is_none() && p.net.is_none());
    }

    #[test]
    fn full_permits_block_round_trips() {
        let yaml = format!(
            "{BASE}\
permits:
  fs:   {{ read: [\"./data/**\"], write: [\"./out/**\"] }}
  net:  {{ http: [\"api.example.com\", \"*.github.com\"] }}
  exec: [\"git\", \"cargo\"]
  tools: [\"nika:read\", \"mcp:browser/*\"]
"
        );
        let wf = parse_mode(&yaml, ParseMode::Strict);
        let p = &wf.permits.expect("present").value;
        assert_eq!(p.fs.as_ref().expect("fs").read, vec!["./data/**"]);
        assert_eq!(p.fs.as_ref().expect("fs").write, vec!["./out/**"]);
        assert_eq!(
            p.net.as_ref().expect("net").http,
            vec!["api.example.com", "*.github.com"]
        );
        assert!(p.allows_program("git") && p.allows_program("cargo"));
        assert!(!p.allows_program("rm"));
        assert!(p.allows_tool("mcp:browser/navigate"));
        assert!(!p.allows_tool("mcp:postgres/query"));
    }

    #[test]
    fn exec_bool_forms() {
        for (lit, expect_any) in [("true", true), ("false", false)] {
            let yaml = format!("{BASE}permits: {{ exec: {lit} }}\n");
            let wf = parse_mode(&yaml, ParseMode::Strict);
            let p = &wf.permits.expect("present").value;
            assert_eq!(p.allows_exec(), expect_any, "exec: {lit}");
            if expect_any {
                assert_eq!(p.exec, Some(ExecPermit::Any));
            } else {
                assert_eq!(p.exec, Some(ExecPermit::No));
            }
        }
    }

    #[test]
    fn unknown_permits_key_rejected_even_in_lenient_mode() {
        // A typo'd capability key must NEVER silently alter the boundary —
        // permits: is strict in BOTH modes.
        let yaml = format!("{BASE}permits: {{ network: {{ http: [\"x\"] }} }}\n");
        let err = parse(&yaml, FileId::new(0), ParseMode::Lenient).expect_err("rejected");
        let msg = err.to_string();
        assert!(msg.contains("network"), "names the bad key: {msg}");
    }

    #[test]
    fn exec_bad_scalar_rejected() {
        let yaml = format!("{BASE}permits: {{ exec: \"maybe\" }}\n");
        let err = parse(&yaml, FileId::new(0), ParseMode::Strict).expect_err("rejected");
        assert!(err.to_string().contains("permits.exec"));
    }
}
