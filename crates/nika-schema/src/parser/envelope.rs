// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Envelope block parsing — `assert:` · `types:` · `outputs:` · `permits:`
//! · `policy:` (spec `01-envelope.md` · the four value authorities parse in
//! `envelope_values.rs`, re-exported below · `vars:`/`env:` are dead forms,
//! refused at `parser/mod.rs` with NIKA-VALUES-001/002).

use marked_yaml::Node;
use marked_yaml::types::MarkedMappingNode;

use crate::error::SchemaError;
use crate::source::Spanned;
use crate::types::{
    AssertProperty, ExecPermit, FsPermits, NetPermits, OutputDecl, Permits, Policy,
};

use super::{Cx, value::json_value};

// The closed key vocabularies live in `nika_vocab::keys` (the C2 descent —
// one vocabulary, one home); the parser reads them through this re-export.
pub(crate) use nika_vocab::keys::{
    CONFIG_KEYS, CONST_TYPED_KEYS, EGRESS_KEYS, INPUT_KEYS, PERMITS_FS_KEYS, PERMITS_KEYS,
    PERMITS_NET_KEYS, SECRET_KEYS, TYPED_OUTPUT_KEYS,
};

// The four value authorities parse in `envelope_values.rs` (the C2 file
// split — ONE coherent unit); the parser's `envelope::parse_*` call paths
// ride this re-export unchanged.
pub(super) use super::envelope_values::{parse_config, parse_const, parse_inputs, parse_secrets};

/// Parse the workflow-level `assert:` block (spec 15 §assert) — a list of the
/// author's obligations, each parsed into the closed [`AssertProperty`]
/// vocabulary. An unknown property, a non-v1 shape, or a malformed body is a
/// refusal at check (`NIKA-ASSERT-001`, carried by the vocab refusal). The
/// engine JUDGES each obligation's honest level (nika-vocab `level`); wiring
/// the check-time genuine decision (does `before`/`bounded` hold on the
/// derived graph?) and `nika trace verify` reporting is the named owed — this
/// parse makes the obligations authorable and typed, refusing the malformed.
pub(super) fn parse_assert(
    cx: &Cx<'_>,
    workflow: &MarkedMappingNode,
) -> Result<Vec<Spanned<AssertProperty>>, SchemaError> {
    let Some(node) = workflow.get_node("assert") else {
        return Ok(Vec::new());
    };
    let Some(seq) = node.as_sequence() else {
        return Err(SchemaError::Validation {
            message: "`assert:` must be a list of obligations (spec 15 §assert · \
                      NIKA-ASSERT-001)"
                .to_owned(),
            span: cx.span(node.span()),
        });
    };
    let mut out = Vec::with_capacity(seq.len());
    for item in seq.iter() {
        let value = json_value(cx, item)?;
        let property =
            AssertProperty::parse(&value).map_err(|refusal| SchemaError::Validation {
                message: refusal.message,
                span: cx.span(item.span()),
            })?;
        out.push(Spanned::new(property, cx.span_or_zero(item.span())));
    }
    Ok(out)
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
                // The `type:` speaks the full TypeExpr (R3b ·
                // LAW-GRAMMAR-0211) — read shape-only; the grammar
                // judgment (`NIKA-TYPE-001/006`) is the analyzer's.
                let r#type = typed
                    .get_node("type")
                    .map(|n| json_value(cx, n).map(|v| Spanned::new(v, cx.span_or_zero(n.span()))))
                    .transpose()?;
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

/// Parse a flat scalar→scalar mapping (`exec.env:` — the envelope
/// `env:` block is dead since C2, refused `NIKA-VALUES-002`).
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
    use crate::types::{OutputDecl, SecretSource, VarDecl};

    fn parse_strict(yaml: &str) -> Result<crate::raw::RawWorkflow, SchemaError> {
        parse(yaml, FileId::new(0), ParseMode::Strict)
    }

    /// The `assert:` block (spec 15 §assert) lowers to the typed obligation
    /// vocabulary; an unknown property is refused at parse with the
    /// `NIKA-ASSERT-001` code the reference names.
    #[test]
    fn assert_block_parses_obligations_and_refuses_the_unknown() {
        let ok = "\
nika: v1
workflow:
  id: gated
assert:
  - no_secret_egress
  - before: { first: gate, second: deploy }
  - bounded: { task: crawl, max_iterations: 100 }
tasks:
  gate:
    exec: { command: [\"true\"] }
";
        let wf = parse_strict(ok).expect("a valid assert: block parses");
        assert_eq!(wf.assert.len(), 3, "three obligations parse");
        assert_eq!(wf.assert[0].value.name(), "no_secret_egress");
        assert_eq!(wf.assert[1].value.name(), "before");
        assert_eq!(wf.assert[2].value.name(), "bounded");

        let bad = "\
nika: v1
workflow:
  id: gated
assert:
  - telepathy: {}
tasks:
  gate:
    exec: { command: [\"true\"] }
";
        let err = parse_strict(bad).expect_err("an unknown assert property is refused");
        assert!(
            err.to_string().contains("NIKA-ASSERT-001"),
            "the spec-15 refusal code rides: {err}"
        );

        // A non-list `assert:` is refused too (it is a list of obligations).
        let not_a_list = "\
nika: v1
workflow:
  id: gated
assert: no_secret_egress
tasks:
  gate:
    exec: { command: [\"true\"] }
";
        assert!(parse_strict(not_a_list).is_err(), "assert: must be a list");
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

    // ── C2 · the four-authority family (accept) + the dead forms (refuse) ──

    #[test]
    fn inputs_typed_declarations() {
        // Spec 01 §inputs · every entry typed (`type` required).
        let yaml = "\
inputs:
  topic:
    type: string
    required: true
    default: \"Rust async 2026\"
    description: \"Subject to research\"
";
        let wf = parse_strict(yaml).expect("parse");
        assert_eq!(wf.inputs.len(), 1);
        assert_eq!(wf.inputs[0].0.value, "topic");
        let VarDecl::Typed {
            r#type,
            required,
            default,
            description,
        } = &wf.inputs[0].1
        else {
            panic!("expected Typed");
        };
        assert_eq!(r#type.value, serde_json::json!("string"));
        assert!(required);
        assert_eq!(default.as_ref().expect("default"), "Rust async 2026");
        assert_eq!(description.as_deref(), Some("Subject to research"));
    }

    #[test]
    fn inputs_type_expr_composite_form_parses_shape_only() {
        // R3b · LAW-GRAMMAR-0211 · the `type:` speaks the full TypeExpr —
        // a constructor map rides raw (the grammar judgment is the
        // analyzer's, never the parser's).
        let yaml = "\
inputs:
  mode:
    type: { enum: [\"fast\", \"slow\"] }
    default: \"fast\"
";
        let wf = parse_strict(yaml).expect("a composite TypeExpr parses");
        let VarDecl::Typed { r#type, .. } = &wf.inputs[0].1 else {
            panic!("expected Typed");
        };
        assert_eq!(
            r#type.value,
            serde_json::json!({ "enum": ["fast", "slow"] })
        );
    }

    #[test]
    fn inputs_untyped_entry_is_refused() {
        // Spec 01 §inputs · a bare literal is NOT an inputs entry (a fixed
        // value is a `const:` entry).
        let yaml = "\
inputs:
  topic: \"hello\"
";
        let err = parse_strict(yaml).expect_err("untyped inputs entry");
        assert!(matches!(err, SchemaError::Validation { .. }), "{err:?}");
        assert!(err.to_string().contains("`type:` required"), "{err}");
    }

    #[test]
    fn config_typed_with_and_without_default() {
        // Spec 01 §config · `type` required · `default:` optional (the
        // deployment supplies when absent).
        let yaml = "\
config:
  log_level: { type: string, default: \"info\" }
  region: { type: string }
";
        let wf = parse_strict(yaml).expect("parse");
        assert_eq!(wf.config.len(), 2);
        assert_eq!(wf.config[0].0.value, "log_level");
        let VarDecl::Typed { default, .. } = &wf.config[0].1 else {
            panic!("expected Typed");
        };
        assert_eq!(default.as_ref().expect("default"), "info");
        let VarDecl::Typed { default: none, .. } = &wf.config[1].1 else {
            panic!("expected Typed");
        };
        assert!(none.is_none());
    }

    #[test]
    fn config_rejects_the_required_key() {
        // `required:` is inputs vocabulary — config is never caller-required.
        let yaml = "\
config:
  region: { type: string, required: true }
";
        let err = parse_strict(yaml).expect_err("required in config");
        assert!(matches!(err, SchemaError::UnknownField { .. }), "{err:?}");
    }

    #[test]
    fn const_bare_literals_and_typed_constant() {
        // Spec 01 §const · bare literal (any YAML value) OR `{type, value}`.
        let yaml = "\
const:
  greeting: \"hello\"
  retries: 3
  limits: { max: 10, min: 1 }
  pi: { type: number, value: 3.5 }
";
        let wf = parse_strict(yaml).expect("parse");
        assert_eq!(wf.consts.len(), 4);
        assert!(matches!(&wf.consts[0].1, VarDecl::Untyped(v) if v == "hello"));
        assert!(
            matches!(&wf.consts[2].1, VarDecl::Untyped(v) if v.is_object()),
            "a mapping without BOTH type+value is a bare literal object"
        );
        let VarDecl::Typed {
            r#type,
            default,
            required,
            ..
        } = &wf.consts[3].1
        else {
            panic!("expected Typed");
        };
        assert_eq!(r#type.value, serde_json::json!("number"));
        assert!(!required, "a constant is never caller-required");
        assert_eq!(
            default.as_ref().expect("value rides default"),
            &serde_json::json!(3.5)
        );
    }

    #[test]
    fn const_typed_extra_key_is_refused() {
        // The typed constant's key set is closed ({type, value} exactly).
        let yaml = "\
const:
  pi: { type: number, value: 3.5, default: 3.0 }
";
        let err = parse_strict(yaml).expect_err("extra key in typed const");
        assert!(matches!(err, SchemaError::UnknownField { .. }), "{err:?}");
    }

    #[test]
    fn inputs_out_of_grammar_type_parses_for_the_analyzer() {
        // R3b · the parser is shape-only: an out-of-grammar `type:` is
        // NOT a parse error — the analyzer refuses it `NIKA-TYPE-001`
        // (the NIKA-PARSE-015 class is retired, never reused).
        let yaml = "\
inputs:
  x:
    type: str
";
        let wf = parse_strict(yaml).expect("shape-only parse admits the raw expr");
        let VarDecl::Typed { r#type, .. } = &wf.inputs[0].1 else {
            panic!("expected Typed");
        };
        assert_eq!(r#type.value, serde_json::json!("str"));
    }

    #[test]
    fn inputs_typed_unknown_key_strict_errors() {
        let yaml = "\
inputs:
  x:
    type: string
    requierd: true
";
        let err = parse_strict(yaml).expect_err("typo key");
        assert!(matches!(err, SchemaError::UnknownField { .. }), "{err:?}");
    }

    #[test]
    fn vars_block_refuses_with_values_001() {
        // C2 · LAW-GRAMMAR-0201 · the dead `vars:` field teaches the
        // classification, never a generic unknown-field error.
        let yaml = "\
vars:
  topic: \"hello\"
";
        let err = parse_strict(yaml).expect_err("vars: is dead");
        let SchemaError::DeadValueForm { form, message, .. } = &err else {
            panic!("expected DeadValueForm, got {err:?}");
        };
        assert!(matches!(form, crate::error::DeadForm::Vars));
        assert_eq!(err.spec_code().to_string(), "NIKA-VALUES-001");
        assert!(message.contains("dead envelope field"), "{message}");
        assert!(
            message.contains("`inputs:`") && message.contains("`const:`"),
            "{message}"
        );
        assert!(message.contains("classify-not-rename"), "{message}");
    }

    #[test]
    fn env_block_refuses_with_values_002() {
        // C2 · LAW-GRAMMAR-0202 · the dead `env:` field.
        let yaml = "\
env:
  LOG_LEVEL: info
";
        let err = parse_strict(yaml).expect_err("env: is dead");
        let SchemaError::DeadValueForm { form, message, .. } = &err else {
            panic!("expected DeadValueForm, got {err:?}");
        };
        assert!(matches!(form, crate::error::DeadForm::Env));
        assert_eq!(err.spec_code().to_string(), "NIKA-VALUES-002");
        assert!(
            message.contains("`config:`") && message.contains("`secrets:`"),
            "{message}"
        );
    }

    #[test]
    fn secrets_full_forms() {
        // Spec 01 §secrets · the explicit provenances (source required · R8).
        let yaml = "\
secrets:
  api_key:
    source: vault
    key: prod/anthropic/api-key
  github_token:
    source: env
    key: GITHUB_TOKEN
";
        let wf = parse_strict(yaml).expect("parse");
        assert_eq!(wf.secrets.len(), 2);
        assert_eq!(wf.secrets[0].1.value.source, SecretSource::Vault);
        assert_eq!(wf.secrets[0].1.value.key, "prod/anthropic/api-key");
        assert_eq!(wf.secrets[1].1.value.source, SecretSource::Env);
    }

    #[test]
    fn secret_without_source_is_refused() {
        // R8 · the flipped dialect: the provenance is required explicitly
        // (the conformance guard fixture · never a defaulted vault).
        let yaml = "\
secrets:
  implicit:
    key: some/path
";
        let err = parse_strict(yaml).expect_err("missing source");
        assert!(matches!(err, SchemaError::BadSecretRef { .. }), "{err:?}");
        assert!(err.to_string().contains("source:"), "{err}");
        assert!(err.to_string().contains("implicit"), "{err}");
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
        assert_eq!(
            r#type.as_ref().map(|t| &t.value),
            Some(&serde_json::json!("string"))
        );
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
/// `None` = the block is absent (F-O8 « absent = zero authority » · every
/// effect refused at the gates · `NIKA-AUTH-006` at check).
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

    if mapping.get_node("env").is_some() {
        let entries = super::tasks::parse_string_list(cx, mapping, "env")?;
        let mut names = Vec::with_capacity(entries.len());
        for entry in entries {
            if !is_env_permit_entry(&entry.value) {
                return Err(SchemaError::Validation {
                    message: format!(
                        "`permits.env` entry `{}` is not an environment variable name \
                         (POSIX shape `[A-Za-z_][A-Za-z0-9_]*` · exact names, no globs · \
                         NEP-0005)",
                        entry.value
                    ),
                    span: Some(entry.span),
                });
            }
            names.push(entry.value);
        }
        permits.env = Some(names);
    }

    Ok(Some(Spanned::new(permits, cx.span_or_zero(node.span()))))
}

/// A `permits.env` entry shape (NEP-0005 law 4): an exact POSIX name, or a
/// string carrying a `${{ }}` island — the island passes the parse so the
/// CHECK refuses it as a non-literal bound (`NIKA-AUTH-007`) with the
/// teaching detail; any other string is a parse-level refusal (the spec
/// schema's `anyOf` shape gate, mirrored).
fn is_env_permit_entry(s: &str) -> bool {
    if s.contains("${{") {
        return true;
    }
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
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
    fn env_permit_parses_exact_names() {
        let yaml = format!("{BASE}permits:\n  env: [\"CI_COMMIT_SHA\", \"_UNDERSCORE1\"]\n");
        let wf = parse_mode(&yaml, ParseMode::Strict);
        let p = &wf.permits.expect("present").value;
        assert_eq!(
            p.env.as_deref(),
            Some(&["CI_COMMIT_SHA".to_owned(), "_UNDERSCORE1".to_owned()][..])
        );
        assert!(p.allows_env_key("CI_COMMIT_SHA"));
        assert!(!p.allows_env_key("HOME"), "the floor is not a grant");
        assert_eq!(p.env_passthrough().len(), 2);
    }

    #[test]
    fn env_permit_refuses_a_non_name_string() {
        // NEP-0005 law 4 · a non-name, non-island string is a parse-level
        // refusal (the spec schema's anyOf shape gate, mirrored).
        for bad in ["AWS_*", "1LEADING", "WITH-DASH", "with space"] {
            let yaml = format!("{BASE}permits:\n  env: [\"{bad}\"]\n");
            let err =
                parse(&yaml, FileId::new(0), ParseMode::Strict).expect_err("non-name refused");
            let msg = err.to_string();
            assert!(msg.contains("permits.env"), "{bad}: {msg}");
        }
    }

    #[test]
    fn env_permit_island_passes_the_parse_for_the_check_refusal() {
        // The island is NOT a parse error: the CHECK refuses it as a
        // non-literal bound (NIKA-AUTH-007) with the teaching detail.
        let yaml = format!("{BASE}permits:\n  env: [\"${{{{ inputs.k }}}}\"]\n");
        let wf = parse_mode(&yaml, ParseMode::Strict);
        let p = &wf.permits.expect("present").value;
        assert_eq!(p.env.as_ref().map(Vec::len), Some(1));
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
