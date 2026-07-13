// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ADR-099 resume keys — the trace IS the checkpoint.
//!
//! Every task that settles as a SUCCESS gets a content-addressed identity
//! stamped onto its `task_completed` trace record (additive NDJSON
//! fields): a **task-definition hash** over the behavior-bearing fields
//! as written, and a **resolved-input hash** over the values its
//! `${{ }}` references resolved to. `nika run --resume <trace>` skips a
//! task iff BOTH match a journaled success (ADR-099 §1) — an edited task
//! or a changed input NEVER silently skips (trap 6).
//!
//! ## The key recipe (ADR-099 + the production-lore brief)
//!
//! - **IN** · task id · verb kind · the raw behavior-bearing fields
//!   (definition) · the rendered input values (action fields + `with:` +
//!   the resolved `for_each` collection) · secrets **by declared
//!   reference identity** (name · source · key — NEVER the value, a hash
//!   of a low-entropy secret is an oracle). Upstream invalidation rides
//!   the rendered values themselves: a changed upstream output changes
//!   exactly the downstream renders it actually flows into.
//! - **OUT** · file mtimes · absolute host paths not referenced by the
//!   task · ambient env (`$HOME`/`$PATH`) · clocks/RNG/PID · run-id /
//!   attempt metadata · map iteration order + serializer whitespace
//!   (RFC 8785 JCS canonicalizes first) · the hash fields themselves.
//!
//! ## Determinism choices
//!
//! - **JCS (RFC 8785)** canonical bytes via `serde_json_canonicalizer`,
//!   then **blake3** — never `BTreeMap` ordering alone, never `Debug`.
//! - **No floats in the key structure**: `temperature:` rides as its
//!   display string; every JSON **number** in a payload is pre-folded to
//!   a tagged string of its `serde_json` literal (full i64/u64 fidelity —
//!   JCS alone serializes numbers as ES6 doubles, which collapses
//!   distinct int64s beyond 2^53 · the CEL `numeric_cmp` bug class).
//! - **Fail-closed eligibility**: any form this recipe cannot serialize
//!   (`#[non_exhaustive]` future variants) or render (deep `item.`
//!   navigation under the fan-out placeholder), and any payload that
//!   would carry a resolved secret VALUE (leaked through an upstream
//!   record), yields NO stamp — the task simply never skips. Honest
//!   degradation, never a wrong skip, never an error.

use std::collections::BTreeMap;

use nika_schema::Spanned;
use nika_schema::raw::{
    ForEachValue, RawAction, RawAgentAction, RawCommand, RawExecAction, RawFinallyTask,
    RawInferAction, RawInvokeAction, RawTask, RawWorkflow, VisionInput,
};
use nika_schema::types::{OnErrorAction, WhenGate};
use serde_json::{Value, json};

use crate::expr::{self, Scope};
use crate::record::TaskRecord;

/// The key-recipe version — bumped when the payload shape changes, so a
/// trace stamped by an older recipe simply never matches (re-runs ·
/// honest) instead of matching wrongly.
pub const KEY_VERSION: u32 = 1;

/// The additive `task_completed` / `task_cache_hit` trace field names
/// (ADR-099 · the compatibility surface: these evolve additively).
pub mod fields {
    /// The task-definition hash (blake3 hex over the JCS definition payload).
    pub const DEF_HASH: &str = "def_hash";
    /// The resolved-input hash (blake3 hex over the JCS input payload).
    pub const INPUT_HASH: &str = "input_hash";
    /// The task's output as ONE compact JSON text (rehydration source).
    pub const OUTPUT: &str = "output";
}

/// Private-use sentinel bracketing the marker vocabulary below — a real
/// workflow string colliding with a marker requires deliberately crafted
/// `U+F8FF` data (documented, adversarial-self-harm class).
const MARK: char = '\u{f8ff}';

/// The `secrets.<name>` stand-in bound during key rendering — the secret
/// participates by declared reference identity (name · source · key),
/// never by value (ADR-099 §1).
fn secret_marker(name: &str, source: &str, key: &str) -> Value {
    Value::String(format!("{MARK}nika:secret:{name}:{source}:{key}{MARK}"))
}

/// The `item` stand-in bound during a fan-out key render — the real
/// per-item data participates through the resolved collection itself.
fn item_marker() -> Value {
    Value::String(format!("{MARK}nika:item{MARK}"))
}

/// One task's resume identity — the typed key payload (ADR-099 · brief
/// §4: a dedicated struct, JCS + blake3, no float fields).
///
/// Fields are private on purpose: the shape IS the compatibility surface
/// (`KEY_VERSION` guards it) — construct via [`ResumeKey::new`], read via
/// the two hash accessors.
#[derive(Debug, Clone)]
pub struct ResumeKey {
    /// Key-recipe version (participates in both hashes).
    v: u32,
    /// The task id (a renamed task is a new identity).
    task: String,
    /// The verb kind (`infer` · `exec` · `invoke` · `agent`).
    verb: String,
    /// The behavior-bearing fields as WRITTEN (raw template strings).
    definition: Value,
    /// The values the task's references RESOLVED to (secrets as markers).
    inputs: Value,
}

impl ResumeKey {
    /// Assemble a key from its typed parts (the builders below produce
    /// `definition` / `inputs`; tests may hand-build payloads).
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

    /// The task-definition hash — blake3 hex over the JCS bytes of
    /// `{v, task, verb, definition}`. `None` = the payload cannot
    /// canonicalize (the task is then not resume-eligible).
    #[must_use]
    pub fn definition_hash(&self) -> Option<String> {
        jcs_blake3(&json!({
            "v": self.v,
            "task": self.task,
            "verb": self.verb,
            "definition": self.definition,
        }))
    }

    /// The resolved-input hash — blake3 hex over the JCS bytes of
    /// `{v, inputs}`.
    #[must_use]
    pub fn input_hash(&self) -> Option<String> {
        jcs_blake3(&json!({ "v": self.v, "inputs": self.inputs }))
    }

    /// The canonical input bytes as text — the secret-material scan
    /// surface (a resolved secret value that leaked into a rendered
    /// input must disqualify the stamp, not ride into a hash oracle).
    fn input_jcs_text(&self) -> Option<String> {
        let folded = fold_numbers(&json!({ "v": self.v, "inputs": self.inputs }));
        serde_json_canonicalizer::to_vec(&folded)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
    }
}

/// JCS-canonicalize (numbers pre-folded to tagged literals) then blake3.
fn jcs_blake3(payload: &Value) -> Option<String> {
    let folded = fold_numbers(payload);
    let bytes = serde_json_canonicalizer::to_vec(&folded).ok()?;
    Some(blake3::hash(&bytes).to_hex().to_string())
}

/// Replace every JSON number with a tagged string of its `serde_json`
/// literal — full int64/float fidelity under JCS (RFC 8785 alone
/// serializes numbers as ES6 doubles: two int64s beyond 2^53 would
/// canonicalize identically and could WRONG-SKIP · the one unforgivable
/// failure mode).
fn fold_numbers(value: &Value) -> Value {
    match value {
        Value::Number(n) => Value::String(format!("{MARK}num:{n}{MARK}")),
        Value::Array(items) => Value::Array(items.iter().map(fold_numbers).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), fold_numbers(v)))
                .collect(),
        ),
        scalar => scalar.clone(),
    }
}

/// The two hex hashes a settled success stamps onto its trace record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResumeStamp {
    pub def_hash: String,
    pub input_hash: String,
}

/// One journaled success read back from a trace — the skip candidate
/// `--resume` folds per task id (ADR-099 §1: a task skips iff BOTH
/// hashes match what THIS run recomputes).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PriorSuccess {
    /// The journaled task-definition hash.
    pub def_hash: String,
    /// The journaled resolved-input hash.
    pub input_hash: String,
    /// The journaled output (rehydrated on a hit — downstream observes
    /// `status: success` and this value exactly as if it ran live).
    pub output: Value,
}

impl PriorSuccess {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(def_hash: String, input_hash: String, output: Value) -> Self {
        Self {
            def_hash,
            input_hash,
            output,
        }
    }
}

/// The fold of a prior trace — task id → its journaled success identity.
/// Built by the composer (the CLI reads the NDJSON trace); consumed via
/// [`crate::Runtime::with_resume_plan`].
pub type ResumePlan = BTreeMap<String, PriorSuccess>;

/// The task ids a task's definition can observe — `depends_on` plus every
/// `tasks.<id>` token in its raw template text. The `--from <task_id>`
/// override walks this REVERSED to force the transitive downstream to
/// re-run even on a hash match (ADR-099 §3). Over-collection is the safe
/// direction (more re-runs, never a wrong skip).
#[must_use]
pub fn referenced_upstreams(task: &RawTask) -> std::collections::BTreeSet<String> {
    let mut out: std::collections::BTreeSet<String> =
        task.depends_on.iter().map(|d| d.value.clone()).collect();
    if let Some(def) = definition_value(task) {
        scan_task_refs(&def.to_string(), &mut out);
    }
    out
}

/// Collect every `tasks.<snake_case_id>` token in `text` (task ids are
/// checker-enforced `snake_case`, so the boundary scan is exact enough —
/// and a false positive only ever forces an extra re-run).
fn scan_task_refs(text: &str, out: &mut std::collections::BTreeSet<String>) {
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

/// Per-run resume context — derived ONCE at run start from the envelope.
pub(crate) struct ResumeContext {
    /// `secrets.<name>` → its by-reference marker (every DECLARED secret,
    /// resolved or not — the key render must never see a value).
    markers: BTreeMap<String, Value>,
    /// The RESOLVED secret values — the leak-guard scan set (a value that
    /// reached a rendered input or an output disqualifies the stamp).
    secret_values: Vec<String>,
    /// The EFFECTIVE default model this run resolves model-less
    /// infer/agent tasks against (`--model` override, else the envelope
    /// `model:` line). Part of those tasks' definition identity (#409 ·
    /// ADR-099 §1: the model an inference runs on IS behavior-bearing —
    /// swapping the envelope model and resuming used to serve the OLD
    /// model's cached output as a hit).
    default_model: Option<String>,
    /// The composer-resolved Agent Skills (`skills:` path → SKILL.md raw
    /// text). A referencing task's DEFINITION identity covers the TEXT,
    /// not just the path (spec 02 §agent skills · the same ADR-099 law
    /// as an edited prompt: a changed skill re-runs the task).
    skills: BTreeMap<String, String>,
}

impl ResumeContext {
    /// Build the context from the workflow's declared `secrets:` block +
    /// the run's resolved values + the composer's `--model` override
    /// (the effective default model falls back to the envelope's) + the
    /// composer-resolved skill texts.
    pub(crate) fn of(
        wf: &RawWorkflow,
        resolved: &BTreeMap<String, Value>,
        model_override: Option<&str>,
        skills: &BTreeMap<String, String>,
    ) -> Self {
        let markers = wf
            .secrets
            .iter()
            .map(|(name, reference)| {
                let source = reference.value.source.to_string();
                (
                    name.value.clone(),
                    secret_marker(&name.value, &source, &reference.value.key),
                )
            })
            .collect();
        let secret_values = resolved
            .values()
            .filter_map(|v| match v {
                Value::String(s) if !s.is_empty() => Some(s.clone()),
                _ => None,
            })
            .collect();
        let default_model = model_override
            .filter(|m| !m.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| wf.model.as_ref().map(|m| m.value.clone()));
        Self {
            markers,
            secret_values,
            default_model,
            skills: skills.clone(),
        }
    }

    /// Does `text` carry any resolved secret value? (The trace MUST NOT
    /// carry secret-derived material — ADR-099 §1.)
    pub(crate) fn leaks_secret(&self, text: &str) -> bool {
        self.secret_values.iter().any(|v| text.contains(v.as_str()))
    }

    /// The by-reference secret markers — the pause rider renders its
    /// journal payload over these (never a resolved value).
    pub(crate) fn markers(&self) -> &BTreeMap<String, Value> {
        &self.markers
    }
}

/// Compute one task's [`ResumeStamp`] — `None` means the task is not
/// resume-eligible this run (future form · render miss · secret leak):
/// it records no key and never skips. Never an error (ADR-099).
pub(crate) fn stamp(
    task: &RawTask,
    records: &BTreeMap<String, TaskRecord>,
    vars: &BTreeMap<String, Value>,
    env: &BTreeMap<String, Value>,
    ctx: &ResumeContext,
) -> Option<ResumeStamp> {
    let mut definition = definition_value(task)?;
    // #409 · a model-less infer/agent task RUNS on the effective default
    // model, so that model joins its DEFINITION identity — swapping the
    // envelope `model:` (or `--model`) re-runs it instead of cache-hitting
    // the old model's output. Tasks that pin their own `model:` already
    // carry it in the definition; the envelope cannot affect them.
    if reads_default_model(task)
        && let Some(model) = ctx.default_model.as_deref()
        && let Some(obj) = definition.as_object_mut()
    {
        obj.insert("default_model".to_owned(), json!(model));
    }
    // #473 · an agent task's `skills:` TEXTS join its definition identity
    // (spec 02 §agent skills · the same law as an edited prompt): editing
    // a SKILL.md re-runs every task that carries it — the paths alone
    // (already in the definition) would cache-hit a stale injection. A
    // referenced path the composer did not resolve makes the task
    // non-eligible (records no key · never skips) — the honest degrade.
    let paths = skill_paths(task);
    if !paths.is_empty() {
        let mut contents = serde_json::Map::new();
        for path in paths {
            let text = ctx.skills.get(path)?;
            contents.insert(path.to_owned(), json!(text));
        }
        definition
            .as_object_mut()?
            .insert("skills_content".to_owned(), Value::Object(contents));
    }
    let inputs = input_value(task, records, vars, env, &ctx.markers)?;
    let key = ResumeKey::new(
        task.id.value.clone(),
        task.action.verb().to_owned(),
        definition,
        inputs,
    );
    // A secret value that flowed into a rendered input (through an
    // upstream record) would make the input hash an oracle — refuse.
    if ctx.leaks_secret(&key.input_jcs_text()?) {
        return None;
    }
    Some(ResumeStamp {
        def_hash: key.definition_hash()?,
        input_hash: key.input_hash()?,
    })
}

/// Does this task's behavior depend on the run's DEFAULT model? True
/// when any of its actions (the main verb · every `on_finally` mini) is
/// an infer/agent WITHOUT its own `model:` — those resolve against the
/// envelope/`--model` default at dispatch, so that default is part of
/// their behavior (#409).
fn reads_default_model(task: &RawTask) -> bool {
    let action_reads = |action: &RawAction| match action {
        RawAction::Infer(a) => a.model.is_none(),
        RawAction::Agent(a) => a.model.is_none(),
        _ => false,
    };
    action_reads(&task.action)
        || task
            .on_finally
            .iter()
            .any(|mini| action_reads(&mini.value.action))
}

/// Every `skills:` path this task carries (the main verb + every
/// `on_finally` mini) — declaration order · duplicates deduped by the
/// map they land in. The per-task twin of `nika_schema::skill_refs`.
fn skill_paths(task: &RawTask) -> Vec<&str> {
    fn of(action: &RawAction) -> Vec<&str> {
        match action {
            RawAction::Agent(a) => a.skills.iter().map(|s| s.value.as_str()).collect(),
            _ => Vec::new(),
        }
    }
    let mut out = of(&task.action);
    for mini in &task.on_finally {
        out.extend(of(&mini.value.action));
    }
    out
}

// ─── the definition payload (raw · behavior-bearing fields as written) ──

/// The behavior-bearing fields as WRITTEN (ADR-099 §1: the verb body ·
/// `with:` · `output:` · `retry:`/`on_error:`/`on_finally:` · `when:` ·
/// `for_each:` — plus the scheduling knobs that change behavior). `None`
/// on any `#[non_exhaustive]` form this recipe does not know.
fn definition_value(task: &RawTask) -> Option<Value> {
    Some(json!({
        "depends_on": task.depends_on.iter().map(|d| d.value.clone()).collect::<Vec<_>>(),
        "when": when_value(task.when.as_ref())?,
        "for_each": for_each_raw(task.for_each.as_ref())?,
        "max_parallel": task.max_parallel.as_ref().map(|m| m.value),
        "fail_fast": task.fail_fast.as_ref().map(|f| f.value),
        "retry": retry_value(task),
        "on_error": on_error_value(task)?,
        "timeout_ms": task.timeout.as_ref().map(duration_ms),
        "with": raw_with_object(&task.with),
        "output": task.output.iter()
            .map(|(name, program)| (name.value.clone(), Value::String(program.value.clone())))
            .collect::<serde_json::Map<_, _>>(),
        "on_finally": finally_values(&task.on_finally)?,
        "action": action_value(&task.action, None)?,
    }))
}

fn when_value(when: Option<&Spanned<WhenGate>>) -> Option<Value> {
    Some(match when.map(|w| &w.value) {
        None => Value::Null,
        Some(WhenGate::Literal(b)) => json!({ "literal": b }),
        Some(WhenGate::Expr(e)) => json!({ "expr": e }),
        // A future gate form is out of this recipe — not eligible.
        Some(_) => return None,
    })
}

fn for_each_raw(for_each: Option<&Spanned<ForEachValue>>) -> Option<Value> {
    Some(match for_each.map(|f| &f.value) {
        None => Value::Null,
        Some(ForEachValue::Expression(e)) => json!({ "expr": e }),
        Some(ForEachValue::List(v)) => json!({ "list": v }),
        Some(_) => return None,
    })
}

fn retry_value(task: &RawTask) -> Value {
    match task.retry.as_ref().map(|r| &r.value) {
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
    let Some(on_error) = task.on_error.as_ref().map(|o| &o.value) else {
        return Some(Value::Null);
    };
    let action = match &on_error.action {
        OnErrorAction::Recover(v) => json!({ "recover": v.value }),
        OnErrorAction::Skip => json!("skip"),
        OnErrorAction::FailWorkflow => json!("fail_workflow"),
        _ => return None,
    };
    Some(json!({
        "action": action,
        "on_codes": on_error.on_codes.iter().map(|c| c.value.clone()).collect::<Vec<_>>(),
    }))
}

fn finally_values(cleanups: &[Spanned<RawFinallyTask>]) -> Option<Value> {
    let mut out = Vec::with_capacity(cleanups.len());
    for mini in cleanups {
        out.push(json!({
            "when": when_value(mini.value.when.as_ref())?,
            "timeout_ms": mini.value.timeout.as_ref().map(duration_ms),
            "action": action_value(&mini.value.action, None)?,
        }));
    }
    Some(Value::Array(out))
}

fn duration_ms(d: &Spanned<std::time::Duration>) -> u64 {
    u64::try_from(d.value.as_millis()).unwrap_or(u64::MAX)
}

/// The raw `with:` pairs as an object — JCS sorts the keys, so authored
/// order never leaks into the hash (trap 3/5 · never completion order).
fn raw_with_object(with: &[(Spanned<String>, Spanned<Value>)]) -> Value {
    Value::Object(
        with.iter()
            .map(|(k, v)| (k.value.clone(), v.value.clone()))
            .collect(),
    )
}

// ─── the input payload (rendered · what the references resolved to) ─────

/// The values the task's `${{ }}` references resolve to RIGHT NOW —
/// the action fields + the `with:` namespace rendered over the run
/// scope (secrets bound as markers), plus the resolved `for_each`
/// collection. `None` = a reference does not render here (the task
/// then runs live and surfaces its real error, or — fan-out deep
/// `item.` navigation — is simply not resume-eligible).
fn input_value(
    task: &RawTask,
    records: &BTreeMap<String, TaskRecord>,
    vars: &BTreeMap<String, Value>,
    env: &BTreeMap<String, Value>,
    markers: &BTreeMap<String, Value>,
) -> Option<Value> {
    let workflow_scope = Scope {
        records,
        vars,
        env,
        secrets: markers,
        with_ns: None,
        item: None,
        index: None,
        permits: None,
    };
    // The collection is the ONE once-evaluated body expression (spec 03)
    // — it carries every per-item value into the hash.
    let items = match task.for_each.as_ref().map(|f| &f.value) {
        None => Value::Null,
        Some(ForEachValue::List(v)) => expr::render_json(v, &workflow_scope).ok()?,
        Some(ForEachValue::Expression(e)) => {
            expr::render_json(&Value::String(e.clone()), &workflow_scope).ok()?
        }
        Some(_) => return None,
    };
    // Fan-out renders bind `item`/`index` to fixed placeholders: the real
    // data already rides in `items`; a template the placeholder cannot
    // satisfy (deep `item.field`) fails the render → not eligible.
    let item_stand_in = item_marker();
    let (item, index) = if task.for_each.is_some() {
        (Some(&item_stand_in), Some(0))
    } else {
        (None, None)
    };
    let base = Scope {
        records,
        vars,
        env,
        secrets: markers,
        with_ns: None,
        item,
        index,
        permits: None,
    };
    let mut with_ns: BTreeMap<String, Value> = BTreeMap::new();
    for (key, value) in &task.with {
        with_ns.insert(
            key.value.clone(),
            expr::render_json(&value.value, &base).ok()?,
        );
    }
    let action_scope = Scope {
        with_ns: Some(&with_ns),
        ..base
    };
    Some(json!({
        "action": action_value(&task.action, Some(&action_scope))?,
        "with": with_ns,
        "items": items,
    }))
}

// ─── the action payload (shared walk · raw when scope is None) ──────────

/// One verb body as a canonical payload. `scope: None` = the RAW template
/// strings (definition side) · `Some` = every templated field rendered to
/// the value it resolves to (input side). ONE walk for both sides, so a
/// field can never be covered by one hash and missed by the other.
fn action_value(action: &RawAction, scope: Option<&Scope<'_>>) -> Option<Value> {
    Some(match action {
        RawAction::Infer(a) => json!({ "infer": infer_value(a, scope)? }),
        RawAction::Exec(a) => json!({ "exec": exec_value(a, scope)? }),
        RawAction::Invoke(a) => json!({ "invoke": invoke_value(a, scope)? }),
        RawAction::Agent(a) => json!({ "agent": agent_value(a, scope)? }),
        // A future verb is out of this recipe — not eligible.
        _ => return None,
    })
}

/// A templated string field — raw, or rendered to its resolved value.
fn text(s: &Spanned<String>, scope: Option<&Scope<'_>>) -> Option<Value> {
    match scope {
        None => Some(Value::String(s.value.clone())),
        Some(sc) => expr::render_json(&Value::String(s.value.clone()), sc).ok(),
    }
}

/// An optional templated string field (`Null` when absent).
fn opt_text(s: Option<&Spanned<String>>, scope: Option<&Scope<'_>>) -> Option<Value> {
    match s {
        None => Some(Value::Null),
        Some(s) => text(s, scope),
    }
}

/// A templated JSON field (`args:` · `schema:`) — raw, or deep-rendered.
fn json_field(v: Option<&Spanned<Value>>, scope: Option<&Scope<'_>>) -> Option<Value> {
    match (v, scope) {
        (None, _) => Some(Value::Null),
        (Some(v), None) => Some(v.value.clone()),
        (Some(v), Some(sc)) => expr::render_json(&v.value, sc).ok(),
    }
}

fn infer_value(a: &RawInferAction, scope: Option<&Scope<'_>>) -> Option<Value> {
    let vision = a
        .vision
        .iter()
        .map(|v| match &v.value {
            VisionInput::File { path } => text(path, scope).map(|p| json!({ "file": p })),
            VisionInput::Url { url } => text(url, scope).map(|u| json!({ "url": u })),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(json!({
        "prompt": text(&a.prompt, scope)?,
        "system": opt_text(a.system.as_ref(), scope)?,
        "model": opt_text(a.model.as_ref(), scope)?,
        // No float fields in the key: the sampling temperature rides as
        // its display string (deterministic shortest round-trip repr).
        "temperature": a.temperature.as_ref().map(|t| t.value.to_string()),
        "max_tokens": a.max_tokens.as_ref().map(|m| m.value),
        "schema": json_field(a.schema.as_ref(), scope)?,
        "thinking": a.thinking.as_ref().map(|t| json!({
            "enabled": t.value.enabled,
            "budget_tokens": t.value.budget_tokens,
        })),
        "vision": vision,
    }))
}

fn exec_value(a: &RawExecAction, scope: Option<&Scope<'_>>) -> Option<Value> {
    let command = match &a.command {
        RawCommand::Shell(s) => json!({ "shell": text(s, scope)? }),
        RawCommand::Argv(parts) => json!({
            "argv": parts.iter().map(|p| text(p, scope)).collect::<Option<Vec<_>>>()?
        }),
        _ => return None,
    };
    let mut env = serde_json::Map::new();
    for (key, value) in &a.env {
        env.insert(key.value.clone(), text(value, scope)?);
    }
    let capture = match a.capture.as_ref().map(|c| c.value) {
        None => Value::Null,
        Some(mode) => {
            // The closed spec enum (a future mode → not eligible).
            let word = match mode {
                nika_schema::types::CaptureMode::Stdout => "stdout",
                nika_schema::types::CaptureMode::Stderr => "stderr",
                nika_schema::types::CaptureMode::Combined => "combined",
                nika_schema::types::CaptureMode::Structured => "structured",
                _ => return None,
            };
            Value::String(word.to_owned())
        }
    };
    Some(json!({
        "command": command,
        "cwd": opt_text(a.cwd.as_ref(), scope)?,
        "env": env,
        "stdin": opt_text(a.stdin.as_ref(), scope)?,
        "capture": capture,
    }))
}

fn invoke_value(a: &RawInvokeAction, scope: Option<&Scope<'_>>) -> Option<Value> {
    Some(json!({
        "tool": text(&a.tool, scope)?,
        "args": json_field(a.args.as_ref(), scope)?,
    }))
}

fn agent_value(a: &RawAgentAction, scope: Option<&Scope<'_>>) -> Option<Value> {
    Some(json!({
        "prompt": text(&a.prompt, scope)?,
        "system": opt_text(a.system.as_ref(), scope)?,
        "model": opt_text(a.model.as_ref(), scope)?,
        "tools": a.tools.iter().map(|t| t.value.clone()).collect::<Vec<_>>(),
        // Static paths (never templated · parser-enforced) — the TEXTS
        // ride the definition separately (`skills_content` · stamp()).
        "skills": a.skills.iter().map(|s| s.value.clone()).collect::<Vec<_>>(),
        "max_turns": a.max_turns.as_ref().map(|m| m.value),
        "max_tokens_total": a.max_tokens_total.as_ref().map(|m| m.value),
        "temperature": a.temperature.as_ref().map(|t| t.value.to_string()),
        "schema": json_field(a.schema.as_ref(), scope)?,
    }))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::record::{TaskRecord, TaskStatus};

    fn parse(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    fn no_secrets() -> ResumeContext {
        ResumeContext {
            markers: BTreeMap::new(),
            secret_values: Vec::new(),
            default_model: None,
            skills: BTreeMap::new(),
        }
    }

    fn stamp_of(
        yaml: &str,
        records: &BTreeMap<String, TaskRecord>,
        vars: &BTreeMap<String, Value>,
    ) -> Option<ResumeStamp> {
        let wf = parse(yaml);
        let ctx = no_secrets();
        stamp(&wf.tasks[0].value, records, vars, &BTreeMap::new(), &ctx)
    }

    fn success_record(output: Value) -> TaskRecord {
        let mut rec = TaskRecord::unran(TaskStatus::Success);
        rec.output = output;
        rec
    }

    const BASE: &str = "nika: v1\nworkflow: t\nvars:\n  topic: { type: string, default: \"news\" }\ntasks:\n  - id: ask\n    infer: { prompt: \"about ${{ vars.topic }}\" }\n";

    #[test]
    fn key_is_stable_across_recomputation() {
        let records = BTreeMap::new();
        let vars = BTreeMap::from([("topic".to_owned(), json!("news"))]);
        let a = stamp_of(BASE, &records, &vars).expect("eligible");
        let b = stamp_of(BASE, &records, &vars).expect("eligible");
        assert_eq!(a, b, "same task + same scope → same stamp, forever");
        assert_eq!(a.def_hash.len(), 64, "blake3 hex");
        assert_eq!(a.input_hash.len(), 64, "blake3 hex");
    }

    #[test]
    fn definition_edit_changes_the_definition_hash() {
        // Trap 6: an edited task NEVER silently skips.
        let edited = BASE.replace("about ${{ vars.topic }}", "summarize ${{ vars.topic }}");
        let records = BTreeMap::new();
        let vars = BTreeMap::from([("topic".to_owned(), json!("news"))]);
        let a = stamp_of(BASE, &records, &vars).expect("eligible");
        let b = stamp_of(&edited, &records, &vars).expect("eligible");
        assert_ne!(a.def_hash, b.def_hash, "prompt edit → new definition");
        // The rendered input changed too (the prompt text renders in).
        assert_ne!(a.input_hash, b.input_hash);
    }

    #[test]
    fn var_change_changes_the_input_hash_not_the_definition() {
        let records = BTreeMap::new();
        let news = BTreeMap::from([("topic".to_owned(), json!("news"))]);
        let rust = BTreeMap::from([("topic".to_owned(), json!("rust"))]);
        let a = stamp_of(BASE, &records, &news).expect("eligible");
        let b = stamp_of(BASE, &records, &rust).expect("eligible");
        assert_eq!(a.def_hash, b.def_hash, "the task text is unchanged");
        assert_ne!(a.input_hash, b.input_hash, "the resolved value changed");
    }

    #[test]
    fn upstream_output_change_cascades_into_the_input_hash() {
        const DOWNSTREAM: &str = "nika: v1\nworkflow: t\ntasks:\n  - id: use\n    exec: { command: [\"echo\", \"${{ tasks.up.output }}\"] }\n";
        let vars = BTreeMap::new();
        let r1 = BTreeMap::from([("up".to_owned(), success_record(json!("v1")))]);
        let r2 = BTreeMap::from([("up".to_owned(), success_record(json!("v2")))]);
        let a = stamp_of(DOWNSTREAM, &r1, &vars).expect("eligible");
        let b = stamp_of(DOWNSTREAM, &r2, &vars).expect("eligible");
        assert_eq!(a.def_hash, b.def_hash);
        assert_ne!(
            a.input_hash, b.input_hash,
            "ancestor invalidation rides the rendered value"
        );
    }

    /// Secrets participate BY NAME (declared reference identity), never
    /// by value: rotating the value leaves the key unchanged (ADR-099's
    /// documented sharp edge — `--from` is the override), while renaming
    /// the reference re-keys.
    #[test]
    fn secret_value_never_participates_the_reference_identity_does() {
        const WITH_SECRET: &str = "nika: v1\nworkflow: t\nsecrets:\n  tok: { source: env, key: MY_TOKEN }\ntasks:\n  - id: call\n    exec: { command: [\"curl\", \"-H\", \"'x:\", \"${{ secrets.tok }}'\"] }\n";
        let wf = parse(WITH_SECRET);
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let ctx_v1 = ResumeContext::of(
            &wf,
            &BTreeMap::from([("tok".to_owned(), json!("secret-value-1"))]),
            None,
            &BTreeMap::new(),
        );
        let ctx_v2 = ResumeContext::of(
            &wf,
            &BTreeMap::from([("tok".to_owned(), json!("secret-value-2"))]),
            None,
            &BTreeMap::new(),
        );
        let a = stamp(
            &wf.tasks[0].value,
            &records,
            &vars,
            &BTreeMap::new(),
            &ctx_v1,
        )
        .expect("eligible");
        let b = stamp(
            &wf.tasks[0].value,
            &records,
            &vars,
            &BTreeMap::new(),
            &ctx_v2,
        )
        .expect("eligible");
        assert_eq!(a, b, "a rotated secret VALUE does not re-key (by-name)");

        // A different declared reference (key path) IS a different identity.
        let rekeyed = WITH_SECRET.replace("MY_TOKEN", "OTHER_TOKEN");
        let wf2 = parse(&rekeyed);
        let ctx2 = ResumeContext::of(
            &wf2,
            &BTreeMap::from([("tok".to_owned(), json!("secret-value-1"))]),
            None,
            &BTreeMap::new(),
        );
        let c = stamp(
            &wf2.tasks[0].value,
            &records,
            &vars,
            &BTreeMap::new(),
            &ctx2,
        )
        .expect("eligible");
        assert_ne!(a.input_hash, c.input_hash, "reference identity re-keys");
    }

    /// #409 · the EFFECTIVE default model is part of a model-less
    /// infer task's DEFINITION identity: swapping the envelope `model:`
    /// (or supplying `--model`) re-runs it — a resume must never serve
    /// output produced by a different model than the file now declares.
    #[test]
    fn default_model_swap_rekeys_a_modelless_infer() {
        const MODELLESS: &str = "nika: v1\nworkflow: t\nmodel: ollama/qwen3.5:4b\ntasks:\n  - id: summary\n    infer: { prompt: \"hi\" }\n";
        // A task that PINS its own `model:` · an exec task — the two
        // no-re-key controls (items live at scope top · lint law).
        const PINNED: &str = "nika: v1\nworkflow: t\nmodel: ollama/qwen3.5:4b\ntasks:\n  - id: summary\n    infer: { prompt: \"hi\", model: \"mock/echo\" }\n";
        const EXEC: &str = "nika: v1\nworkflow: t\nmodel: ollama/qwen3.5:4b\ntasks:\n  - id: run\n    exec: { command: [\"echo\", \"hi\"] }\n";
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let env = BTreeMap::new();
        let stamp_with = |yaml: &str, over: Option<&str>| {
            let wf = parse(yaml);
            let ctx = ResumeContext::of(&wf, &BTreeMap::new(), over, &BTreeMap::new());
            stamp(&wf.tasks[0].value, &records, &vars, &env, &ctx).expect("eligible")
        };

        // The issue's exact repro: edit ONLY the envelope `model:` line.
        let a = stamp_with(MODELLESS, None);
        let swapped = MODELLESS.replace("ollama/qwen3.5:4b", "ollama/llama3.2:3b");
        let b = stamp_with(&swapped, None);
        assert_ne!(
            a.def_hash, b.def_hash,
            "the envelope model re-keys the model-less infer"
        );

        // A `--model` override replaces the resolved default → re-keys too.
        let o = stamp_with(MODELLESS, Some("mistral/small"));
        assert_ne!(a.def_hash, o.def_hash, "a --model override re-keys");
        // …and the SAME override is stable (no churn).
        assert_eq!(
            o.def_hash,
            stamp_with(MODELLESS, Some("mistral/small")).def_hash
        );

        // A task that PINS its own `model:` ignores the envelope — the
        // default cannot affect what it runs, so no needless re-run.
        let p1 = stamp_with(PINNED, None);
        let p2 = stamp_with(
            &PINNED.replace("ollama/qwen3.5:4b", "ollama/llama3.2:3b"),
            None,
        );
        assert_eq!(
            p1.def_hash, p2.def_hash,
            "a pinned per-task model ignores the envelope"
        );

        // An exec task never reads the default model — stable across it.
        let e1 = stamp_with(EXEC, None);
        let e2 = stamp_with(
            &EXEC.replace("ollama/qwen3.5:4b", "ollama/llama3.2:3b"),
            None,
        );
        assert_eq!(e1.def_hash, e2.def_hash, "exec ignores the model line");
    }

    /// #473 · an agent task's `skills:` participate in its DEFINITION
    /// identity by TEXT (spec 02 §agent skills · the ADR-099 law): an
    /// edited SKILL.md re-runs the task; the same text is stable; a
    /// different PATH with the same text re-keys (the path list is part
    /// of the verb body as written).
    #[test]
    fn skill_edit_rekeys_the_agent_definition() {
        const WITH_SKILL: &str = "nika: v1\nworkflow: t\nmodel: mock/echo\ntasks:\n  - id: go\n    agent: { prompt: \"hi\", skills: [\"s/SKILL.md\"] }\n";
        // The skill-less control (items live at scope top · lint law).
        const PLAIN: &str = "nika: v1\nworkflow: t\nmodel: mock/echo\ntasks:\n  - id: go\n    agent: { prompt: \"hi\" }\n";
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let env = BTreeMap::new();
        let stamp_with = |yaml: &str, skills: &BTreeMap<String, String>| {
            let wf = parse(yaml);
            let ctx = ResumeContext::of(&wf, &BTreeMap::new(), None, skills);
            stamp(&wf.tasks[0].value, &records, &vars, &env, &ctx)
        };
        let v1 = BTreeMap::from([(
            "s/SKILL.md".to_owned(),
            "---\nname: s\ndescription: d\n---\nv1 body\n".to_owned(),
        )]);
        let v2 = BTreeMap::from([(
            "s/SKILL.md".to_owned(),
            "---\nname: s\ndescription: d\n---\nv2 body\n".to_owned(),
        )]);

        let a = stamp_with(WITH_SKILL, &v1).expect("eligible");
        let b = stamp_with(WITH_SKILL, &v2).expect("eligible");
        assert_ne!(a.def_hash, b.def_hash, "an edited skill re-runs the task");

        // Same text → stable (no churn).
        let a2 = stamp_with(WITH_SKILL, &v1).expect("eligible");
        assert_eq!(a, a2, "unchanged skill text → same stamp");

        // A different path carrying the SAME text still re-keys (the
        // path list is the verb body as written · ADR-099 §1).
        let repathed = WITH_SKILL.replace("s/SKILL.md", "other/SKILL.md");
        let moved = BTreeMap::from([(
            "other/SKILL.md".to_owned(),
            "---\nname: s\ndescription: d\n---\nv1 body\n".to_owned(),
        )]);
        let c = stamp_with(&repathed, &moved).expect("eligible");
        assert_ne!(a.def_hash, c.def_hash, "the path is part of the body");

        // A referenced skill the composer did not resolve → NOT eligible
        // (records no key · never skips · the honest degrade).
        assert!(
            stamp_with(WITH_SKILL, &BTreeMap::new()).is_none(),
            "unresolved skill text → no resume claim"
        );

        // A skill-less agent ignores the map entirely (control).
        let p1 = stamp_with(PLAIN, &v1).expect("eligible");
        let p2 = stamp_with(PLAIN, &BTreeMap::new()).expect("eligible");
        assert_eq!(p1, p2, "no skills: → the map never participates");
    }

    /// A resolved secret value that leaked into the rendered inputs via
    /// an UPSTREAM record disqualifies the stamp — the trace never
    /// carries secret-derived material, not even inside a hash.
    #[test]
    fn secret_leaked_through_an_upstream_record_disables_the_stamp() {
        const DOWNSTREAM: &str = "nika: v1\nworkflow: t\nsecrets:\n  tok: { source: env, key: T }\ntasks:\n  - id: use\n    exec: { command: [\"echo\", \"${{ tasks.up.output }}\"] }\n";
        let wf = parse(DOWNSTREAM);
        let ctx = ResumeContext::of(
            &wf,
            &BTreeMap::from([("tok".to_owned(), json!("hunter2-secret"))]),
            None,
            &BTreeMap::new(),
        );
        let vars = BTreeMap::new();
        let leaked = BTreeMap::from([(
            "up".to_owned(),
            success_record(json!("prefix hunter2-secret suffix")),
        )]);
        assert!(
            stamp(&wf.tasks[0].value, &leaked, &vars, &BTreeMap::new(), &ctx).is_none(),
            "a secret value in the rendered inputs → not resume-eligible"
        );
        let clean = BTreeMap::from([("up".to_owned(), success_record(json!("safe")))]);
        assert!(
            stamp(&wf.tasks[0].value, &clean, &vars, &BTreeMap::new(), &ctx).is_some(),
            "the same task without the leak stays eligible"
        );
    }

    /// Trap 5 — map order + serializer whitespace never reach the hash:
    /// two `with:` blocks with the same entries in different authored
    /// order produce the SAME stamp (JCS sorts keys at every depth).
    #[test]
    fn with_declaration_order_is_canonicalized_away() {
        const AB: &str = "nika: v1\nworkflow: t\ntasks:\n  - id: t\n    with: { a: \"1\", b: \"2\" }\n    exec: { command: [\"echo\", \"${{ with.a }}\", \"${{ with.b }}\"] }\n";
        const BA: &str = "nika: v1\nworkflow: t\ntasks:\n  - id: t\n    with: { b: \"2\", a: \"1\" }\n    exec: { command: [\"echo\", \"${{ with.a }}\", \"${{ with.b }}\"] }\n";
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let a = stamp_of(AB, &records, &vars).expect("eligible");
        let b = stamp_of(BA, &records, &vars).expect("eligible");
        assert_eq!(a, b, "authored map order is not behavior");
    }

    /// The ES6-double trap: two int64 values beyond 2^53 that a plain
    /// RFC 8785 number serialization would COLLAPSE must produce distinct
    /// input hashes (the number pre-fold carries full i64 fidelity).
    #[test]
    fn int64_beyond_2p53_never_collide() {
        const WF: &str = "nika: v1\nworkflow: t\nvars:\n  id: { type: integer, default: 1 }\ntasks:\n  - id: t\n    exec: { command: [\"echo\", \"${{ vars.id }}\"] }\n";
        let records = BTreeMap::new();
        // 2^53 + 1 and 2^53 + 2 are the SAME f64 — distinct i64s.
        let a_vars = BTreeMap::from([("id".to_owned(), json!(9_007_199_254_740_993_i64))]);
        let b_vars = BTreeMap::from([("id".to_owned(), json!(9_007_199_254_740_994_i64))]);
        let a = stamp_of(WF, &records, &a_vars).expect("eligible");
        let b = stamp_of(WF, &records, &b_vars).expect("eligible");
        assert_ne!(a.input_hash, b.input_hash, "int64 fidelity survives JCS");
    }

    /// No float FIELDS in the key: `temperature:` rides as its display
    /// string and still distinguishes values.
    #[test]
    fn temperature_rides_as_a_string_and_distinguishes() {
        const T7: &str = "nika: v1\nworkflow: t\ntasks:\n  - id: t\n    infer: { prompt: \"x\", temperature: 0.7 }\n";
        let t8 = T7.replace("0.7", "0.8");
        let records = BTreeMap::new();
        let vars = BTreeMap::new();
        let a = stamp_of(T7, &records, &vars).expect("eligible");
        let b = stamp_of(&t8, &records, &vars).expect("eligible");
        assert_ne!(a.def_hash, b.def_hash);
    }

    /// The fan-out collection participates in the input hash (a changed
    /// item set re-runs); deep `item.` navigation under the placeholder
    /// is not resume-eligible (fail-closed, never a wrong skip).
    #[test]
    fn fan_out_collection_participates_and_deep_item_nav_is_ineligible() {
        const SHALLOW: &str = "nika: v1\nworkflow: t\nvars:\n  urls: [\"a\", \"b\"]\ntasks:\n  - id: fan\n    for_each: ${{ vars.urls }}\n    exec: { command: [\"echo\", \"${{ item }}\"] }\n";
        const DEEP: &str = "nika: v1\nworkflow: t\nvars:\n  rows: [{ url: \"a\" }]\ntasks:\n  - id: fan\n    for_each: ${{ vars.rows }}\n    exec: { command: [\"echo\", \"${{ item.url }}\"] }\n";
        let records = BTreeMap::new();
        let ab = BTreeMap::from([("urls".to_owned(), json!(["a", "b"]))]);
        let ac = BTreeMap::from([("urls".to_owned(), json!(["a", "c"]))]);
        let a = stamp_of(SHALLOW, &records, &ab).expect("shallow item is eligible");
        let b = stamp_of(SHALLOW, &records, &ac).expect("shallow item is eligible");
        assert_ne!(a.input_hash, b.input_hash, "the item set is an input");

        let rows = BTreeMap::from([("rows".to_owned(), json!([{ "url": "a" }]))]);
        assert!(
            stamp_of(DEEP, &records, &rows).is_none(),
            "deep item navigation cannot key deterministically → never skips"
        );
    }

    /// The recipe version participates: a bumped `KEY_VERSION` re-keys
    /// everything (old traces re-run · honest · never a wrong match).
    #[test]
    fn key_version_participates_in_both_hashes() {
        let base = ResumeKey::new("t".into(), "exec".into(), json!({}), json!({}));
        let mut bumped = base.clone();
        bumped.v = KEY_VERSION + 1;
        assert_ne!(base.definition_hash(), bumped.definition_hash());
        assert_ne!(base.input_hash(), bumped.input_hash());
    }

    /// The number fold is total + structural (arrays/objects recurse).
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

/// The trace-carry proof: a REAL run through the runtime (mock seams)
/// stamps `def_hash` + `input_hash` + `output` onto every success
/// `task_completed` frame — the artifact `--resume` later reads.
#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod trace_carry_tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use nika_event::EventKind;
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_providers::{ProviderRegistry, ProvidersConfig};
    use nika_types::resource::Value as FieldValue;
    use nika_verb_agent::AgentVerb;
    use nika_verb_exec::ExecVerb;
    use nika_verb_infer::InferVerb;
    use nika_verb_invoke::InvokeVerb;
    use serde_json::Value;

    use crate::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};

    fn str_field<'a>(event: &'a nika_event::Event, key: &str) -> Option<&'a str> {
        event.fields.iter().find(|kv| kv.key == key).and_then(|kv| {
            if let FieldValue::String(s) = &kv.value {
                Some(s.as_str())
            } else {
                None
            }
        })
    }

    #[tokio::test]
    async fn success_task_completed_carries_the_resume_fields() {
        const WORKFLOW: &str =
            "nika: v1\nworkflow: carry\ntasks:\n  - id: say\n    exec: { command: [\"echo\", \"hi\"] }\n";
        let wf = nika_schema::parse(
            WORKFLOW,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_schema::check(&wf);
        assert!(report.is_clean());

        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
        let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(MockShell::new().enqueue_ok("said\n"))),
            Arc::clone(&invoke),
            InferVerb::new(registry, "mock/echo"),
            AgentVerb::new(
                Arc::new(MockProvider::new("mock")),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/echo",
            ),
            MockClock::new(),
            RuntimeConfig::default(),
        );
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("clean run");
        assert!(outcome.ok);

        let completed = sink
            .events()
            .iter()
            .find(|e| e.kind == EventKind::TaskCompleted)
            .expect("a task_completed frame");
        let def = str_field(completed, super::fields::DEF_HASH).expect("def_hash rides");
        let input = str_field(completed, super::fields::INPUT_HASH).expect("input_hash rides");
        assert_eq!(def.len(), 64, "blake3 hex");
        assert_eq!(input.len(), 64, "blake3 hex");
        // The output field parses back to EXACTLY the record's output —
        // the rehydration contract.
        let output_text = str_field(completed, super::fields::OUTPUT).expect("output rides");
        let rehydrated: Value = serde_json::from_str(output_text).expect("output is JSON");
        assert_eq!(rehydrated, outcome.records["say"].output);

        // Recompute the stamp from the same coordinates — it matches the
        // journaled fields (the skip predicate `--resume` evaluates).
        let ctx = super::ResumeContext::of(&wf, &BTreeMap::new(), None, &BTreeMap::new());
        let stamp = super::stamp(
            &wf.tasks[0].value,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
            &ctx,
        )
        .expect("eligible");
        assert_eq!(stamp.def_hash, def);
        assert_eq!(stamp.input_hash, input);
    }

    const TWO_TASKS: &str = "nika: v1\nworkflow: resume\ntasks:\n  - id: a\n    exec: { command: [\"echo\", \"one\"] }\n  - id: b\n    depends_on: [a]\n    exec: { command: [\"echo\", \"two\", \"${{ tasks.a.output }}\"] }\n";

    /// Run [`TWO_TASKS`] over mock seams with an optional resume plan —
    /// returns the outcome + the emitted stream.
    async fn run_two_tasks(
        shell: MockShell,
        plan: Option<super::ResumePlan>,
    ) -> (crate::RunOutcome, VecSink) {
        let wf = nika_schema::parse(
            TWO_TASKS,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let report = nika_schema::check(&wf);
        assert!(report.is_clean());
        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
        let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
        let mut runtime = Runtime::new(
            ExecVerb::new(Arc::new(shell)),
            Arc::clone(&invoke),
            InferVerb::new(registry, "mock/echo"),
            AgentVerb::new(
                Arc::new(MockProvider::new("mock")),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/echo",
            ),
            MockClock::new(),
            RuntimeConfig::default(),
        );
        if let Some(plan) = plan {
            runtime = runtime.with_resume_plan(plan);
        }
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let outcome = runtime
            .run(&wf, &report, &mut stamper, &mut sink)
            .await
            .expect("clean run");
        (outcome, sink)
    }

    /// The full ADR-099 fold: run → read the journaled identity → resume
    /// with a plan → the matched task CACHE-HITS (visible `task_cache_hit`
    /// · rehydrated output · no `task_started`) and the rest runs live.
    #[tokio::test]
    async fn resume_plan_skips_matching_tasks_and_runs_the_rest() {
        // First run — both tasks execute; harvest a's journaled identity.
        let (first, sink) = run_two_tasks(
            MockShell::new().enqueue_ok("one\n").enqueue_ok("two\n"),
            None,
        )
        .await;
        assert!(first.ok);
        assert!(first.cache_hits.is_empty(), "a fresh run never cache-hits");
        let completed_a = sink
            .events()
            .iter()
            .find(|e| e.kind == EventKind::TaskCompleted && str_field(e, "task") == Some("a"))
            .expect("a completed");
        let plan = super::ResumePlan::from([(
            "a".to_owned(),
            super::PriorSuccess::new(
                str_field(completed_a, super::fields::DEF_HASH)
                    .expect("def_hash")
                    .to_owned(),
                str_field(completed_a, super::fields::INPUT_HASH)
                    .expect("input_hash")
                    .to_owned(),
                serde_json::from_str(
                    str_field(completed_a, super::fields::OUTPUT).expect("output"),
                )
                .expect("output parses"),
            ),
        )]);

        // Resume — ONLY b's shell response is queued: a must never dispatch.
        let (resumed, sink) =
            run_two_tasks(MockShell::new().enqueue_ok("two\n"), Some(plan.clone())).await;
        assert!(resumed.ok);
        assert_eq!(resumed.cache_hits, vec!["a".to_owned()]);
        let kinds_for = |task: &str| {
            sink.events()
                .iter()
                .filter(|e| str_field(e, "task") == Some(task))
                .map(|e| e.kind)
                .collect::<Vec<_>>()
        };
        assert!(
            kinds_for("a").contains(&EventKind::TaskCacheHit),
            "the skip is VISIBLE"
        );
        assert!(
            !kinds_for("a").contains(&EventKind::TaskStarted),
            "a never re-executed"
        );
        assert!(
            kinds_for("b").contains(&EventKind::TaskStarted),
            "b ran live"
        );
        // Rehydration parity: a's record output matches the first run's.
        assert_eq!(resumed.records["a"].output, first.records["a"].output);

        // A stale identity (input hash mismatch) refuses the skip — both
        // hashes MUST match (ADR-099 §1).
        let mut stale = plan;
        if let Some(entry) = stale.get_mut("a") {
            entry.input_hash = "0".repeat(64);
        }
        let (rerun, _) = run_two_tasks(
            MockShell::new().enqueue_ok("one\n").enqueue_ok("two\n"),
            Some(stale),
        )
        .await;
        assert!(
            rerun.cache_hits.is_empty(),
            "hash mismatch → re-run, never skip"
        );
    }

    /// `referenced_upstreams` sees BOTH edge kinds — explicit
    /// `depends_on` and `${{ tasks.<id> }}` template references (the
    /// `--from` transitive-downstream walk rides it).
    #[test]
    fn referenced_upstreams_collects_explicit_and_template_edges() {
        let wf = nika_schema::parse(
            TWO_TASKS,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses");
        let ups = super::referenced_upstreams(&wf.tasks[1].value);
        assert!(ups.contains("a"), "depends_on + template ref both seen");
        assert!(super::referenced_upstreams(&wf.tasks[0].value).is_empty());
    }
}
