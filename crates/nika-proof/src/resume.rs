// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Content-addressed resume identities (ADR-099).
//!
//! This module owns the pure recipe: span-free task definitions, canonical
//! JSON bytes, domain-stable hashes, and the prior-success data carried by a
//! trace. Runtime rendering remains in `nika-runtime`; no effect crosses this
//! boundary.

use std::collections::{BTreeMap, BTreeSet};

use nika_schema::Spanned;
use nika_schema::raw::{
    ForEachValue, RawAction, RawAgentAction, RawCommand, RawExecAction, RawInferAction,
    RawInvokeAction, RawTask, RawWorkflow, VisionInput,
};
use nika_schema::types::{AfterPredicate, OnErrorAction, WhenGate};
use serde_json::{Value, json};

/// Key recipe version. A changed recipe never matches an older trace.
pub const KEY_VERSION: u32 = 3;

/// Private-use sentinel bracketing recipe markers.
pub const MARK: char = '\u{f8ff}';

/// Additive `task_completed` / `task_cache_hit` field names.
pub mod fields {
    /// Task-definition hash.
    pub const DEF_HASH: &str = "def_hash";
    /// Resolved-input hash.
    pub const INPUT_HASH: &str = "input_hash";
    /// Compact JSON output used to rehydrate a hit.
    pub const OUTPUT: &str = "output";
}

/// Resume posture when execution proceeds without a verified chain.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResumeUnverified {
    /// The operator explicitly accepted a broken chain.
    Declared(String),
    /// The source trace contained no chain.
    Unchained(String),
}

impl ResumeUnverified {
    /// Boot-manifest posture token.
    #[must_use]
    pub fn posture(&self) -> &'static str {
        match self {
            Self::Declared(_) => "declared",
            Self::Unchained(_) => "unchained",
        }
    }

    /// One-line finding recorded by the manifest.
    #[must_use]
    pub fn finding(&self) -> &str {
        match self {
            Self::Declared(finding) | Self::Unchained(finding) => finding,
        }
    }
}

/// Marker for a declared secret reference; never contains its value.
#[must_use]
pub fn secret_marker(name: &str, source: &str, key: &str) -> Value {
    Value::String(format!("{MARK}nika:secret:{name}:{source}:{key}{MARK}"))
}

fn item_marker() -> Value {
    Value::String(format!("{MARK}nika:item{MARK}"))
}

/// Build a navigable, value-free stand-in for a fan-out item.
#[must_use]
pub fn item_stand_in(items: &Value) -> Value {
    match items {
        Value::Array(arr) => {
            let mut shape = Value::Null;
            for element in arr {
                shape = merge_item_shape(&shape, element);
            }
            mask_item_leaves(&shape)
        }
        other => mask_item_leaves(other),
    }
}

fn merge_item_shape(acc: &Value, next: &Value) -> Value {
    match (acc, next) {
        (Value::Null, value) => value.clone(),
        (Value::Object(a), Value::Object(b)) => {
            let mut out = a.clone();
            for (key, value) in b {
                let existing = out.get(key).cloned().unwrap_or(Value::Null);
                out.insert(key.clone(), merge_item_shape(&existing, value));
            }
            Value::Object(out)
        }
        (Value::Array(a), Value::Array(b)) => {
            let mut out = Vec::with_capacity(a.len().max(b.len()));
            for index in 0..a.len().max(b.len()) {
                out.push(merge_item_shape(
                    a.get(index).unwrap_or(&Value::Null),
                    b.get(index).unwrap_or(&Value::Null),
                ));
            }
            Value::Array(out)
        }
        (Value::Object(_) | Value::Array(_), _) => acc.clone(),
        (_, Value::Object(_) | Value::Array(_)) => next.clone(),
        _ => acc.clone(),
    }
}

fn mask_item_leaves(value: &Value) -> Value {
    match value {
        Value::Object(map) if !map.is_empty() => Value::Object(
            map.iter()
                .map(|(key, child)| (key.clone(), mask_item_leaves(child)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(mask_item_leaves).collect()),
        _ => item_marker(),
    }
}

/// One task's typed resume identity.
#[derive(Debug, Clone)]
pub struct ResumeKey {
    v: u32,
    task: String,
    verb: String,
    definition: Value,
    inputs: Value,
}

impl ResumeKey {
    /// Assemble a key from its canonical parts.
    #[must_use]
    pub fn new(task: String, verb: String, definition: Value, inputs: Value) -> Self {
        Self {
            v: KEY_VERSION,
            task,
            verb,
            definition,
            inputs,
        }
    }

    /// Definition hash over recipe version, task, verb, and definition.
    #[must_use]
    pub fn definition_hash(&self) -> Option<String> {
        jcs_blake3(&json!({
            "v": self.v,
            "task": self.task,
            "verb": self.verb,
            "definition": self.definition,
        }))
    }

    /// Input hash over recipe version and resolved inputs.
    #[must_use]
    pub fn input_hash(&self) -> Option<String> {
        jcs_blake3(&json!({ "v": self.v, "inputs": self.inputs }))
    }

    /// Canonical input text used by the runtime's secret-material scan.
    #[must_use]
    pub fn input_jcs_text(&self) -> Option<String> {
        let folded = fold_numbers(&json!({ "v": self.v, "inputs": self.inputs }));
        serde_json_canonicalizer::to_vec(&folded)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
    }
}

fn jcs_blake3(payload: &Value) -> Option<String> {
    let folded = fold_numbers(payload);
    let bytes = serde_json_canonicalizer::to_vec(&folded).ok()?;
    Some(blake3::hash(&bytes).to_hex().to_string())
}

/// Shared canonical digest door for receipt families.
#[must_use]
pub fn jcs_blake3_hex(payload: &Value) -> Option<String> {
    jcs_blake3(payload)
}

fn fold_numbers(value: &Value) -> Value {
    match value {
        Value::Number(number) => Value::String(format!("{MARK}num:{number}{MARK}")),
        Value::Array(items) => Value::Array(items.iter().map(fold_numbers).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), fold_numbers(value)))
                .collect(),
        ),
        scalar => scalar.clone(),
    }
}

/// One journaled success eligible for a resume hit.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PriorSuccess {
    /// Journaled definition hash.
    pub def_hash: String,
    /// Journaled input hash.
    pub input_hash: String,
    /// Journaled output.
    pub output: Value,
}

impl PriorSuccess {
    /// Construct a prior success.
    #[must_use]
    pub fn new(def_hash: String, input_hash: String, output: Value) -> Self {
        Self {
            def_hash,
            input_hash,
            output,
        }
    }
}

/// Prior successes indexed by task id.
pub type ResumePlan = BTreeMap<String, PriorSuccess>;

/// Task ids observed by a task definition.
#[must_use]
pub fn referenced_upstreams(task: &RawTask) -> BTreeSet<String> {
    let mut out = nika_check::analyzer::edges::producer_ids(task)
        .into_iter()
        .collect();
    if let Some(definition) = definition_value(task) {
        scan_task_refs(&definition.to_string(), &mut out);
    }
    out
}

fn scan_task_refs(text: &str, out: &mut BTreeSet<String>) {
    let mut rest = text;
    while let Some(at) = rest.find("tasks.") {
        let after = &rest[at + "tasks.".len()..];
        let end = after
            .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
            .unwrap_or(after.len());
        if end > 0 {
            out.insert(after[..end].to_owned());
        }
        rest = &after[end..];
    }
}

/// Whether a task executes an inference-bearing action.
#[must_use]
pub fn touches_intelligence(task: &RawTask) -> bool {
    matches!(task.action, RawAction::Infer(_) | RawAction::Agent(_))
}

/// Static child-workflow targets carried by a task definition.
#[must_use]
pub fn workflow_targets(task: &RawTask) -> Vec<&str> {
    match &task.action {
        RawAction::Invoke(action) => match &action.target {
            nika_schema::raw::RawInvokeTarget::Workflow(workflow) => {
                vec![workflow.value.as_str()]
            }
            nika_schema::raw::RawInvokeTarget::Tool(_) => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Agent-skill paths carried by a task definition.
#[must_use]
pub fn skill_paths(task: &RawTask) -> Vec<&str> {
    match &task.action {
        RawAction::Agent(action) => action
            .skills
            .iter()
            .map(|skill| skill.value.as_str())
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether a task resolves its model from the run default.
#[must_use]
pub fn reads_default_model(task: &RawTask) -> bool {
    match &task.action {
        RawAction::Infer(action) => action.model.is_none(),
        RawAction::Agent(action) => action.model.is_none(),
        _ => false,
    }
}

/// Cleanup tasks attached to `producer`, in declaration order.
#[must_use]
pub fn unwind_tasks_of<'a>(wf: &'a RawWorkflow, producer: &str) -> Vec<&'a RawTask> {
    wf.tasks
        .iter()
        .map(|task| &task.value)
        .filter(|task| {
            task.after.iter().any(|(target, predicate)| {
                target.value == producer && matches!(predicate.value, AfterPredicate::Unwind)
            })
        })
        .collect()
}

/// Span-free, behavior-bearing task definition used by resume and proof.
#[must_use]
pub fn definition_value(task: &RawTask) -> Option<Value> {
    Some(json!({
        "after": task.after.iter()
            .map(|(target, pred)| json!([target.value, pred.value.as_str()]))
            .collect::<Vec<_>>(),
        "when": when_value(task.when.as_ref()),
        "for_each": for_each_raw(task.for_each.as_ref())?,
        "max_parallel": task.max_parallel.as_ref().map(|m| m.value),
        "fail_fast": task.fail_fast.as_ref().map(|f| f.value),
        "retry": retry_value(task),
        "on_error": on_error_value(task)?,
        "timeout_ms": task.timeout.as_ref().map(duration_ms),
        "with": raw_with_object(&task.with),
        "output": task.extract.iter()
            .map(|(name, program)| (name.value.clone(), Value::String(program.value.clone())))
            .collect::<serde_json::Map<_, _>>(),
        "action": raw_action_value(&task.action)?,
    }))
}

fn when_value(when: Option<&Spanned<WhenGate>>) -> Value {
    match when.map(|value| &value.value) {
        None => Value::Null,
        Some(WhenGate::Literal(value)) => json!({ "literal": value }),
        Some(WhenGate::Expr(expression)) => json!({ "expr": expression }),
    }
}

fn for_each_raw(for_each: Option<&Spanned<ForEachValue>>) -> Option<Value> {
    Some(match for_each.map(|value| &value.value) {
        None => Value::Null,
        Some(ForEachValue::Expression(expression)) => json!({ "expr": expression }),
        Some(ForEachValue::List(value)) => json!({ "list": value }),
        Some(_) => return None,
    })
}

fn retry_value(task: &RawTask) -> Value {
    match task.retry.as_ref().map(|retry| &retry.value) {
        None => Value::Null,
        Some(retry) => json!({
            "max_attempts": retry.max_attempts,
            "backoff_ms": retry.backoff_ms,
            "backoff_strategy": retry.backoff_strategy.to_string(),
            "backoff_max_ms": retry.backoff_max_ms,
            "jitter": retry.jitter,
            "on_codes": retry.on_codes,
        }),
    }
}

fn on_error_value(task: &RawTask) -> Option<Value> {
    let Some(on_error) = task.on_error.as_ref().map(|value| &value.value) else {
        return Some(Value::Null);
    };
    let action = match &on_error.action {
        OnErrorAction::Recover(value) => json!({ "recover": value.value }),
        OnErrorAction::Skip => json!("skip"),
        _ => return None,
    };
    Some(json!({
        "action": action,
        "on_codes": on_error.on_codes.iter().map(|code| code.value.clone()).collect::<Vec<_>>(),
    }))
}

fn duration_ms(duration: &Spanned<std::time::Duration>) -> u64 {
    u64::try_from(duration.value.as_millis()).unwrap_or(u64::MAX)
}

fn raw_with_object(with: &[(Spanned<String>, Spanned<Value>)]) -> Value {
    Value::Object(
        with.iter()
            .map(|(key, value)| (key.value.clone(), value.value.clone()))
            .collect(),
    )
}

fn raw_action_value(action: &RawAction) -> Option<Value> {
    Some(match action {
        RawAction::Infer(action) => json!({ "infer": raw_infer_value(action)? }),
        RawAction::Exec(action) => json!({ "exec": raw_exec_value(action)? }),
        RawAction::Invoke(action) => json!({ "invoke": raw_invoke_value(action) }),
        RawAction::Agent(action) => json!({ "agent": raw_agent_value(action) }),
        _ => return None,
    })
}

fn raw_text(value: &Spanned<String>) -> Value {
    Value::String(value.value.clone())
}

fn raw_opt_text(value: Option<&Spanned<String>>) -> Value {
    value.map_or(Value::Null, raw_text)
}

fn raw_json(value: Option<&Spanned<Value>>) -> Value {
    value.map_or(Value::Null, |value| value.value.clone())
}

fn raw_infer_value(action: &RawInferAction) -> Option<Value> {
    let vision = action
        .vision
        .iter()
        .map(|value| match &value.value {
            VisionInput::File { path } => Some(json!({ "file": raw_text(path) })),
            VisionInput::Url { url } => Some(json!({ "url": raw_text(url) })),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(json!({
        "prompt": raw_text(&action.prompt),
        "system": raw_opt_text(action.system.as_ref()),
        "model": raw_opt_text(action.model.as_ref()),
        "temperature": action.temperature.as_ref().map(|value| value.value.to_string()),
        "max_tokens": action.max_tokens.as_ref().map(|value| value.value),
        "schema": raw_json(action.schema.as_ref()),
        "thinking": action.thinking.as_ref().map(|value| json!({
            "enabled": value.value.enabled,
            "budget_tokens": value.value.budget_tokens,
        })),
        "vision": vision,
    }))
}

fn raw_exec_value(action: &RawExecAction) -> Option<Value> {
    let command = match &action.command {
        RawCommand::Shell(shell) => json!({ "shell": raw_text(shell) }),
        RawCommand::Argv(parts) => {
            json!({ "argv": parts.iter().map(raw_text).collect::<Vec<_>>() })
        }
        _ => return None,
    };
    let env = action
        .env
        .iter()
        .map(|(key, value)| (key.value.clone(), raw_text(value)))
        .collect::<serde_json::Map<_, _>>();
    let capture = match action.capture.as_ref().map(|value| value.value) {
        None => Value::Null,
        Some(nika_schema::types::CaptureMode::Stdout) => json!("stdout"),
        Some(nika_schema::types::CaptureMode::Stderr) => json!("stderr"),
        Some(nika_schema::types::CaptureMode::Combined) => json!("combined"),
        Some(nika_schema::types::CaptureMode::Structured) => json!("structured"),
        Some(_) => return None,
    };
    Some(json!({
        "command": command,
        "cwd": raw_opt_text(action.cwd.as_ref()),
        "env": env,
        "stdin": raw_opt_text(action.stdin.as_ref()),
        "capture": capture,
    }))
}

fn raw_invoke_value(action: &RawInvokeAction) -> Value {
    match &action.target {
        nika_schema::raw::RawInvokeTarget::Tool(tool) => json!({
            "tool": raw_text(tool),
            "args": raw_json(action.args.as_ref()),
        }),
        nika_schema::raw::RawInvokeTarget::Workflow(workflow) => json!({
            "workflow": raw_text(workflow),
            "args": raw_json(action.args.as_ref()),
        }),
    }
}

fn raw_agent_value(action: &RawAgentAction) -> Value {
    json!({
        "prompt": raw_text(&action.prompt),
        "system": raw_opt_text(action.system.as_ref()),
        "model": raw_opt_text(action.model.as_ref()),
        "tools": action.tools.iter().map(|value| value.value.clone()).collect::<Vec<_>>(),
        "skills": action.skills.iter().map(|value| value.value.clone()).collect::<Vec<_>>(),
        "max_turns": action.max_turns.as_ref().map(|value| value.value),
        "max_tokens_total": action.max_tokens_total.as_ref().map(|value| value.value),
        "temperature": action.temperature.as_ref().map(|value| value.value.to_string()),
        "schema": raw_json(action.schema.as_ref()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_version_participates_in_both_hashes() {
        let base = ResumeKey::new("t".into(), "exec".into(), json!({}), json!({}));
        let mut bumped = base.clone();
        bumped.v = KEY_VERSION + 1;
        assert_ne!(base.definition_hash(), bumped.definition_hash());
        assert_ne!(base.input_hash(), bumped.input_hash());
    }

    #[test]
    fn fold_numbers_tags_every_number_at_every_depth() {
        let folded = fold_numbers(&json!({ "a": [1, { "b": 2.5 }], "c": "s" }));
        assert_eq!(
            folded,
            json!({
                "a": [format!("{MARK}num:1{MARK}"), { "b": format!("{MARK}num:2.5{MARK}") }],
                "c": "s"
            })
        );
    }
}
