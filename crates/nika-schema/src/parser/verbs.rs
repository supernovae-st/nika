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
    RawAction, RawAgentAction, RawCommand, RawExecAction, RawInferAction, RawInvokeAction,
    Thinking, VisionInput,
};
use crate::source::Spanned;
use crate::types::{CaptureMode, DecodeMode};

use super::envelope::parse_string_map;
use super::value::json_value;
use super::{Cx, tasks::parse_string_list};

/// The 4 canonical verb keys — CLOSED set (D-2026-05-22-N18 · « 4 verbs
/// absolute » · `fetch` is the `nika:fetch` builtin under `invoke`).
pub(super) const VERB_KEYS: &[&str] = &["infer", "exec", "invoke", "agent"];

/// `infer:` fields (spec `02-verbs.md` §infer field table).
pub(crate) const INFER_KEYS: &[&str] = &[
    "prompt",
    "system",
    "model",
    "temperature",
    "max_tokens",
    "schema",
    "thinking",
    "vision",
];

/// `exec:` fields (spec `02-verbs.md` §exec field table + `decode:` ·
/// spec 09 §decode).
pub(crate) const EXEC_KEYS: &[&str] = &[
    "command", "shell", "cwd", "env", "stdin", "capture", "decode",
];

/// `invoke:` fields (spec `02-verbs.md` §invoke field table + spec
/// `14-composition.md` §the form — `tool:` XOR `workflow:`).
pub(crate) const INVOKE_KEYS: &[&str] = &["tool", "workflow", "args"];

/// `agent:` fields (spec `02-verbs.md` §agent field table).
pub(crate) const AGENT_KEYS: &[&str] = &[
    "prompt",
    "system",
    "model",
    "tools",
    "skills",
    "max_turns",
    "max_tokens_total",
    "temperature",
    "schema",
];

/// `thinking:` sub-fields (spec `02-verbs.md` · `{ enabled, budget_tokens }`).
pub(crate) const THINKING_KEYS: &[&str] = &["enabled", "budget_tokens"];

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
    let command = parse_command(cx, body)?;
    let mut action = RawExecAction::with_command(command);
    action.cwd = cx.opt_scalar(body, "cwd")?;
    if let Some(node) = body.get_node("env") {
        action.env = parse_string_map(cx, node, "env")?;
    }
    action.stdin = cx.opt_scalar(body, "stdin")?;
    action.capture = parse_capture(cx, body)?;
    action.decode = parse_decode(cx, body)?;
    Ok(action)
}

/// Parse the exec body — `command:` (argv) XOR `shell:` (one /bin/sh line).
///
/// Spec `02-verbs.md` §exec (0.103 · #75 D1) · semantics never fork on a
/// YAML type: `command:` is argv-only (`execve` · injection-safe by
/// construction) and `shell:` is the EXPLICIT shell door (pipes · redirects
/// · the blocklist). Exactly one of the two; a string `command:` is the
/// pre-0.103 implicit-shell form and is rejected with the migration
/// teaching. An empty argv has no program to run → a parse error.
fn parse_command(cx: &Cx<'_>, body: &MarkedMappingNode) -> Result<RawCommand, SchemaError> {
    let cmd = body.get_node("command");
    let shell = body.get_node("shell");
    if cmd.is_some() && shell.is_some() {
        return Err(SchemaError::Validation {
            message: "`exec:` takes exactly one of `command:` (argv) | `shell:` (one \
                      /bin/sh line) — two bodies is two meanings (02 §exec)"
                .to_owned(),
            span: cx.span(body.span()),
        });
    }
    if let Some(_node) = shell {
        // The explicit shell door — one scalar line through `/bin/sh -c`.
        return Ok(RawCommand::Shell(cx.require_scalar(body, "shell", "exec")?));
    }
    let Some(node) = cmd else {
        return Err(SchemaError::MissingField {
            field: "exec.command (argv) | exec.shell".to_owned(),
            span: cx.span(body.span()),
        });
    };
    let Some(seq) = node.as_sequence() else {
        // The pre-0.103 implicit-shell string — the typed dead form so
        // `check --fix` matches it (the message lives on the variant).
        return Err(SchemaError::D1StringCommand {
            span: cx.span(node.span()),
        });
    };
    if seq.is_empty() {
        return Err(SchemaError::Validation {
            message: "`exec.command` array must not be empty (no program to run)".to_owned(),
            span: cx.span(node.span()),
        });
    }
    let mut parts = Vec::with_capacity(seq.len());
    for item in seq.iter() {
        let scalar = item.as_scalar().ok_or_else(|| SchemaError::Validation {
            message: "each `exec.command` array element must be a string".to_owned(),
            span: cx.span(item.span()),
        })?;
        parts.push(Spanned::new(
            scalar.as_str().to_owned(),
            cx.span_or_zero(scalar.span()),
        ));
    }
    Ok(RawCommand::Argv(parts))
}

/// Parse `invoke:` — builtin / MCP tool call OR child-workflow call.
///
/// The target is a TAGGED UNION (spec `14-composition.md` §the form):
/// exactly one of `tool:` | `workflow:`. Both present, or neither, is a
/// validation error — the same shape as two verbs on one task.
fn parse_invoke_body(
    cx: &Cx<'_>,
    body: &MarkedMappingNode,
) -> Result<RawInvokeAction, SchemaError> {
    cx.check_unknown_keys(body, INVOKE_KEYS, "`invoke:`")?;
    if body.get_node("tool").is_some() && body.get_node("workflow").is_some() {
        return Err(SchemaError::Validation {
            message: "`invoke:` takes exactly one of `tool:` | `workflow:` — two targets \
                      is two meanings (14 §the form · the same law as two verbs on one \
                      task)"
                .to_owned(),
            span: cx.span(body.span()),
        });
    }
    let mut action = if body.get_node("workflow").is_some() {
        let target = cx.require_scalar(body, "workflow", "invoke")?;
        if target.value.trim().is_empty() {
            return Err(SchemaError::Validation {
                message: "`invoke.workflow` must not be empty — a filesystem path or a \
                          pinned `registry:owner/name@version` (14 §the form)"
                    .to_owned(),
                span: Some(target.span),
            });
        }
        RawInvokeAction::workflow_call(target)
    } else {
        let Some(_) = body.get_node("tool") else {
            return Err(SchemaError::MissingField {
                field: "invoke.tool | invoke.workflow".to_owned(),
                span: cx.span(body.span()),
            });
        };
        let tool = cx.require_scalar(body, "tool", "invoke")?;
        validate_tool_ref(&tool)?;
        RawInvokeAction::new(tool)
    };
    if let Some(node) = body.get_node("args") {
        if node.as_mapping().is_none() {
            return Err(SchemaError::Validation {
                message: "`invoke.args` must be a YAML mapping (tool arguments)".to_owned(),
                span: cx.span(node.span()),
            });
        }
        action.args = Some(Spanned::new(
            json_value(cx, node)?,
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
    for tool in &action.tools {
        validate_whitelist_namespace(tool)?;
    }
    // `skills:` — Agent Skill (SKILL.md) paths · EXPLICIT static paths
    // only (the permits explicitness law): the composer loads them
    // BEFORE the run, so a `${{ }}` path would defeat audit-before-run.
    action.skills = parse_string_list(cx, body, "skills")?;
    for skill in &action.skills {
        validate_skill_path(skill)?;
    }
    action.max_turns = parse_max_turns(cx, body)?;
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

/// `max_turns` — the agent loop bound. A LITERAL integer only (templates
/// are rejected by the u32 parse), so its range is statically knowable:
/// the runtime refuses anything outside 1..=1000 (NIKA-465), and
/// `nika check` must catch that BEFORE the run — a `max_turns: 0` that
/// audits clean then dies at dispatch breaks audit-before-run.
///
/// The ceiling MUST equal `nika_verb_agent::MAX_TURNS_CEILING` (this
/// crate cannot import the runtime one — dependency direction); the
/// runtime stays the enforcer, this is the early mirror.
const MAX_TURNS_CEILING: u32 = 1000;

fn parse_max_turns(
    cx: &Cx<'_>,
    body: &MarkedMappingNode,
) -> Result<Option<Spanned<u32>>, SchemaError> {
    let Some(node) = body.get_node("max_turns") else {
        return Ok(None);
    };
    let value = node
        .as_scalar()
        .and_then(MarkedScalarNode::as_u32)
        .ok_or_else(|| SchemaError::Validation {
            message: "`max_turns` must be a non-negative integer".to_owned(),
            span: cx.span(node.span()),
        })?;
    if !(1..=MAX_TURNS_CEILING).contains(&value) {
        return Err(SchemaError::Validation {
            message: format!("`max_turns` must be 1-{MAX_TURNS_CEILING} (got {value})"),
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
        json_value(cx, node)?,
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

/// `decode:` — closed enum `text` (default) · `json` · `jsonl` · `bytes`
/// (spec 09 §decode · `artifact-ref` reserved for the artifact lanes).
fn parse_decode(
    cx: &Cx<'_>,
    body: &MarkedMappingNode,
) -> Result<Option<Spanned<DecodeMode>>, SchemaError> {
    let Some(node) = body.get_node("decode") else {
        return Ok(None);
    };
    let Some(scalar) = node.as_scalar() else {
        return Err(SchemaError::Validation {
            message: "`decode` must be a scalar".to_owned(),
            span: cx.span(node.span()),
        });
    };
    let mode =
        DecodeMode::from_str_opt(scalar.as_str()).ok_or_else(|| SchemaError::Validation {
            message: format!(
                "unknown decode mode `{}` (text·json·jsonl·bytes)",
                scalar.as_str()
            ),
            span: cx.span(scalar.span()),
        })?;
    Ok(Some(Spanned::new(mode, cx.span_or_zero(scalar.span()))))
}

/// Validate ONE agent `tools:` whitelist glob's namespace (spec
/// `02-verbs.md` §agent · the namespace set is CLOSED at `{nika:, mcp:}`).
///
/// The entries are globs (`nika:*` · `mcp:browser/*`), optionally negated
/// (`!mcp:x`); a colon-less pattern (`*` · `**`) names no namespace and is
/// legal (it matches nothing namespaced · least-privilege). But a glob
/// that DOES name a namespace must name a v1 one — this is what catches a
/// `agent:compose` left over from before `nika:compose` (the compose
/// intrinsic moved into the closed `nika:` set · ADR-096).
fn validate_whitelist_namespace(tool: &Spanned<String>) -> Result<(), SchemaError> {
    // This function used to claim « parity with validate_tool_ref
    // (invoke) » in a comment. It was NOT true — it refused a second
    // colon, invoke allowed one. A hand-maintained mirror that asserts a
    // parity it does not have is worse than no claim, so the parity is
    // now structural: both call the same L0 grammar.
    nika_vocab::tool_ref::glob_namespace(&tool.value)
        .map(|_| ())
        .map_err(|defect| SchemaError::Validation {
            message: match defect {
                nika_vocab::tool_ref::ToolRefDefect::UnknownNamespace => format!(
                    "unknown tool namespace in agent whitelist `{}` — {} (the compose \
                     intrinsic is `nika:compose`)",
                    tool.value,
                    defect.teaching()
                ),
                _ => format!(
                    "invalid agent whitelist glob `{}` — {}",
                    tool.value,
                    defect.teaching()
                ),
            },
            span: Some(tool.span),
        })
}

/// Validate ONE agent `skills:` entry (spec `02-verbs.md` §agent skills ·
/// « explicit file paths · no globs · no templates »).
///
/// A skill file is loaded at COMPOSE time — before any scope exists — so
/// a `${{ }}` path could never resolve (and would defeat audit-before-run:
/// the checker must be able to read exactly what the agent will carry).
/// Same explicitness law as `permits:`.
fn validate_skill_path(skill: &Spanned<String>) -> Result<(), SchemaError> {
    if skill.value.trim().is_empty() {
        return Err(SchemaError::Validation {
            message: "`skills` entries must be non-empty file paths (a SKILL.md per entry)"
                .to_owned(),
            span: Some(skill.span),
        });
    }
    if skill.value.contains("${{") {
        return Err(SchemaError::SkillPathNotStatic {
            path: skill.value.clone(),
            why: "carries a `${{ }}` template",
            span: Some(skill.span),
        });
    }
    if skill.value.contains(['*', '?', '[']) {
        return Err(SchemaError::SkillPathNotStatic {
            path: skill.value.clone(),
            why: "is a glob",
            span: Some(skill.span),
        });
    }
    Ok(())
}

/// Validate the tool reference grammar (spec `02-verbs.md` §invoke ·
/// « the **colon** marks the namespace boundary (exactly once), the
/// **slash** separates the path within it » · `nika:<path>` OR
/// `mcp:<server>/<tool>`).
fn validate_tool_ref(tool: &Spanned<String>) -> Result<(), SchemaError> {
    nika_vocab::tool_ref::parse(&tool.value)
        .map(|_| ())
        .map_err(|defect| SchemaError::Validation {
            message: tool_ref_message(&tool.value, defect),
            span: Some(tool.span),
        })
}

/// The refusal sentence, in the ONE voice `nika-vocab` holds.
///
/// The `unknown tool namespace` opening is preserved verbatim — a test
/// pins that phrase, and an author who searched for it once should find
/// it again.
fn tool_ref_message(text: &str, defect: nika_vocab::tool_ref::ToolRefDefect) -> String {
    use nika_vocab::tool_ref::ToolRefDefect;
    let teaching = defect.teaching();
    match defect {
        ToolRefDefect::UnknownNamespace => {
            format!("unknown tool namespace in `{text}` — {teaching}")
        }
        _ => format!("invalid tool reference `{text}` — {teaching}"),
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

    // ── agent max_turns range (audit-before-run · NIKA-465 mirror) ──

    fn agent_wf(max_turns: &str) -> String {
        format!(
            "tasks:\n  go:\n    agent:\n      system: \"do\"\n      \
             prompt: \"task\"\n      tools: [\"nika:done\"]\n      \
             max_turns: {max_turns}\n      max_tokens_total: 1000\n"
        )
    }

    #[test]
    fn max_turns_range_is_caught_at_parse_not_deferred_to_runtime() {
        // The runtime refuses max_turns outside 1..=1000 (NIKA-465); the
        // static check must catch a LITERAL out-of-range value BEFORE the
        // run, or a `max_turns: 0` audits clean then dies at dispatch.
        for ok in ["1", "3", "1000"] {
            assert!(
                parse_strict(&agent_wf(ok)).is_ok(),
                "max_turns {ok} is valid"
            );
        }
        for bad in ["0", "1001", "99999"] {
            let err = parse_strict(&agent_wf(bad)).expect_err("out of range");
            let msg = format!("{err:?}");
            assert!(
                msg.contains("max_turns") && msg.contains("1-1000"),
                "{bad}: {msg}"
            );
        }
    }

    // ── infer ───────────────────────────────────────────────────────

    #[test]
    fn infer_full_fields() {
        let yaml = "\
tasks:
  analyze:
    infer:
      system: \"You are a precise analyst.\"
      prompt: \"Analyze ${{ inputs.topic }}\"
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
        assert_eq!(action.prompt.value, "Analyze ${{ inputs.topic }}");
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
  t:
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
  t:
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
  t:
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
  t:
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
  ocr:
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
  t:
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
  t:
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
  t:
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
  build:
    exec:
      command: [\"cargo\", \"build\", \"--release\"]
      cwd: ./engine
      env:
        RUST_LOG: debug
      stdin: \"${{ inputs.input }}\"
      capture: structured
";
        let RawAction::Exec(action) = one_action(yaml) else {
            panic!("expected Exec");
        };
        assert_eq!(action.command.argv_program(), Some("cargo"));
        assert!(action.command.shell_str().is_none(), "argv, not shell");
        assert_eq!(action.cwd.expect("cwd").value, "./engine");
        assert_eq!(action.env.len(), 1);
        assert_eq!(action.env[0].0.value, "RUST_LOG");
        assert_eq!(action.stdin.expect("stdin").value, "${{ inputs.input }}");
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
  t:
    exec:
      cwd: ./
";
        let err = parse_strict(yaml).expect_err("no command");
        assert!(
            matches!(&err, SchemaError::MissingField { field, .. }
                if field == "exec.command (argv) | exec.shell"),
            "{err:?}"
        );
    }

    #[test]
    fn exec_bad_capture_mode_errors() {
        let yaml = "\
tasks:
  t:
    exec:
      command: [ls]
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
  read_config:
    invoke:
      tool: \"nika:read\"
      args:
        path: \"./config.yaml\"
  query_db:
    invoke:
      tool: \"mcp:postgres/query\"
      args:
        sql: \"SELECT 1\"
";
        let wf = parse_strict(yaml).expect("parse");
        let RawAction::Invoke(ref builtin) = wf.tasks[0].value.action else {
            panic!("expected Invoke");
        };
        assert_eq!(builtin.tool().expect("tool target").value, "nika:read");
        assert_eq!(
            builtin.args.as_ref().expect("args").value["path"],
            "./config.yaml"
        );
        let RawAction::Invoke(ref mcp) = wf.tasks[1].value.action else {
            panic!("expected Invoke");
        };
        assert_eq!(mcp.tool().expect("tool target").value, "mcp:postgres/query");
    }

    #[test]
    fn invoke_without_tool_errors() {
        // Conformance fixture verbs-shape/007-invoke-without-tool.
        let yaml = "\
tasks:
  t:
    invoke:
      args: { x: 1 }
";
        let err = parse_strict(yaml).expect_err("no tool");
        assert!(
            matches!(&err, SchemaError::MissingField { field, .. }
                if field == "invoke.tool | invoke.workflow"),
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
            let yaml = format!("tasks:\n  t:\n    invoke:\n      tool: \"{bad}\"\n");
            let err = parse_strict(&yaml).expect_err("bad tool ref");
            assert!(
                matches!(err, SchemaError::Validation { .. }),
                "{bad} → {err:?}"
            );
        }
    }

    /// ⭐ The id the CHECKER used to wave through and the RUNTIME then
    /// refused. Measured 2026-08-15 on the tree: `nika:x\u{7f}y` PASSED
    /// here and was rejected at the verb boundary.
    ///
    /// That is the dangerous direction — a control character in a
    /// forwarded tool name is a log-injection vector (it rides into the
    /// `ToolCall`, and from there into the log and event fields), and the
    /// author learned nothing at the one moment they were still reading.
    /// One grammar now, at L0, so both readers refuse it in the same
    /// words.
    #[test]
    fn a_control_character_in_a_tool_id_is_refused_at_check_time() {
        for (bad, why) in [
            ("nika:x\u{7f}y", "DEL"),
            ("nika:x\u{1}y", "SOH"),
            ("mcp:srv/a\nb", "newline"),
            (" nika:read", "leading space"),
            ("nika:read ", "trailing space"),
        ] {
            let yaml = format!("tasks:\n  t:\n    invoke:\n      tool: \"{bad}\"\n");
            assert!(
                parse_strict(&yaml).is_err(),
                "`{}` must refuse — {why}",
                bad.escape_debug()
            );
        }
        // A space in the MIDDLE is not a control character, and the two
        // readers always agreed it rides. Pinned so the tightening above
        // is not read as « refuse anything unusual ».
        let ok = "tasks:\n  t:\n    invoke:\n      tool: \"nika:re ad\"\n";
        assert!(parse_strict(ok).is_ok(), "an interior space still rides");
    }

    #[test]
    fn invoke_args_must_be_mapping() {
        let yaml = "\
tasks:
  t:
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
    fn agent_whitelist_rejects_an_unknown_namespace() {
        // The closed-namespace gate (ADR-096 · the compose move): a third
        // namespace fails at parse with a pointer to nika:compose.
        let yaml = "nika: w\ntasks:\n  t:\n    agent:\n      prompt: \"go\"\n      tools: [\"agent:compose\"]\n";
        let err = parse(yaml, FileId::new(0), ParseMode::Strict).expect_err("rejected");
        assert!(
            err.to_string().contains("unknown tool namespace")
                && err.to_string().contains("nika:compose"),
            "{err}"
        );
        // nika: + mcp: globs (incl. negation) + a bare `*` stay legal.
        let ok = "nika: w\ntasks:\n  t:\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:*\", \"mcp:srv/*\", \"!mcp:srv/danger\", \"nika:compose\", \"*\"]\n";
        assert!(parse(ok, FileId::new(0), ParseMode::Strict).is_ok());
        // A SECOND colon is malformed even in a glob — parity with the
        // invoke `validate_tool_ref` boundary rule.
        let two_colons = "nika: w\ntasks:\n  t:\n    agent:\n      prompt: \"go\"\n      tools: [\"nika:read:typo\"]\n";
        let err = parse(two_colons, FileId::new(0), ParseMode::Strict).expect_err("rejected");
        assert!(err.to_string().contains("exactly once"), "{err}");
        // Uppercase namespace is not a v1 namespace.
        let upper = "nika: w\ntasks:\n  t:\n    agent:\n      prompt: \"go\"\n      tools: [\"NIKA:read\"]\n";
        assert!(parse(upper, FileId::new(0), ParseMode::Strict).is_err());
    }

    #[test]
    fn agent_full_fields() {
        let yaml = "\
tasks:
  research:
    agent:
      system: \"You are a research assistant.\"
      prompt: \"Research ${{ inputs.topic }}\"
      model: anthropic/claude-sonnet-4-6
      tools:
        - \"nika:fetch\"
        - \"mcp:browser/*\"
      skills:
        - \".agents/skills/nika-authoring/SKILL.md\"
      max_turns: 20
      max_tokens_total: 100000
      temperature: 0.7
      schema:
        type: object
";
        let RawAction::Agent(action) = one_action(yaml) else {
            panic!("expected Agent");
        };
        assert_eq!(action.prompt.value, "Research ${{ inputs.topic }}");
        assert_eq!(action.tools.len(), 2);
        assert_eq!(action.tools[1].value, "mcp:browser/*");
        assert_eq!(action.skills.len(), 1, "agent: MAY declare skills:");
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
  t:
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
  chat:
    agent:
      prompt: \"Just talk\"
";
        let RawAction::Agent(action) = one_action(yaml) else {
            panic!("expected Agent");
        };
        assert!(action.tools.is_empty(), "no tools → pure conversation");
    }

    #[test]
    fn agent_skills_list_parses_in_source_order() {
        let yaml = "\
tasks:
  research:
    agent:
      prompt: \"go\"
      skills:
        - \".agents/skills/nika-authoring/SKILL.md\"
        - \"docs/skills/review/SKILL.md\"
";
        let RawAction::Agent(action) = one_action(yaml) else {
            panic!("expected Agent");
        };
        assert_eq!(action.skills.len(), 2);
        assert_eq!(
            action.skills[0].value,
            ".agents/skills/nika-authoring/SKILL.md"
        );
        assert_eq!(action.skills[1].value, "docs/skills/review/SKILL.md");
    }

    #[test]
    fn agent_skills_absent_means_none() {
        let yaml = "\
tasks:
  chat:
    agent:
      prompt: \"Just talk\"
";
        let RawAction::Agent(action) = one_action(yaml) else {
            panic!("expected Agent");
        };
        assert!(action.skills.is_empty(), "skills are opt-in");
    }

    #[test]
    fn agent_skills_must_be_a_list_of_strings() {
        // A scalar where the sequence goes — the parse_string_list contract.
        let scalar = "\
tasks:
  t:
    agent:
      prompt: \"go\"
      skills: \"SKILL.md\"
";
        let err = parse_strict(scalar).expect_err("scalar skills");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("sequence")),
            "{err:?}"
        );
        // A non-string entry.
        let mapping_entry = "\
tasks:
  t:
    agent:
      prompt: \"go\"
      skills:
        - path: \"SKILL.md\"
";
        let err = parse_strict(mapping_entry).expect_err("mapping entry");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("string")),
            "{err:?}"
        );
    }

    #[test]
    fn agent_skills_reject_templates_and_empties() {
        // `${{ }}` in a skill path defeats audit-before-run — refused at
        // parse (the permits explicitness law).
        let templated = "\
tasks:
  t:
    agent:
      prompt: \"go\"
      skills: [\"${{ inputs.skill }}\"]
";
        let err = parse_strict(templated).expect_err("templated path");
        assert!(
            matches!(&err, SchemaError::SkillPathNotStatic { .. }),
            "{err:?}"
        );
        assert_eq!(err.spec_code().to_string(), "NIKA-AGENT-003");
        assert!(err.to_string().contains("static"), "{err}");
        let globbed = "\
tasks:
  t:
    agent:
      prompt: \"go\"
      skills: [\"skills/*/SKILL.md\"]
";
        let err = parse_strict(globbed).expect_err("glob path");
        assert!(
            matches!(&err, SchemaError::SkillPathNotStatic { why, .. } if *why == "is a glob"),
            "{err:?}"
        );
        assert_eq!(err.spec_code().to_string(), "NIKA-AGENT-003");
        let empty = "\
tasks:
  t:
    agent:
      prompt: \"go\"
      skills: [\"\"]
";
        let err = parse_strict(empty).expect_err("empty path");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("non-empty")),
            "{err:?}"
        );
    }

    #[test]
    fn agent_unknown_sibling_field_still_refused() {
        // The field table stays CLOSED — adding `skills` must not loosen
        // strict mode for anything else (`skils` is the typo class).
        let yaml = "\
tasks:
  t:
    agent:
      prompt: \"go\"
      skils: [\"SKILL.md\"]
";
        assert!(
            parse_strict(yaml).is_err(),
            "unknown `skils:` key must be refused in strict mode"
        );
    }

    #[test]
    fn agent_with_schema_valid() {
        // Conformance fixture verbs-shape/003 · D-2026-05-23-N1 ·
        // agent: MAY declare schema: (validates the final message).
        let yaml = "\
tasks:
  extract:
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
  t:
    infer: just-a-prompt
";
        let err = parse_strict(yaml).expect_err("scalar body");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("mapping")),
            "{err:?}"
        );
    }
}

#[cfg(test)]
mod argv_command {
    use crate::parser::{ParseMode, parse};
    use crate::raw::{RawAction, RawCommand};
    use crate::source::FileId;

    fn exec_command(yaml: &str) -> RawCommand {
        let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("parse");
        let RawAction::Exec(a) = &wf.tasks[0].value.action else {
            panic!("expected exec");
        };
        a.command.clone()
    }

    #[test]
    fn scalar_command_is_shell() {
        let c = exec_command("nika: w\ntasks:\n  t:\n    exec: { shell: \"a | b\" }\n");
        assert!(matches!(c, RawCommand::Shell(_)));
        assert_eq!(c.shell_str(), Some("a | b"));
        assert_eq!(c.argv_program(), None);
    }

    #[test]
    fn array_command_is_argv() {
        let c =
            exec_command("nika: w\ntasks:\n  t:\n    exec: { command: [\"git\", \"status\"] }\n");
        assert!(matches!(c, RawCommand::Argv(_)));
        assert_eq!(c.argv_program(), Some("git"), "argv[0] is the program");
        assert_eq!(c.shell_str(), None, "argv has no shell string");
        assert_eq!(c.text_fragments(), vec!["git", "status"]);
    }

    #[test]
    fn empty_array_command_is_rejected() {
        let err = parse(
            "nika: w\ntasks:\n  t:\n    exec: { command: [] }\n",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect_err("empty argv rejected");
        assert!(err.to_string().contains("must not be empty"), "{err}");
    }

    #[test]
    fn non_string_array_element_is_rejected() {
        // A nested mapping is not a scalar (a bare number `42` is a YAML
        // scalar string `"42"` and is fine — argv elements are strings).
        let err = parse(
            "nika: w\ntasks:\n  t:\n    exec: { command: [\"git\", { a: 1 }] }\n",
            FileId::new(0),
            ParseMode::Strict,
        )
        .expect_err("non-scalar element rejected");
        assert!(err.to_string().contains("must be a string"), "{err}");
    }
}
