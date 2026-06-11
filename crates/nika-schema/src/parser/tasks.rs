// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task-list parsing — YAML `tasks:` sequence → `Vec<Spanned<RawTask>>`.
//!
//! The canonical v1 task field set is CLOSED (spec `03-dag.md`
//! §forward-compat) · `id` · `depends_on` · `when` · `for_each` ·
//! `max_parallel` · `fail_fast` · `retry` · `on_error` · `timeout` ·
//! `on_finally` · `with` · `output` · plus exactly one verb key.

use std::time::Duration;

use marked_yaml::types::MarkedMappingNode;

use crate::error::SchemaError;
use crate::raw::{ForEachValue, RawFinallyTask, RawTask};
use crate::source::Spanned;
use crate::types::{
    BackoffStrategy, OnError, OnErrorAction, RetryConfig, WhenGate, is_valid_error_code,
    parse_go_duration,
};

use super::value::json_value;
use super::verbs::{VERB_KEYS, parse_verb};
use super::{Cx, validate_task_id};

/// The canonical task-level keys (verbs handled separately).
const TASK_KEYS: &[&str] = &[
    "id",
    "depends_on",
    "when",
    "for_each",
    "max_parallel",
    "fail_fast",
    "retry",
    "on_error",
    "timeout",
    "with",
    "output",
    "on_finally",
];

/// Keys allowed on an `on_finally:` mini-task (spec 03 §`on_finally` ·
/// `when` + per-cleanup `timeout` + the verb).
const FINALLY_KEYS: &[&str] = &["when", "timeout"];

/// Keys of a `retry:` block (spec 05 §retry).
const RETRY_KEYS: &[&str] = &[
    "max_attempts",
    "backoff_ms",
    "backoff_strategy",
    "backoff_max_ms",
    "jitter",
    "on_codes",
];

/// Keys of an `on_error:` block (spec 05 §`on_error` · exactly one
/// ACTION + the optional `on_codes` filter).
const ON_ERROR_KEYS: &[&str] = &["recover", "skip", "fail_workflow", "on_codes"];

/// The `on_error:` ACTION keys (mutually exclusive · exactly one).
const ON_ERROR_ACTION_KEYS: &[&str] = &["recover", "skip", "fail_workflow"];

/// Parse the top-level `tasks:` sequence into `Vec<Spanned<RawTask>>`.
///
/// Returns `Ok(vec![])` when the `tasks:` key is absent (the analyzer
/// reports the missing/empty envelope field). Returns a
/// [`SchemaError::Validation`] if the key is present but the value is
/// not a YAML sequence, or if any element is not a mapping.
pub(super) fn parse_tasks(
    cx: &Cx<'_>,
    workflow: &MarkedMappingNode,
) -> Result<Vec<Spanned<RawTask>>, SchemaError> {
    let Some(node) = workflow.get_node("tasks") else {
        return Ok(Vec::new());
    };
    let Some(seq) = node.as_sequence() else {
        return Err(SchemaError::Validation {
            message: "`tasks` must be a YAML sequence".to_owned(),
            span: cx.span(node.span()),
        });
    };

    let mut tasks = Vec::with_capacity(seq.len());
    for item in seq.iter() {
        let Some(task_map) = item.as_mapping() else {
            return Err(SchemaError::Validation {
                message: "each entry in `tasks` must be a mapping".to_owned(),
                span: cx.span(item.span()),
            });
        };
        let task = parse_task(cx, task_map)?;
        let span = cx.span_or_zero(item.span());
        tasks.push(Spanned::new(task, span));
    }
    Ok(tasks)
}

/// Parse one task mapping.
fn parse_task(cx: &Cx<'_>, mapping: &MarkedMappingNode) -> Result<RawTask, SchemaError> {
    let id = cx
        .opt_scalar(mapping, "id")?
        .ok_or_else(|| SchemaError::MissingField {
            field: "id".to_owned(),
            span: cx.span(mapping.span()),
        })?;
    validate_task_id(&id)?;
    let task_label = id.value.clone();

    // Strict-mode unknown-field check · the known set is the closed
    // task fields + the 4 verb keys.
    let mut known: Vec<&str> = TASK_KEYS.to_vec();
    known.extend_from_slice(VERB_KEYS);
    cx.check_unknown_keys(mapping, &known, &format!("task `{task_label}`"))?;

    let action = parse_verb(cx, mapping, &task_label)?;
    let mut task = RawTask::new(id, action);

    task.depends_on = parse_string_list(cx, mapping, "depends_on")?;
    task.when = parse_when(cx, mapping)?;
    task.for_each = parse_for_each(cx, mapping)?;
    task.max_parallel = parse_max_parallel(cx, mapping)?;
    task.fail_fast = parse_bool_field(cx, mapping, "fail_fast")?;
    task.retry = parse_retry(cx, mapping)?;
    task.on_error = parse_on_error(cx, mapping)?;
    task.timeout = parse_timeout(cx, mapping, "timeout")?;
    task.with = parse_with(cx, mapping)?;
    task.output = parse_output_bindings(cx, mapping)?;
    task.on_finally = parse_on_finally(cx, mapping, &task_label)?;

    Ok(task)
}

/// `when:` — a `${{ … }}` CEL string OR the YAML boolean literal
/// (spec 03 §when shape rules · `when: true` = the always-pattern).
///
/// marked-yaml coerces booleans for PLAIN-style scalars only, so a
/// quoted `"true"` stays a string — exactly the spec's split (the
/// literal form is the YAML boolean · a quoted "true" is a bare string
/// the analyzer rejects).
fn parse_when(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
) -> Result<Option<Spanned<WhenGate>>, SchemaError> {
    let Some(node) = mapping.get_node("when") else {
        return Ok(None);
    };
    if let Some(b) = node
        .as_scalar()
        .and_then(marked_yaml::types::MarkedScalarNode::as_bool)
    {
        return Ok(Some(Spanned::new(
            WhenGate::Literal(b),
            cx.span_or_zero(node.span()),
        )));
    }
    Ok(cx
        .opt_scalar(mapping, "when")?
        .map(|s| Spanned::new(WhenGate::Expr(s.value), s.span)))
}

/// Extract an optional list of string scalars under `key`.
pub(super) fn parse_string_list(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
    key: &str,
) -> Result<Vec<Spanned<String>>, SchemaError> {
    let Some(node) = mapping.get_node(key) else {
        return Ok(Vec::new());
    };
    let Some(seq) = node.as_sequence() else {
        return Err(SchemaError::Validation {
            message: format!("`{key}` must be a YAML sequence of strings"),
            span: cx.span(node.span()),
        });
    };
    let mut out = Vec::with_capacity(seq.len());
    for item in seq.iter() {
        let Some(scalar) = item.as_scalar() else {
            return Err(SchemaError::Validation {
                message: format!("each entry in `{key}` must be a string"),
                span: cx.span(item.span()),
            });
        };
        out.push(Spanned::new(
            scalar.as_str().to_owned(),
            cx.span_or_zero(scalar.span()),
        ));
    }
    Ok(out)
}

/// `for_each:` — an expression string OR a literal YAML list (spec
/// `03-dag.md` §`for_each` · « The collection is either a literal list
/// or a reference to an upstream task's array output »).
fn parse_for_each(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
) -> Result<Option<Spanned<ForEachValue>>, SchemaError> {
    let Some(node) = mapping.get_node("for_each") else {
        return Ok(None);
    };
    let span = cx.span_or_zero(node.span());
    if let Some(scalar) = node.as_scalar() {
        return Ok(Some(Spanned::new(
            ForEachValue::Expression(scalar.as_str().to_owned()),
            span,
        )));
    }
    if node.as_sequence().is_some() {
        return Ok(Some(Spanned::new(
            ForEachValue::List(json_value(cx, node)?),
            span,
        )));
    }
    Err(SchemaError::Validation {
        message: "`for_each` must be a `${{ … }}` expression or a literal list".to_owned(),
        span: cx.span(node.span()),
    })
}

/// `max_parallel:` — positive integer ≥ 1 (spec 03 §`max_parallel` ·
/// « **Positive integer** · `1` to `n`. `1` = sequential »).
fn parse_max_parallel(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
) -> Result<Option<Spanned<u32>>, SchemaError> {
    let Some(node) = mapping.get_node("max_parallel") else {
        return Ok(None);
    };
    let value = node
        .as_scalar()
        .and_then(marked_yaml::types::MarkedScalarNode::as_u32)
        .ok_or_else(|| SchemaError::Validation {
            message: "`max_parallel` must be a positive integer".to_owned(),
            span: cx.span(node.span()),
        })?;
    if value == 0 {
        return Err(SchemaError::Validation {
            message: "`max_parallel` must be ≥ 1".to_owned(),
            span: cx.span(node.span()),
        });
    }
    Ok(Some(Spanned::new(value, cx.span_or_zero(node.span()))))
}

/// An optional boolean scalar field (`fail_fast:` · `retry.jitter:`).
fn parse_bool_field(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
    key: &str,
) -> Result<Option<Spanned<bool>>, SchemaError> {
    let Some(node) = mapping.get_node(key) else {
        return Ok(None);
    };
    let value = node
        .as_scalar()
        .and_then(marked_yaml::types::MarkedScalarNode::as_bool)
        .ok_or_else(|| SchemaError::Validation {
            message: format!("`{key}` must be a boolean"),
            span: cx.span(node.span()),
        })?;
    Ok(Some(Spanned::new(value, cx.span_or_zero(node.span()))))
}

/// `timeout:` — a Go-duration STRING scalar (spec 03 §timeout).
///
/// A bare YAML number (`timeout: 30`) is ambiguous (ms? s?) and
/// rejected; the value must carry units. Quoted strings (`"30s"`) are
/// the canonical form.
pub(super) fn parse_timeout(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
    key: &str,
) -> Result<Option<Spanned<Duration>>, SchemaError> {
    let Some(node) = mapping.get_node(key) else {
        return Ok(None);
    };
    let Some(scalar) = node.as_scalar() else {
        return Err(SchemaError::BadTimeout {
            reason: "must be a Go-duration string (e.g. \"30s\" · \"5m\" · \"1h30m\")".to_owned(),
            span: cx.span(node.span()),
        });
    };
    let text = scalar.as_str();
    // Plain (unquoted) scalars that parse as a bare number are the YAML
    // trap the spec forbids · « `30` unquoted parses as integer ·
    // ambiguous · forbidden ».
    if scalar.may_coerce() && (text.parse::<i64>().is_ok() || text.parse::<f64>().is_ok()) {
        return Err(SchemaError::BadTimeout {
            reason: format!("bare number `{text}` is ambiguous — use a quoted Go-duration string"),
            span: cx.span(scalar.span()),
        });
    }
    let duration = parse_go_duration(text).map_err(|e| SchemaError::BadTimeout {
        reason: e.to_string(),
        span: cx.span(scalar.span()),
    })?;
    Ok(Some(Spanned::new(duration, cx.span_or_zero(scalar.span()))))
}

/// `retry:` — `{ max_attempts (req ≥1) · backoff_ms · backoff_strategy ·
/// backoff_max_ms · jitter · on_codes }` (spec 05 §retry).
fn parse_retry(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
) -> Result<Option<Spanned<RetryConfig>>, SchemaError> {
    let Some(node) = mapping.get_node("retry") else {
        return Ok(None);
    };
    let Some(retry_map) = node.as_mapping() else {
        return Err(SchemaError::BadRetry {
            reason: "`retry` must be a mapping".to_owned(),
            span: cx.span(node.span()),
        });
    };
    cx.check_unknown_keys(retry_map, RETRY_KEYS, "`retry:`")?;

    let max_attempts = retry_map
        .get_node("max_attempts")
        .ok_or_else(|| SchemaError::BadRetry {
            reason: "`max_attempts` is required".to_owned(),
            span: cx.span(retry_map.span()),
        })?;
    let max_attempts_value = max_attempts
        .as_scalar()
        .and_then(marked_yaml::types::MarkedScalarNode::as_u32)
        .ok_or_else(|| SchemaError::BadRetry {
            reason: "`max_attempts` must be a positive integer".to_owned(),
            span: cx.span(max_attempts.span()),
        })?;
    if max_attempts_value == 0 {
        return Err(SchemaError::BadRetry {
            reason: "`max_attempts` must be ≥ 1 (total attempts including the first try)"
                .to_owned(),
            span: cx.span(max_attempts.span()),
        });
    }

    let mut config = RetryConfig::new(max_attempts_value);

    if let Some(n) = retry_map.get_node("backoff_ms") {
        config.backoff_ms = n
            .as_scalar()
            .and_then(marked_yaml::types::MarkedScalarNode::as_u64)
            .ok_or_else(|| SchemaError::BadRetry {
                reason: "`backoff_ms` must be a non-negative integer".to_owned(),
                span: cx.span(n.span()),
            })?;
    }
    if let Some(n) = retry_map.get_node("backoff_strategy") {
        let scalar = n.as_scalar().ok_or_else(|| SchemaError::BadRetry {
            reason: "`backoff_strategy` must be a scalar".to_owned(),
            span: cx.span(n.span()),
        })?;
        config.backoff_strategy =
            BackoffStrategy::from_str_opt(scalar.as_str()).ok_or_else(|| {
                SchemaError::BadRetry {
                    reason: format!(
                        "unknown backoff_strategy `{}` (fixed·linear·exponential)",
                        scalar.as_str()
                    ),
                    span: cx.span(scalar.span()),
                }
            })?;
    }
    if let Some(n) = retry_map.get_node("backoff_max_ms") {
        config.backoff_max_ms = n
            .as_scalar()
            .and_then(marked_yaml::types::MarkedScalarNode::as_u64)
            .ok_or_else(|| SchemaError::BadRetry {
                reason: "`backoff_max_ms` must be a non-negative integer".to_owned(),
                span: cx.span(n.span()),
            })?;
    }
    if let Some(n) = retry_map.get_node("jitter") {
        config.jitter = n
            .as_scalar()
            .and_then(marked_yaml::types::MarkedScalarNode::as_bool)
            .ok_or_else(|| SchemaError::BadRetry {
                reason: "`jitter` must be a boolean".to_owned(),
                span: cx.span(n.span()),
            })?;
    }
    for code in parse_string_list(cx, retry_map, "on_codes")? {
        // Spec 05 · on_codes lists canonical codes matching
        // ^NIKA-[A-Z]{2,9}(-[A-Z][A-Z0-9_]{1,15})?-[0-9]{3}$ — not HTTP statuses.
        if !is_valid_error_code(&code.value) {
            return Err(SchemaError::BadRetry {
                reason: format!(
                    "`on_codes` entry `{}` is not a canonical NIKA-<NS>-<NNN> code",
                    code.value
                ),
                span: Some(code.span),
            });
        }
        config.on_codes.push(code.value);
    }

    Ok(Some(Spanned::new(config, cx.span_or_zero(node.span()))))
}

/// `on_error:` — exactly one ACTION (`recover` | `skip: true` |
/// `fail_workflow: true`) + the optional `on_codes:` filter (spec 05
/// §`on_error` · the catch-side mirror of `retry.on_codes`).
fn parse_on_error(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
) -> Result<Option<Spanned<OnError>>, SchemaError> {
    let Some(node) = mapping.get_node("on_error") else {
        return Ok(None);
    };
    let Some(on_error_map) = node.as_mapping() else {
        return Err(SchemaError::BadOnError {
            reason: "`on_error` must be a mapping".to_owned(),
            span: cx.span(node.span()),
        });
    };
    cx.check_unknown_keys(on_error_map, ON_ERROR_KEYS, "`on_error:`")?;

    let present: Vec<&str> = ON_ERROR_ACTION_KEYS
        .iter()
        .copied()
        .filter(|k| on_error_map.get_node(k).is_some())
        .collect();
    let mode = match present.as_slice() {
        [one] => *one,
        [] => {
            return Err(SchemaError::BadOnError {
                reason: "exactly one of `recover`, `skip`, `fail_workflow` required (none found \
                         — `on_codes` alone is a filter with nothing to filter)"
                    .to_owned(),
                span: cx.span(on_error_map.span()),
            });
        }
        many => {
            return Err(SchemaError::BadOnError {
                reason: format!(
                    "actions are mutually exclusive — found {}",
                    many.join(" + ")
                ),
                span: cx.span(on_error_map.span()),
            });
        }
    };

    let action = match mode {
        "recover" => {
            let value_node =
                on_error_map
                    .get_node("recover")
                    .ok_or_else(|| SchemaError::BadOnError {
                        reason: "`recover` value missing".to_owned(),
                        span: cx.span(on_error_map.span()),
                    })?;
            OnErrorAction::Recover(Spanned::new(
                json_value(cx, value_node)?,
                cx.span_or_zero(value_node.span()),
            ))
        }
        "skip" => {
            require_true_flag(cx, on_error_map, "skip")?;
            OnErrorAction::Skip
        }
        _ => {
            require_true_flag(cx, on_error_map, "fail_workflow")?;
            OnErrorAction::FailWorkflow
        }
    };

    let mut policy = OnError::new(action);
    for code in parse_string_list(cx, on_error_map, "on_codes")? {
        // Same canonical regex as retry.on_codes (spec 05) — exact
        // codes route the catch · never HTTP statuses.
        if !is_valid_error_code(&code.value) {
            return Err(SchemaError::BadOnError {
                reason: format!(
                    "`on_codes` entry `{}` is not a canonical NIKA-<NS>-<NNN> code",
                    code.value
                ),
                span: Some(code.span),
            });
        }
        policy.on_codes.push(code);
    }
    Ok(Some(Spanned::new(policy, cx.span_or_zero(node.span()))))
}

/// `skip:` / `fail_workflow:` carry the literal `true` (spec 05 syntax ·
/// `skip: true`) — anything else is a shape error.
fn require_true_flag(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
    key: &str,
) -> Result<(), SchemaError> {
    let node = mapping
        .get_node(key)
        .ok_or_else(|| SchemaError::BadOnError {
            reason: format!("`{key}` value missing"),
            span: cx.span(mapping.span()),
        })?;
    let is_true = node
        .as_scalar()
        .and_then(marked_yaml::types::MarkedScalarNode::as_bool)
        == Some(true);
    if is_true {
        Ok(())
    } else {
        Err(SchemaError::BadOnError {
            reason: format!("`{key}` must be `true` (omit the block for default behavior)"),
            span: cx.span(node.span()),
        })
    }
}

/// `with:` — task-scope variable injection · key → any YAML value.
fn parse_with(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
) -> Result<super::SpannedEntries<serde_json::Value>, SchemaError> {
    let Some(node) = mapping.get_node("with") else {
        return Ok(Vec::new());
    };
    let Some(with_map) = node.as_mapping() else {
        return Err(SchemaError::Validation {
            message: "`with` must be a YAML mapping".to_owned(),
            span: cx.span(node.span()),
        });
    };
    let mut out = Vec::with_capacity(with_map.len());
    for (key, value) in with_map.iter() {
        out.push((
            Spanned::new(key.as_str().to_owned(), cx.span_or_zero(key.span())),
            Spanned::new(json_value(cx, value)?, cx.span_or_zero(value.span())),
        ));
    }
    Ok(out)
}

/// `output:` — named jq bindings · key → jq expression string
/// (spec 04 §output binding).
fn parse_output_bindings(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
) -> Result<super::SpannedEntries<String>, SchemaError> {
    let Some(node) = mapping.get_node("output") else {
        return Ok(Vec::new());
    };
    let Some(output_map) = node.as_mapping() else {
        return Err(SchemaError::Validation {
            message: "`output` must be a YAML mapping of name → jq expression".to_owned(),
            span: cx.span(node.span()),
        });
    };
    let mut out = Vec::with_capacity(output_map.len());
    for (key, value) in output_map.iter() {
        let Some(scalar) = value.as_scalar() else {
            return Err(SchemaError::Validation {
                message: format!(
                    "output binding `{}` must be a jq expression string",
                    key.as_str()
                ),
                span: cx.span(value.span()),
            });
        };
        out.push((
            Spanned::new(key.as_str().to_owned(), cx.span_or_zero(key.span())),
            Spanned::new(scalar.as_str().to_owned(), cx.span_or_zero(scalar.span())),
        ));
    }
    Ok(out)
}

/// `on_finally:` — a sequence of cleanup mini-tasks (spec 03 §`on_finally`).
fn parse_on_finally(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
    task_label: &str,
) -> Result<Vec<Spanned<RawFinallyTask>>, SchemaError> {
    let Some(node) = mapping.get_node("on_finally") else {
        return Ok(Vec::new());
    };
    let Some(seq) = node.as_sequence() else {
        return Err(SchemaError::Validation {
            message: "`on_finally` must be a YAML sequence of cleanup tasks".to_owned(),
            span: cx.span(node.span()),
        });
    };
    let mut out = Vec::with_capacity(seq.len());
    for item in seq.iter() {
        let Some(cleanup_map) = item.as_mapping() else {
            return Err(SchemaError::Validation {
                message: "each `on_finally` entry must be a mapping".to_owned(),
                span: cx.span(item.span()),
            });
        };
        let mut known: Vec<&str> = FINALLY_KEYS.to_vec();
        known.extend_from_slice(VERB_KEYS);
        cx.check_unknown_keys(
            cleanup_map,
            &known,
            &format!("on_finally of task `{task_label}`"),
        )?;

        let action = parse_verb(cx, cleanup_map, &format!("{task_label}.on_finally"))?;
        let mut finally = RawFinallyTask::new(action);
        finally.when = parse_when(cx, cleanup_map)?;
        finally.timeout = parse_timeout(cx, cleanup_map, "timeout")?;
        out.push(Spanned::new(finally, cx.span_or_zero(item.span())));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::error::SchemaError;
    use crate::parser::{ParseMode, parse};
    use crate::raw::{RawAction, RawWorkflow};
    use crate::source::FileId;
    use crate::types::{BackoffStrategy, OnErrorAction, WhenGate};

    fn parse_strict(yaml: &str) -> Result<RawWorkflow, SchemaError> {
        parse(yaml, FileId::new(0), ParseMode::Strict)
    }

    fn one_task(yaml: &str) -> crate::raw::RawTask {
        parse_strict(yaml).expect("parse").tasks.remove(0).value
    }

    #[test]
    fn parse_minimal_infer_task() {
        let yaml = "\
tasks:
  - id: greet
    infer:
      prompt: \"Say hello\"
";
        let task = one_task(yaml);
        assert_eq!(task.id.value, "greet");
        let RawAction::Infer(ref action) = task.action else {
            panic!("expected Infer");
        };
        assert_eq!(action.prompt.value, "Say hello");
    }

    #[test]
    fn task_without_id_errors() {
        let yaml = "\
tasks:
  - exec:
      command: ls
";
        let err = parse_strict(yaml).expect_err("missing id");
        assert!(
            matches!(&err, SchemaError::MissingField { field, .. } if field == "id"),
            "{err:?}"
        );
    }

    #[test]
    fn task_id_not_snake_case_errors() {
        // Conformance fixture verbs-shape/004 · `my-task` is CEL-unsafe.
        let yaml = "\
tasks:
  - id: my-task
    infer:
      prompt: \"hi\"
";
        let err = parse_strict(yaml).expect_err("kebab id");
        assert!(matches!(err, SchemaError::BadTaskId { .. }), "{err:?}");
    }

    #[test]
    fn task_no_verb_errors() {
        // Conformance fixture verbs-shape/002.
        let yaml = "\
tasks:
  - id: greet
    depends_on: []
";
        let err = parse_strict(yaml).expect_err("no verb");
        assert!(
            matches!(&err, SchemaError::MissingVerb { task, .. } if task == "greet"),
            "{err:?}"
        );
    }

    #[test]
    fn task_two_verbs_errors() {
        // Conformance fixture verbs-shape/001.
        let yaml = "\
tasks:
  - id: greet
    infer:
      prompt: \"hi\"
    exec:
      command: \"echo hi\"
";
        let err = parse_strict(yaml).expect_err("two verbs");
        let SchemaError::MultipleVerbs { verbs, .. } = err else {
            panic!("expected MultipleVerbs, got {err:?}");
        };
        assert!(verbs.contains("infer") && verbs.contains("exec"));
    }

    #[test]
    fn fetch_key_is_not_a_verb() {
        // Spec D-2026-05-22-N18 · fetch is the `nika:fetch` builtin via
        // invoke — a top-level `fetch:` key is an unknown field (strict).
        let yaml = "\
tasks:
  - id: poll
    fetch:
      url: https://api.example.com/v1/status
";
        let err = parse_strict(yaml).expect_err("fetch: is not a verb");
        assert!(
            matches!(&err, SchemaError::UnknownField { field, .. } if field == "fetch"),
            "{err:?}"
        );
    }

    #[test]
    fn depends_on_when_for_each() {
        let yaml = "\
tasks:
  - id: a
    exec: { command: echo a }
  - id: b
    depends_on: [a]
    when: ${{ tasks.a.status == 'success' }}
    for_each: ${{ vars.items }}
    exec: { command: echo b }
";
        let wf = parse_strict(yaml).expect("parse");
        let b = &wf.tasks[1].value;
        assert_eq!(b.depends_on.len(), 1);
        assert_eq!(b.depends_on[0].value, "a");
        assert_eq!(
            b.when.as_ref().expect("when").value,
            WhenGate::Expr("${{ tasks.a.status == 'success' }}".into())
        );
        assert_eq!(
            b.for_each.as_ref().expect("for_each").value,
            crate::raw::ForEachValue::Expression("${{ vars.items }}".into())
        );
    }

    #[test]
    fn max_parallel_and_fail_fast() {
        let yaml = "\
tasks:
  - id: scrape_all
    for_each: ${{ vars.urls }}
    max_parallel: 5
    fail_fast: false
    exec: { command: echo }
";
        let task = one_task(yaml);
        assert_eq!(task.max_parallel.expect("max_parallel").value, 5);
        assert!(!task.fail_fast.expect("fail_fast").value);
    }

    #[test]
    fn max_parallel_zero_errors() {
        let yaml = "\
tasks:
  - id: x
    max_parallel: 0
    exec: { command: echo }
";
        let err = parse_strict(yaml).expect_err("zero");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("≥ 1")),
            "{err:?}"
        );
    }

    #[test]
    fn timeout_quoted_go_duration() {
        let yaml = "\
tasks:
  - id: t
    timeout: \"1h30m\"
    exec: { command: echo }
";
        let task = one_task(yaml);
        assert_eq!(
            task.timeout.expect("timeout").value,
            Duration::from_secs(5400)
        );
    }

    #[test]
    fn timeout_bare_number_errors() {
        // Spec 03 · « `30` unquoted parses as integer · ambiguous ·
        // forbidden ».
        let yaml = "\
tasks:
  - id: t
    timeout: 30
    exec: { command: echo }
";
        let err = parse_strict(yaml).expect_err("bare number");
        assert!(
            matches!(&err, SchemaError::BadTimeout { reason, .. } if reason.contains("ambiguous")),
            "{err:?}"
        );
    }

    #[test]
    fn timeout_bad_unit_errors() {
        // Conformance fixture errors/001-timeout-bad-format · "5w".
        let yaml = "\
tasks:
  - id: t
    timeout: \"5w\"
    exec: { command: echo }
";
        let err = parse_strict(yaml).expect_err("bad unit");
        assert!(matches!(err, SchemaError::BadTimeout { .. }), "{err:?}");
    }

    #[test]
    fn retry_full_block() {
        // Spec 05 §retry syntax example.
        let yaml = "\
tasks:
  - id: flaky_api
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://flaky.example.com/data\" }
    retry:
      max_attempts: 5
      backoff_ms: 1000
      backoff_strategy: exponential
      backoff_max_ms: 30000
      jitter: true
      on_codes:
        - NIKA-BUILTIN-FETCH-001
        - NIKA-PROVIDER-001
";
        let task = one_task(yaml);
        let retry = task.retry.expect("retry").value;
        assert_eq!(retry.max_attempts, 5);
        assert_eq!(retry.backoff_ms, 1000);
        assert_eq!(retry.backoff_strategy, BackoffStrategy::Exponential);
        assert_eq!(retry.backoff_max_ms, 30_000);
        assert!(retry.jitter);
        assert_eq!(retry.on_codes.len(), 2);
    }

    #[test]
    fn retry_defaults_applied() {
        let yaml = "\
tasks:
  - id: t
    retry: { max_attempts: 3 }
    exec: { command: echo }
";
        let retry = one_task(yaml).retry.expect("retry").value;
        assert_eq!(retry.max_attempts, 3);
        assert_eq!(retry.backoff_ms, 1000);
        assert_eq!(retry.backoff_strategy, BackoffStrategy::Exponential);
        assert_eq!(retry.backoff_max_ms, 60_000);
        assert!(retry.jitter, "jitter defaults true");
    }

    #[test]
    fn retry_missing_max_attempts_errors() {
        let yaml = "\
tasks:
  - id: t
    retry: { backoff_ms: 500 }
    exec: { command: echo }
";
        let err = parse_strict(yaml).expect_err("no max_attempts");
        assert!(
            matches!(&err, SchemaError::BadRetry { reason, .. } if reason.contains("max_attempts")),
            "{err:?}"
        );
    }

    #[test]
    fn retry_zero_max_attempts_errors() {
        let yaml = "\
tasks:
  - id: t
    retry: { max_attempts: 0 }
    exec: { command: echo }
";
        let err = parse_strict(yaml).expect_err("zero attempts");
        assert!(matches!(err, SchemaError::BadRetry { .. }), "{err:?}");
    }

    #[test]
    fn retry_bad_on_code_errors() {
        // Spec 05 · on_codes are canonical codes — not HTTP statuses.
        let yaml = "\
tasks:
  - id: t
    retry:
      max_attempts: 2
      on_codes: [\"503\"]
    exec: { command: echo }
";
        let err = parse_strict(yaml).expect_err("http status");
        assert!(
            matches!(&err, SchemaError::BadRetry { reason, .. } if reason.contains("503")),
            "{err:?}"
        );
    }

    #[test]
    fn on_error_recover_ref() {
        // Spec 05 · recover takes a ${{ }} ref OR a literal.
        let yaml = "\
tasks:
  - id: api_call
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://api.example.com\" }
    on_error:
      recover: ${{ tasks.cached.output }}
";
        let task = one_task(yaml);
        let policy = task.on_error.expect("on_error").value;
        let OnErrorAction::Recover(value) = policy.action else {
            panic!("expected Recover");
        };
        assert_eq!(value.value, "${{ tasks.cached.output }}");
        assert!(policy.on_codes.is_empty(), "no filter declared");
    }

    #[test]
    fn on_error_recover_literal() {
        let yaml = "\
tasks:
  - id: get_count
    invoke: { tool: \"mcp:db/count_users\" }
    on_error:
      recover: 0
";
        let task = one_task(yaml);
        let OnErrorAction::Recover(value) = task.on_error.expect("on_error").value.action else {
            panic!("expected Recover");
        };
        assert_eq!(value.value, 0);
    }

    #[test]
    fn on_error_skip_and_fail_workflow() {
        let yaml = "\
tasks:
  - id: a
    exec: { command: echo }
    on_error: { skip: true }
  - id: b
    exec: { command: echo }
    on_error: { fail_workflow: true }
";
        let wf = parse_strict(yaml).expect("parse");
        assert!(matches!(
            wf.tasks[0].value.on_error.as_ref().expect("a").value.action,
            OnErrorAction::Skip
        ));
        assert!(matches!(
            wf.tasks[1].value.on_error.as_ref().expect("b").value.action,
            OnErrorAction::FailWorkflow
        ));
    }

    #[test]
    fn on_error_two_fields_errors() {
        // Spec 05 · mutually exclusive · two-or-zero = parse error.
        let yaml = "\
tasks:
  - id: t
    exec: { command: echo }
    on_error:
      skip: true
      fail_workflow: true
";
        let err = parse_strict(yaml).expect_err("two fields");
        assert!(matches!(err, SchemaError::BadOnError { .. }), "{err:?}");
    }

    #[test]
    fn on_error_on_codes_filter() {
        // Spec 05 · on_codes = catch-side filter beside exactly one action.
        let yaml = "\
tasks:
  - id: slow_fetch
    invoke: { tool: \"nika:fetch\", args: { url: \"https://slow.example.com\" } }
    on_error:
      on_codes: [NIKA-TIMEOUT-001, NIKA-BUILTIN-JSON_MERGE_PATCH-001]
      recover: { stale: true }
";
        let policy = one_task(yaml).on_error.expect("on_error").value;
        assert!(matches!(policy.action, OnErrorAction::Recover(_)));
        assert_eq!(policy.on_codes.len(), 2);
        assert_eq!(
            policy.on_codes[1].value,
            "NIKA-BUILTIN-JSON_MERGE_PATCH-001"
        );
    }

    #[test]
    fn on_error_on_codes_alone_errors() {
        // A filter with nothing to filter (spec 05 · one action required).
        let yaml = "\
tasks:
  - id: t
    exec: { command: echo }
    on_error:
      on_codes: [NIKA-TIMEOUT-001]
";
        let err = parse_strict(yaml).expect_err("filter alone");
        assert!(matches!(err, SchemaError::BadOnError { .. }), "{err:?}");
    }

    #[test]
    fn when_boolean_literal_forms() {
        // Spec 03 §when shape rules · the YAML boolean literal is legal
        // (`when: true` = the always-pattern) · a QUOTED \"true\" stays a
        // bare string (marked-yaml coerces plain style only).
        let yaml = "\
tasks:
  - id: work
    exec: { command: echo }
  - id: record
    depends_on: [work]
    when: true
    exec: { command: echo }
  - id: never
    depends_on: [work]
    when: false
    exec: { command: echo }
  - id: quoted
    depends_on: [work]
    when: \"true\"
    exec: { command: echo }
";
        let wf = parse_strict(yaml).expect("parse");
        assert_eq!(
            wf.tasks[1].value.when.as_ref().expect("record").value,
            WhenGate::Literal(true)
        );
        assert_eq!(
            wf.tasks[2].value.when.as_ref().expect("never").value,
            WhenGate::Literal(false)
        );
        assert_eq!(
            wf.tasks[3].value.when.as_ref().expect("quoted").value,
            WhenGate::Expr("true".into()),
            "a quoted \"true\" is a bare string · the analyzer rejects it"
        );
    }

    #[test]
    fn when_yaml11_bool_aliases_stay_strings() {
        // marked-yaml's as_bool accepts true/True/TRUE + false forms ONLY —
        // the YAML 1.1 aliases (`yes` · `on`) stay strings, exactly the
        // spec's split (03 §when · the literal form is true/false).
        let yaml = "\
tasks:
  - id: work
    exec: { command: echo }
  - id: legacy
    depends_on: [work]
    when: yes
    exec: { command: echo }
";
        let wf = parse_strict(yaml).expect("parse");
        assert_eq!(
            wf.tasks[1].value.when.as_ref().expect("legacy").value,
            WhenGate::Expr("yes".into()),
            "`when: yes` is NOT a boolean literal · the analyzer rejects it as a bare string"
        );
    }

    #[test]
    fn on_error_zero_fields_errors() {
        let yaml = "\
tasks:
  - id: t
    exec: { command: echo }
    on_error: {}
";
        let err = parse_strict(yaml).expect_err("zero fields");
        assert!(matches!(err, SchemaError::BadOnError { .. }), "{err:?}");
    }

    #[test]
    fn with_values_spanned_json() {
        let yaml = "\
tasks:
  - id: summarize
    with:
      content: ${{ tasks.research.output }}
      style: \"concise\"
      config:
        max_words: 100
    infer: { prompt: \"Summarize\" }
";
        let task = one_task(yaml);
        assert_eq!(task.with.len(), 3);
        assert_eq!(task.with[0].0.value, "content");
        assert_eq!(task.with[0].1.value, "${{ tasks.research.output }}");
        assert_eq!(task.with[2].1.value["max_words"], 100);
    }

    #[test]
    fn output_bindings() {
        let yaml = "\
tasks:
  - id: api_call
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://api.example.com/data\" }
    output:
      user_count: \".data.users | length\"
      first_user: \".data.users[0]\"
";
        let task = one_task(yaml);
        assert_eq!(task.output.len(), 2);
        assert_eq!(task.output[0].0.value, "user_count");
        assert_eq!(task.output[0].1.value, ".data.users | length");
    }

    #[test]
    fn on_finally_mini_tasks() {
        // Spec 03 §on_finally + example 16.
        let yaml = "\
tasks:
  - id: test
    timeout: \"5m\"
    exec:
      command: \"cargo test\"
    on_finally:
      - exec:
          command: \"rm -rf /tmp/x\"
      - when: ${{ tasks.test.status == 'failed' }}
        timeout: \"10s\"
        invoke:
          tool: nika:emit
          args: { event: \"done\" }
";
        let task = one_task(yaml);
        assert_eq!(task.on_finally.len(), 2);
        assert!(task.on_finally[0].value.when.is_none());
        let second = &task.on_finally[1].value;
        assert!(second.when.is_some());
        assert_eq!(
            second.timeout.as_ref().expect("timeout").value,
            Duration::from_secs(10)
        );
        assert!(matches!(second.action, RawAction::Invoke(_)));
    }

    #[test]
    fn unknown_task_field_strict_errors() {
        let yaml = "\
tasks:
  - id: t
    condition: \"${{ x }}\"
    exec: { command: echo }
";
        // `condition:` is the BROUILLON-era key — the canonical key is
        // `when:` (spec 03) · strict mode rejects it.
        let err = parse_strict(yaml).expect_err("brouillon key");
        assert!(
            matches!(&err, SchemaError::UnknownField { field, .. } if field == "condition"),
            "{err:?}"
        );
    }

    #[test]
    fn tasks_as_mapping_errors() {
        let err = parse_strict("tasks:\n  a: b\n").expect_err("tasks must be a sequence");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("sequence")),
            "{err:?}"
        );
    }

    #[test]
    fn task_entry_must_be_mapping() {
        let err = parse_strict("tasks:\n  - just_a_scalar\n").expect_err("scalar task");
        assert!(matches!(err, SchemaError::Validation { .. }));
    }

    #[test]
    fn parse_preserves_task_span() {
        let yaml = "\
tasks:
  - id: t1
    exec:
      command: echo
";
        let wf = parse(yaml, FileId::new(9), ParseMode::Strict).expect("parse");
        assert_eq!(wf.tasks[0].span.file, FileId::new(9));
    }
}
