// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `on_error.recover` await (spec 05 §recover resolution steps 2-4).
//!
//! A `recover: ${{ tasks.X.output }}` reference is NOT an execution-order
//! edge: resolution happens at RECOVERY time, and a referent that is not
//! yet terminal is AWAITED — deterministic, never a race. The await rides
//! the EXISTING ordered settle spine (never a new scheduler) ·
//!
//! 1. **Classify** ([`classify_await`]) — a recover render failure whose
//!    every failing island names a not-yet-terminal `tasks.X` root
//!    becomes a [`PendingRecovery`]; any other unresolved root keeps the
//!    immediate fail (the recovery fails as if `on_error:` were absent).
//! 2. **Park** ([`settle_or_park`]) — the settle spine holds the finish
//!    in a task-id-ordered side table; NO frame is emitted yet (the whole
//!    story defers, so the stream keeps per-task contiguity).
//! 3. **Retry on the spine** — after every settlement the covered parks
//!    resolve (transitively: a resolution can cover another park) and
//!    settle through the ONE emit site (`task_recovered` + terminal).
//! 4. **Workflow-end** ([`resolve_at_end`]) — parks still waiting after
//!    the last wave (mutual recovery cycles) resolve against the FINAL
//!    records where every still-parked task reads as its PRE-recovery
//!    FAILED record (recovery never rewrites the referent's history).
//!    Bounded by task count — nothing hangs.
//!
//! Determinism: parks mutate only on the sequential settle spine, the
//! side table is a `BTreeMap` (task-id order), and resolution consults
//! nothing but the records — no clocks, signals or timeouts, so the
//! event stream stays byte-identical for any wave-parallelism cap. On
//! the streamed wave (#412) settles land in a SIDE map the caller
//! merges after the wave, so the spine's terminal truth during the wave
//! is `frozen-prior ∪ side-map` — both evolve only on that same spine.
//!
//! Boundaries (pinned by tests) · a `for_each` ITERATION never parks
//! (the fan-out settles as one task — its collector downgrades a pending
//! classification to the immediate render failure) · a run that PAUSES
//! (ADR-099) drops its parks unemitted — those tasks simply have not
//! happened yet and re-run on `--resume`, like the blocked prompt itself.

use std::collections::{BTreeMap, BTreeSet};

use nika_schema::raw::RawWorkflow;
use nika_schema::types::OnErrorAction;
use nika_tmpl::expression::{NamespaceRef, expr_refs, scan_templates};
use serde_json::Value;

use crate::errors::RuntimeError;

use crate::expr::{self, Scope};
use crate::record::{TaskErrorRecord, TaskRecord, TaskStatus};
use crate::resume::ResumeContext;
use crate::stamp::{EventSink, Stamper};
use crate::task::{
    FailedOutcome, Finish, RanTask, RetryStamp, RunResult, SettleAs, bind_outputs,
    runtime_error_record, success_output,
};

/// A recovery whose render awaits not-yet-terminal referents (spec 05
/// §recover step 3) — everything the deferred render needs, owned (the
/// pipeline scope is gone by resolution time).
pub(crate) struct PendingRecovery {
    /// The attempt-loop failure `recover:` fired on — the pre-recovery
    /// truth: its code marks `task_recovered`, its spend rides the
    /// terminal frame, and its record is what a still-parked task reads
    /// as in the workflow-end view.
    pub failed: FailedOutcome,
    /// The classification-time render failure — what a non-parking
    /// surface (a `for_each` iteration · an undeclared awaited root)
    /// reports instead of awaiting.
    pub render_error: TaskErrorRecord,
    /// The not-yet-terminal `tasks.<id>` roots the render awaits.
    pub awaiting: BTreeSet<String>,
    /// The task's rendered `with:` namespace.
    pub with_ns: BTreeMap<String, Value>,
}

/// The run-scoped READ surfaces a deferred resolution needs (the records
/// stay a separate `&mut` — they are the spine's write surface).
#[derive(Clone, Copy)]
pub(crate) struct ResolveScope<'a> {
    pub wf: &'a RawWorkflow,
    pub inputs: &'a BTreeMap<String, Value>,
    pub consts: &'a BTreeMap<String, Value>,
    pub secrets: &'a BTreeMap<String, Value>,
    pub resume_ctx: &'a ResumeContext,
    pub jq_clock: nika_cap::JqClock,
    pub run_start: nika_kernel::tool_executor::ToolRunStart,
}

/// The settle spine's park table — task-id ordered (`BTreeMap`), so
/// every drain and the workflow-end pass are deterministic.
pub(crate) struct ParkedRecoveries {
    entries: BTreeMap<String, Parked>,
}

/// One parked finish, fully deconstructed (total — no placeholder
/// results, no unreachable arms at resolution time).
struct Parked {
    /// The task's index in `wf.tasks` — re-borrows the `RawTask` at
    /// resolution (the recover template + the `output:` bindings).
    task_index: usize,
    note: String,
    retries: Vec<RetryStamp>,
    agent_events: Vec<crate::agent_events::StampedAgentEvent>,
    duration_ms: u64,
    resume: Option<crate::resume::ResumeStamp>,
    /// The dispatch boundary's permit decisions recorded before the park
    /// (NEP-0007) — they ride to resolution like the declassify events.
    decisions: Vec<crate::witness::PermitDecision>,
    /// The `declassify:` receipt evidence computed when the task ran —
    /// the door opened BEFORE the park; the events ride to resolution.
    declassified: Vec<crate::task::DeclassifyEvidence>,
    pending: Box<PendingRecovery>,
}

impl ParkedRecoveries {
    pub(crate) fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }
}

/// Classify a recover render failure (spec 05 §recover step 3): `Some`
/// with the awaited task ids when EVERY failing island of `template`
/// names at least one not-yet-terminal `tasks.X` root — the render can
/// still succeed once those referents settle. `None` keeps the immediate
/// fail: a structural fault, or a failing island a terminal state cannot
/// repair (an unknown namespace · a broken path INTO a terminal record).
///
/// Islands are probed through the render's own resolver seam
/// ([`Scope::resolve_expr`]) so this classification cannot drift from
/// what the render accepts.
pub(crate) fn classify_await(template: &Value, scope: &Scope<'_>) -> Option<BTreeSet<String>> {
    let mut leaves = Vec::new();
    string_leaves(template, &mut leaves);
    let mut awaiting = BTreeSet::new();
    for leaf in leaves {
        let Ok(islands) = scan_templates(leaf) else {
            return None; // structural fault — no terminal state repairs it
        };
        for island in islands {
            if scope.resolve_expr(&island.src).is_ok() {
                continue;
            }
            let pending: Vec<String> = expr_refs(&island.expr)
                .into_iter()
                .filter_map(|r| match r {
                    NamespaceRef::Tasks { id, .. } if !scope.records().contains_key(&id) => {
                        Some(id)
                    }
                    _ => None,
                })
                .collect();
            if pending.is_empty() {
                return None; // not explained by a pending referent — fail now
            }
            awaiting.extend(pending);
        }
    }
    (!awaiting.is_empty()).then_some(awaiting)
}

/// String leaves of a recover template, in the order the render visits
/// them (values only — object keys are never rendered).
fn string_leaves<'v>(value: &'v Value, out: &mut Vec<&'v str>) {
    match value {
        Value::String(s) => out.push(s),
        Value::Array(items) => items.iter().for_each(|v| string_leaves(v, out)),
        Value::Object(map) => map.values().for_each(|v| string_leaves(v, out)),
        _ => {}
    }
}

/// The spine's settle step: park a pending recovery, settle everything
/// else into `live` (the wave's side map on the streamed spine), then
/// drain every park the spine's terminal truth covers. That truth is
/// `prior ∪ live` — the frozen prior-wave records the pipelines read
/// PLUS this wave's settles so far (disjoint by construction: a task
/// settles once). One call per finish — the only integration point the
/// settle loop needs.
// REASON: the settle surface (pens + run accumulators) + the park table —
// mirrors `settle` itself.
#[allow(clippy::too_many_arguments)]
pub(crate) fn settle_or_park(
    finish: Finish,
    scope: &ResolveScope<'_>,
    parked: &mut ParkedRecoveries,
    prior: &BTreeMap<String, TaskRecord>,
    live: &mut BTreeMap<String, TaskRecord>,
    ok: &mut bool,
    cache_hits: &mut Vec<String>,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    if let Some(finish) = try_park(finish, scope, parked) {
        crate::settle::settle(finish, live, ok, cache_hits, stamper, sink);
    }
    drain_ready(scope, parked, prior, live, ok, cache_hits, stamper, sink);
}

/// Reassemble a `Finish` around a settle — the shared tail of every
/// park path (declined · not-parkable · resolved).
#[allow(clippy::too_many_arguments)] // the Finish parts, one slot each
fn finish_with(
    id: String,
    settle: SettleAs,
    named: BTreeMap<String, Value>,
    resume: Option<crate::resume::ResumeStamp>,
    integrity: nika_cap::Integrity,
    declassified: Vec<crate::task::DeclassifyEvidence>,
    approval: Option<crate::approval::ApprovalAttestation>,
) -> Finish {
    Finish {
        id,
        settle,
        named,
        resume,
        integrity,
        declassified,
        approval,
    }
}

/// Park a pending-recovery finish — `None` when parked (nothing settles
/// yet). `Some(finish)` settles normally: either the finish untouched
/// (not a pending recovery), or its immediate failure when an awaited
/// root is NOT a declared task (such a root can never reach a terminal
/// state — the recovery fails exactly as if nothing had parked · spec 05).
fn try_park(
    finish: Finish,
    scope: &ResolveScope<'_>,
    parked: &mut ParkedRecoveries,
) -> Option<Finish> {
    let Finish {
        id,
        settle: settled_as,
        named,
        resume,
        integrity,
        declassified,
        approval,
    } = finish;
    let ran = match settled_as {
        SettleAs::Ran(ran) => ran,
        other => {
            let done = finish_with(id, other, named, resume, integrity, declassified, approval);
            return Some(done);
        }
    };
    let RanTask {
        note,
        retries,
        agent_events,
        decisions,
        evidence,
        duration_ms,
        result,
    } = *ran;
    let pending = match result {
        RunResult::PendingRecovery(pending) => pending,
        other => {
            let ran = RanTask {
                note,
                retries,
                agent_events,
                decisions,
                evidence,
                duration_ms,
                result: other,
            };
            let settle = SettleAs::Ran(Box::new(ran));
            let done = finish_with(id, settle, named, resume, integrity, declassified, approval);
            return Some(done);
        }
    };
    let declared = |t: &String| scope.wf.tasks.iter().any(|s| s.value.id.value == *t);
    if let Some(task_index) =
        task_index(scope.wf, &id).filter(|_| pending.awaiting.iter().all(declared))
    {
        parked.entries.insert(
            id,
            Parked {
                task_index,
                note,
                retries,
                agent_events,
                duration_ms,
                resume,
                decisions,
                declassified,
                pending,
            },
        );
        return None;
    }
    // An awaited root that is NOT a declared task can never reach a
    // terminal state — the recovery fails NOW, exactly as if nothing
    // had parked (spec 05).
    let PendingRecovery {
        failed,
        render_error,
        ..
    } = *pending;
    let ran = RanTask {
        note,
        retries,
        agent_events,
        decisions,
        // F-P6 · the parked failure's evidence rides back out.
        evidence: failed.evidence,
        duration_ms,
        result: RunResult::Failed {
            error: render_error,
            cost_usd: failed.cost_usd,
            cost_unpriced: failed.cost_unpriced,
            access: failed.access,
        },
    };
    let settle = SettleAs::Ran(Box::new(ran));
    let done = finish_with(id, settle, named, resume, integrity, declassified, approval);
    Some(done)
}

/// Resolve + settle every park the spine's terminal truth covers, to a
/// fixpoint (a resolution settles a record into `live`, which can cover
/// another park — chains drain transitively). The truth is `prior ∪
/// live`: the wave-frozen prior records plus the settles so far, checked
/// clone-free; a resolution renders against `live` directly when `prior`
/// is empty (the workflow-end shape) and against a per-resolution merged
/// view otherwise (the rare path — the same clone-for-a-scope trade as
/// the cleanup overlay). Resolutions settle into `live` (the wave's one
/// write surface). Task-id order · bounded (each pass removes one entry).
// REASON: the settle surface + the park table — same shape as settle_or_park.
#[allow(clippy::too_many_arguments)]
fn drain_ready(
    scope: &ResolveScope<'_>,
    parked: &mut ParkedRecoveries,
    prior: &BTreeMap<String, TaskRecord>,
    live: &mut BTreeMap<String, TaskRecord>,
    ok: &mut bool,
    cache_hits: &mut Vec<String>,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    loop {
        let ready = parked
            .entries
            .iter()
            .find(|(_, p)| {
                p.pending
                    .awaiting
                    .iter()
                    .all(|t| prior.contains_key(t) || live.contains_key(t))
            })
            .map(|(id, _)| id.clone());
        let Some(id) = ready else { return };
        let Some(park) = parked.entries.remove(&id) else {
            return;
        };
        let finish = if prior.is_empty() {
            resolve_parked(id, park, scope, live)
        } else {
            let mut view = prior.clone();
            view.extend(live.iter().map(|(k, v)| (k.clone(), v.clone())));
            resolve_parked(id, park, scope, &view)
        };
        crate::settle::settle(finish, live, ok, cache_hits, stamper, sink);
    }
}

/// The workflow-end pass (spec 05 §recover step 3 tail): drain what the
/// final records cover, then resolve the rest — mutual recovery cycles —
/// against a FROZEN view where every still-parked task reads as its
/// PRE-recovery FAILED record (recovery never rewrites the referent's
/// history; both sides of a cycle see the other's failure, whatever they
/// themselves resolve to). Settles in task-id order. Also runs before a
/// budget abort, so no parked task is left frameless.
// REASON: the settle surface + the park table — same shape as settle_or_park.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_at_end(
    scope: &ResolveScope<'_>,
    parked: &mut ParkedRecoveries,
    records: &mut BTreeMap<String, TaskRecord>,
    ok: &mut bool,
    cache_hits: &mut Vec<String>,
    stamper: &mut dyn Stamper,
    sink: &mut dyn EventSink,
) {
    // Post-merge, the run-wide records ARE the whole truth (prior empty).
    let no_prior = BTreeMap::new();
    drain_ready(
        scope, parked, &no_prior, records, ok, cache_hits, stamper, sink,
    );
    if parked.entries.is_empty() {
        return;
    }
    let mut view = records.clone();
    for (id, park) in &parked.entries {
        view.insert(
            id.clone(),
            failed_view_record(&park.pending.failed.record, park.retries.len()),
        );
    }
    let ids: Vec<String> = parked.entries.keys().cloned().collect();
    let mut resolved = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(park) = parked.entries.remove(&id) {
            resolved.push(resolve_parked(id, park, scope, &view));
        }
    }
    for finish in resolved {
        crate::settle::settle(finish, records, ok, cache_hits, stamper, sink);
    }
}

/// A still-parked task as the workflow-end view reads it: its
/// pre-recovery failure (status + the original typed error + the
/// outcome axis — a recover template may read `tasks.<parked>.cause`,
/// so the view triages exactly like a live settle · spec 13).
fn failed_view_record(error: &TaskErrorRecord, retries: usize) -> TaskRecord {
    let attempts = u32::try_from(retries).unwrap_or(u32::MAX).saturating_add(1);
    let mut record = TaskRecord::unran(
        TaskStatus::Failure,
        crate::record::failure_cause(error, attempts),
    );
    record.attempts = Some(attempts);
    record.error = Some(error.clone());
    record
}

/// Resolve one park against `view`: re-render the recover template —
/// success takes the normal recovered path (`recovered_from` marks the
/// original code · the failed attempts' spend rides), a render error
/// takes the normal recovery-failed path. `output:` bindings re-evaluate
/// over the FINAL value (the recovery substitutes the raw output BEFORE
/// binding extraction · spec 05), and the ADR-099 resume stamp passes
/// the same secret-leak filter the live pipeline applies.
fn resolve_parked(
    id: String,
    park: Parked,
    scope: &ResolveScope<'_>,
    view: &BTreeMap<String, TaskRecord>,
) -> Finish {
    let Parked {
        task_index,
        note,
        retries,
        agent_events,
        decisions,
        duration_ms,
        resume,
        declassified,
        pending,
    } = park;
    let PendingRecovery {
        failed,
        render_error,
        awaiting: _,
        with_ns,
    } = *pending;
    let FailedOutcome {
        record,
        cost_usd,
        cost_unpriced,
        evidence,
        access,
    } = failed;
    let result = match recover_template(scope.wf, task_index) {
        Some(template) => {
            let render_scope = Scope::workflow_with_value_authorities(
                view,
                scope.inputs,
                scope.consts,
                scope.secrets,
            )
            // Iterations never park (fan-out boundary), and rendering performs
            // no effect, so neither loop locals nor permits ride this scope.
            .with_task_context(Some(&with_ns), None, None, None);
            match expr::render_json(template, &render_scope) {
                Ok(value) => RunResult::recovered(value, record, cost_usd, cost_unpriced),
                Err(err) => RunResult::Failed {
                    error: runtime_error_record(&RuntimeError::from(err)),
                    cost_usd,
                    cost_unpriced,
                    access: access.clone(),
                },
            }
        }
        // Total-function backstop (a park is only ever built FROM the
        // recover arm): no template ⇒ the classification-time failure.
        None => RunResult::Failed {
            error: render_error,
            cost_usd,
            cost_unpriced,
            access: access.clone(),
        },
    };
    let mut settled_as = SettleAs::Ran(Box::new(RanTask {
        note,
        retries,
        agent_events,
        decisions,
        // F-P6 · the parked failure's evidence rides back out.
        evidence,
        duration_ms,
        result,
    }));
    let named = match scope.wf.tasks.get(task_index) {
        Some(task) => bind_outputs(&task.value, &mut settled_as, scope.jq_clock),
        None => BTreeMap::new(),
    };
    let resume = resume.filter(|_| {
        success_output(&settled_as)
            .and_then(|v| serde_json::to_string(v).ok())
            .is_none_or(|text| !scope.resume_ctx.leaks_secret(&text))
    });
    let integrity = parked_integrity(scope, task_index, view);
    finish_with(
        id,
        settled_as,
        named,
        resume,
        integrity,
        declassified,
        // A recovered prompt's answer came from the recovery, never the
        // prompter — there is no approval decision to attest (NEP-0013).
        None,
    )
}

/// The F-O1 label over the FINAL view (the recover template's own reads
/// propagate — the recovered output embeds what it saw). Split out of
/// [`resolve_parked`] for the 100-line fn ratchet · semantics unchanged.
fn parked_integrity(
    scope: &ResolveScope<'_>,
    task_index: usize,
    view: &BTreeMap<String, TaskRecord>,
) -> nika_cap::Integrity {
    scope
        .wf
        .tasks
        .get(task_index)
        .map_or_else(nika_cap::Integrity::trusted, |task| {
            crate::integrity::task_integrity(&task.value, view)
        })
}

/// The declared recover template of `wf.tasks[index]`, when it is one.
fn recover_template(wf: &RawWorkflow, index: usize) -> Option<&Value> {
    let on_error = wf.tasks.get(index)?.value.on_error.as_ref()?;
    match &on_error.value.action {
        OnErrorAction::Recover(v) => Some(&v.value),
        _ => None,
    }
}

/// The index of `id` in the workflow's task list.
fn task_index(wf: &RawWorkflow, id: &str) -> Option<usize> {
    wf.tasks.iter().position(|t| t.value.id.value == id)
}

#[cfg(test)]
mod tests;
