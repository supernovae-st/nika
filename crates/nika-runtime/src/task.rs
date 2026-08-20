// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The per-task pipeline (spec 03/05) — gate → `with:` → `for_each:` →
//! attempt loop (`retry:` × `timeout:`) → `on_error:` → unwind.
//!
//! The pipeline is PURE with respect to the pens: it returns a
//! [`Finish`] describing everything that happened (retries · result ·
//! duration) and the settle pass in `lib.rs` emits the events in wave
//! order — concurrency never touches emission (the ordered-settlement
//! contract · deterministic reservations · Blelloch et al. `PPoPP` 2012).

use std::collections::BTreeMap;
use std::time::Duration;

use futures_util::StreamExt;
use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::provider::{ProviderInferDyn, ProviderMeta};
use nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn;
use nika_kernel::clock::ClockDyn;
use nika_kernel::http::HttpPostDyn;
use nika_kernel::process::ShellRunDyn;
use nika_kernel::tool_executor::ToolExecuteDyn;
use nika_schema::raw::{ForEachValue, RawAction, RawTask, RawWorkflow};
use nika_schema::types::{OnErrorAction, Permits, WhenGate};
use serde_json::Value;

use crate::Runtime;
use crate::dispatch::DispatchCtx;
use crate::dispatch::DispatchOk;
use crate::errors::RuntimeError;
use crate::expr::{self, Scope};
use crate::record::{TaskErrorRecord, TaskRecord, TaskStatus};
use crate::retry::jitter_key;
pub(crate) use crate::retry::on_error_applies;
use crate::witness::PermitWitness;
use with_map::{render_boundary_with, render_with};

mod fan_out;
mod with_map;

/// The spec wire code for a task-level timeout (spec 03 §timeout ·
/// catchable by `on_error:` · never retryable).
///
/// SPEC-PLANE code (the canon is the spec 05 table · resolvable via
/// `nika_pack::error_codes()` · NOT a `nika_error::codes` registry
/// entry — that registry carries the engine-internal NIKA-1700 range).
/// Pinned against the embedded canon by `emitted_spec_codes_resolve_in_the_embedded_canon`.
pub(crate) const TIMEOUT_CODE: &str = "NIKA-TIMEOUT-001";

/// The spec wire code for an expression type error at evaluation —
/// the runtime's emission site is the non-array `for_each` collection
/// (spec 03 · same spec-plane discipline as [`TIMEOUT_CODE`]).
pub(crate) const VAR_TYPE_CODE: &str = "NIKA-VAR-006";

/// One task's complete, pen-free outcome — the settle pass turns this
/// into events + a record.
mod declassify;
mod failed;
mod finally;

pub(crate) use declassify::{DeclassifyEvidence, declassify_evidence};
pub(crate) use failed::FailedOutcome;

/// Assemble the `Finish` of a RAN task (the output bindings spec 04 ·
/// the resume filter · the F-O1 declassify evidence · the F-P4 approval
/// attestation) — split out of `run_task_pipeline` for the 100-line fn
/// ratchet · semantics unchanged.
// REASON: the ran assembly threads the task + its computed parts — 10
// params, each one a distinct pipeline product (same trade as the caller).
#[allow(clippy::too_many_arguments)]
fn assemble_ran_finish(
    task: &RawTask,
    id: String,
    mut settle: SettleAs,
    resume: Option<crate::resume::ResumeStamp>,
    resume_ctx: &crate::resume::ResumeContext,
    inputs: &BTreeMap<String, Value>,
    records: &BTreeMap<String, TaskRecord>,
    integrity: nika_cap::Integrity,
    approval: Option<crate::approval::ApprovalAttestation>,
) -> Finish {
    // `output:` named bindings (spec 04 §Output binding) — evaluated
    // over the task's FINAL raw output, BEFORE settle emits the
    // terminal frame, so a binding error (NIKA-VAR-002/004) turns a
    // success into a failure (the cascade) rather than landing after
    // a `TaskCompleted`. The map carries one entry per declared
    // binding (the value on success · `Null` on a non-success ·
    // defined-null reads).
    let named = bind_outputs(task, &mut settle);
    let resume = filter_leaky_resume(resume, &settle, resume_ctx);
    // F-O1 PR-3 · the task RAN — the door was used: the receipt
    // carries one `declassify` event per declared entry (the settle
    // spine emits them after `task_started`).
    let declassified = declassify_evidence(task, inputs, records);
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

pub(crate) struct Finish {
    pub id: String,
    pub settle: SettleAs,
    /// `output:` named bindings (spec 04 §Output binding) — `<name>` →
    /// the jq result over the raw output (or `Null` for a non-success
    /// task · defined-null). Empty when the task declares no `output:`.
    pub named: BTreeMap<String, Value>,
    /// ADR-099 resume identity — stamped onto a SUCCESS `task_completed`
    /// record (additive trace fields). `None` = not resume-eligible this
    /// run (future form · render miss · secret leak) — the task records
    /// no key and simply never skips (honest degradation).
    pub resume: Option<crate::resume::ResumeStamp>,
    /// The coarse runtime integrity label (F-O1 PR-1 · additive) —
    /// computed from the task's static reference surface + the settled
    /// upstream records ([`crate::integrity::task_integrity`]) for a task
    /// that RAN or cache-hit; [`nika_cap::Integrity::Trusted`] for a
    /// task that never started (its output is `Null` — no content
    /// flowed). The settle spine stamps it on the record; no gate
    /// consumes it yet (PR-2).
    pub integrity: nika_cap::Integrity,
    /// The `declassify:` receipt evidence (F-O1 PR-3 · NEP-0004 law 5) —
    /// one entry per declared door, emitted as `declassify` events between
    /// `task_started` and the terminal frame when the task RAN (the door
    /// was used). Empty everywhere else (a skipped/cancelled/cache-hit
    /// task never opened it).
    pub declassified: Vec<DeclassifyEvidence>,
    /// The F-P4 approval attestation (NEP-0013 law 4) — `Some` when a
    /// prompt's ticket DECIDED (allow · deny · dedup · an engine
    /// refusal), emitted as `approval_decided` beside the terminal
    /// frame. `None` everywhere else (a blocked prompt carries its mint
    /// on `workflow_paused`; a recovered prompt's answer came from the
    /// recovery, never the prompter — nothing to attest).
    pub approval: Option<crate::approval::ApprovalAttestation>,
}

/// How the task settles (spec 03 §task states).
pub(crate) enum SettleAs {
    /// The default gate became unsatisfiable (upstream failure /
    /// cancellation) — a decision, not a defect. `blocked_by` names the
    /// first unsatisfied dependency (the WHY the journal can teach).
    Cancelled {
        note: &'static str,
        blocked_by: Option<String>,
    },
    /// The `when:` gate evaluated false · or an empty `for_each`
    /// collection (pure skip · no cascade).
    SkippedGate {
        note: &'static str,
        /// The CEL text that evaluated false (`when:` closes only) —
        /// the journal answers « why did this not run » verbatim.
        expr: Option<String>,
    },
    /// A pre-dispatch failure (gate eval · `with:` render · `for_each`
    /// collection) — never started · no `on_finally:` (spec 03).
    FailedBeforeStart {
        stage: &'static str,
        error: TaskErrorRecord,
    },
    /// ADR-099 `--resume` cache hit — the task's identity matched a
    /// journaled success: its output is REHYDRATED (never re-executed ·
    /// no `TaskStarted` · no `on_finally:` — the original run already
    /// ran its cleanup). Settles as a plain success downstream.
    CacheHit { output: Value },
    /// The task ran (dispatched at least once). Boxed: the attempt
    /// history is intrinsically the big variant (and grows with every
    /// attestation lane — the F-P6 binding evidence pushed the enum past
    /// clippy's `large_enum_variant` wall) — the settle channel moves
    /// `Finish` by value, so the big story stays off it.
    Ran(Box<RanTask>),
}

/// A task that started — attempt history + terminal result.
pub(crate) struct RanTask {
    /// The dispatch note (`invoke · <tool>` · …) — `TaskStarted`.
    pub note: String,
    /// One entry per retry that was scheduled (`TaskRetrying`).
    pub retries: Vec<RetryStamp>,
    /// The agent loop's decisions across ALL attempts, in order,
    /// attempt-stamped (+ iteration on fan-out lanes) — ADR-096 · empty
    /// for non-agent verbs · the settle pass emits them between the
    /// retry frames and the terminal frame. Includes the prefix of an
    /// attempt a timeout cut short (the buffer lives OUTSIDE the
    /// cancellable region — review F1).
    pub agent_events: Vec<crate::agent_events::StampedAgentEvent>,
    /// The dispatch boundary's permit decisions across attempts (NEP-0007
    /// law 2 · spec 17) — one `permit_checked` frame each at settle.
    pub decisions: Vec<crate::witness::PermitDecision>,
    /// F-P6 · the settling dispatch's binding evidence — `Fired` (the
    /// terminal frame carries both digests) or `Refused` (the finding
    /// rides even under an `on_error:` recovery — never a warn). `None`
    /// for the un-gated verbs (infer · agent) · a never-fired task · the
    /// fan-out aggregate's pair (the `child: None` precedent — but a
    /// diverged iteration's finding DOES ride).
    pub evidence: Option<crate::dispatch::commit::CommitEvidence>,
    /// Clock-measured wall time across attempts + cleanup (0 under a
    /// mock clock · real under the production clock — the event-stream
    /// determinism contract is "deterministic seams in · deterministic
    /// stream out", and the clock is a seam).
    pub duration_ms: u64,
    /// The terminal result after `retry:` + `on_error:`.
    pub result: RunResult,
}

impl RanTask {
    /// Attempts made, counting every attempt including the settling one
    /// (spec 13 §payload): one per SCHEDULED retry plus the first — for
    /// a budget-cut task the in-flight attempt the race dropped IS the
    /// settling one, so the count stays honest there too.
    pub(crate) fn attempts(&self) -> u32 {
        u32::try_from(self.retries.len())
            .unwrap_or(u32::MAX)
            .saturating_add(1)
    }
}

/// One scheduled retry (the `TaskRetrying` event payload).
pub(crate) struct RetryStamp {
    pub attempt: u32,
    pub max_attempts: u32,
    pub delay_ms: u64,
}

/// The three read-only value namespaces every lane threads whole
/// (`inputs` · `const` · `secrets` — the value authorities are exactly
/// three since `config:` died) — one alias, four signatures.
type ValueBags<'a> = (
    &'a BTreeMap<String, Value>,
    &'a BTreeMap<String, Value>,
    &'a BTreeMap<String, Value>,
);

/// The task's terminal result.
pub(crate) enum RunResult {
    /// Success — the verb's value (or the `on_error: recover` value) +
    /// token spend when the verb reports it + an optional non-fatal
    /// diagnostic (rides `TaskCompleted` as a `warning` field · the OBS-E
    /// blank-answer class left this channel when it was promoted to the
    /// typed NIKA-INFER-004 failure · #651).
    Success {
        value: Value,
        tokens: Option<i64>,
        /// `Some(original error)` when `on_error.recover` repaired this
        /// success — the settle path emits `task_recovered` (one site ·
        /// INV#24) before the terminal `task_completed`, and the record
        /// keeps the WHOLE original error as `recovered_from`
        /// (spec 13 §payload · success(recovered)).
        recovered_from: Option<TaskErrorRecord>,
        warning: Option<String>,
        /// The child-run summary when the task was an `invoke: workflow:`
        /// call (spec 14 law 8 · rides the terminal frame as `child`).
        /// Boxed — the row is cold (see `DispatchOk::child`).
        child: Option<Box<crate::child::ChildRunSummary>>,
        /// Real spend (catalog × usage split + tool-reported) · None =
        /// unpriced · honest. (The by-source attribution key lives on
        /// `DispatchOk` — the ledger debits at that leaf, before the
        /// result folds to this settle shape.)
        cost_usd: Option<f64>,
        /// Why (part of) the spend is NOT in `cost_usd` — rides the
        /// terminal frame as `cost_unpriced`.
        cost_unpriced: Option<nika_types::cost::UnpricedReason>,
        /// The resolved `provider/name` an infer/agent success ran on
        /// (D-2026-08-04-N1) — rides the terminal frame as structured
        /// `model` · `provider` · `access` · `billing` fields, retiring
        /// the note-string parse. `None` on verbs that name no model
        /// (exec · invoke) and on recovered author-supplied values.
        model: Option<String>,
    },
    /// `on_error: skip` — skipped with the original error readable
    /// (spec 05 · the one coexist state). The billed-then-skipped spend
    /// rides so the frame says what the attempt cost.
    SkippedWithError {
        error: TaskErrorRecord,
        cost_usd: Option<f64>,
        cost_unpriced: Option<nika_types::cost::UnpricedReason>,
    },
    /// Failed after retries with no recovery — with the spend the
    /// billed attempts incurred (already ledger-debited per attempt).
    Failed {
        error: TaskErrorRecord,
        cost_usd: Option<f64>,
        cost_unpriced: Option<nika_types::cost::UnpricedReason>,
    },
    /// `on_error: recover` whose reference awaits not-yet-terminal
    /// referents (spec 05 §recover step 3 · a recover ref is NOT an
    /// edge): the outcome is DECIDED on the ordered settle spine once
    /// every awaited task is terminal — parked there, never settled
    /// here. A `for_each` iteration never parks: its collector
    /// downgrades this to the immediate render failure.
    PendingRecovery(Box<crate::recover::PendingRecovery>),
}

struct IterationLocals<'a> {
    item: &'a Value,
    index: usize,
}

impl<S, T, H, P, D, C> Runtime<S, T, H, P, D, C>
where
    S: ShellRunDyn + Sync,
    T: ToolExecuteDyn,
    H: HttpPostDyn + Send + Sync + 'static,
    P: ProviderInferDyn + ProviderMeta,
    D: ToolDefinitionProviderDyn,
    C: ClockDyn + Sync,
{
    /// The full per-task pipeline (pen-free · see module docs).
    ///
    /// `records` is the wave-frozen view — same-wave tasks never
    /// reference each other (checker law), so freezing at wave entry
    /// is sound and keeps the read side shareable across the wave's
    /// concurrent pipelines.
    // REASON: the pipeline threads the run's shared read surfaces + the
    // spend ledger — 9 params, each one a distinct run-scoped seam.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_task_pipeline(
        &self,
        task: &RawTask,
        wf: &RawWorkflow,
        records: &BTreeMap<String, TaskRecord>,
        inputs: &BTreeMap<String, Value>,
        consts: &BTreeMap<String, Value>,
        secrets: &BTreeMap<String, Value>,
        permits: Option<&Permits>,
        types: &BTreeMap<String, nika_types::types::NikaType>,
        resume_ctx: &crate::resume::ResumeContext,
        ledger: &crate::ledger::RunLedger,
    ) -> Finish {
        let id = task.id.value.clone();
        // ── GATE-v2 (spec 03 §gate algebra) — structural per-edge
        //    admission over the derived edges · cannot error ──────────
        if let Some(finish) = gate_finish(task, id.clone(), records) {
            return finish;
        }

        // ── The boundary (spec 03 §dispatch pipeline) — `with:`
        //    materializes, then `when:` judges over LOCAL names.
        //    Boundary errors settle failure OUTSIDE on_error scope
        //    (the armor covers the verb, not the boundary). ───────────
        let boundary_with = match render_boundary_with(task, records, inputs, consts, secrets) {
            Ok(ns) => ns,
            Err(err) => return with_error_finish(id, task, &err),
        };
        if let Some(finish) = when_finish(task, id.clone(), &boundary_with, inputs, consts, secrets)
        {
            return finish;
        }

        // F-O1 · the coarse integrity label — computed from the task AS
        // AUTHORED (an `--answer` binding below is the operator's act,
        // never an ingress) over the wave-frozen records. Carried on the
        // Finish; the settle spine stamps it.
        let integrity = crate::integrity::task_integrity(task, records);

        // ── ADR-099 resume identity + the skip verdict — extracted
        //    (the 100-line fn ratchet · semantics unchanged) ──
        let (resume, skip) =
            self.resume_skip_finish(task, &id, records, inputs, consts, resume_ctx, &integrity);
        if let Some(finish) = skip {
            return finish;
        }

        // ── F-P4 · the approval ticket (NEP-0013) — mint BEFORE the
        //    ask · dedup + rate-limit · the resumed `--answer` validated
        //    against the shown hash BEFORE it binds. ──
        let gated = match self.approval_gated_task(task, records, inputs, consts, resume_ctx) {
            Ok(gated) => gated,
            Err(refusal) => return *refusal,
        };
        let task = &*gated;

        // ── `for_each:` fan-out or the single lane ──────────────────
        let settle = self
            .run_lanes(
                task,
                wf,
                boundary_with,
                records,
                (inputs, consts, secrets),
                permits,
                types,
                ledger,
                &integrity,
            )
            .await;
        // F-P4 · the ask RESOLVED (or not) — the attestation assembles
        // here and the settle spine journals it (`approval_decided`).
        let approval = self
            .approvals
            .attest_outcome(&id, &settle, self.now_unix_ms());
        assemble_ran_finish(
            task, id, settle, resume, resume_ctx, inputs, records, integrity, approval,
        )
    }

    /// The ADR-099 resume gate: the stamp (computed from the task AS
    /// AUTHORED — an `--answer` never re-keys: a prompt's answer is
    /// output non-determinism, like an infer's — §4 replays it) plus the
    /// skip verdict when BOTH hashes match a journaled success (an
    /// edited task or a changed input re-runs · §1 · a freshly-supplied
    /// `--answer` FORCES the ask — operator intent is explicit, never
    /// replay an old answer over a new one). The stamp returns for the
    /// leak filter downstream. A cache hit keeps the pipeline's computed
    /// `integrity` (the rehydrated output carries the task's provenance).
    #[allow(clippy::too_many_arguments)] // the run-scoped reads + the F-O1 label
    fn resume_skip_finish(
        &self,
        task: &RawTask,
        id: &String,
        records: &BTreeMap<String, TaskRecord>,
        inputs: &BTreeMap<String, Value>,
        consts: &BTreeMap<String, Value>,
        resume_ctx: &crate::resume::ResumeContext,
        integrity: &nika_cap::Integrity,
    ) -> (Option<crate::resume::ResumeStamp>, Option<Finish>) {
        let resume = crate::resume::stamp(task, records, inputs, consts, resume_ctx);
        let skip = if self.prompt_answers.contains_key(id) {
            None
        } else {
            self.cache_hit_finish(task, id, resume.as_ref())
        }
        .map(|mut finish| {
            finish.integrity = integrity.clone();
            finish
        });
        (resume, skip)
    }

    /// The F-P4 gate segment (NEP-0013) — split out of the pipeline for
    /// the 100-line fn ratchet: `Ok` is the task to run (borrowed unless
    /// an answer/dedup bound a clone — the binding rides the answered
    /// branch, dispatch-only, never the resume identity), `Err` is the
    /// typed refusal's Finish (boxed — the happy path stays slim).
    /// Semantics unchanged.
    #[allow(clippy::too_many_arguments)] // the run-scoped reads
    fn approval_gated_task<'a>(
        &self,
        task: &'a RawTask,
        records: &BTreeMap<String, TaskRecord>,
        inputs: &BTreeMap<String, Value>,
        consts: &BTreeMap<String, Value>,
        resume_ctx: &crate::resume::ResumeContext,
    ) -> Result<std::borrow::Cow<'a, RawTask>, Box<Finish>> {
        match self.approval_gate(task, records, inputs, consts, resume_ctx) {
            crate::approval::Gate::NotPrompt => Ok(std::borrow::Cow::Borrowed(task)),
            crate::approval::Gate::Run(bound) => Ok(std::borrow::Cow::Owned(*bound)),
            crate::approval::Gate::Refused(refusal) => Err(Box::new(approval_refusal_finish(
                task.id.value.clone(),
                task,
                *refusal,
            ))),
        }
    }

    /// The execution lane split: `for_each:` fan-out when declared ·
    /// the single lane otherwise (spec 03 §dispatch pipeline).
    // REASON: the same run-scoped seams as the pipeline.
    #[allow(clippy::too_many_arguments)]
    async fn run_lanes(
        &self,
        task: &RawTask,
        wf: &RawWorkflow,
        boundary_with: BTreeMap<String, Value>,
        records: &BTreeMap<String, TaskRecord>,
        (inputs, consts, secrets): ValueBags<'_>,
        permits: Option<&Permits>,
        types: &BTreeMap<String, nika_types::types::NikaType>,
        ledger: &crate::ledger::RunLedger,
        integrity: &nika_cap::Integrity,
    ) -> SettleAs {
        match task.for_each.as_ref() {
            None => {
                self.run_single(
                    task,
                    wf,
                    boundary_with,
                    records,
                    (inputs, consts, secrets),
                    permits,
                    types,
                    ledger,
                    integrity,
                )
                .await
            }
            Some(spanned) => {
                self.run_fan_out(
                    task,
                    wf,
                    &spanned.value,
                    &boundary_with,
                    records,
                    (inputs, consts, secrets),
                    permits,
                    types,
                    ledger,
                    integrity,
                )
                .await
            }
        }
    }

    /// The ADR-099 skip check — `Some(finish)` iff a resume plan is
    /// present AND this task's recomputed identity matches its journaled
    /// success (both hashes · §1). The hit settles as a rehydrated
    /// success (`task_cache_hit` at the pens — VISIBLE, never silent);
    /// its `output:` bindings re-evaluate over the rehydrated value
    /// (pure jq · same programs, same input · spec 04).
    fn cache_hit_finish(
        &self,
        task: &RawTask,
        id: &str,
        resume: Option<&crate::resume::ResumeStamp>,
    ) -> Option<Finish> {
        let stamp = resume?;
        let prior = self.resume_plan.get(id)?;
        if prior.def_hash != stamp.def_hash || prior.input_hash != stamp.input_hash {
            return None;
        }
        let mut settle = SettleAs::CacheHit {
            output: prior.output.clone(),
        };
        let named = bind_outputs(task, &mut settle);
        Some(Finish {
            id: id.to_owned(),
            settle,
            named,
            resume: Some(stamp.clone()),
            // Stamped by the caller (`resume_skip_finish`) with the
            // pipeline's computed label — the rehydrated output carries
            // the task's provenance.
            integrity: nika_cap::Integrity::trusted(),
            // A cache hit never ran HERE — the original run recorded the
            // door (no new `declassify` event).
            declassified: Vec::new(),
            // Nor was any approval decided HERE — the original run's
            // `approval_decided` rides its own chain (ADR-099 §4).
            approval: None,
        })
    }

    /// The single-execution lane (no `for_each:`).
    // REASON: the run-scoped seams plus the boundary render + the F-O1 label.
    #[allow(clippy::too_many_arguments)]
    async fn run_single(
        &self,
        task: &RawTask,
        wf: &RawWorkflow,
        with_ns: BTreeMap<String, Value>,
        records: &BTreeMap<String, TaskRecord>,
        (inputs, consts, secrets): ValueBags<'_>,
        permits: Option<&Permits>,
        types: &BTreeMap<String, nika_types::types::NikaType>,
        ledger: &crate::ledger::RunLedger,
        integrity: &nika_cap::Integrity,
    ) -> SettleAs {
        // `with:` materialized at the boundary (spec 03 §dispatch
        // pipeline) — the single lane consumes it as rendered.
        let scope = Scope {
            records,
            inputs,
            consts,
            secrets,
            with_ns: Some(&with_ns),
            item: None,
            index: None,
            permits,
        };
        let started = self.clock.now();
        let witness = std::sync::Arc::new(PermitWitness::new());
        let attempt = self.attempt_loop(task, &scope, types, ledger, &witness);
        let mut ran = nika_builtin::witness::scope_attempt_witness(witness.clone(), attempt).await;
        // `on_finally:` — the task STARTED (spec 03 · success AND
        // failure · before the failure propagates in the DAG). The
        // cleanup lane's decisions ride a dedicated witness (the
        // parent's is already drained by attempt_loop) merged right after.
        let finally_witness = std::sync::Arc::new(PermitWitness::new());
        let finally = self.run_finally(task, wf, &scope, &ran, integrity, &finally_witness);
        nika_builtin::witness::scope_attempt_witness(finally_witness.clone(), finally).await;
        ran.decisions.extend(finally_witness.take());
        ran.duration_ms = self.since_ms(started);
        SettleAs::Ran(Box::new(ran))
    }

    /// The `for_each:` fan-out lane (spec 03 · closed at v1).
    // REASON: same run-scoped seams as the pipeline + the F-O1 label.
    #[allow(clippy::too_many_arguments)]
    async fn run_fan_out(
        &self,
        task: &RawTask,
        wf: &RawWorkflow,
        collection: &ForEachValue,
        boundary_with: &BTreeMap<String, Value>,
        records: &BTreeMap<String, TaskRecord>,
        (inputs, consts, secrets): ValueBags<'_>,
        permits: Option<&Permits>,
        types: &BTreeMap<String, nika_types::types::NikaType>,
        ledger: &crate::ledger::RunLedger,
        integrity: &nika_cap::Integrity,
    ) -> SettleAs {
        // The collection resolves on the PRE-fan-out surface (the
        // item-free boundary bindings) · empty settles `skipped`.
        let items = match fan_out::resolve_fan_out_items(
            collection,
            boundary_with,
            inputs,
            consts,
            secrets,
        ) {
            Ok(items) => items,
            Err(settle) => return *settle,
        };

        let started = self.clock.now();
        let fail_fast = task.fail_fast.as_ref().is_none_or(|f| f.value);
        let cap = task
            .max_parallel
            .as_ref()
            .map_or(items.len(), |m| (m.value as usize).max(1));
        let total = items.len();

        // Iterations dispatch concurrently (cap = `max_parallel`) ·
        // settle in INPUT order (the same ordered-settlement law as
        // waves · positions stay aligned · spec 03 §null-at-index).
        // Budget gate at ADMISSION (`take_while` runs at `buffered` PULL):
        // in-flight complete and count · unpulled never start · with
        // `max_parallel` ≥ items, only a capped fan-out starves mid-run.
        let mut stream = futures_util::stream::iter(
            items
                .iter()
                .enumerate()
                .take_while(|_| !ledger.tripped())
                .map(|(index, item)| {
                    let locals = IterationLocals { item, index };
                    self.run_iteration(
                        task,
                        records,
                        (inputs, consts, secrets),
                        locals,
                        permits,
                        types,
                        ledger,
                    )
                }),
        )
        .buffered(cap);

        let mut acc = fan_out::collect_fan_out(&mut stream, total, fail_fast).await;
        drop(stream);

        // Budget starvation: iterations that were never admitted leave
        // the accumulation short — the task fails with the budget code
        // (same class as `fail_fast`'s early stop · partial array).
        if acc.outputs.len() < total && ledger.tripped() && acc.first_error.is_none() {
            acc.first_error = Some(fan_out::budget_stop_record(total - acc.outputs.len()));
        }

        let result = fan_out::fan_out_result(
            acc.outputs,
            acc.tokens_sum,
            (acc.first_error, acc.first_recovered_from),
            (acc.cost_sum, acc.unpriced),
        );
        let retries = acc.retries;
        let agent_events = acc.agent_events;
        let mut ran = RanTask {
            note: fan_out::fan_note(total, acc.recovered),
            retries,
            agent_events,
            decisions: acc.decisions,
            // F-P6 · no pair aggregates N iterations (the `child: None` precedent).
            evidence: None,
            duration_ms: 0,
            result,
        };
        // `on_finally:` runs ONCE after all iterations (spec 03 ·
        // `item`/`index` are NOT in scope there).
        let finally_scope =
            Self::fan_out_finally_scope(records, (inputs, consts, secrets), permits);
        let finally_witness = std::sync::Arc::new(PermitWitness::new());
        let finally = self.run_finally(task, wf, &finally_scope, &ran, integrity, &finally_witness);
        nika_builtin::witness::scope_attempt_witness(finally_witness.clone(), finally).await;
        ran.decisions.extend(finally_witness.take());
        ran.duration_ms = self.since_ms(started);
        SettleAs::Ran(Box::new(ran))
    }

    /// The `on_finally:` scope for a fan-out — `item`/`index` out of
    /// scope by law, and `permits` MUST flow so a fan-out `on_finally`
    /// exec is enforced like every other (NIKA-SEC-004) —
    /// `Scope::workflow` would drop it to None (the cleanup-bypass gap).
    fn fan_out_finally_scope<'a>(
        records: &'a BTreeMap<String, TaskRecord>,
        (inputs, consts, secrets): ValueBags<'a>,
        permits: Option<&'a Permits>,
    ) -> Scope<'a> {
        Scope {
            records,
            inputs,
            consts,
            secrets,
            with_ns: None,
            item: None,
            index: None,
            permits,
        }
    }

    /// One `for_each` iteration · per-iteration `with:` + locals +
    /// attempt loop (`retry:`/`timeout:`/`on_error:` per iteration).
    // REASON: the per-iteration seams (item · index) on top of the
    // pipeline's run-scoped ones — each param is distinct state.
    #[allow(clippy::too_many_arguments)]
    async fn run_iteration(
        &self,
        task: &RawTask,
        records: &BTreeMap<String, TaskRecord>,
        (inputs, consts, secrets): ValueBags<'_>,
        locals: IterationLocals<'_>,
        permits: Option<&Permits>,
        types: &BTreeMap<String, nika_types::types::NikaType>,
        ledger: &crate::ledger::RunLedger,
    ) -> RanTask {
        let with_ns = match render_with(
            task,
            records,
            inputs,
            consts,
            secrets,
            Some(locals.item),
            Some(locals.index),
        ) {
            Ok(ns) => ns,
            Err(err) => {
                return RanTask {
                    note: format!("for_each[{}]", locals.index),
                    retries: Vec::new(),
                    agent_events: Vec::new(),
                    decisions: Vec::new(),
                    evidence: None,
                    duration_ms: 0,
                    result: RunResult::Failed {
                        error: runtime_error_record(&err),
                        cost_usd: None,
                        cost_unpriced: None,
                    },
                };
            }
        };
        let scope = Scope {
            records,
            inputs,
            consts,
            secrets,
            with_ns: Some(&with_ns),
            item: Some(locals.item),
            index: Some(locals.index),
            permits,
        };
        let witness = std::sync::Arc::new(PermitWitness::new());
        let attempt = self.attempt_loop(task, &scope, types, ledger, &witness);
        let mut ran = nika_builtin::witness::scope_attempt_witness(witness.clone(), attempt).await;
        // Stamp the lane: without it a 2-iteration fan-out and a retried
        // single lane produce indistinguishable flat streams (review F3).
        #[allow(clippy::cast_possible_truncation)] // fan-out ≪ u32::MAX
        for stamped in &mut ran.agent_events {
            stamped.iteration = Some(locals.index as u32);
        }
        ran
    }

    /// The per-attempt dispatch context (the fn-length law's
    /// extraction) — the bound `--answer` for THIS task rides it (B5).
    fn task_ctx<'a>(
        &'a self,
        task: &'a RawTask,
        deadline: Option<std::time::Duration>,
        child_budget: Option<f64>,
        witness: &'a PermitWitness,
    ) -> DispatchCtx<'a> {
        let mut ctx = DispatchCtx::of_task(task, deadline, child_budget, witness);
        ctx.gate_answer = self.prompt_answers.get(&task.id.value).cloned();
        ctx
    }

    async fn attempt_loop(
        &self,
        task: &RawTask,
        scope: &Scope<'_>,
        types: &BTreeMap<String, nika_types::types::NikaType>,
        ledger: &crate::ledger::RunLedger,
        witness: &PermitWitness,
    ) -> RanTask {
        let started = self.clock.now();
        let max_attempts = task
            .retry
            .as_ref()
            .map_or(1, |r| r.value.max_attempts.max(1));
        let jitter_key = jitter_key(task, scope);
        let mut note = String::new();
        let mut retries: Vec<RetryStamp> = Vec::new();
        // Outside the timeout-cancellable region — survives the attempt's drop (review F1).
        let agent_buffer = crate::agent_events::BufferingObserver::new();
        let mut attempt_marks: Vec<usize> = Vec::new();
        let outcome = {
            // `budget` = the task's ONE `timeout:` — enforced below AND at dispatch (F1).
            let budget = task.timeout.as_ref().map(|t| t.value);
            // The `returns:` contract, resolved ONCE (spec 09 · W3) — `None` = gradual.
            let contract = crate::contract::TaskContract::of(task, types);
            // F-O1 PR-2 · the re-gate's per-template oracle — computed ONCE, used per attempt.
            let value_taint = crate::integrity::ValueTaint::of_task(task, scope.records);
            // law 6 · the child budget reads the ledger AT CALL TIME (per attempt).
            let ctx = || self.task_ctx(task, budget, ledger.remaining_usd(), witness);
            let attempts = async {
                let mut attempt = 1_u32;
                // Spend of FAILED attempts — folded onto the terminal frame.
                let mut failed_cost: Option<f64> = None;
                let mut failed_unpriced: Option<nika_types::cost::UnpricedReason> = None;
                loop {
                    let dispatched = self
                        .dispatch(
                            &task.action,
                            scope,
                            &value_taint,
                            &agent_buffer,
                            ctx(),
                            contract.as_ref(),
                        )
                        .await;
                    note.clone_from(&dispatched.note);
                    attempt_marks.push(agent_buffer.len());
                    match dispatched.result {
                        Ok(mut ok) => {
                            // THE leaf debit site (its OWN spend — failed
                            // attempts debited theirs; frame reports all).
                            ledger.debit_ok(&ok);
                            ok.fold_failed_spend(failed_cost, failed_unpriced);
                            return Ok(ok);
                        }
                        Err(failed) => {
                            let delay = self.failed_attempt_delay(
                                task,
                                failed,
                                ledger,
                                &mut failed_cost,
                                &mut failed_unpriced,
                                attempt,
                                max_attempts,
                                &jitter_key,
                            )?;
                            retries.push(RetryStamp {
                                attempt,
                                max_attempts,
                                delay_ms: delay,
                            });
                            self.clock.sleep(Duration::from_millis(delay)).await;
                            attempt += 1;
                        }
                    }
                }
            };

            self.race_budget(attempts, budget).await
        };

        let duration_ms = self.since_ms(started);
        if note.is_empty() {
            verb_note_prefix(&task.action).clone_into(&mut note); // timed out pre-dispatch
        }

        let (result, evidence) = dispatch_result(task, scope, outcome);
        RanTask {
            note,
            retries,
            agent_events: crate::agent_events::stamp_attempts(
                agent_buffer.into_events(),
                &attempt_marks,
            ),
            decisions: witness.take(),
            evidence,
            duration_ms,
            result,
        }
    }

    /// Race the attempt future against the task's ONE `timeout:` budget
    /// (spec 03 · the total across retries and their backoff sleeps).
    async fn race_budget<F>(
        &self,
        attempts: F,
        budget: Option<Duration>,
    ) -> Result<DispatchOk, FailedOutcome>
    where
        F: Future<Output = Result<DispatchOk, FailedOutcome>>,
    {
        match budget {
            None => attempts.await,
            Some(limit) => {
                let attempts = std::pin::pin!(attempts);
                let timer = std::pin::pin!(self.clock.sleep(limit));
                match futures_util::future::select(attempts, timer).await {
                    futures_util::future::Either::Left((result, _)) => result,
                    futures_util::future::Either::Right(((), _)) => {
                        // The attempt loop is dropped — futures
                        // cancellation at the await point · exec
                        // subprocesses die via the runner's
                        // kill-on-drop contract.
                        Err(FailedOutcome {
                            record: TaskErrorRecord {
                                code: TIMEOUT_CODE.to_owned(),
                                message: format!(
                                    "task exceeded its timeout of {} ms",
                                    limit.as_millis()
                                ),
                                transient: false, // never retryable (spec 03)
                            },
                            // The cancelled in-flight attempt may have
                            // billed server-side; nothing was reported, so
                            // nothing can honestly ride (the documented
                            // timeout-cancellation class).
                            cost_usd: None,
                            cost_unpriced: None,
                            // The dropped attempt's binding evidence dies
                            // with it (futures-cancellation — the gate
                            // verdict was never journaled either).
                            evidence: None,
                        })
                    }
                }
            }
        }
    }

    /// Milliseconds since `started` per the injected clock.
    fn since_ms(&self, started: std::time::Instant) -> u64 {
        // checked_duration_since: the ClockDyn contract does not forbid
        // a non-monotonic now() (an injected seam) — `duration_since`
        // would PANIC there (std contract) · a backwards clock reads as
        // 0 elapsed, honestly (review P1 · rust-pro angles).
        self.clock
            .now()
            .checked_duration_since(started)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    }
}

/// Evaluate the task's `output:` named bindings (spec 04 §Output binding)
/// over its FINAL raw output, returning `<name>` → value.
///
/// - **Success** · each binding's jq runs over the success raw output. The
///   FIRST binding that errors (NIKA-VAR-002 cardinality · NIKA-VAR-004
///   runtime) REPLACES the success in `settle` with a failure (the
///   binding is part of producing the output · its failure fails the task
///   · spec 04 §binding rules + 05) and the returned map is all-`Null`.
/// - **Non-success** (skipped · cancelled · failed · failed-before-start)
///   · every declared binding reads defined-`Null` (spec 04 · so a
///   downstream `tasks.X.<name>` of a skipped branch resolves to null,
///   not a 1702). The success VALUE is never recomputed.
///
/// Returns an empty map when the task declares no `output:` (the common
/// lane pays nothing). `pub(crate)`: the recover-await resolution binds
/// over the deferred value through THIS one site (spec 05 · the recovery
/// substitutes the raw output BEFORE binding extraction).
pub(crate) fn bind_outputs(task: &RawTask, settle: &mut SettleAs) -> BTreeMap<String, Value> {
    if task.extract.is_empty() {
        return BTreeMap::new();
    }
    // The success raw output, if this task succeeded — bindings extract
    // from it. Borrow it read-only first; the failure-replacement below
    // only runs on the error path (no borrow conflict).
    if let Some(output) = success_output(settle) {
        match eval_all_bindings(task, output) {
            Ok(named) => return named,
            // A binding failed → the task fails (NIKA-VAR-002/004). The
            // terminal frame becomes TaskFailed · bindings read null.
            Err(error) => {
                replace_success_with_failure(settle, error);
            }
        }
    }
    // Non-success (or just-failed-by-binding): every declared binding is
    // defined-null (spec 04).
    null_bindings(task)
}

/// Every declared `output:` binding mapped to defined-`Null` (spec 04 ·
/// the read of a binding on a non-success task). Names are unique (the
/// checker · §rules) · empty when the task declares no `output:`.
fn null_bindings(task: &RawTask) -> BTreeMap<String, Value> {
    task.extract
        .iter()
        .map(|(name, _)| (name.value.clone(), Value::Null))
        .collect()
}

/// The boundary `with:` render's failure Finish (spec 03 · a boundary
/// error settles failure OUTSIDE `on_error` scope — the armor covers
/// the verb, not the boundary) — split out of `run_task_pipeline` for
/// the 100-line fn ratchet · semantics unchanged.
fn with_error_finish(id: String, task: &RawTask, err: &crate::errors::RuntimeError) -> Finish {
    Finish {
        id,
        settle: SettleAs::FailedBeforeStart {
            stage: "with",
            error: runtime_error_record(err),
        },
        named: null_bindings(task),
        resume: None,
        // Never started — no content flowed (the F-O1 label is trusted
        // by default · the door never opened either).
        integrity: nika_cap::Integrity::trusted(),
        declassified: Vec::new(),
        approval: None,
    }
}

/// The F-P4 refusal's Finish (NEP-0013 · the gate's `Refused` arm) —
/// split out of `run_task_pipeline` for the 100-line fn ratchet: the
/// task never starts (the capability was refused before dispatch), the
/// typed `NIKA-SEC-010` cascades, and the deny attestation rides to the
/// settle spine. Semantics unchanged.
fn approval_refusal_finish(
    id: String,
    task: &RawTask,
    refusal: crate::approval::Refusal,
) -> Finish {
    Finish {
        id,
        settle: SettleAs::FailedBeforeStart {
            stage: "approval",
            error: TaskErrorRecord {
                code: crate::approval::APPROVAL_CODE.to_owned(),
                message: refusal.detail,
                transient: false,
            },
        },
        named: null_bindings(task),
        resume: None,
        // Never started — no content flowed.
        integrity: nika_cap::Integrity::trusted(),
        declassified: Vec::new(),
        approval: Some(refusal.attestation),
    }
}

/// The success raw output of a settled task (the value bindings extract
/// from), or `None` when the task did not settle as a plain success.
/// `pub(crate)`: the recover-await resolution re-applies the pipeline's
/// binding + leak-filter steps over the deferred value.
pub(crate) fn success_output(settle: &SettleAs) -> Option<&Value> {
    match settle {
        SettleAs::Ran(ran) => match &ran.result {
            RunResult::Success { value, .. } => Some(value),
            RunResult::SkippedWithError { .. }
            | RunResult::Failed { .. }
            | RunResult::PendingRecovery(_) => None,
        },
        // A cache hit IS a success — bindings extract from the
        // rehydrated output (ADR-099 · downstream parity with live).
        SettleAs::CacheHit { output } => Some(output),
        SettleAs::Cancelled { .. }
        | SettleAs::SkippedGate { .. }
        | SettleAs::FailedBeforeStart { .. } => None,
    }
}

/// Drop the resume stamp when the success output carries a resolved
/// secret VALUE — it must not reach the trace (ADR-099 §1: no
/// secret-derived material); the task then records no key and re-runs
/// live on resume.
fn filter_leaky_resume(
    resume: Option<crate::resume::ResumeStamp>,
    settle: &SettleAs,
    resume_ctx: &crate::resume::ResumeContext,
) -> Option<crate::resume::ResumeStamp> {
    resume.filter(|_| {
        success_output(settle)
            .and_then(|v| serde_json::to_string(v).ok())
            .is_none_or(|text| !resume_ctx.leaks_secret(&text))
    })
}

/// Evaluate every `output:` binding over `output` (the raw success value)
/// — returns the full map, or the FIRST binding error (spec 04 ordering:
/// bindings are evaluated in declaration order · the first failure wins).
///
/// The error record carries the SPEC-PLANE wire code (`NIKA-VAR-002` /
/// `NIKA-VAR-004` · `spec_code()`), the user-facing form a downstream
/// `on_error.on_codes:` filters on — same convention as the `for_each`
/// NIKA-VAR-006 site (NOT the engine-internal `nika_code()` 1703).
fn eval_all_bindings(
    task: &RawTask,
    output: &Value,
) -> Result<BTreeMap<String, Value>, TaskErrorRecord> {
    let mut named = BTreeMap::new();
    for (name, program) in &task.extract {
        let value =
            crate::jq::eval_binding(&name.value, &program.value, output).map_err(|err| {
                TaskErrorRecord {
                    code: err.spec_code(),
                    // wire_message (not to_string) — OutputBinding's Display is
                    // code-first (`{code} · {msg}`); the code rides its own
                    // field, so the record message stays code-less (no double
                    // render in tasks.X.error.message / the TaskFailed detail).
                    message: err.wire_message(),
                    transient: err.is_transient(),
                }
            })?;
        named.insert(name.value.clone(), value);
    }
    Ok(named)
}

/// Turn a settled SUCCESS into a FAILURE in place (an `output:` binding
/// errored · the success it would have reported is discarded). Only ever
/// called on a success-shaped settle (see [`bind_outputs`]).
fn replace_success_with_failure(settle: &mut SettleAs, error: TaskErrorRecord) {
    match settle {
        SettleAs::Ran(ran) if matches!(ran.result, RunResult::Success { .. }) => {
            // The dispatch DID run and may have billed — its spend stays
            // on the failed frame (the binding failure is downstream).
            let (cost_usd, cost_unpriced) = match &ran.result {
                RunResult::Success {
                    cost_usd,
                    cost_unpriced,
                    ..
                } => (*cost_usd, *cost_unpriced),
                _ => (None, None),
            };
            ran.result = RunResult::Failed {
                error,
                cost_usd,
                cost_unpriced,
            };
        }
        // A binding that fails over a REHYDRATED output fails the task
        // the same way (it never started — the pre-start failure shape).
        SettleAs::CacheHit { .. } => {
            *settle = SettleAs::FailedBeforeStart {
                stage: "output",
                error,
            };
        }
        _ => {}
    }
}

/// GATE-v2 (spec 03 §gate algebra) — `Some(finish)` when an incoming
/// edge's producer settled OUTSIDE that edge's pass-set (the task
/// settles `cancelled` · dead-path elimination · the cascade). The
/// gate is STRUCTURAL: pass-sets are context-free, no user expression
/// evaluates here — it cannot error. A gate-cancelled task never
/// produced an output: every declared binding reads defined-null.
fn gate_finish(
    task: &RawTask,
    id: String,
    records: &BTreeMap<String, TaskRecord>,
) -> Option<Finish> {
    use nika_check::analyzer::edges::SettledState;
    for (producer, kind) in nika_check::analyzer::edges::incoming_of(task)
        .into_iter()
        // The `unwind` edge is E_f: it does not gate. Cleanup is
        // DISPATCHED off its producer's settle, never admitted through
        // the precedence gate — reading it here cancelled every cleanup
        // task (the producer has no record while the wave runs).
        .filter(|(_, kind)| kind.is_scheduling())
    {
        // Missing record: the checker law makes it unreachable (every
        // target resolves · waves order producers first) — defensively
        // treated as not-admitting, loudly NOT silently-open.
        let settled = records.get(&producer).map(|r| match r.status {
            TaskStatus::Success => SettledState::Success,
            TaskStatus::Failure => SettledState::Failure,
            TaskStatus::Skipped => SettledState::Skipped,
            TaskStatus::Cancelled => SettledState::Cancelled,
        });
        if !settled.is_some_and(|s| kind.admits(s)) {
            return Some(Finish {
                id,
                settle: SettleAs::Cancelled {
                    note: "gate: an edge did not admit",
                    blocked_by: Some(producer),
                },
                named: null_bindings(task),
                resume: None,
                // Never ran — the output is `Null`, no content flowed.
                integrity: nika_cap::Integrity::trusted(),
                declassified: Vec::new(),
                approval: None,
            });
        }
    }
    None
}

/// The `when:` stage (spec 03 §when · POST-gate) — a LOCAL business
/// condition over `{vars · env · secrets · with}` (the boundary
/// bindings) — never the global tasks namespace (empty records =
/// defense-in-depth; the checker refused any `tasks.*` here). `false`
/// settles `skipped` (a decision, never a dead path); an evaluation
/// error settles failure OUTSIDE `on_error` scope.
fn when_finish(
    task: &RawTask,
    id: String,
    boundary_with: &BTreeMap<String, Value>,
    inputs: &BTreeMap<String, Value>,
    consts: &BTreeMap<String, Value>,
    secrets: &BTreeMap<String, Value>,
) -> Option<Finish> {
    let gate = task.when.as_ref()?;
    let empty_records = BTreeMap::new();
    let scope = Scope {
        records: &empty_records,
        inputs,
        consts,
        secrets,
        with_ns: Some(boundary_with),
        item: None,
        index: None,
        permits: None,
    };
    let settle = match eval_gate(&gate.value, &scope) {
        Ok(true) => return None,
        Ok(false) => SettleAs::SkippedGate {
            note: "when: closed (post-gate)",
            expr: match &gate.value {
                nika_schema::types::WhenGate::Expr(cel) => Some(cel.clone()),
                // `when: false` — the literal IS the story; the
                // note already says the condition closed.
                nika_schema::types::WhenGate::Literal(_) => None,
            },
        },
        Err(err) => SettleAs::FailedBeforeStart {
            stage: "when",
            error: runtime_error_record(&err),
        },
    };
    Some(Finish {
        id,
        settle,
        named: null_bindings(task),
        resume: None,
        // Never started (the gate closed · the boundary refused) — the
        // output is `Null`, no content flowed.
        integrity: nika_cap::Integrity::trusted(),
        declassified: Vec::new(),
        approval: None,
    })
}

/// Evaluate a `when:` gate value (shared by tasks + cleanup mini-tasks).
fn eval_gate(gate: &WhenGate, scope: &Scope<'_>) -> Result<bool, RuntimeError> {
    match gate {
        // CLOSED vocabulary (nika-vocab) — a future gate form is a spec
        // change that must land HERE explicitly, never silently closed.
        WhenGate::Literal(b) => Ok(*b),
        WhenGate::Expr(body) => expr::eval_when(body, scope),
    }
}

/// Map an attempt-loop outcome to the terminal [`RunResult`] PLUS the
/// F-P6 binding evidence: a success carries the value + token spend +
/// the optional success-riding diagnostic straight through · a failure
/// runs the `on_error:` policy (spec 05). The evidence is lifted BEFORE
/// the fold so it rides OUTSIDE it — a recovered divergence keeps its
/// finding (never a warn), and a post-gate verb failure keeps the passed
/// gate's attestation.
fn dispatch_result(
    task: &RawTask,
    scope: &Scope<'_>,
    outcome: Result<DispatchOk, FailedOutcome>,
) -> (RunResult, Option<crate::dispatch::commit::CommitEvidence>) {
    let evidence = match &outcome {
        Ok(ok) => ok
            .commit
            .clone()
            .map(crate::dispatch::commit::CommitEvidence::Fired),
        Err(failed) => failed.evidence.clone(),
    };
    let result = match outcome {
        Ok(DispatchOk {
            value,
            tokens,
            warning,
            child,
            cost_usd,
            cost_source,
            cost_unpriced,
            commit: _,
        }) => RunResult::Success {
            value,
            tokens,
            recovered_from: None,
            warning,
            child,
            cost_usd,
            cost_unpriced,
            // The by-source key IS the resolved model (`provider/name`)
            // — the same fact, now a structured frame field too.
            model: cost_source,
        },
        Err(failed) => apply_on_error(task, scope, failed),
    };
    (result, evidence)
}

/// `on_error:` (spec 05) — filter (`on_codes`) → ONE action.
fn apply_on_error(task: &RawTask, scope: &Scope<'_>, failed: FailedOutcome) -> RunResult {
    let FailedOutcome {
        record: error,
        cost_usd,
        cost_unpriced,
        evidence,
    } = failed;
    let Some(on_error) = task.on_error.as_ref() else {
        return RunResult::Failed {
            error,
            cost_usd,
            cost_unpriced,
        };
    };
    if !on_error_applies(&on_error.value, &error) {
        // Unlisted code falls through to the default fail (spec 05).
        return RunResult::Failed {
            error,
            cost_usd,
            cost_unpriced,
        };
    }
    match &on_error.value.action {
        OnErrorAction::Recover(value) => match expr::render_json(&value.value, scope) {
            Ok(recovered) => RunResult::recovered(recovered, error, cost_usd, cost_unpriced),
            // A render failure explained ONLY by not-yet-terminal task
            // referents AWAITS them (spec 05 §recover step 3 · a recover
            // ref is not an edge): the settle spine decides the outcome
            // once they settle. Any other unresolved root → the task
            // fails as if `on_error:` were absent (§recover step 4).
            Err(err) => {
                let render_error = runtime_error_record(&err);
                match crate::recover::classify_await(&value.value, scope) {
                    Some(awaiting) => {
                        RunResult::PendingRecovery(Box::new(crate::recover::PendingRecovery {
                            // F-P6 · the evidence parks WITH the failure —
                            // a recovered divergence keeps its finding.
                            failed: FailedOutcome::new(error, cost_usd, cost_unpriced, evidence),
                            render_error,
                            awaiting,
                            with_ns: scope.with_ns.cloned().unwrap_or_default(),
                        }))
                    }
                    None => RunResult::Failed {
                        error: render_error,
                        cost_usd,
                        cost_unpriced,
                    },
                }
            }
        },
        OnErrorAction::Skip => RunResult::SkippedWithError {
            error,
            cost_usd,
            cost_unpriced,
        },
        // #[non_exhaustive] · refuse loudly.
        other => RunResult::Failed {
            error: TaskErrorRecord {
                code: nika_error::codes::NIKA_1703.to_string(),
                message: format!("on_error action not wired in the runtime yet: {other:?}"),
                transient: false,
            },
            cost_usd,
            cost_unpriced,
        },
    }
}

/// A `RuntimeError` as a task error record. The wire code is the SPEC-PLANE
/// `spec_code()` (`NIKA-VAR-001` for an unresolved ref · `NIKA-VAR-005` for an
/// out-of-subset form · …) — the identifier `tasks.X.error.code` exposes and
/// `on_codes:` filters on — NEVER the engine-internal `nika_code()` (spec 05
/// §142 · internal codes MUST NOT leak into workflow-visible errors).
/// `pub(crate)`: the recover-await resolution maps its deferred render
/// failure through the SAME site.
pub(crate) fn runtime_error_record(err: &RuntimeError) -> TaskErrorRecord {
    TaskErrorRecord {
        code: err.spec_code(),
        message: err.wire_message(),
        transient: err.is_transient(),
    }
}

/// The verb's note prefix when the dispatch never produced one.
fn verb_note_prefix(action: &RawAction) -> &'static str {
    match action {
        RawAction::Invoke(_) => "invoke · ?",
        RawAction::Exec(_) => "exec · ?",
        RawAction::Infer(_) => "infer · ?",
        RawAction::Agent(_) => "agent · ?",
        _ => "verb · ?",
    }
}
