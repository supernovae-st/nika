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
//!   (`#[non_exhaustive]` future variants) or render (a missing upstream
//!   · a secret leak), yields NO stamp — the task simply never skips.
//!   Honest degradation, never a wrong skip, never an error. A `for_each`
//!   body that navigates `item.field` IS eligible: the resolved collection
//!   carries every per-item value, and the stand-in is shaped from that
//!   collection so field navigation renders to a marker (never a miss).

use std::collections::BTreeMap;

use nika_schema::Spanned;
use nika_schema::raw::{
    ForEachValue, RawAction, RawAgentAction, RawCommand, RawExecAction, RawInferAction,
    RawInvokeAction, RawTask, RawWorkflow, VisionInput,
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

/// The resume's chain-trust posture when the run proceeded WITHOUT a
/// verified chain (ADR-099 trust amendment · 2026-08-08) — attested on
/// the boot manifest as `resume_unverified: <posture>` +
/// `resume_unverified_finding`, so no unverified ancestor launders
/// silently into a journal claiming a clean one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResumeUnverified {
    /// The operator named `--resume-unverified` past a BROKEN chain —
    /// the finding carries the walk's one-line evidence (sanitized).
    Declared(String),
    /// The trace carries NO chain (a `--json` stream capture · a
    /// pre-0.96 journal): the chainless-capture compat — and the
    /// strip-the-chain forgery (delete every `chain` field) lands
    /// exactly here; attested, never silent.
    Unchained(String),
}

impl ResumeUnverified {
    /// The boot-manifest posture token (`declared` · `unchained`).
    #[must_use]
    pub fn posture(&self) -> &'static str {
        match self {
            Self::Declared(_) => "declared",
            Self::Unchained(_) => "unchained",
        }
    }

    /// The one-line finding the manifest journals.
    #[must_use]
    pub fn finding(&self) -> &str {
        match self {
            Self::Declared(finding) | Self::Unchained(finding) => finding,
        }
    }
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

/// A walkable stand-in for `item` during a *task-level* fan-out stamp.
///
/// The collection itself is the input identity. This object only has to
/// satisfy `item.field` / nested navigation so a prompt like
/// `${{ item.stem }}` does not fail eligibility (the string marker
/// cannot — CEL's `.field` on a string is `NIKA-VAR-001`). Leaves are
/// the same marker; keys are the union of every element's shape.
fn item_stand_in(items: &Value) -> Value {
    match items {
        Value::Array(arr) => {
            let mut shape = Value::Null;
            for el in arr {
                shape = merge_item_shape(&shape, el);
            }
            mask_item_leaves(&shape)
        }
        other => mask_item_leaves(other),
    }
}

/// Union of two JSON shapes (objects merge keys · arrays pad to max
/// length · a container wins over a scalar · first non-null scalar
/// keeps). Used only to make the stand-in navigable.
fn merge_item_shape(acc: &Value, next: &Value) -> Value {
    match (acc, next) {
        (Value::Null, v) => v.clone(),
        (Value::Object(a), Value::Object(b)) => {
            let mut out = a.clone();
            for (k, bv) in b {
                let existing = out.get(k).cloned().unwrap_or(Value::Null);
                out.insert(k.clone(), merge_item_shape(&existing, bv));
            }
            Value::Object(out)
        }
        (Value::Array(a), Value::Array(b)) => {
            let n = a.len().max(b.len());
            let mut out = Vec::with_capacity(n);
            for i in 0..n {
                let av = a.get(i).unwrap_or(&Value::Null);
                let bv = b.get(i).unwrap_or(&Value::Null);
                out.push(merge_item_shape(av, bv));
            }
            Value::Array(out)
        }
        (Value::Object(_) | Value::Array(_), _) => acc.clone(),
        (_, Value::Object(_) | Value::Array(_)) => next.clone(),
        _ => acc.clone(),
    }
}

/// Replace every leaf with [`item_marker`] so rendered action text never
/// carries a real item value (those already ride in `items`).
fn mask_item_leaves(v: &Value) -> Value {
    match v {
        Value::Object(m) if !m.is_empty() => Value::Object(
            m.iter()
                .map(|(k, child)| (k.clone(), mask_item_leaves(child)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(mask_item_leaves).collect()),
        _ => item_marker(),
    }
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

/// The shared digest door (F-O1 PR-3 · the `declassify` receipt's value
/// digest reads the SAME canonical fold as the resume identity hashes —
/// one digest law per receipt).
pub(crate) fn jcs_blake3_hex(payload: &Value) -> Option<String> {
    jcs_blake3(payload)
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

/// The task ids a task's definition can observe — its incoming edges
/// (`with:` refs + `after:` targets · the boundary) plus every
/// `tasks.<id>` token in its raw template text. The `--from <task_id>`
/// override walks this REVERSED to force the transitive downstream to
/// re-run even on a hash match (ADR-099 §3). Over-collection is the safe
/// direction (more re-runs, never a wrong skip).
#[must_use]
pub fn referenced_upstreams(task: &RawTask) -> std::collections::BTreeSet<String> {
    let mut out: std::collections::BTreeSet<String> =
        nika_check::analyzer::edges::producer_ids(task)
            .into_iter()
            .collect();
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
    /// The composer-resolved child-workflow closure digests (`workflow:`
    /// target as written → the transitive source-closure digest). A
    /// calling task's DEFINITION identity covers the child's CONTENT,
    /// not just its path (spec 14 law 10 at the `def_hash` tier · the
    /// same ADR-099 trap-6 law as an edited task: an edited child —
    /// or grandchild — re-runs the call instead of serving the old
    /// child's cached output). A target the composer did not resolve
    /// makes the task non-eligible (records no key · never skips).
    child_closures: BTreeMap<String, String>,
    /// The operator's `--access` pin (R-1 · P3) — behavior-bearing like
    /// the model: a run pinned `codex-acp` resumed under `api` must
    /// RE-RUN, never serve the other path's cached output (envelope
    /// fidelity differs by access class). The CHOSEN-access half lands
    /// with the B6 registry — the rider's own trigger (« the moment >1
    /// access can serve one provider ») is unreachable while every
    /// provider carries exactly one row.
    access_pin: Option<String>,
}

impl ResumeContext {
    /// Build the context from the workflow's declared `secrets:` block +
    /// the run's resolved values + the composer's `--model` override
    /// (the effective default model falls back to the envelope's) + the
    /// composer-resolved skill texts + the composer-resolved child
    /// closure digests (spec 14 · the composition resume identity).
    pub(crate) fn of(
        wf: &RawWorkflow,
        resolved: &BTreeMap<String, Value>,
        model_override: Option<&str>,
        skills: &BTreeMap<String, String>,
        child_closures: &BTreeMap<String, String>,
        access_pin: Option<&str>,
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
            child_closures: child_closures.clone(),
            access_pin: access_pin.filter(|p| !p.is_empty()).map(ToOwned::to_owned),
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
    inputs: &BTreeMap<String, Value>,
    consts: &BTreeMap<String, Value>,
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
    // R-1 (P3 · the #409 precedent's ACCESS twin — pin half): the pin an
    // infer/agent task runs under is behavior-bearing — a run pinned
    // `codex-acp` resumed under `api` RE-RUNS, never serves the other
    // path's cached output. The chosen-access half lands with B6.
    if touches_intelligence(task)
        && let Some(pin) = ctx.access_pin.as_deref()
        && let Some(obj) = definition.as_object_mut()
    {
        obj.insert("access_pin".to_owned(), json!(pin));
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
    // Spec 14 law 10 (the def_hash tier) · ADR-099 trap 6 across the
    // file boundary: a child-workflow call's DEFINITION identity covers
    // the child's transitive source closure — the composer resolves one
    // digest per static target (the #473 skills seam: the composer owns
    // the file reads, the runtime keys identity on what it hands over).
    // A target with no digest → non-eligible (never skips · the honest
    // degrade — a wrong skip is the one unforgivable failure mode).
    let targets = workflow_targets(task);
    if !targets.is_empty() {
        let mut closures = serde_json::Map::new();
        for target in targets {
            let digest = ctx.child_closures.get(target)?;
            closures.insert(target.to_owned(), json!(digest));
        }
        definition
            .as_object_mut()?
            .insert("child_closure".to_owned(), Value::Object(closures));
    }
    let inputs = input_value(task, records, inputs, consts, &ctx.markers)?;
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
/// when its action is
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
}

/// The R-1 detector (P3): does this task run an infer/agent action
/// (main verb or any `on_finally` mini)? Those tasks' identities carry
/// the access pin; every other action kind never reads it.
fn touches_intelligence(task: &RawTask) -> bool {
    let is_ai = |a: &RawAction| matches!(a, RawAction::Infer(_) | RawAction::Agent(_));
    is_ai(&task.action)
}

/// Every STATIC child-workflow target this task carries (the main verb
/// plus every `on_finally` mini) — the `skill_paths` twin for spec 14.
/// A `tool:` invoke never lands here: its identity is the tool ref plus
/// its args alone, unchanged.
fn workflow_targets(task: &RawTask) -> Vec<&str> {
    fn of(action: &RawAction) -> Option<&str> {
        match action {
            RawAction::Invoke(a) => match &a.target {
                nika_schema::raw::RawInvokeTarget::Workflow(w) => Some(w.value.as_str()),
                nika_schema::raw::RawInvokeTarget::Tool(_) => None,
            },
            _ => None,
        }
    }
    of(&task.action).into_iter().collect()
}

/// Every `skills:` path this task carries — declaration order ·
/// duplicates deduped by the map they land in. The per-task twin of
/// `nika_schema::skill_refs`.
fn skill_paths(task: &RawTask) -> Vec<&str> {
    fn of(action: &RawAction) -> Vec<&str> {
        match action {
            RawAction::Agent(a) => a.skills.iter().map(|s| s.value.as_str()).collect(),
            _ => Vec::new(),
        }
    }
    of(&task.action)
}

// ─── the definition payload (raw · behavior-bearing fields as written) ──

/// The behavior-bearing fields as WRITTEN (ADR-099 §1: the verb body ·
/// `with:` · `output:` · `retry:`/`on_error:`/`on_finally:` · `when:` ·
/// `for_each:` — plus the scheduling knobs that change behavior). `None`
/// on any `#[non_exhaustive]` form this recipe does not know. (W2
/// re-keyed the definition — `after:` replaced `depends_on` · prior
/// resume caches re-run one-shot, the assumed pre-1.0 cost.)
///
/// `pub(crate)` because the W6 semantic hash ([`crate::proof::ir`]) reuses
/// THIS span-free desugared projection as a task's semantic subtree — one
/// canonicalization discipline for both the resume identity and the
/// semantic hash it generalizes (spec 15 · "seed: the `ResumeKey`'s
/// JCS+blake3 definition hash, generalized").
pub(crate) fn definition_value(task: &RawTask) -> Option<Value> {
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
        "action": action_value(&task.action, None)?,
    }))
}

fn when_value(when: Option<&Spanned<WhenGate>>) -> Value {
    match when.map(|w| &w.value) {
        None => Value::Null,
        // CLOSED vocabulary (nika-vocab) — both gate forms named.
        Some(WhenGate::Literal(b)) => json!({ "literal": b }),
        Some(WhenGate::Expr(e)) => json!({ "expr": e }),
    }
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
        _ => return None,
    };
    Some(json!({
        "action": action,
        "on_codes": on_error.on_codes.iter().map(|c| c.value.clone()).collect::<Vec<_>>(),
    }))
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
/// then runs live and surfaces its real error). A fan-out body's
/// `item.field` navigation is eligible: the stand-in is shaped from
/// the collection so the render cannot miss on a real key.
fn input_value(
    task: &RawTask,
    records: &BTreeMap<String, TaskRecord>,
    inputs: &BTreeMap<String, Value>,
    consts: &BTreeMap<String, Value>,
    markers: &BTreeMap<String, Value>,
) -> Option<Value> {
    let workflow_scope = Scope {
        records,
        inputs,
        consts,
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
    // Fan-out renders bind `item`/`index` to a *shaped* stand-in: real
    // values already ride in `items`; field navigation resolves to the
    // marker so `${{ item.stem }}` stays stamp-eligible.
    let stand_in = item_stand_in(&items);
    let (item, index) = if task.for_each.is_some() {
        (Some(&stand_in), Some(0))
    } else {
        (None, None)
    };
    let base = Scope {
        records,
        inputs,
        consts,
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
    Some(match &a.target {
        nika_schema::raw::RawInvokeTarget::Tool(t) => json!({
            "tool": text(t, scope)?,
            "args": json_field(a.args.as_ref(), scope)?,
        }),
        // The child call's identity is its STATIC target + rendered args
        // (the child's own semantic identity lives on the child's chain —
        // the trace forest, spec 14 law 8).
        nika_schema::raw::RawInvokeTarget::Workflow(w) => json!({
            "workflow": text(w, scope)?,
            "args": json_field(a.args.as_ref(), scope)?,
        }),
    })
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
mod tests;
