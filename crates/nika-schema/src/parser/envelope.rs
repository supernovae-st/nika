// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Envelope block parsing — `vars:` · `env:` · `secrets:` · `outputs:`
//! (spec `01-envelope.md`).

use marked_yaml::Node;
use marked_yaml::types::MarkedMappingNode;

use crate::error::SchemaError;
use crate::source::Spanned;
use crate::types::{OutputDecl, SecretRef, SecretSource, VarDecl, VarType};

use super::{Cx, value::node_to_json};

/// Keys of the typed `vars:` form (spec 01 §vars).
const TYPED_VAR_KEYS: &[&str] = &["type", "required", "default", "description"];

/// Keys of a `secrets:` entry (spec 01 §secrets).
const SECRET_KEYS: &[&str] = &["source", "key"];

/// Keys of the typed `outputs:` form (spec 01 §outputs).
const TYPED_OUTPUT_KEYS: &[&str] = &["value", "type", "description"];

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
            VarDecl::Untyped(node_to_json(value))
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

    let default = mapping.get_node("default").map(node_to_json);

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

/// Parse `secrets:` — each entry MUST be a `{ source, key }` reference.
///
/// Spec 01 §secrets · « A secret is always a **reference to a store** —
/// never an inline literal. » A scalar value is therefore a parse error.
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

        let key_scalar = entry
            .get_scalar("key")
            .ok_or_else(|| SchemaError::BadSecretRef {
                reason: format!("secret `{}` is missing its `key`", name.value),
                span: cx.span(entry.span()),
            })?;

        let span = cx.span_or_zero(entry.span());
        out.push((
            name,
            Spanned::new(SecretRef::new(source, key_scalar.as_str()), span),
        ));
    }
    Ok(out)
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
