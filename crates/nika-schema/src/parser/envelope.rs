// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Envelope block parsing — `outputs:` · `permits:` · `run:` (spec
//! `01-envelope.md` · the three value authorities parse in
//! `envelope_values.rs`, re-exported below · `vars:`/`env:` are dead
//! forms, refused at `parser/mod.rs` with NIKA-VALUES-001/002 ·
//! `assert:`/`types:`/`policy:`/`config:` died with the 9-key envelope
//! and are refused as unknown keys, NIKA-PARSE).

use marked_yaml::Node;
use marked_yaml::types::MarkedMappingNode;

use crate::error::SchemaError;
use crate::source::Spanned;
use crate::types::{
    ExecPermit, FsPermits, NetPermits, OutputDecl, Permits, RunClock, RunDecl, RunEntropy,
};

use super::{Cx, value::json_value};

// The closed key vocabularies live in `nika_vocab::keys` (the C2 descent —
// one vocabulary, one home); the parser reads them through this re-export.
pub(crate) use nika_vocab::keys::{
    CONST_TYPED_KEYS, EGRESS_KEYS, INPUT_KEYS, PERMITS_FS_KEYS, PERMITS_KEYS, PERMITS_NET_KEYS,
    RUN_ENTROPY_MAP_KEYS, RUN_KEYS, SECRET_KEYS, TYPED_OUTPUT_KEYS,
};

// The three value authorities parse in `envelope_values.rs` (the C2 file
// split — ONE coherent unit); the parser's `envelope::parse_*` call paths
// ride this re-export unchanged.
pub(super) use super::envelope_values::{parse_const, parse_inputs, parse_secrets};

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

    /// The `assert:` block died with the 9-key envelope (spec 15 · « the
    /// subtraction is the fix »): it judged NOTHING — an obligation naming
    /// an ordering over two tasks that do not exist was accepted `clean ·
    /// risk low`, where the same mistake one field away (`after:`) is
    /// `NIKA-DAG-002`. One file in 661 carried it, and that file was the
    /// probe written to demonstrate the defect. The builtin `nika:assert`
    /// is untouched — that half works.
    #[test]
    fn the_assert_block_is_refused() {
        let dead = "\
nika: gated
assert:
  - no_secret_egress
tasks:
  gate:
    exec: { command: [\"true\"] }
";
        let err = parse_strict(dead).expect_err("the dead key is refused");
        assert!(
            format!("{err:?}").contains("assert"),
            "the refusal names the key: {err:?}"
        );
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

    /// The `config:` key died with the 9-key envelope (2026-08-13):
    /// measured zero real usage, its `default:` was its only possible
    /// source, and under the taint lattice `config.p` and `inputs.p`
    /// produced the SAME `NIKA-AUTH-008` by the same path. A
    /// deployment-supplied value is an `inputs:` entry with
    /// `required: false` and a `default:`.
    /// The fixture MUST carry the dead key. A codemod once migrated it
    /// away (`config:` → an `inputs:` entry) and left the assertion
    /// standing: the test then proved that a perfectly valid workflow is
    /// refused, which it is not. The bytes below are the spec's own
    /// `core/envelope/023-config-block-rejected` input (`NIKA-PARSE` ·
    /// `config:` is simply not an envelope key).
    #[test]
    fn the_config_block_is_refused() {
        let dead = "\
nika: t
config:
  log_level: { type: string, default: \"info\" }
tasks:
  s:
    infer: { prompt: \"x\" }
";
        let err = parse_strict(dead).expect_err("the dead key is refused");
        assert_eq!(err.spec_code().namespace, "PARSE", "{err:?}");
        assert!(
            format!("{err:?}").contains("config"),
            "the refusal names the key: {err:?}"
        );
        // The same envelope with the key removed is CLEAN — the refusal
        // is about `config:`, not about anything else in the fixture.
        let live = "\
nika: t
inputs:
  log_level: { type: string, required: false, default: \"info\" }
tasks:
  s:
    infer: { prompt: \"x\" }
";
        parse_strict(live).expect("the nine-key envelope parses");
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
            message.contains("`inputs:`") && message.contains("`secrets:`"),
            "{message}"
        );
        // `config:` died with the 9-key envelope — the env refusal may
        // never route an author to a key the parser also refuses.
        assert!(!message.contains("`config:`"), "{message}");
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

/// Parse `run:` — the run's entropy + clock declaration (F-P3).
///
/// Shape-only, with the CLOSED key set `{entropy, clock}` refused in BOTH
/// parse modes (the `permits:`/`policy:` precedent: a typo'd declaration
/// silently not binding would mis-declare the run's determinism contract).
/// The value forms are the `assert:` vocabulary idiom — bare scalars for
/// the parameterless values (`none` · `ambient` · `system` · `virtual`),
/// a single-key map for the one parameterized value (`{ seeded: <u64> }`).
///
/// The ONE semantic judgment the parser makes here is the declared
/// CONTRADICTION ([`RunDecl::contradiction_class`]): a determinism demand
/// sharing the block with a declared non-deterministic source, each
/// class riding its dedicated mint (`NIKA-PARSE-026` ambient × virtual ·
/// `NIKA-PARSE-027` none|seeded × system · NEP-0010). Everything else —
/// which seams the declaration pilots, the body-level entropy-source
/// judgment — belongs to the composer and the checker, never to a
/// shape pass.
pub(super) fn parse_run(
    cx: &Cx<'_>,
    workflow: &MarkedMappingNode,
) -> Result<Option<Spanned<RunDecl>>, SchemaError> {
    let Some(node) = workflow.get_node("run") else {
        return Ok(None);
    };
    let mapping = require_mapping(cx, node, "run")?;
    cx.check_unknown_keys_always(mapping, RUN_KEYS, "`run:`")?;

    let entropy = match mapping.get_node("entropy") {
        Some(entropy_node) => Some(parse_run_entropy(cx, entropy_node)?),
        None => None,
    };
    let clock = match mapping.get_node("clock") {
        Some(clock_node) => {
            let scalar = clock_node
                .as_scalar()
                .ok_or_else(|| SchemaError::Validation {
                    message: "`run.clock` must be a scalar (`system` | `virtual`)".to_owned(),
                    span: cx.span(clock_node.span()),
                })?;
            let clock = match scalar.as_str() {
                "system" => RunClock::System,
                "virtual" => RunClock::Virtual,
                other => {
                    return Err(SchemaError::Validation {
                        message: format!(
                            "`run.clock` must be `system` or `virtual` — got `{other}` (F-P3 · \
                             the closed clock vocabulary)"
                        ),
                        span: cx.span(scalar.span()),
                    });
                }
            };
            Some(clock)
        }
        None => None,
    };

    let decl = RunDecl::new(entropy, clock);
    if let Some(class) = decl.contradiction_class() {
        return Err(SchemaError::RunContradiction {
            class,
            span: cx.span(node.span()),
        });
    }
    Ok(Some(Spanned::new(decl, cx.span_or_zero(node.span()))))
}

/// `run.entropy` — `none` · `ambient` · `{ seeded: <u64> }` (the
/// single-key map carries the one parameterized value).
fn parse_run_entropy(cx: &Cx<'_>, node: &marked_yaml::Node) -> Result<RunEntropy, SchemaError> {
    if let Some(scalar) = node.as_scalar() {
        return match scalar.as_str() {
            "none" => Ok(RunEntropy::None),
            "ambient" => Ok(RunEntropy::Ambient),
            other => Err(SchemaError::Validation {
                message: format!(
                    "`run.entropy` must be `none`, `ambient`, or `{{ seeded: <u64> }}` — got \
                     `{other}` (F-P3 · the closed entropy vocabulary)"
                ),
                span: cx.span(scalar.span()),
            }),
        };
    }
    let mapping = require_mapping(cx, node, "run.entropy")?;
    cx.check_unknown_keys_always(mapping, RUN_ENTROPY_MAP_KEYS, "`run.entropy`")?;
    let Some(seed_node) = mapping.get_node("seeded") else {
        return Err(SchemaError::Validation {
            message: "`run.entropy` as a map is exactly `{ seeded: <u64> }`".to_owned(),
            span: cx.span(node.span()),
        });
    };
    let Some(seed_scalar) = seed_node.as_scalar() else {
        return Err(SchemaError::Validation {
            message: "`run.entropy.seeded` must be a non-negative integer (u64)".to_owned(),
            span: cx.span(seed_node.span()),
        });
    };
    let seed = seed_scalar
        .as_str()
        .parse::<u64>()
        .map_err(|_| SchemaError::Validation {
            message: format!(
                "`run.entropy.seeded` must be a non-negative integer (u64) — got `{}`",
                seed_scalar.as_str()
            ),
            span: cx.span(seed_scalar.span()),
        })?;
    Ok(RunEntropy::Seeded(seed))
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
nika: demo
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
  net:  {{ http: [\"api.example.com\", \"api.github.com\"] }}
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
            vec!["api.example.com", "api.github.com"]
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

// ── F-P3 · the `run:` block (entropy + clock declaration) ─────────────

#[cfg(test)]
mod run_tests {
    use crate::parser::{ParseMode, parse};
    use crate::source::FileId;
    use crate::types::{RunClock, RunEntropy};

    const BASE: &str = "\
nika: demo
";

    fn parse_strict(yaml: &str) -> Result<crate::raw::RawWorkflow, crate::error::SchemaError> {
        parse(yaml, FileId::new(0), ParseMode::Strict)
    }

    #[test]
    fn absent_run_block_is_none() {
        let wf = parse_strict(BASE).expect("parse");
        assert!(wf.run.is_none(), "absent = the undeclared status quo");
    }

    #[test]
    fn every_entropy_form_parses() {
        let wf = parse_strict(&format!("{BASE}run: {{ entropy: none }}\n")).expect("none");
        assert_eq!(wf.run.expect("run").value.entropy, Some(RunEntropy::None));
        let wf = parse_strict(&format!("{BASE}run: {{ entropy: ambient }}\n")).expect("ambient");
        assert_eq!(
            wf.run.expect("run").value.entropy,
            Some(RunEntropy::Ambient)
        );
        let wf =
            parse_strict(&format!("{BASE}run: {{ entropy: {{ seeded: 42 }} }}\n")).expect("seeded");
        assert_eq!(
            wf.run.expect("run").value.entropy,
            Some(RunEntropy::Seeded(42))
        );
    }

    #[test]
    fn block_form_and_clock_parse() {
        let yaml = format!("{BASE}run:\n  entropy:\n    seeded: 7\n  clock: virtual\n");
        let wf = parse_strict(&yaml).expect("block form parses");
        let decl = wf.run.expect("run").value;
        assert_eq!(decl.entropy, Some(RunEntropy::Seeded(7)));
        assert_eq!(decl.clock, Some(RunClock::Virtual));
        let wf = parse_strict(&format!("{BASE}run: {{ clock: system }}\n")).expect("system");
        assert_eq!(
            wf.run.expect("run").value.clock,
            Some(RunClock::System),
            "an explicit system clock parses (the spelled-out status quo)"
        );
    }

    #[test]
    fn ambient_times_virtual_is_refused() {
        // F-P3 (a) — the determinism demand × the ambient declaration.
        let yaml = format!("{BASE}run: {{ entropy: ambient, clock: virtual }}\n");
        let err = parse_strict(&yaml).expect_err("contradiction refused");
        assert_eq!(
            err.spec_code().to_string(),
            "NIKA-PARSE-026",
            "the dedicated mint (NEP-0010)"
        );
        let msg = err.to_string();
        assert!(msg.contains("contradicts itself"), "{msg}");
        assert!(msg.contains("ambient") && msg.contains("virtual"), "{msg}");
    }

    #[test]
    fn deterministic_entropy_times_system_clock_is_refused() {
        // The mirror — seeded/none force the virtual clock; an explicit
        // wall clock breaks the byte-identical-journal law.
        for entropy in ["none", "{ seeded: 42 }"] {
            let yaml = format!("{BASE}run: {{ entropy: {entropy}, clock: system }}\n");
            let err = parse_strict(&yaml).expect_err("contradiction refused");
            assert_eq!(
                err.spec_code().to_string(),
                "NIKA-PARSE-027",
                "{entropy}: the dedicated mint (NEP-0010)"
            );
            assert!(
                err.to_string().contains("contradicts itself"),
                "{entropy}: {err}"
            );
        }
    }

    #[test]
    fn redundant_virtual_beside_seeded_is_legal() {
        let yaml = format!("{BASE}run: {{ entropy: {{ seeded: 42 }}, clock: virtual }}\n");
        let wf = parse_strict(&yaml).expect("coherent (redundant but named)");
        assert_eq!(wf.run.expect("run").value.clock, Some(RunClock::Virtual));
    }

    #[test]
    fn unknown_run_key_is_refused_in_both_modes() {
        // A typo'd declaration silently not binding is the permits:/policy:
        // class — strict in BOTH modes.
        for mode in [ParseMode::Strict, ParseMode::Lenient] {
            let yaml = format!("{BASE}run: {{ entropi: none }}\n");
            let err = parse(&yaml, FileId::new(0), mode).expect_err("typo key refused");
            assert!(err.to_string().contains("entropi"), "{err}");
        }
    }

    #[test]
    fn bad_entropy_and_clock_values_are_refused() {
        let yaml = format!("{BASE}run: {{ entropy: pseudo }}\n");
        let err = parse_strict(&yaml).expect_err("unknown entropy");
        assert!(err.to_string().contains("pseudo"), "{err}");
        let yaml = format!("{BASE}run: {{ clock: sidereal }}\n");
        let err = parse_strict(&yaml).expect_err("unknown clock");
        assert!(err.to_string().contains("sidereal"), "{err}");
        let yaml = format!("{BASE}run: {{ entropy: {{ seeded: -1 }} }}\n");
        let err = parse_strict(&yaml).expect_err("negative seed");
        assert!(err.to_string().contains("u64"), "{err}");
        let yaml = format!("{BASE}run: {{ entropy: {{ seeded: 42, salt: 1 }} }}\n");
        let err = parse_strict(&yaml).expect_err("unknown entropy map key");
        assert!(err.to_string().contains("salt"), "{err}");
        let yaml = format!("{BASE}run: fast\n");
        assert!(parse_strict(&yaml).is_err(), "run: must be a mapping");
    }
}
