// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task-list parsing — YAML `tasks:` sequence → `Vec<Spanned<RawTask>>`.
//!
//! The canonical v1 task field set is CLOSED (spec `03-dag.md`
//! §forward-compat + NEP-0004 law 7's ONE grammar addition) · `with` ·
//! `after` · `when` · `for_each` (a BLOCK · `items` + the two knobs) ·
//! `retry` · `on_error` · `timeout` · `extract` · `lift` · `group` ·
//! plus exactly one verb key. Cleanup is a TASK joined by an `unwind`
//! edge now — `on_finally:` was a second grammar for a task body.

use std::time::Duration;

use marked_yaml::types::MarkedMappingNode;
use nika_vocab::after::predicate_refusal;

use crate::error::SchemaError;
use crate::raw::RawTask;
use crate::source::Spanned;
use crate::types::{
    AfterPredicate, BackoffStrategy, OnError, OnErrorAction, RetryConfig, WhenGate,
    is_valid_error_code, parse_go_duration,
};

use super::value::json_value;
use super::verbs::{VERB_KEYS, parse_verb};
use super::{Cx, validate_task_id};

/// Maximum tasks per workflow (untrusted-input resource bound · see the
/// security note on `parser::CharToByte::new`). The analyzer's DAG
/// passes (cycle detection · edge-target resolution) are super-linear
/// in places; >10k tasks is machine-generated and should compose
/// sub-workflows. Generous — no hand-written workflow approaches it.
pub(super) const MAX_TASKS: usize = 10_000;

pub(crate) use nika_vocab::keys::{ON_ERROR_KEYS, RETRY_KEYS, TASK_KEYS};

/// The `on_error:` ACTION keys (mutually exclusive · exactly one).
const ON_ERROR_ACTION_KEYS: &[&str] = &["recover", "skip"];

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
    // W1 « the map » — the dead sequence form gets its migration teaching.
    if node.as_sequence().is_some() {
        return Err(SchemaError::W1TasksSequence {
            span: cx.span(node.span()),
        });
    }
    let Some(map) = node.as_mapping() else {
        return Err(SchemaError::Validation {
            message: "`tasks` must be a YAML map keyed by task id".to_owned(),
            span: cx.span(node.span()),
        });
    };

    // Task-count bound (untrusted-input guard · see parser::mod security
    // note). Loud, not a silent truncate.
    if map.len() > MAX_TASKS {
        return Err(SchemaError::Validation {
            message: format!(
                "workflow declares {} tasks (max {MAX_TASKS}) — compose \
                 sub-workflows instead (resource bound)",
                map.len()
            ),
            span: cx.span(node.span()),
        });
    }

    let mut tasks = Vec::with_capacity(map.len());
    for (key, value) in map.iter() {
        // the map KEY is the identity — its span anchors every surface
        // (hover · outline · semanticDocument · goto-definition). marked-yaml
        // gives keys a point span; widen it to the token (ids are ASCII by
        // grammar, so byte length = token length).
        let mut key_span = cx.span_or_zero(key.span());
        if key_span.end.0 <= key_span.start.0 {
            let len = u32::try_from(key.as_str().len()).unwrap_or(u32::MAX);
            key_span.end = crate::source::ByteOffset::new(key_span.start.0.saturating_add(len));
        }
        let id = Spanned::new(key.as_str().to_owned(), key_span);
        validate_task_id(&id)?;
        let Some(task_map) = value.as_mapping() else {
            return Err(SchemaError::Validation {
                message: format!("task `{}` must be a mapping", id.value),
                span: cx.span(value.span()),
            });
        };
        let task = parse_task(cx, task_map, id)?;
        // the task's span runs KEY → end of body (a breakpoint or range
        // on the declaring key line belongs to the task)
        let body_span = cx.span_or_zero(value.span());
        let span = crate::source::Span::new(
            key_span.file,
            key_span.start,
            if body_span.end.0 >= key_span.start.0 {
                body_span.end
            } else {
                key_span.end
            },
        );
        tasks.push(Spanned::new(task, span));
    }
    Ok(tasks)
}

/// Parse one task mapping (identity = the map key, passed in).
fn parse_task(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
    id: Spanned<String>,
) -> Result<RawTask, SchemaError> {
    // W1 « the map » — a lingering `id:` field gets its migration
    // teaching before the generic unknown-field check.
    if let Some(node) = mapping.get_node("id") {
        return Err(SchemaError::W1TaskIdField {
            task: id.value.clone(),
            span: cx.span(node.span()),
        });
    }
    // W2 « the flow » — a lingering `depends_on:` gets its migration
    // teaching (data → with: · control → after:) before the generic
    // unknown-field check. The first dep seeds the teaching's example.
    if let Some(node) = mapping.get_node("depends_on") {
        let task_hint = node
            .as_sequence()
            .and_then(|seq| seq.iter().next())
            .and_then(marked_yaml::Node::as_scalar)
            .map_or_else(|| "producer".to_owned(), |s| s.as_str().to_owned());
        return Err(SchemaError::W2DependsOnField {
            task: id.value.clone(),
            task_hint,
            span: cx.span(node.span()),
        });
    }
    let task_label = id.value.clone();

    // Strict-mode unknown-field check · the known set is the closed
    // task fields + the 4 verb keys.
    let mut known: Vec<&str> = TASK_KEYS.to_vec();
    known.extend_from_slice(VERB_KEYS);
    cx.check_unknown_keys(mapping, &known, &format!("task `{task_label}`"))?;

    let action = parse_verb(cx, mapping, &task_label)?;
    let mut task = RawTask::new(id, action);

    task.after = parse_after(cx, mapping, &task_label)?;
    task.when = parse_when(cx, mapping)?;
    let (for_each, max_parallel, fail_fast) = super::for_each::parse_for_each(cx, mapping)?;
    task.for_each = for_each;
    task.max_parallel = max_parallel;
    task.fail_fast = fail_fast;
    task.retry = parse_retry(cx, mapping)?;
    task.on_error = parse_on_error(cx, mapping)?;
    task.timeout = parse_timeout(cx, mapping, "timeout")?;
    task.with = parse_with(cx, mapping)?;
    task.extract = parse_extract_bindings(cx, mapping)?;
    task.returns = parse_returns(cx, mapping)?;
    task.lift = super::lift::parse_lift(cx, mapping, &task_label)?;
    task.group = parse_group(cx, mapping, &task_label)?;

    Ok(task)
}

/// `returns:` — the task's output contract (spec 09) · a named type
/// (scalar) or an inline type expression (mapping) · kept RAW; the
/// grammar (`NIKA-TYPE-001/006`) is the analyzer's job via the type
/// core (one truth · the parser is shape-only).
fn parse_returns(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
) -> Result<Option<Spanned<serde_json::Value>>, SchemaError> {
    let Some(node) = mapping.get_node("returns") else {
        return Ok(None);
    };
    Ok(Some(Spanned::new(
        json_value(cx, node)?,
        cx.span_or_zero(node.span()),
    )))
}

type AfterEntries = Vec<(Spanned<String>, Spanned<AfterPredicate>)>;

/// `after:` — the CONTROL boundary · a map `{producer: predicate}` over the
/// CLOSED outcome-class set (03 §after · R5 · `NIKA-DAG-005` · targets: DAG-002).
fn parse_after(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
    task: &str,
) -> Result<AfterEntries, SchemaError> {
    let Some(node) = mapping.get_node("after") else {
        return Ok(Vec::new());
    };
    let Some(map) = node.as_mapping() else {
        return Err(SchemaError::Validation {
            message: format!(
                "task `{task}` `after:` must be a map {{producer-task: predicate}} — \
                 never a list (03-dag.md §after)"
            ),
            span: cx.span(node.span()),
        });
    };
    let mut out = Vec::with_capacity(map.len());
    for (key, value) in map.iter() {
        let target = Spanned::new(key.as_str().to_owned(), cx.span_or_zero(key.span()));
        let Some(scalar) = value.as_scalar() else {
            return Err(SchemaError::UnknownAfterPredicate {
                message: predicate_refusal(task, &target.value, "(not a string)"),
                task: task.to_owned(),
                target: target.value,
                predicate: "(not a string)".to_owned(),
                span: cx.span(value.span()),
            });
        };
        let raw = scalar.as_str();
        let Some(predicate) = AfterPredicate::parse(raw) else {
            return Err(SchemaError::UnknownAfterPredicate {
                message: predicate_refusal(task, &target.value, raw),
                task: task.to_owned(),
                target: target.value,
                predicate: raw.to_owned(),
                span: cx.span(value.span()),
            });
        };
        out.push((
            target,
            Spanned::new(predicate, cx.span_or_zero(value.span())),
        ));
    }
    Ok(out)
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
        super::refuse_ambiguous_plain_scalar(
            scalar,
            &format!("{key} entry"),
            cx.span_or_zero(scalar.span()),
        )?;
        out.push(Spanned::new(
            scalar.as_str().to_owned(),
            cx.span_or_zero(scalar.span()),
        ));
    }
    Ok(out)
}

/// `group:` — fan-in MEMBERSHIP (spec 03 §group). One name, matching
/// `^[a-z][a-z0-9_]*$` like a task key. Membership only: it carries no
/// predicate, no ordering and no data.
///
/// A group name MAY coincide with a task key — the roots disambiguate
/// structurally (`group.probes` vs `tasks.probes`).
fn parse_group(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
    task_label: &str,
) -> Result<Option<Spanned<String>>, SchemaError> {
    let Some(node) = mapping.get_node("group") else {
        return Ok(None);
    };
    let Some(scalar) = node.as_scalar() else {
        return Err(SchemaError::Validation {
            message: format!("`group` on task `{task_label}` must be one name (a string)"),
            span: cx.span(node.span()),
        });
    };
    let name = scalar.as_str();
    let shaped = name.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !shaped {
        return Err(SchemaError::Validation {
            message: format!(
                "invalid group name `{name}` on task `{task_label}` — must match \
                 ^[a-z][a-z0-9_]*$ (snake_case, like a task key)"
            ),
            span: cx.span(node.span()),
        });
    }
    Ok(Some(Spanned::new(
        name.to_owned(),
        cx.span_or_zero(node.span()),
    )))
}

/// An optional boolean scalar field (`fail_fast:` · `retry.jitter:`).
pub(super) fn parse_bool_field(
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
            reason: format!(
                "bare number `{text}` is ambiguous — use a quoted Go-duration string \
                 (e.g. \"30s\" · \"5m\" · \"1h30m\")"
            ),
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
            reason: "`retry` must be a mapping · `{ max_attempts, backoff_ms, \
                     backoff_strategy, backoff_max_ms, jitter, on_codes }`"
                .to_owned(),
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
/// the optional `on_codes:` filter (spec 05
/// §`on_error` · the catch-side mirror of `retry.on_codes`).
fn parse_on_error(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
) -> Result<Option<Spanned<OnError>>, SchemaError> {
    let Some(node) = mapping.get_node("on_error") else {
        return Ok(None);
    };
    let Some(on_error_map) = node.as_mapping() else {
        // A scalar here is usually the action name typed bare
        // (`on_error: recover`) — teach the mapping SHAPE and the closed
        // action vocabulary, not just the type mismatch.
        return Err(SchemaError::BadOnError {
            reason: "`on_error` must be a mapping carrying exactly one action — \
                     `recover:` · `skip: true` \
                     (e.g. `on_error: { skip: true }`)"
                .to_owned(),
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
                reason: "exactly one of `recover`, `skip` required (none found \
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
        // The set is CLOSED at two: `fail_workflow: true` died with the
        // spec's 3-modes→2 cut (2026-08-11). It only ever spelled the
        // DEFAULT out loud, and a keyword whose whole job is to restate
        // the absence of a keyword is a keyword that teaches nothing.
        #[allow(
            clippy::unreachable,
            reason = "the exactly-one-of check above admits only the two arms"
        )]
        other => unreachable!("unknown on_error action: {other}"),
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

/// `skip:` carries the literal `true` (spec 05 syntax ·
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

/// `extract:` — named jq bindings · key → jq expression string (spec 04
/// §binding rules). The reserved-projection refusal is unchanged.
fn parse_extract_bindings(
    cx: &Cx<'_>,
    mapping: &MarkedMappingNode,
) -> Result<super::SpannedEntries<String>, SchemaError> {
    let Some(node) = mapping.get_node("extract") else {
        return Ok(Vec::new());
    };
    let Some(extract_map) = node.as_mapping() else {
        return Err(SchemaError::Validation {
            message: "`extract` must be a YAML mapping of name → jq expression".to_owned(),
            span: cx.span(node.span()),
        });
    };
    let mut out = Vec::with_capacity(extract_map.len());
    for (key, value) in extract_map.iter() {
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

#[cfg(test)]
pub(crate) mod tests {
    use std::time::Duration;

    use crate::error::SchemaError;
    use crate::parser::{ParseMode, parse};
    use crate::raw::{RawAction, RawWorkflow};
    use crate::source::FileId;
    use crate::types::{AfterPredicate, BackoffStrategy, OnErrorAction, WhenGate};

    pub(crate) fn parse_strict(yaml: &str) -> Result<RawWorkflow, SchemaError> {
        parse(yaml, FileId::new(0), ParseMode::Strict)
    }

    pub(crate) fn one_task(yaml: &str) -> crate::raw::RawTask {
        parse_strict(yaml).expect("parse").tasks.remove(0).value
    }

    #[test]
    fn parse_minimal_infer_task() {
        let yaml = "\
tasks:
  greet:
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
    fn task_id_field_is_taught_the_key() {
        // W1 · NIKA-PARSE-023: an `id:` field inside a task body is the dead
        // form — the map key IS the identity.
        let yaml = "\
tasks:
  a:
    id: a
    exec:
      command: [ls]
";
        let err = parse_strict(yaml).expect_err("id field");
        assert!(
            matches!(&err, SchemaError::W1TaskIdField { task, .. } if task == "a"),
            "{err:?}"
        );
    }

    #[test]
    fn task_id_not_snake_case_errors() {
        // Conformance fixture verbs-shape/004 · `my-task` is CEL-unsafe.
        let yaml = "\
tasks:
  my-task:
    infer:
      prompt: \"hi\"
";
        let err = parse_strict(yaml).expect_err("kebab id");
        assert!(matches!(err, SchemaError::BadTaskId { .. }), "{err:?}");
    }

    #[test]
    fn task_no_verb_errors() {
        // Conformance fixture verbs-shape/002 · a task carrying only
        // flow-control keys binds zero verbs.
        let yaml = "\
tasks:
  greet:
    when: true
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
  greet:
    infer:
      prompt: \"hi\"
    exec:
      shell: \"echo hi\"
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
  poll:
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
    fn future_clause_budget_rejects_cleanly() {
        // Forward-compat anchor (overnight 2026-07-05) · the v0.2 seed
        // clauses (budget: / approve: / policy: · draft ADRs in nika-spec)
        // MUST fail as clean UnknownField on today's parser — never a
        // panic, never silent acceptance. When a clause ships, its test
        // here flips from expect_err to a positive parse.
        let yaml = "\
tasks:
  capped:
    budget: { usd: 0.50 }
    infer: { prompt: hello }
";
        let err = parse_strict(yaml).expect_err("budget: is not shipped");
        assert!(
            matches!(&err, SchemaError::UnknownField { field, .. } if field == "budget"),
            "{err:?}"
        );
    }

    #[test]
    fn future_clause_approve_rejects_cleanly() {
        let yaml = "\
tasks:
  gated:
    approve: true
    exec: { shell: rm -rf ./dist }
";
        let err = parse_strict(yaml).expect_err("approve: is not shipped");
        assert!(
            matches!(&err, SchemaError::UnknownField { field, .. } if field == "approve"),
            "{err:?}"
        );
    }

    #[test]
    fn shipped_clause_retry_parses_with_the_real_grammar() {
        // Anti-redundancy anchor (overnight 2026-07-05) · retry:/on_error:
        // ALREADY ship (spec 05 · this night's socratic hypothesis « retry
        // is the missing primitive » was refuted by this very test) — the
        // v0.2 seed ADRs are budget/approve/policy only.
        let yaml = "\
tasks:
  flaky:
    retry: { max_attempts: 3, backoff_strategy: exponential, on_codes: [NIKA-INFER-001] }
    infer: { prompt: hello }
";
        let wf = parse_strict(yaml).expect("retry with the real keys parses");
        assert!(wf.tasks[0].value.retry.is_some());
    }

    #[test]
    fn after_when_for_each() {
        let yaml = "\
tasks:
  a:
    exec: { shell: echo a }
  b:
    after: { a: success }
    when: ${{ inputs.flag == true }}
    for_each: { items: \"${{ inputs.items }}\" }
    exec: { shell: echo b }
";
        let wf = parse_strict(yaml).expect("parse");
        let b = &wf.tasks[1].value;
        assert_eq!(b.after.len(), 1);
        assert_eq!(b.after[0].0.value, "a");
        assert_eq!(b.after[0].1.value, AfterPredicate::Success);
        assert_eq!(
            b.when.as_ref().expect("when").value,
            WhenGate::Expr("${{ inputs.flag == true }}".into())
        );
        assert_eq!(
            b.for_each.as_ref().expect("for_each").value,
            crate::raw::ForEachValue::Expression("${{ inputs.items }}".into())
        );
    }

    #[test]
    fn r5_dead_predicate_spellings_refuse_teaching_in_both_modes() {
        // The R5 flag-day (spec #118 · LAW-GRAMMAR-0231): `succeeded` /
        // `failed` are DEAD spellings — the refusal TEACHES the respelling
        // and the `nika check --fix` repair (mode-independent, the C2
        // dead-form doctrine), and it rides NIKA-DAG-005 (the spec
        // registers no separate code for out-of-set spellings).
        for dead in ["succeeded", "failed"] {
            let yaml = format!(
                "tasks:\n  tests:\n    exec: {{ shell: echo t }}\n  deploy:\n    after: {{ tests: {dead} }}\n    exec: {{ shell: echo d }}\n"
            );
            for mode in [ParseMode::Strict, ParseMode::Lenient] {
                let err = parse(&yaml, FileId::new(0), mode).expect_err("dead spelling");
                let SchemaError::UnknownAfterPredicate {
                    message, predicate, ..
                } = &err
                else {
                    panic!("expected UnknownAfterPredicate, got {err:?}");
                };
                let to = nika_vocab::after::dead_spelling_respelling(dead).expect("dead");
                assert_eq!(predicate, dead);
                assert!(message.contains("dead predicate spelling"), "{message}");
                assert!(message.contains(&format!("respell as `{to}`")), "{message}");
                assert!(message.contains("nika check --fix"), "{message}");
                assert_eq!(err.spec_code().to_string(), "NIKA-DAG-005");
            }
        }
    }

    #[test]
    fn unknown_after_predicate_names_the_closed_outcome_class_set() {
        // A genuinely-unknown predicate gets the closed-set text (never
        // the dead-spelling teaching) — conformance dag-topology/016.
        let yaml = "tasks:\n  t:\n    exec: { shell: echo t }\n  d:\n    after: { t: passed }\n    exec: { shell: echo d }\n";
        let err = parse_strict(yaml).expect_err("unknown predicate");
        let SchemaError::UnknownAfterPredicate { message, .. } = &err else {
            panic!("expected UnknownAfterPredicate, got {err:?}");
        };
        assert!(message.contains("is not a predicate"), "{message}");
        assert!(
            message.contains("success · failure · skipped · terminal · unwind"),
            "{message}"
        );
        assert_eq!(err.spec_code().to_string(), "NIKA-DAG-005");
    }

    #[test]
    fn max_parallel_and_fail_fast() {
        let yaml = "\
tasks:
  scrape_all:
    for_each:
      items: \"${{ inputs.urls }}\"
      max_parallel: 5
      fail_fast: false
    exec: { command: [echo] }
";
        let task = one_task(yaml);
        assert_eq!(task.max_parallel.expect("max_parallel").value, 5);
        assert!(!task.fail_fast.expect("fail_fast").value);
    }

    #[test]
    fn max_parallel_zero_errors() {
        // the knob only exists inside the block now — a task-level
        // `max_parallel:` is an unknown field, and a zero inside the
        // block is still the >= 1 refusal
        let yaml = "\
tasks:
  x:
    for_each:
      items: [1]
      max_parallel: 0
    exec: { command: [echo] }
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
  t:
    timeout: \"1h30m\"
    exec: { command: [echo] }
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
  t:
    timeout: 30
    exec: { command: [echo] }
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
  t:
    timeout: \"5w\"
    exec: { command: [echo] }
";
        let err = parse_strict(yaml).expect_err("bad unit");
        assert!(matches!(err, SchemaError::BadTimeout { .. }), "{err:?}");
    }

    #[test]
    fn retry_full_block() {
        // Spec 05 §retry syntax example.
        let yaml = "\
tasks:
  flaky_api:
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
  t:
    retry: { max_attempts: 3 }
    exec: { command: [echo] }
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
  t:
    retry: { backoff_ms: 500 }
    exec: { command: [echo] }
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
  t:
    retry: { max_attempts: 0 }
    exec: { command: [echo] }
";
        let err = parse_strict(yaml).expect_err("zero attempts");
        assert!(matches!(err, SchemaError::BadRetry { .. }), "{err:?}");
    }

    #[test]
    fn retry_bad_on_code_errors() {
        // Spec 05 · on_codes are canonical codes — not HTTP statuses.
        let yaml = "\
tasks:
  t:
    retry:
      max_attempts: 2
      on_codes: [\"503\"]
    exec: { command: [echo] }
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
  api_call:
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
  get_count:
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
    fn on_error_skip_parses_and_fail_workflow_is_refused() {
        let yaml = "\
tasks:
  a:
    exec: { command: [echo] }
    on_error: { skip: true }
";
        let wf = parse_strict(yaml).expect("parse");
        assert!(matches!(
            wf.tasks[0].value.on_error.as_ref().expect("a").value.action,
            OnErrorAction::Skip
        ));
        // The third mode died 2026-08-11 (3 modes → 2): it only ever
        // spelled the DEFAULT out loud, and the default needs no keyword.
        let dead = "\
tasks:
  b:
    exec: { command: [echo] }
    on_error: { fail_workflow: true }
";
        assert!(parse_strict(dead).is_err(), "the dead mode is refused");
    }

    #[test]
    fn on_error_scalar_teaches_the_mapping_shape() {
        // The sweep's third mute surface (2026-07-11): `on_error: recover`
        // typed bare said only « must be a mapping » — the reason now
        // carries the closed action vocabulary AND a paste-able example.
        let yaml = "\
tasks:
  t:
    exec: { command: [echo] }
    on_error: recover
";
        let err = parse_strict(yaml).expect_err("scalar on_error refused");
        let msg = err.to_string();
        assert!(msg.contains("exactly one action"), "{msg}");
        assert!(
            msg.contains("`recover:`") && msg.contains("`skip: true`"),
            "{msg}"
        );
        assert!(msg.contains("on_error: { skip: true }"), "{msg}");
    }

    #[test]
    fn on_error_two_fields_errors() {
        // Spec 05 · mutually exclusive · two-or-zero = parse error.
        let yaml = "\
tasks:
  t:
    exec: { command: [echo] }
    on_error:
      skip: true
      recover: 0
";
        let err = parse_strict(yaml).expect_err("two fields");
        assert!(matches!(err, SchemaError::BadOnError { .. }), "{err:?}");
    }

    #[test]
    fn on_error_on_codes_filter() {
        // Spec 05 · on_codes = catch-side filter beside exactly one action.
        let yaml = "\
tasks:
  slow_fetch:
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
  t:
    exec: { command: [echo] }
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
  work:
    exec: { command: [echo] }
  record:
    after: { work: terminal }
    when: true
    exec: { command: [echo] }
  never:
    after: { work: terminal }
    when: false
    exec: { command: [echo] }
  quoted:
    after: { work: terminal }
    when: \"true\"
    exec: { command: [echo] }
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
  work:
    exec: { command: [echo] }
  legacy:
    after: { work: terminal }
    when: yes
    exec: { command: [echo] }
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
  t:
    exec: { command: [echo] }
    on_error: {}
";
        let err = parse_strict(yaml).expect_err("zero fields");
        assert!(matches!(err, SchemaError::BadOnError { .. }), "{err:?}");
    }

    #[test]
    fn with_values_spanned_json() {
        let yaml = "\
tasks:
  summarize:
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
  api_call:
    invoke:
      tool: \"nika:fetch\"
      args: { url: \"https://api.example.com/data\" }
    extract:
      user_count: \".data.users | length\"
      first_user: \".data.users[0]\"
";
        let task = one_task(yaml);
        assert_eq!(task.extract.len(), 2);
        assert_eq!(task.extract[0].0.value, "user_count");
        assert_eq!(task.extract[0].1.value, ".data.users | length");
    }

    #[test]
    fn unknown_task_field_strict_errors() {
        let yaml = "\
tasks:
  t:
    condition: \"${{ x }}\"
    exec: { command: [echo] }
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
    fn tasks_sequence_is_taught_the_map() {
        // W1 · NIKA-PARSE-022: the `- id:` sequence form is dead — the map
        // is the one shape, and the teaching carries the span.
        let err = parse_strict("tasks:\n  - just_a_scalar\n").expect_err("sequence tasks");
        assert!(
            matches!(&err, SchemaError::W1TasksSequence { span: Some(_) }),
            "{err:?}"
        );
    }

    #[test]
    fn task_entry_must_be_mapping() {
        let err = parse_strict("tasks:\n  a: b\n").expect_err("scalar task");
        assert!(
            matches!(&err, SchemaError::Validation { message, .. } if message.contains("mapping")),
            "{err:?}"
        );
    }

    #[test]
    fn parse_preserves_task_span() {
        let yaml = "\
tasks:
  t1:
    exec:
      command: [echo]
";
        let wf = parse(yaml, FileId::new(9), ParseMode::Strict).expect("parse");
        assert_eq!(wf.tasks[0].span.file, FileId::new(9));
    }
}
