// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Verb-body parsing — the 4 canonical verbs of `02-verbs.md`.
//!
//! « A verb is a **distinct native execution model** the engine itself
//! implements. » The set is CLOSED at 4 (D-2026-05-22-N18) · `infer` ·
//! `exec` · `invoke` · `agent`. Each verb's field table is closed too —
//! strict mode rejects unknowns (`02-verbs.md` §forward-compat).

use marked_yaml::types::{MarkedMappingNode, MarkedScalarNode};

use crate::error::SchemaError;
use crate::raw::{
    RawAction, RawAgentAction, RawExecAction, RawInferAction, RawInvokeAction, Thinking,
    VisionInput,
};
use crate::source::Spanned;
use crate::types::CaptureMode;

use super::envelope::parse_string_map;
use super::value::node_to_json;
use super::{Cx, tasks::parse_string_list};

/// The 4 canonical verb keys — CLOSED set (D-2026-05-22-N18 · « 4 verbs
/// absolute » · `fetch` is the `nika:fetch` builtin under `invoke`).
pub(super) const VERB_KEYS: &[&str] = &["infer", "exec", "invoke", "agent"];

/// `infer:` fields (spec `02-verbs.md` §infer field table).
const INFER_KEYS: &[&str] = &[
    "prompt",
    "system",
    "model",
    "temperature",
    "max_tokens",
    "schema",
    "thinking",
    "vision",
];

/// `exec:` fields (spec `02-verbs.md` §exec field table).
const EXEC_KEYS: &[&str] = &["command", "cwd", "env", "stdin", "capture"];

/// `invoke:` fields (spec `02-verbs.md` §invoke field table).
const INVOKE_KEYS: &[&str] = &["tool", "args"];

/// `agent:` fields (spec `02-verbs.md` §agent field table).
const AGENT_KEYS: &[&str] = &[
    "prompt",
    "system",
    "model",
    "tools",
    "max_turns",
    "max_tokens_total",
    "temperature",
    "schema",
];

/// `thinking:` sub-fields (spec `02-verbs.md` · `{ enabled, budget_tokens }`).
const THINKING_KEYS: &[&str] = &["enabled", "budget_tokens"];

/// Locate the task's verb and parse its body.
///
/// Spec `02-verbs.md` · « A task **must** specify exactly one of these.
/// Multiple verbs on a single task is a validation error. »
pub(super) fn parse_verb(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
    task_label: &str,
) -> Result<RawAction, SchemaError> {
    let present: Vec<&str> = VERB_KEYS
        .iter()
        .copied()
        .filter(|verb| mapping.get_node(verb).is_some())
        .collect();

    let verb = match present.as_slice() {
        [one] => *one,
        [] => {
            return Err(SchemaError::MissingVerb {
                task: task_label.to_owned(),
                span: cx.span(mapping.span()),
            });
        }
        many => {
            return Err(SchemaError::MultipleVerbs {
                task: task_label.to_owned(),
                verbs: many.join(", "),
                span: cx.span(mapping.span()),
            });
        }
    };

    let body_node = mapping
        .get_node(verb)
        .ok_or_else(|| SchemaError::Validation {
            message: format!("verb `{verb}` vanished mid-parse"),
            span: cx.span(mapping.span()),
        })?;
    let Some(body) = body_node.as_mapping() else {
        return Err(SchemaError::Validation {
            message: format!("`{verb}:` body must be a YAML mapping"),
            span: cx.span(body_node.span()),
        });
    };

    match verb {
        "infer" => parse_infer_body(cx, body).map(|a| RawAction::Infer(Box::new(a))),
        "exec" => parse_exec_body(cx, body).map(RawAction::Exec),
        "invoke" => parse_invoke_body(cx, body).map(RawAction::Invoke),
        _ => parse_agent_body(cx, body).map(|a| RawAction::Agent(Box::new(a))),
    }
}

/// Parse `infer:` — single LLM call.
fn parse_infer_body(cx: &Cx<'_>, body: &MarkedMappingNode) -> Result<RawInferAction, SchemaError> {
    cx.check_unknown_keys(body, INFER_KEYS, "`infer:`")?;
    let prompt = cx.require_scalar(body, "prompt", "infer")?;
    let mut action = RawInferAction::new(prompt);
    action.system = cx.opt_scalar(body, "system")?;
    action.model = cx.opt_scalar(body, "model")?;
    action.temperature = parse_temperature(cx, body)?;
    action.max_tokens = parse_u32_field(cx, body, "max_tokens")?;
    action.schema = parse_schema(cx, body)?;
    action.thinking = parse_thinking(cx, body)?;
    action.vision = parse_vision(cx, body)?;
    Ok(action)
}

/// Parse `exec:` — shell command.
fn parse_exec_body(cx: &Cx<'_>, body: &MarkedMappingNode) -> Result<RawExecAction, SchemaError> {
    cx.check_unknown_keys(body, EXEC_KEYS, "`exec:`")?;
    let command = cx.require_scalar(body, "command", "exec")?;
    let mut action = RawExecAction::new(command);
    action.cwd = cx.opt_scalar(body, "cwd")?;
    if let Some(node) = body.get_node("env") {
        action.env = parse_string_map(cx, node, "env")?;
    }
    action.stdin = cx.opt_scalar(body, "stdin")?;
    action.capture = parse_capture(cx, body)?;
    Ok(action)
}

/// Parse `invoke:` — builtin / MCP tool call.
fn parse_invoke_body(
    cx: &Cx<'_>,
    body: &MarkedMappingNode,
) -> Result<RawInvokeAction, SchemaError> {
    cx.check_unknown_keys(body, INVOKE_KEYS, "`invoke:`")?;
    let tool = cx.require_scalar(body, "tool", "invoke")?;
    validate_tool_ref(&tool)?;
    let mut action = RawInvokeAction::new(tool);
    if let Some(node) = body.get_node("args") {
        if node.as_mapping().is_none() {
            return Err(SchemaError::Validation {
                message: "`invoke.args` must be a YAML mapping (tool arguments)".to_owned(),
                span: cx.span(node.span()),
            });
        }
        action.args = Some(Spanned::new(
            node_to_json(node),
            cx.span_or_zero(node.span()),
        ));
    }
    Ok(action)
}

/// Parse `agent:` — multi-turn agentic loop.
fn parse_agent_body(cx: &Cx<'_>, body: &MarkedMappingNode) -> Result<RawAgentAction, SchemaError> {
    cx.check_unknown_keys(body, AGENT_KEYS, "`agent:`")?;
    let prompt = cx.require_scalar(body, "prompt", "agent")?;
    let mut action = RawAgentAction::new(prompt);
    action.system = cx.opt_scalar(body, "system")?;
    action.model = cx.opt_scalar(body, "model")?;
    // `tools:` — whitelist globs · default-deny (« if absent the agent
    // gets NO tools · pure conversation · least-privilege »).
    action.tools = parse_string_list(cx, body, "tools")?;
    action.max_turns = parse_u32_field(cx, body, "max_turns")?;
    action.max_tokens_total = parse_u64_field(cx, body, "max_tokens_total")?;
    action.temperature = parse_temperature(cx, body)?;
    action.schema = parse_schema(cx, body)?;
    Ok(action)
}

// ── Field helpers ───────────────────────────────────────────────────

/// `temperature:` — number 0-2 (spec field tables).
fn parse_temperature(
    cx: &Cx<'_>,
    body: &MarkedMappingNode,
) -> Result<Option<Spanned<f64>>, SchemaError> {
    let Some(node) = body.get_node("temperature") else {
        return Ok(None);
    };
    let value = node
        .as_scalar()
        .and_then(MarkedScalarNode::as_f64)
        .ok_or_else(|| SchemaError::Validation {
            message: "`temperature` must be a number".to_owned(),
            span: cx.span(node.span()),
        })?;
    if !(0.0..=2.0).contains(&value) {
        return Err(SchemaError::Validation {
            message: format!("`temperature` must be in 0-2 (got {value})"),
            span: cx.span(node.span()),
        });
    }
    Ok(Some(Spanned::new(value, cx.span_or_zero(node.span()))))
}

/// An optional non-negative `u32` integer field.
fn parse_u32_field(
    cx: &Cx<'_>,
    body: &MarkedMappingNode,
    key: &str,
) -> Result<Option<Spanned<u32>>, SchemaError> {
    let Some(node) = body.get_node(key) else {
        return Ok(None);
    };
    let value = node
        .as_scalar()
        .and_then(MarkedScalarNode::as_u32)
        .ok_or_else(|| SchemaError::Validation {
            message: format!("`{key}` must be a non-negative integer"),
            span: cx.span(node.span()),
        })?;
    Ok(Some(Spanned::new(value, cx.span_or_zero(node.span()))))
}

/// An optional non-negative `u64` integer field.
fn parse_u64_field(
    cx: &Cx<'_>,
    body: &MarkedMappingNode,
    key: &str,
) -> Result<Option<Spanned<u64>>, SchemaError> {
    let Some(node) = body.get_node(key) else {
        return Ok(None);
    };
    let value = node
        .as_scalar()
        .and_then(MarkedScalarNode::as_u64)
        .ok_or_else(|| SchemaError::Validation {
            message: format!("`{key}` must be a non-negative integer"),
            span: cx.span(node.span()),
        })?;
    Ok(Some(Spanned::new(value, cx.span_or_zero(node.span()))))
}

/// `schema:` — a JSON Schema object (spec field tables · type `object`).
fn parse_schema(
    cx: &Cx<'_>,
    body: &MarkedMappingNode,
) -> Result<Option<Spanned<serde_json::Value>>, SchemaError> {
    let Some(node) = body.get_node("schema") else {
        return Ok(None);
    };
    if node.as_mapping().is_none() {
        return Err(SchemaError::Validation {
            message: "`schema` must be a JSON Schema object (YAML mapping)".to_owned(),
            span: cx.span(node.span()),
        });
    }
    Ok(Some(Spanned::new(
        node_to_json(node),
        cx.span_or_zero(node.span()),
    )))
}

/// `thinking:` — `{ enabled (required bool), budget_tokens }`.
fn parse_thinking(
    cx: &Cx<'_>,
    body: &MarkedMappingNode,
) -> Result<Option<Spanned<Thinking>>, SchemaError> {
    let Some(node) = body.get_node("thinking") else {
        return Ok(None);
    };
    let Some(thinking_map) = node.as_mapping() else {
        return Err(SchemaError::Validation {
            message: "`thinking` must be a mapping `{ enabled, budget_tokens }`".to_owned(),
            span: cx.span(node.span()),
        });
    };
    cx.check_unknown_keys(thinking_map, THINKING_KEYS, "`infer.thinking:`")?;

    let enabled_node =
        thinking_map
            .get_node("enabled")
            .ok_or_else(|| SchemaError::MissingField {
                field: "thinking.enabled".to_owned(),
                span: cx.span(thinking_map.span()),
            })?;
    let enabled = enabled_node
        .as_scalar()
        .and_then(MarkedScalarNode::as_bool)
        .ok_or_else(|| SchemaError::Validation {
            message: "`thinking.enabled` must be a boolean".to_owned(),
            span: cx.span(enabled_node.span()),
        })?;

    let mut thinking = Thinking::new(enabled);
    if let Some(budget) = parse_u32_field(cx, thinking_map, "budget_tokens")? {
        thinking.budget_tokens = Some(budget.value);
    }
    Ok(Some(Spanned::new(thinking, cx.span_or_zero(node.span()))))
}

/// `vision:` — array of image inputs · each `{ source: file|url, path|url }`.
fn parse_vision(
    cx: &Cx<'_>,
    body: &MarkedMappingNode,
) -> Result<Vec<Spanned<VisionInput>>, SchemaError> {
    let Some(node) = body.get_node("vision") else {
        return Ok(Vec::new());
    };
    let Some(seq) = node.as_sequence() else {
        return Err(SchemaError::Validation {
            message: "`vision` must be a YAML sequence of image inputs".to_owned(),
            span: cx.span(node.span()),
        });
    };
    let mut out = Vec::with_capacity(seq.len());
    for item in seq.iter() {
        let Some(input_map) = item.as_mapping() else {
            return Err(SchemaError::Validation {
                message: "each `vision` entry must be a mapping".to_owned(),
                span: cx.span(item.span()),
            });
        };
        out.push(Spanned::new(
            parse_vision_input(cx, input_map)?,
            cx.span_or_zero(item.span()),
        ));
    }
    Ok(out)
}

/// Parse one vision input by its `source:` discriminant.
fn parse_vision_input(
    cx: &Cx<'_>,
    input_map: &MarkedMappingNode,
) -> Result<VisionInput, SchemaError> {
    let source = cx.require_scalar(input_map, "source", "vision")?;
    match source.value.as_str() {
        "file" => {
            cx.check_unknown_keys(input_map, &["source", "path"], "`vision` (source: file)")?;
            let path = cx.require_scalar(input_map, "path", "vision")?;
            Ok(VisionInput::File { path })
        }
        "url" => {
            cx.check_unknown_keys(input_map, &["source", "url"], "`vision` (source: url)")?;
            let url = cx.require_scalar(input_map, "url", "vision")?;
            Ok(VisionInput::Url { url })
        }
        other => Err(SchemaError::Validation {
            message: format!("unknown vision source `{other}` (file·url)"),
            span: Some(source.span),
        }),
    }
}

/// `capture:` — closed enum `stdout` (default) · `stderr` · `combined` ·
/// `structured`.
fn parse_capture(
    cx: &Cx<'_>,
    body: &MarkedMappingNode,
) -> Result<Option<Spanned<CaptureMode>>, SchemaError> {
    let Some(node) = body.get_node("capture") else {
        return Ok(None);
    };
    let Some(scalar) = node.as_scalar() else {
        return Err(SchemaError::Validation {
            message: "`capture` must be a scalar".to_owned(),
            span: cx.span(node.span()),
        });
    };
    let mode =
        CaptureMode::from_str_opt(scalar.as_str()).ok_or_else(|| SchemaError::Validation {
            message: format!(
                "unknown capture mode `{}` (stdout·stderr·combined·structured)",
                scalar.as_str()
            ),
            span: cx.span(scalar.span()),
        })?;
    Ok(Some(Spanned::new(mode, cx.span_or_zero(scalar.span()))))
}

/// Validate the tool reference grammar (spec `02-verbs.md` §invoke ·
/// « the **colon** marks the namespace boundary (exactly once), the
/// **slash** separates the path within it » · `nika:<path>` OR
/// `mcp:<server>/<tool>`).
fn validate_tool_ref(tool: &Spanned<String>) -> Result<(), SchemaError> {
    let text = &tool.value;
    let mut parts = text.splitn(2, ':');
    let namespace = parts.next().unwrap_or_default();
    let Some(path) = parts.next() else {
        return Err(SchemaError::Validation {
            message: format!(
                "invalid tool reference `{text}` — expected `nika:<path>` or `mcp:<server>/<tool>`"
            ),
            span: Some(tool.span),
        });
    };
    if path.contains(':') {
        return Err(SchemaError::Validation {
            message: format!(
                "invalid tool reference `{text}` — the colon marks the namespace boundary exactly once"
            ),
            span: Some(tool.span),
        });
    }
    match namespace {
        "nika" if !path.is_empty() => Ok(()),
        "mcp" => {
            let mut mcp = path.splitn(2, '/');
            let server = mcp.next().unwrap_or_default();
            let tool_name = mcp.next().unwrap_or_default();
            if server.is_empty() || tool_name.is_empty() {
                return Err(SchemaError::Validation {
                    message: format!(
                        "invalid MCP tool reference `{text}` — expected `mcp:<server>/<tool>`"
                    ),
                    span: Some(tool.span),
                });
            }
            Ok(())
        }
        _ => Err(SchemaError::Validation {
            message: format!(
                "unknown tool namespace in `{text}` — v1 namespaces are `nika:` and `mcp:`"
            ),
            span: Some(tool.span),
        }),
    }
}

#[cfg(test)]
mod tests {
    use crate::error::SchemaError;
    use crate::parser::{ParseMode, parse};
    use crate::raw::{RawAction, VisionInput};
    use crate::source::FileId;
    use crate::types::CaptureMode;

    fn parse_strict(yaml: &str) -> Result<crate::raw::RawWorkflow, SchemaError> {
        parse(yaml, FileId::new(0), ParseMode::Strict)
    }

    fn one_action(yaml: &str) -> RawAction {
        parse_strict(yaml)
            .expect("parse")
            .tasks
            .remove(0)
            .value
            .action
    }

    // ── infer ───────────────────────────────────────────────────────

    #[test]
    fn infer_full_fields() {
        let yaml = "\
tasks:
  - id: analyze
    infer:
      system: \"You are a precise analyst.\"
      prompt: \"Analyze ${{ vars.topic }}\"
      model: anthropic/claude-sonnet-4-6
      temperature: 0.2
      max_tokens: 4000
      schema:
        type: object
        properties:
          sentiment: { type: string }
      thinking:
        enabled: true
        budget_tokens: 8000
";
        let RawAction::Infer(action) = one_action(yaml) else {
            panic!("expected Infer");
        };
        assert_eq!(action.prompt.value, "Analyze ${{ vars.topic }}");
        assert_eq!(
            action.system.expect("system").value,
            "You are a precise analyst."
        );
        assert_eq!(
            action.model.expect("model").value,
            "anthropic/claude-sonnet-4-6"
        );
        let temperature = action.temperature.expect("temperature").value;
        assert!((temperature - 0.2).abs() < f64::EPSILON);
        assert_eq!(action.max_tokens.expect("max_tokens").value, 4000);
        assert_eq!(action.schema.expect("schema").value["type"], "object");
        let thinking = action.thinking.expect("thinking").value;
        assert!(thinking.enabled);
        assert_eq!(thinking.budget_tokens, Some(8000));
    }

    #[test]
    fn infer_without_prompt_errors() {
        // Conformance fixture verbs-shape/006-infer-without-prompt.
        let yaml = "\
tasks:
  - id: t
    infer:
      system: \"You are helpful.\"
";
        let err = parse_strict(yaml).expect_err("no prompt");
        assert!(
            matches!(&err, SchemaError::MissingField { field, .. } if field == "infer.prompt"),
            "{err:?}"
        );
    }

    #[test]
    fn infer_temperature_out_of_range_errors() {
        let yaml = "\
tasks:
  - id: t
    infer:
      prompt: hi
      temperature: 3.5
";
        let err = parse_strict(yaml).expect_err("temp 3.5");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("0-2")),
            "{err:?}"
        );
    }

    #[test]
    fn infer_schema_must_be_object() {
        let yaml = "\
tasks:
  - id: t
    infer:
      prompt: hi
      schema: \"not-an-object\"
";
        let err = parse_strict(yaml).expect_err("scalar schema");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("JSON Schema")),
            "{err:?}"
        );
    }

    #[test]
    fn infer_thinking_requires_enabled() {
        let yaml = "\
tasks:
  - id: t
    infer:
      prompt: hi
      thinking:
        budget_tokens: 1000
";
        let err = parse_strict(yaml).expect_err("no enabled");
        assert!(
            matches!(&err, SchemaError::MissingField { field, .. } if field == "thinking.enabled"),
            "{err:?}"
        );
    }

    #[test]
    fn infer_vision_file_and_url() {
        let yaml = "\
tasks:
  - id: ocr
    infer:
      prompt: \"Read the text in these images\"
      vision:
        - source: file
          path: ./scan.png
        - source: url
          url: https://example.com/photo.jpg
";
        let RawAction::Infer(action) = one_action(yaml) else {
            panic!("expected Infer");
        };
        assert_eq!(action.vision.len(), 2);
        assert!(
            matches!(&action.vision[0].value, VisionInput::File { path } if path.value == "./scan.png")
        );
        assert!(
            matches!(&action.vision[1].value, VisionInput::Url { url } if url.value == "https://example.com/photo.jpg")
        );
    }

    #[test]
    fn infer_vision_unknown_source_errors() {
        let yaml = "\
tasks:
  - id: t
    infer:
      prompt: hi
      vision:
        - source: base64
          data: abcd
";
        let err = parse_strict(yaml).expect_err("base64 source");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("base64")),
            "{err:?}"
        );
    }

    #[test]
    fn infer_unknown_field_strict_errors() {
        // Conformance fixture verbs-shape/009 · 'temperture' typo.
        let yaml = "\
tasks:
  - id: t
    infer:
      prompt: hi
      temperture: 0.5
";
        let err = parse_strict(yaml).expect_err("typo field");
        assert!(
            matches!(&err, SchemaError::UnknownField { field, .. } if field == "temperture"),
            "{err:?}"
        );
    }

    #[test]
    fn infer_unknown_field_lenient_ignored() {
        let yaml = "\
tasks:
  - id: t
    infer:
      prompt: hi
      temperture: 0.5
";
        let wf = parse(yaml, FileId::new(0), ParseMode::Lenient).expect("lenient");
        assert_eq!(wf.tasks.len(), 1);
    }

    // ── exec ────────────────────────────────────────────────────────

    #[test]
    fn exec_full_fields() {
        let yaml = "\
tasks:
  - id: build
    exec:
      command: \"cargo build --release\"
      cwd: ./engine
      env:
        RUST_LOG: debug
      stdin: \"${{ vars.input }}\"
      capture: structured
";
        let RawAction::Exec(action) = one_action(yaml) else {
            panic!("expected Exec");
        };
        assert_eq!(action.command.value, "cargo build --release");
        assert_eq!(action.cwd.expect("cwd").value, "./engine");
        assert_eq!(action.env.len(), 1);
        assert_eq!(action.env[0].0.value, "RUST_LOG");
        assert_eq!(action.stdin.expect("stdin").value, "${{ vars.input }}");
        assert_eq!(
            action.capture.expect("capture").value,
            CaptureMode::Structured
        );
    }

    #[test]
    fn exec_without_command_errors() {
        // Conformance fixture verbs-shape/005-exec-without-command.
        let yaml = "\
tasks:
  - id: t
    exec:
      cwd: ./
";
        let err = parse_strict(yaml).expect_err("no command");
        assert!(
            matches!(&err, SchemaError::MissingField { field, .. } if field == "exec.command"),
            "{err:?}"
        );
    }

    #[test]
    fn exec_bad_capture_mode_errors() {
        let yaml = "\
tasks:
  - id: t
    exec:
      command: ls
      capture: everything
";
        let err = parse_strict(yaml).expect_err("bad capture");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("everything")),
            "{err:?}"
        );
    }

    // ── invoke ──────────────────────────────────────────────────────

    #[test]
    fn invoke_builtin_and_mcp() {
        let yaml = "\
tasks:
  - id: read_config
    invoke:
      tool: \"nika:read\"
      args:
        path: \"./config.yaml\"
  - id: query_db
    invoke:
      tool: \"mcp:postgres/query\"
      args:
        sql: \"SELECT 1\"
";
        let wf = parse_strict(yaml).expect("parse");
        let RawAction::Invoke(ref builtin) = wf.tasks[0].value.action else {
            panic!("expected Invoke");
        };
        assert_eq!(builtin.tool.value, "nika:read");
        assert_eq!(
            builtin.args.as_ref().expect("args").value["path"],
            "./config.yaml"
        );
        let RawAction::Invoke(ref mcp) = wf.tasks[1].value.action else {
            panic!("expected Invoke");
        };
        assert_eq!(mcp.tool.value, "mcp:postgres/query");
    }

    #[test]
    fn invoke_without_tool_errors() {
        // Conformance fixture verbs-shape/007-invoke-without-tool.
        let yaml = "\
tasks:
  - id: t
    invoke:
      args: { x: 1 }
";
        let err = parse_strict(yaml).expect_err("no tool");
        assert!(
            matches!(&err, SchemaError::MissingField { field, .. } if field == "invoke.tool"),
            "{err:?}"
        );
    }

    #[test]
    fn invoke_bad_tool_refs_error() {
        for bad in [
            "fetch",        // no namespace colon
            "nika:",        // empty path
            "mcp:postgres", // mcp without /tool
            "mcp:/query",   // empty server
            "custom:thing", // unknown namespace
            "nika:a:b",     // two colons
        ] {
            let yaml = format!("tasks:\n  - id: t\n    invoke:\n      tool: \"{bad}\"\n");
            let err = parse_strict(&yaml).expect_err("bad tool ref");
            assert!(
                matches!(err, SchemaError::Validation { .. }),
                "{bad} → {err:?}"
            );
        }
    }

    #[test]
    fn invoke_args_must_be_mapping() {
        let yaml = "\
tasks:
  - id: t
    invoke:
      tool: \"nika:read\"
      args: just-a-string
";
        let err = parse_strict(yaml).expect_err("scalar args");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("args")),
            "{err:?}"
        );
    }

    // ── agent ───────────────────────────────────────────────────────

    #[test]
    fn agent_full_fields() {
        let yaml = "\
tasks:
  - id: research
    agent:
      system: \"You are a research assistant.\"
      prompt: \"Research ${{ vars.topic }}\"
      model: anthropic/claude-sonnet-4-6
      tools:
        - \"nika:fetch\"
        - \"mcp:browser/*\"
      max_turns: 20
      max_tokens_total: 100000
      temperature: 0.7
      schema:
        type: object
";
        let RawAction::Agent(action) = one_action(yaml) else {
            panic!("expected Agent");
        };
        assert_eq!(action.prompt.value, "Research ${{ vars.topic }}");
        assert_eq!(action.tools.len(), 2);
        assert_eq!(action.tools[1].value, "mcp:browser/*");
        assert_eq!(action.max_turns.expect("max_turns").value, 20);
        assert_eq!(
            action.max_tokens_total.expect("max_tokens_total").value,
            100_000
        );
        assert!(action.schema.is_some(), "agent: MAY declare schema:");
    }

    #[test]
    fn agent_without_prompt_errors() {
        // Conformance fixture verbs-shape/008-agent-without-prompt.
        let yaml = "\
tasks:
  - id: t
    agent:
      system: \"You are helpful.\"
";
        let err = parse_strict(yaml).expect_err("no prompt");
        assert!(
            matches!(&err, SchemaError::MissingField { field, .. } if field == "agent.prompt"),
            "{err:?}"
        );
    }

    #[test]
    fn agent_tools_absent_is_default_deny() {
        let yaml = "\
tasks:
  - id: chat
    agent:
      prompt: \"Just talk\"
";
        let RawAction::Agent(action) = one_action(yaml) else {
            panic!("expected Agent");
        };
        assert!(action.tools.is_empty(), "no tools → pure conversation");
    }

    #[test]
    fn agent_with_schema_valid() {
        // Conformance fixture verbs-shape/003 · D-2026-05-23-N1 ·
        // agent: MAY declare schema: (validates the final message).
        let yaml = "\
tasks:
  - id: extract
    agent:
      prompt: \"Extract the facts\"
      schema:
        type: object
        properties:
          facts: { type: array }
";
        let RawAction::Agent(action) = one_action(yaml) else {
            panic!("expected Agent");
        };
        assert_eq!(action.schema.expect("schema").value["type"], "object");
    }

    #[test]
    fn verb_body_must_be_mapping() {
        let yaml = "\
tasks:
  - id: t
    infer: just-a-prompt
";
        let err = parse_strict(yaml).expect_err("scalar body");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("mapping")),
            "{err:?}"
        );
    }
}
