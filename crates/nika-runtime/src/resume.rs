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

use nika_proof::{MARK, item_stand_in, secret_marker};
use nika_schema::Spanned;
use nika_schema::raw::{
    ForEachValue, RawAction, RawAgentAction, RawCommand, RawExecAction, RawInferAction,
    RawInvokeAction, RawTask, RawWorkflow, VisionInput,
};
use serde_json::{Value, json};

use crate::expr::{self, Scope};
use crate::record::TaskRecord;

pub use nika_proof::{
    KEY_VERSION, PriorSuccess, ResumeKey, ResumePlan, ResumeUnverified, fields,
    referenced_upstreams,
};
pub(crate) use nika_proof::{
    definition_value, jcs_blake3_hex, skill_paths, touches_intelligence, workflow_targets,
};

/// The two hex hashes a settled success stamps onto its trace record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResumeStamp {
    pub def_hash: String,
    pub input_hash: String,
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
    /// fidelity differs by access class). The chosen route is bound
    /// independently from the frozen probe snapshot below.
    access_pin: Option<String>,
    /// The composer-frozen access candidates for this run. Resume keys
    /// resolve through this SAME snapshot as dispatch, so changing the
    /// selected adapter/profile cannot reuse output from another access
    /// envelope. Empty preserves the env-free embedded-runtime posture:
    /// no route claim is made when the embedder supplied no probes.
    access_probes: Vec<nika_providers::probe::ProviderProbe>,
}

impl ResumeContext {
    /// Build the context from the workflow's declared `secrets:` block +
    /// the run's resolved values + the composer's `--model` override
    /// (the effective default model falls back to the envelope's) + the
    /// composer-resolved skill texts + the composer-resolved child
    /// closure digests (spec 14 · the composition resume identity).
    /// Access probes attach separately through [`Self::with_access_probes`]
    /// because env-free embedders may intentionally provide none.
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
            access_probes: Vec::new(),
        }
    }

    /// Bind the composer-frozen route candidates to resume identity.
    pub(crate) fn with_access_probes(
        mut self,
        probes: &[nika_providers::probe::ProviderProbe],
    ) -> Self {
        self.access_probes = probes.to_vec();
        self
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

/// Resolve one task definition and its rendered inputs under this run.
fn task_identity(
    task: &RawTask,
    records: &BTreeMap<String, TaskRecord>,
    inputs: &BTreeMap<String, Value>,
    consts: &BTreeMap<String, Value>,
    ctx: &ResumeContext,
) -> Option<(Value, Value)> {
    let mut definition = definition_value(task)?;
    // #409 · a model-less infer/agent task RUNS on the effective default
    // model, so that model joins its DEFINITION identity — swapping the
    // envelope `model:` (or `--model`) re-runs it instead of cache-hitting
    // the old model's output. Tasks that pin their own `model:` already
    // carry it in the definition; the envelope cannot affect them.
    if nika_proof::reads_default_model(task)
        && let Some(model) = ctx.default_model.as_deref()
        && let Some(obj) = definition.as_object_mut()
    {
        obj.insert("default_model".to_owned(), json!(model));
    }
    // R-1 (P3 · the #409 precedent's ACCESS twin): the pin an
    // infer/agent task runs under is behavior-bearing — a run pinned
    // `codex-acp` resumed under `api` RE-RUNS, never serves the other
    // path's cached output.
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
    // R-1 chosen-access half: resolve over the SAME frozen probes and
    // rendered model dispatch will consume. A changed adapter/profile id
    // re-keys before the cache-hit gate. If probes were supplied but a
    // templated model cannot resolve here, the task becomes ineligible —
    // re-running is the fail-closed direction, never a cross-seat hit.
    match selected_access_identity(task, &inputs, ctx) {
        SelectedAccessIdentity::Absent => {}
        SelectedAccessIdentity::Present(access) => {
            definition
                .as_object_mut()?
                .insert("selected_access".to_owned(), access);
        }
        SelectedAccessIdentity::Ineligible => return None,
    }
    Some((definition, inputs))
}

/// Compute one task's [`ResumeStamp`] including its ordered unwind closure.
/// `None` means this run cannot prove the complete identity, so it records
/// no key and never skips (ADR-099's fail-closed direction).
pub(crate) fn stamp(
    task: &RawTask,
    wf: &RawWorkflow,
    records: &BTreeMap<String, TaskRecord>,
    inputs: &BTreeMap<String, Value>,
    consts: &BTreeMap<String, Value>,
    ctx: &ResumeContext,
) -> Option<ResumeStamp> {
    let (mut definition, mut resolved_inputs) = task_identity(task, records, inputs, consts, ctx)?;
    let cleanups = nika_proof::unwind_tasks_of(wf, task.id.value.as_str());
    if !cleanups.is_empty() {
        let mut definitions = Vec::with_capacity(cleanups.len());
        let mut cleanup_inputs = Vec::with_capacity(cleanups.len());
        for cleanup in cleanups {
            let (cleanup_definition, resolved) =
                task_identity(cleanup, records, inputs, consts, ctx)?;
            definitions.push(json!({
                "task": cleanup.id.value,
                "verb": cleanup.action.verb(),
                "definition": cleanup_definition,
            }));
            cleanup_inputs.push(json!({ "task": cleanup.id.value, "inputs": resolved }));
        }
        definition
            .as_object_mut()?
            .insert("unwind".to_owned(), Value::Array(definitions));
        resolved_inputs
            .as_object_mut()?
            .insert("unwind".to_owned(), Value::Array(cleanup_inputs));
    }
    let key = ResumeKey::new(
        task.id.value.clone(),
        task.action.verb().to_owned(),
        definition,
        resolved_inputs,
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

/// Whether the resume recipe can bind the route dispatch will consume.
enum SelectedAccessIdentity {
    /// This task/path has no access identity to bind.
    Absent,
    /// The exact selected route, ready for the definition payload.
    Present(Value),
    /// A route exists at dispatch time but cannot be named safely here.
    Ineligible,
}

/// The route identity dispatch will consume, or no claim for a
/// non-intelligence task / an embedder that supplied no probe snapshot.
fn selected_access_identity(
    task: &RawTask,
    resolved_inputs: &Value,
    ctx: &ResumeContext,
) -> SelectedAccessIdentity {
    if !touches_intelligence(task) || ctx.access_probes.is_empty() {
        return SelectedAccessIdentity::Absent;
    }
    let (kind, authored_model, routes_through_harness) = match &task.action {
        RawAction::Infer(action) => ("infer", action.model.as_ref(), false),
        RawAction::Agent(action) => ("agent", action.model.as_ref(), true),
        _ => return SelectedAccessIdentity::Absent,
    };
    let model = if authored_model.is_some() {
        let Some(model) = resolved_inputs
            .pointer(&format!("/action/{kind}/model"))
            .and_then(Value::as_str)
        else {
            return SelectedAccessIdentity::Ineligible;
        };
        model
    } else {
        let Some(model) = ctx.default_model.as_deref() else {
            return SelectedAccessIdentity::Ineligible;
        };
        model
    };
    // A fan-out stand-in in a templated model is deliberately not a
    // guessed provider. No stamp means no stale cache hit.
    if model.contains(MARK) {
        return SelectedAccessIdentity::Ineligible;
    }
    let mut candidates =
        nika_providers::candidates_for(&ctx.access_probes, nika_providers::provider_of(model));
    // `infer` still dispatches through `InferVerb`'s native provider
    // registry. Only `agent` consumes an ACP seat today; stamping an
    // infer with the resolver's preferred harness would attest a path it
    // never executes and could preserve a cross-path cache hit.
    if !routes_through_harness {
        candidates.retain(|candidate| candidate.class != nika_types::access::AccessClass::Harness);
    }
    let Ok(plan) =
        nika_providers::resolve_access(model, &candidates, None, ctx.access_pin.as_deref())
    else {
        return SelectedAccessIdentity::Ineligible;
    };
    SelectedAccessIdentity::Present(json!({
        "id": plan.access,
        "class": plan.chosen.as_str(),
        "billing": plan.billing.as_str(),
    }))
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
    let when = task
        .when
        .as_ref()
        .map(|gate| crate::task::eval_gate(&gate.value, &action_scope))
        .transpose()
        .ok()?;
    Some(json!({
        "action": action_value(&task.action, &action_scope)?,
        "with": with_ns,
        "items": items,
        "when": when,
    }))
}

// ─── rendered action payload ────────────────────────────────────────────

fn action_value(action: &RawAction, scope: &Scope<'_>) -> Option<Value> {
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
fn text(s: &Spanned<String>, scope: &Scope<'_>) -> Option<Value> {
    expr::render_json(&Value::String(s.value.clone()), scope).ok()
}

/// An optional templated string field (`Null` when absent).
fn opt_text(s: Option<&Spanned<String>>, scope: &Scope<'_>) -> Option<Value> {
    match s {
        None => Some(Value::Null),
        Some(s) => text(s, scope),
    }
}

/// A templated JSON field (`args:` · `schema:`) — raw, or deep-rendered.
fn json_field(v: Option<&Spanned<Value>>, scope: &Scope<'_>) -> Option<Value> {
    match v {
        None => Some(Value::Null),
        Some(v) => expr::render_json(&v.value, scope).ok(),
    }
}

fn infer_value(a: &RawInferAction, scope: &Scope<'_>) -> Option<Value> {
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

fn exec_value(a: &RawExecAction, scope: &Scope<'_>) -> Option<Value> {
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

fn invoke_value(a: &RawInvokeAction, scope: &Scope<'_>) -> Option<Value> {
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

fn agent_value(a: &RawAgentAction, scope: &Scope<'_>) -> Option<Value> {
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
