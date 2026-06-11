// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Envelope block parsing — `vars:` · `env:` · `secrets:` · `permits:` ·
//! `outputs:` (spec `01-envelope.md`).

use marked_yaml::Node;
use marked_yaml::types::MarkedMappingNode;

use crate::error::SchemaError;
use crate::source::Spanned;
use crate::types::{
    ExecPermit, FsPermits, NetPermits, OutputDecl, Permits, SecretRef, SecretSource, VarDecl,
    VarType,
};

use super::{Cx, value::node_to_json};

/// Keys of the typed `vars:` form (spec 01 §vars).
const TYPED_VAR_KEYS: &[&str] = &["type", "required", "default", "description"];

/// Keys of a `secrets:` entry (spec 01 §secrets · `key` XOR `path` ·
/// discriminated by `source`).
const SECRET_KEYS: &[&str] = &["source", "key", "path"];

/// Keys of the typed `outputs:` form (spec 01 §outputs).
const TYPED_OUTPUT_KEYS: &[&str] = &["value", "type", "description"];

/// Keys of the `permits:` block (spec 01 §permits · closed).
const PERMITS_KEYS: &[&str] = &["fs", "net", "exec", "tools"];

/// Keys of `permits.fs` (closed).
const PERMITS_FS_KEYS: &[&str] = &["read", "write"];

/// Keys of `permits.net` (closed).
const PERMITS_NET_KEYS: &[&str] = &["http"];

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

        let span = cx.span_or_zero(entry.span());
        out.push((
            name,
            Spanned::new(SecretRef::new(source, reference.as_str()), span),
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
        permits.fs = Some(FsPermits {
            read: string_list_values(cx, fs_map, "read")?,
            write: string_list_values(cx, fs_map, "write")?,
        });
    }

    if let Some(net_node) = mapping.get_node("net") {
        let net_map = require_mapping(cx, net_node, "permits.net")?;
        cx.check_unknown_keys_always(net_map, PERMITS_NET_KEYS, "`permits.net`")?;
        permits.net = Some(NetPermits {
            http: string_list_values(cx, net_map, "http")?,
        });
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
workflow: demo
tasks:
  - id: t
    exec: { command: \"true\" }
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
