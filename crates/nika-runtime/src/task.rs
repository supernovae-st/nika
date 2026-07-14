// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The per-task pipeline (spec 03/05) — gate → `with:` → `for_each:` →
//! attempt loop (`retry:` × `timeout:`) → `on_error:` → `on_finally:`.
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
use nika_schema::raw::{ForEachValue, RawAction, RawFinallyTask, RawTask};
use nika_schema::types::{OnError, OnErrorAction, Permits, WhenGate};
use serde_json::Value;

use crate::Runtime;
use crate::dispatch::DispatchOk;
use crate::errors::RuntimeError;
use crate::expr::{self, Scope};
use crate::record::{TaskErrorRecord, TaskRecord, TaskStatus};
use crate::retry::{delay_ms, rand_unit};

mod fan_out;

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

/// Default per-cleanup-task timeout (spec 03 §`on_finally`).
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(30);

/// One task's complete, pen-free outcome — the settle pass turns this
/// into events + a record.
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
    /// The task ran (dispatched at least once).
    Ran(RanTask),
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
    /// Clock-measured wall time across attempts + cleanup (0 under a
    /// mock clock · real under the production clock — the event-stream
    /// determinism contract is "deterministic seams in · deterministic
    /// stream out", and the clock is a seam).
    pub duration_ms: u64,
    /// The terminal result after `retry:` + `on_error:`.
    pub result: RunResult,
}

/// One scheduled retry (the `TaskRetrying` event payload).
pub(crate) struct RetryStamp {
    pub attempt: u32,
    pub max_attempts: u32,
    pub delay_ms: u64,
}

/// A settled attempt-loop failure — the error + the spend the failed
/// attempts had already incurred (per-attempt debits happened live;
/// these fields feed the terminal frame).
pub(crate) struct FailedOutcome {
    pub record: TaskErrorRecord,
    pub cost_usd: Option<f64>,
    pub cost_unpriced: Option<nika_types::cost::UnpricedReason>,
}

impl FailedOutcome {
    fn new(
        record: TaskErrorRecord,
        cost_usd: Option<f64>,
        cost_unpriced: Option<nika_types::cost::UnpricedReason>,
    ) -> Self {
        Self {
            record,
            cost_usd,
            cost_unpriced,
        }
    }
}

/// The task's terminal result.
pub(crate) enum RunResult {
    /// Success — the verb's value (or the `on_error: recover` value) +
    /// token spend when the verb reports it + an optional non-fatal
    /// diagnostic (OBS-E · a reasoning model's blank-answer footgun ·
    /// rides `TaskCompleted` as a `warning` field).
    Success {
        value: Value,
        tokens: Option<i64>,
        /// `Some(code)` when `on_error.recover` repaired this success —
        /// the settle path emits `task_recovered` (one site · INV#24)
        /// before the terminal `task_completed`.
        recovered_from: Option<String>,
        warning: Option<String>,
        /// Real spend (catalog × usage split + tool-reported) · None =
        /// unpriced · honest. (The by-source attribution key lives on
        /// `DispatchOk` — the ledger debits at that leaf, before the
        /// result folds to this settle shape.)
        cost_usd: Option<f64>,
        /// Why (part of) the spend is NOT in `cost_usd` — rides the
        /// terminal frame as `cost_unpriced`.
        cost_unpriced: Option<nika_types::cost::UnpricedReason>,
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
    // spend ledger — 8 params, each one a distinct run-scoped seam.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_task_pipeline(
        &self,
        task: &RawTask,
        records: &BTreeMap<String, TaskRecord>,
        vars: &BTreeMap<String, Value>,
        env: &BTreeMap<String, Value>,
        secrets: &BTreeMap<String, Value>,
        permits: Option<&Permits>,
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
        let boundary_with = match render_boundary_with(task, records, vars, env, secrets) {
            Ok(ns) => ns,
            Err(err) => {
                return Finish {
                    id,
                    settle: SettleAs::FailedBeforeStart {
                        stage: "with",
                        error: runtime_error_record(&err),
                    },
                    named: null_bindings(task),
                    resume: None,
                };
            }
        };
        if let Some(finish) = when_finish(task, id.clone(), &boundary_with, vars, env, secrets) {
            return finish;
        }

        // ── ADR-099 resume identity — computed from the task AS
        //    AUTHORED (an `--answer` never re-keys: a prompt's answer is
        //    output non-determinism, like an infer's — §4 replays it) ──
        let resume = crate::resume::stamp(task, records, vars, env, resume_ctx);

        // ── ADR-099 `--resume` skip — BOTH hashes must match a journaled
        //    success (an edited task or a changed input re-runs · §1). A
        //    freshly-supplied `--answer` FORCES the ask (operator intent
        //    is explicit — never replay an old answer over a new one). ──
        if !self.prompt_answers.contains_key(&id)
            && let Some(finish) = self.cache_hit_finish(task, &id, resume.as_ref())
        {
            return finish;
        }

        // ── `--answer task=value` (ADR-099 rider) — bind the supplied
        //    answer as the prompt's `default:` (the answered branch of
        //    the stdlib contract · dispatch-only, never the identity) ──
        let answered;
        let task = match self.task_with_prompt_answer(task) {
            Some(bound) => {
                answered = bound;
                &answered
            }
            None => task,
        };

        // ── `for_each:` fan-out or the single lane ──────────────────
        let mut settle = self
            .run_lanes(
                task,
                boundary_with,
                records,
                vars,
                env,
                secrets,
                permits,
                ledger,
            )
            .await;
        // `output:` named bindings (spec 04 §Output binding) — evaluated
        // over the task's FINAL raw output, BEFORE settle emits the
        // terminal frame, so a binding error (NIKA-VAR-002/004) turns a
        // success into a failure (the cascade) rather than landing after
        // a `TaskCompleted`. The map carries one entry per declared
        // binding (the value on success · `Null` on a non-success ·
        // defined-null reads).
        let named = bind_outputs(task, &mut settle);
        let resume = filter_leaky_resume(resume, &settle, resume_ctx);
        Finish {
            id,
            settle,
            named,
            resume,
        }
    }

    /// The execution lane split: `for_each:` fan-out when declared ·
    /// the single lane otherwise (spec 03 §dispatch pipeline).
    // REASON: the same run-scoped seams as the pipeline — 8 params.
    #[allow(clippy::too_many_arguments)]
    async fn run_lanes(
        &self,
        task: &RawTask,
        boundary_with: BTreeMap<String, Value>,
        records: &BTreeMap<String, TaskRecord>,
        vars: &BTreeMap<String, Value>,
        env: &BTreeMap<String, Value>,
        secrets: &BTreeMap<String, Value>,
        permits: Option<&Permits>,
        ledger: &crate::ledger::RunLedger,
    ) -> SettleAs {
        match task.for_each.as_ref() {
            None => {
                self.run_single(
                    task,
                    boundary_with,
                    records,
                    vars,
                    env,
                    secrets,
                    permits,
                    ledger,
                )
                .await
            }
            Some(spanned) => {
                self.run_fan_out(
                    task,
                    &spanned.value,
                    &boundary_with,
                    records,
                    vars,
                    env,
                    secrets,
                    permits,
                    ledger,
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
        })
    }

    /// Bind a supplied `--answer` to this task's `nika:prompt` as its
    /// `default:` (the answered branch of the stdlib contract — the
    /// builtin validates the TYPE per mode, so a bad answer fails with
    /// the same honest PROMPT-001/002 diagnostics). `None` = no answer
    /// for this task, or not a direct prompt invocation — unchanged.
    fn task_with_prompt_answer(&self, task: &RawTask) -> Option<RawTask> {
        let answer = self.prompt_answers.get(&task.id.value)?;
        let RawAction::Invoke(invoke) = &task.action else {
            return None;
        };
        if invoke.tool.value != "nika:prompt" {
            return None;
        }
        let mut bound = task.clone();
        let RawAction::Invoke(invoke) = &mut bound.action else {
            return None; // unreachable — same variant as above
        };
        if let Some(args) = invoke.args.as_mut() {
            // Non-object args fail the builtin's own validation — never
            // silently rewritten here.
            if let Value::Object(map) = &mut args.value {
                map.insert("default".to_owned(), answer.clone());
            }
        } else {
            // No args at all (message missing → the builtin refuses
            // loudly anyway) — still bind, one behavior.
            let span = task.id.span;
            invoke.args = Some(nika_schema::Spanned::new(
                serde_json::json!({ "default": answer }),
                span,
            ));
        }
        Some(bound)
    }

    /// The single-execution lane (no `for_each:`).
    // REASON: the run-scoped seams plus the boundary render — 9 params.
    #[allow(clippy::too_many_arguments)]
    async fn run_single(
        &self,
        task: &RawTask,
        with_ns: BTreeMap<String, Value>,
        records: &BTreeMap<String, TaskRecord>,
        vars: &BTreeMap<String, Value>,
        env: &BTreeMap<String, Value>,
        secrets: &BTreeMap<String, Value>,
        permits: Option<&Permits>,
        ledger: &crate::ledger::RunLedger,
    ) -> SettleAs {
        // `with:` materialized at the boundary (spec 03 §dispatch
        // pipeline) — the single lane consumes it as rendered.
        let scope = Scope {
            records,
            vars,
            env,
            secrets,
            with_ns: Some(&with_ns),
            item: None,
            index: None,
            permits,
        };
        let started = self.clock.now();
        let mut ran = self.attempt_loop(task, &scope, ledger).await;
        // `on_finally:` — the task STARTED (spec 03 · success AND
        // failure · before the failure propagates in the DAG).
        self.run_finally(task, &scope, &ran).await;
        ran.duration_ms = self.since_ms(started);
        SettleAs::Ran(ran)
    }

    /// The `for_each:` fan-out lane (spec 03 · closed at v1).
    // REASON: same run-scoped seams as the pipeline — 8 distinct params.
    #[allow(clippy::too_many_arguments)]
    async fn run_fan_out(
        &self,
        task: &RawTask,
        collection: &ForEachValue,
        boundary_with: &BTreeMap<String, Value>,
        records: &BTreeMap<String, TaskRecord>,
        vars: &BTreeMap<String, Value>,
        env: &BTreeMap<String, Value>,
        secrets: &BTreeMap<String, Value>,
        permits: Option<&Permits>,
        ledger: &crate::ledger::RunLedger,
    ) -> SettleAs {
        // The collection resolves on the PRE-fan-out surface (the
        // item-free boundary bindings) · empty settles `skipped`.
        let items =
            match fan_out::resolve_fan_out_items(collection, boundary_with, vars, env, secrets) {
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
                    self.run_iteration(task, records, vars, env, secrets, locals, permits, ledger)
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
            acc.first_error,
            (acc.cost_sum, acc.unpriced),
        );
        let retries = acc.retries;
        let agent_events = acc.agent_events;
        let mut ran = RanTask {
            note: format!("for_each · {total} items"),
            retries,
            agent_events,
            duration_ms: 0,
            result,
        };
        // `on_finally:` runs ONCE after all iterations (spec 03 ·
        // `item`/`index` are NOT in scope there). `permits` MUST flow so a
        // fan-out `on_finally` exec is enforced like every other (NIKA-SEC-004)
        // — `Scope::workflow` would drop it to None (the cleanup-bypass gap).
        let finally_scope = Scope {
            records,
            vars,
            env,
            secrets,
            with_ns: None,
            item: None,
            index: None,
            permits,
        };
        self.run_finally(task, &finally_scope, &ran).await;
        ran.duration_ms = self.since_ms(started);
        SettleAs::Ran(ran)
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
        vars: &BTreeMap<String, Value>,
        env: &BTreeMap<String, Value>,
        secrets: &BTreeMap<String, Value>,
        locals: IterationLocals<'_>,
        permits: Option<&Permits>,
        ledger: &crate::ledger::RunLedger,
    ) -> RanTask {
        let with_ns = match render_with(
            task,
            records,
            vars,
            env,
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
            vars,
            env,
            secrets,
            with_ns: Some(&with_ns),
            item: Some(locals.item),
            index: Some(locals.index),
            permits,
        };
        let mut ran = self.attempt_loop(task, &scope, ledger).await;
        // Stamp the lane: without it a 2-iteration fan-out and a retried
        // single lane produce indistinguishable flat streams (review F3).
        #[allow(clippy::cast_possible_truncation)] // fan-out ≪ u32::MAX
        for stamped in &mut ran.agent_events {
            stamped.iteration = Some(locals.index as u32);
        }
        ran
    }

    /// The attempt loop — `retry:` (transient-only · `on_codes` filter ·
    /// full-jitter backoff via the clock seam) racing the task's ONE
    /// `timeout:` budget (spec 03 · "including any retries and their
    /// backoff sleeps") · then `on_error:` (spec 05).
    async fn attempt_loop(
        &self,
        task: &RawTask,
        scope: &Scope<'_>,
        ledger: &crate::ledger::RunLedger,
    ) -> RanTask {
        let started = self.clock.now();
        let max_attempts = task
            .retry
            .as_ref()
            .map_or(1, |r| r.value.max_attempts.max(1));
        let jitter_key = jitter_key(task, scope);
        let mut note = String::new();
        let mut retries: Vec<RetryStamp> = Vec::new();
        // OUTSIDE the timeout-cancellable region: a timed-out attempt's
        // decisions survive the drop of the attempt future (review F1).
        let agent_buffer = crate::agent_events::BufferingObserver::new();
        let mut attempt_marks: Vec<usize> = Vec::new();
        let outcome = {
            // The attempt loop borrows the accumulators; the borrow ends
            // when the loop future is consumed/dropped (both arms below).
            // `budget` = the task's ONE `timeout:` — the total enforced
            // below AND handed to dispatch (the infer transport · F1).
            let budget = task.timeout.as_ref().map(|t| t.value);
            let attempts = async {
                let mut attempt = 1_u32;
                // Spend of FAILED attempts (ledger-debited per attempt) —
                // folded onto the terminal frame at the end.
                let mut failed_cost: Option<f64> = None;
                let mut failed_unpriced: Option<nika_types::cost::UnpricedReason> = None;
                loop {
                    let dispatched = self
                        .dispatch(&task.action, scope, &agent_buffer, budget)
                        .await;
                    note.clone_from(&dispatched.note);
                    attempt_marks.push(agent_buffer.len());
                    match dispatched.result {
                        Ok(mut ok) => {
                            // THE leaf debit site (its OWN spend — failed
                            // attempts debited theirs already; the frame
                            // then reports the whole task's cost).
                            ledger.debit_ok(&ok);
                            ok.fold_failed_spend(failed_cost, failed_unpriced);
                            return Ok(ok);
                        }
                        Err(failed) => {
                            // Debits PER ATTEMPT — a retry storm is never
                            // invisible to `--max-cost-usd`.
                            let error = failed.debit_and_fold(
                                ledger,
                                &mut failed_cost,
                                &mut failed_unpriced,
                            );
                            // Retry iff attempts remain AND the policy
                            // admits the error (spec 05).
                            let Some(delay) =
                                self.retry_delay(task, &error, attempt, max_attempts, &jitter_key)
                            else {
                                return Err(FailedOutcome::new(
                                    error,
                                    failed_cost,
                                    failed_unpriced,
                                ));
                            };
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
            // Timed out before the first dispatch note landed.
            verb_note_prefix(&task.action).clone_into(&mut note);
        }

        let result = dispatch_result(task, scope, outcome);
        RanTask {
            note,
            retries,
            agent_events: crate::agent_events::stamp_attempts(
                agent_buffer.into_events(),
                &attempt_marks,
            ),
            duration_ms,
            result,
        }
    }

    /// The retry decision — `Some(delay_ms)` when attempts remain AND
    /// the policy admits the error (spec 05 · the config is present by
    /// construction past the gate) · `None` = the failure is final.
    fn retry_delay(
        &self,
        task: &RawTask,
        error: &TaskErrorRecord,
        attempt: u32,
        max_attempts: u32,
        jitter_key: &str,
    ) -> Option<u64> {
        let eligible = attempt < max_attempts && retry_eligible(task, error);
        let cfg = task.retry.as_ref().filter(|_| eligible).map(|r| &r.value)?;
        Some(delay_ms(
            cfg,
            attempt,
            rand_unit(self.config.jitter_seed, jitter_key, attempt),
        ))
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
                        })
                    }
                }
            }
        }
    }

    /// Run the cleanup mini-tasks (spec 03 §`on_finally` · sequential ·
    /// best-effort · errors swallowed · per-cleanup timeout 30s).
    async fn run_finally(&self, task: &RawTask, scope: &Scope<'_>, ran: &RanTask) {
        if task.on_finally.is_empty() {
            return;
        }
        // The cleanup scope sees the PARENT's fresh status/error via a
        // one-record overlay (spec 03 · status/error routing).
        // PERF (documented trade): one records-map clone per
        // task-WITH-cleanup (early-return above keeps the common lane
        // free) · workflow size is certificate-bounded (degree-1) — a
        // copy-on-read overlay Scope would save it at the cost of a
        // two-level resolve on EVERY lookup.
        let mut records = scope.records.clone();
        records.insert(task.id.value.clone(), preview_record(ran));
        let cleanup_scope = Scope {
            records: &records,
            vars: scope.vars,
            env: scope.env,
            secrets: scope.secrets, // a cleanup may reference secrets.X too
            with_ns: scope.with_ns,
            item: None, // locals out of scope after the fan-out (spec 03)
            index: None,
            permits: scope.permits, // on_finally exec stays within the boundary
        };
        for mini in &task.on_finally {
            self.run_one_cleanup(&mini.value, &cleanup_scope).await;
        }
    }

    /// One cleanup mini-task · own `when:` + `timeout:` · outcome
    /// swallowed (best-effort semantics · spec 03).
    async fn run_one_cleanup(&self, mini: &RawFinallyTask, scope: &Scope<'_>) {
        if let Some(gate) = mini.when.as_ref() {
            match eval_gate(&gate.value, scope) {
                Ok(true) => {}
                // Closed gate OR eval error → the cleanup is skipped
                // (a cleanup error never propagates).
                _ => return,
            }
        }
        let limit = mini.timeout.as_ref().map_or(CLEANUP_TIMEOUT, |t| t.value);
        // Cleanup agent decisions are NOT collected (best-effort lane ·
        // outcome dropped by design) — a throwaway buffer satisfies the
        // dispatch seam; collecting it is a trigger-gated ratchet.
        let cleanup_buffer = crate::agent_events::BufferingObserver::new();
        let attempt =
            std::pin::pin!(self.dispatch(&mini.action, scope, &cleanup_buffer, Some(limit)));
        let timer = std::pin::pin!(self.clock.sleep(limit));
        // Either way the outcome is dropped — cleanup observability is
        // the cleanup's own effects (e.g. `nika:emit` · spec 03).
        let _ = futures_util::future::select(attempt, timer).await;
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

/// The jitter stream selector — fan-out iterations get DISTINCT
/// streams (anti-thundering-herd applies WITHIN a fan-out too ·
/// Brooker 2015: same-task iterations retrying the same upstream
/// must not synchronize) while staying replay-stable (the index
/// is part of the deterministic coordinates). Collision-free: task
/// ids are `snake_case` (checker law · CEL-safe) so `[` can never
/// appear in a real id — iteration `t[0]` can't collide with a task
/// NAMED `t[0]`.
fn jitter_key(task: &RawTask, scope: &Scope<'_>) -> String {
    match scope.index {
        Some(i) => format!("{}[{i}]", task.id.value),
        None => task.id.value.clone(),
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
    if task.output.is_empty() {
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
    task.output
        .iter()
        .map(|(name, _)| (name.value.clone(), Value::Null))
        .collect()
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
    for (name, program) in &task.output {
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
    use nika_schema::analyzer::edges::SettledState;
    for (producer, kind) in nika_schema::analyzer::edges::incoming_of(task) {
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
    vars: &BTreeMap<String, Value>,
    env: &BTreeMap<String, Value>,
    secrets: &BTreeMap<String, Value>,
) -> Option<Finish> {
    let gate = task.when.as_ref()?;
    let empty_records = BTreeMap::new();
    let scope = Scope {
        records: &empty_records,
        vars,
        env,
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
                _ => None,
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
    })
}

/// The boundary `with:` render (spec 03 §dispatch pipeline) — ALL
/// bindings for a single-lane task · only the loop-local-free ones for
/// a fan-out task (the item/index-bound ones re-render per iteration).
fn render_boundary_with(
    task: &RawTask,
    records: &BTreeMap<String, TaskRecord>,
    vars: &BTreeMap<String, Value>,
    env: &BTreeMap<String, Value>,
    secrets: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, RuntimeError> {
    let scope = Scope {
        records,
        vars,
        env,
        secrets, // `with: { tok: "${{ secrets.X }}" }` resolves here (MINOR-B)
        with_ns: None,
        item: None,
        index: None,
        permits: None, // `with:` rendering performs no effect (no exec sink)
    };
    let fan_out = task.for_each.is_some();
    task.with
        .iter()
        .filter(|(_key, value)| !(fan_out && references_loop_locals(&value.value)))
        .map(|(key, value)| Ok((key.value.clone(), expr::render_json(&value.value, &scope)?)))
        .collect()
}

/// Whether a JSON value's `${{ }}` islands reference the `for_each`
/// loop-locals (`item` / `index`) — those bindings are per-iteration.
fn references_loop_locals(value: &Value) -> bool {
    use nika_schema::expression::{NamespaceRef, expr_refs, scan_templates};
    match value {
        Value::String(s) => {
            let Ok(islands) = scan_templates(s) else {
                return false;
            };
            islands.iter().any(|island| {
                expr_refs(&island.expr)
                    .into_iter()
                    .any(|r| matches!(r, NamespaceRef::Item | NamespaceRef::Index))
            })
        }
        Value::Array(items) => items.iter().any(references_loop_locals),
        Value::Object(map) => map.values().any(references_loop_locals),
        _ => false,
    }
}

/// The parent's preview record for the cleanup scope (spec 03 · the
/// cleanup sees `tasks.<parent>.status` / `.error`). A PENDING recovery
/// previews as its pre-recovery failure: the attempts DID fail, and the
/// deferred render has produced no value when the cleanup runs (the
/// cleanup is task-scoped · it never awaits the spine).
fn preview_record(ran: &RanTask) -> TaskRecord {
    let mut record = TaskRecord::unran(match ran.result {
        RunResult::Success { .. } => TaskStatus::Success,
        RunResult::SkippedWithError { .. } => TaskStatus::Skipped,
        RunResult::Failed { .. } | RunResult::PendingRecovery(_) => TaskStatus::Failure,
    });
    match &ran.result {
        RunResult::Success { value, .. } => record.output = value.clone(),
        RunResult::SkippedWithError { error, .. } | RunResult::Failed { error, .. } => {
            record.error = Some(error.clone());
        }
        RunResult::PendingRecovery(pending) => {
            record.error = Some(pending.failed.record.clone());
        }
    }
    record.duration_ms = Some(ran.duration_ms);
    record
}

/// Evaluate a `when:` gate value (shared by tasks + cleanup mini-tasks).
fn eval_gate(gate: &WhenGate, scope: &Scope<'_>) -> Result<bool, RuntimeError> {
    match gate {
        WhenGate::Literal(b) => Ok(*b),
        WhenGate::Expr(body) => expr::eval_when(body, scope),
        // #[non_exhaustive] · a future gate form is out of the v0
        // subset by definition · loud, never silently closed.
        other => Err(RuntimeError::WhenUnsupported {
            expr: format!("{other:?}"),
        }),
    }
}

/// Map an attempt-loop outcome to the terminal [`RunResult`]: a success
/// carries the value + token spend + the optional OBS-E diagnostic
/// straight through · a failure runs the `on_error:` policy (spec 05).
fn dispatch_result(
    task: &RawTask,
    scope: &Scope<'_>,
    outcome: Result<DispatchOk, FailedOutcome>,
) -> RunResult {
    match outcome {
        Ok(DispatchOk {
            value,
            tokens,
            warning,
            cost_usd,
            cost_source: _,
            cost_unpriced,
        }) => RunResult::Success {
            value,
            tokens,
            recovered_from: None,
            warning,
            cost_usd,
            cost_unpriced,
        },
        Err(failed) => apply_on_error(task, scope, failed),
    }
}

/// `on_error:` (spec 05) — filter (`on_codes`) → ONE action.
fn apply_on_error(task: &RawTask, scope: &Scope<'_>, failed: FailedOutcome) -> RunResult {
    let FailedOutcome {
        record: error,
        cost_usd,
        cost_unpriced,
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
            Ok(recovered) => RunResult::Success {
                value: recovered,
                tokens: None,
                // the ONE producer of the marker (spec 05 §recover)
                recovered_from: Some(error.code),
                // A recovered value is author-supplied · no model reasoning
                // — but the FAILED attempts' spend already happened and
                // stays on the frame (recovery does not refund it).
                warning: None,
                cost_usd,
                cost_unpriced,
            },
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
                            failed: FailedOutcome::new(error, cost_usd, cost_unpriced),
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
        OnErrorAction::FailWorkflow => RunResult::Failed {
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

/// `retry:` eligibility (spec 05 · transient-only unless `on_codes`).
fn retry_eligible(task: &RawTask, error: &TaskErrorRecord) -> bool {
    let Some(retry) = task.retry.as_ref() else {
        return false;
    };
    if retry.value.on_codes.is_empty() {
        error.transient
    } else {
        retry.value.on_codes.iter().any(|c| c == &error.code)
    }
}

/// `on_error.on_codes` filter (spec 05 · empty = applies to all).
/// `pub(crate)`: the ADR-099 pause rider consults it too (an authored
/// route for PROMPT-001 wins over the pause).
pub(crate) fn on_error_applies(on_error: &OnError, error: &TaskErrorRecord) -> bool {
    on_error.on_codes.is_empty() || on_error.on_codes.iter().any(|c| c.value == error.code)
}

/// Render the task's `with:` map (spec 03 · per-iteration in fan-out ·
/// entries cannot reference each other · spec 04).
fn render_with(
    task: &RawTask,
    records: &BTreeMap<String, TaskRecord>,
    vars: &BTreeMap<String, Value>,
    env: &BTreeMap<String, Value>,
    secrets: &BTreeMap<String, Value>,
    item: Option<&Value>,
    index: Option<usize>,
) -> Result<BTreeMap<String, Value>, RuntimeError> {
    let scope = Scope {
        records,
        vars,
        env,
        secrets, // `with: { tok: "${{ secrets.X }}" }` resolves here (MINOR-B)
        with_ns: None,
        item,
        index,
        permits: None, // `with:` rendering performs no effect (no exec sink)
    };
    task.with
        .iter()
        .map(|(key, value)| Ok((key.value.clone(), expr::render_json(&value.value, &scope)?)))
        .collect()
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
