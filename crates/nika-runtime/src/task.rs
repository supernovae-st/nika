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
use nika_kernel::ai::provider::ProviderInferDyn;
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
}

/// How the task settles (spec 03 §task states).
pub(crate) enum SettleAs {
    /// The default gate became unsatisfiable (upstream failure /
    /// cancellation) — a decision, not a defect.
    Cancelled { note: &'static str },
    /// The `when:` gate evaluated false · or an empty `for_each`
    /// collection (pure skip · no cascade).
    SkippedGate { note: &'static str },
    /// A pre-dispatch failure (gate eval · `with:` render · `for_each`
    /// collection) — never started · no `on_finally:` (spec 03).
    FailedBeforeStart {
        stage: &'static str,
        error: TaskErrorRecord,
    },
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
    /// attempt-stamped (+ iteration on fan-out lanes) — ADR-093 · empty
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

/// The task's terminal result.
pub(crate) enum RunResult {
    /// Success — the verb's value (or the `on_error: recover` value) +
    /// token spend when the verb reports it.
    Success { value: Value, tokens: Option<i64> },
    /// `on_error: skip` — skipped with the original error readable
    /// (spec 05 · the one coexist state).
    SkippedWithError { error: TaskErrorRecord },
    /// Failed after retries with no recovery.
    Failed { error: TaskErrorRecord },
}

impl<S, T, H, P, D, C> Runtime<S, T, H, P, D, C>
where
    S: ShellRunDyn + Sync,
    T: ToolExecuteDyn,
    H: HttpPostDyn + Send + Sync + 'static,
    P: ProviderInferDyn,
    D: ToolDefinitionProviderDyn,
    C: ClockDyn + Sync,
{
    /// The full per-task pipeline (pen-free · see module docs).
    ///
    /// `records` is the wave-frozen view — same-wave tasks never
    /// reference each other (checker law), so freezing at wave entry
    /// is sound and keeps the read side shareable across the wave's
    /// concurrent pipelines.
    pub(crate) async fn run_task_pipeline(
        &self,
        task: &RawTask,
        records: &BTreeMap<String, TaskRecord>,
        vars: &BTreeMap<String, Value>,
        permits: Option<&Permits>,
    ) -> Finish {
        let id = task.id.value.clone();
        let gate_scope = Scope::workflow(records, vars);

        // ── The gate (spec 03 §task states) ─────────────────────────
        match task.when.as_ref() {
            // Default gate · run iff ALL deps ∈ {success, skipped} ·
            // else `cancelled` (Dead-Path-Elimination · the cascade).
            None => {
                if !default_gate_open(task, records) {
                    return Finish {
                        id,
                        settle: SettleAs::Cancelled {
                            note: "upstream failed",
                        },
                    };
                }
            }
            // Explicit `when:` REPLACES the default gate — evaluated
            // once deps are terminal whatever their status (the
            // always-pattern · spec 05 §workflow-level).
            Some(gate) => match eval_gate(&gate.value, &gate_scope) {
                Ok(true) => {}
                Ok(false) => {
                    return Finish {
                        id,
                        settle: SettleAs::SkippedGate {
                            note: "when: gate closed",
                        },
                    };
                }
                Err(err) => {
                    return Finish {
                        id,
                        settle: SettleAs::FailedBeforeStart {
                            stage: "when",
                            error: runtime_error_record(&err),
                        },
                    };
                }
            },
        }

        // ── `for_each:` fan-out or the single lane ──────────────────
        let settle = match task.for_each.as_ref() {
            None => self.run_single(task, records, vars, permits).await,
            Some(spanned) => {
                self.run_fan_out(task, &spanned.value, records, vars, permits)
                    .await
            }
        };
        Finish { id, settle }
    }

    /// The single-execution lane (no `for_each:`).
    async fn run_single(
        &self,
        task: &RawTask,
        records: &BTreeMap<String, TaskRecord>,
        vars: &BTreeMap<String, Value>,
        permits: Option<&Permits>,
    ) -> SettleAs {
        // `with:` renders ONCE here (per-iteration in the fan-out lane).
        let with_ns = match render_with(task, records, vars, None, None) {
            Ok(ns) => ns,
            Err(err) => {
                return SettleAs::FailedBeforeStart {
                    stage: "with",
                    error: runtime_error_record(&err),
                };
            }
        };
        let scope = Scope {
            records,
            vars,
            with_ns: Some(&with_ns),
            item: None,
            index: None,
            permits,
        };
        let started = self.clock.now();
        let mut ran = self.attempt_loop(task, &scope).await;
        // `on_finally:` — the task STARTED (spec 03 · success AND
        // failure · before the failure propagates in the DAG).
        self.run_finally(task, &scope, &ran).await;
        ran.duration_ms = self.since_ms(started);
        SettleAs::Ran(ran)
    }

    /// The `for_each:` fan-out lane (spec 03 · closed at v1).
    async fn run_fan_out(
        &self,
        task: &RawTask,
        collection: &ForEachValue,
        records: &BTreeMap<String, TaskRecord>,
        vars: &BTreeMap<String, Value>,
        permits: Option<&Permits>,
    ) -> SettleAs {
        let scope = Scope::workflow(records, vars);
        let items = match resolve_collection(collection, &scope) {
            Ok(items) => items,
            Err(settle) => return *settle,
        };

        // Empty collection → the task is `skipped` (spec 03).
        if items.is_empty() {
            return SettleAs::SkippedGate {
                note: "for_each · empty collection",
            };
        }

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
        let mut stream =
            futures_util::stream::iter(items.iter().enumerate().map(|(index, item)| {
                self.run_iteration(task, records, vars, item, index, permits)
            }))
            .buffered(cap);

        let mut outputs: Vec<Value> = Vec::with_capacity(total);
        let mut retries: Vec<RetryStamp> = Vec::new();
        let mut agent_events: Vec<crate::agent_events::StampedAgentEvent> = Vec::new();
        let mut first_error: Option<TaskErrorRecord> = None;
        // Truth accounting · per-iteration token spend SUMS onto the
        // parent (a 50-infer fan-out must never report zero to the
        // cost meter) · None until any iteration reports.
        let mut tokens_sum: Option<i64> = None;

        while let Some(iter_ran) = stream.next().await {
            retries.extend(iter_ran.retries);
            agent_events.extend(iter_ran.agent_events);
            match iter_ran.result {
                RunResult::Success { value, tokens } => {
                    outputs.push(value);
                    if let Some(n) = tokens {
                        tokens_sum = Some(tokens_sum.unwrap_or(0).saturating_add(n));
                    }
                }
                // Per-iteration `on_error: skip` contributes null at its
                // index — positional alignment survives (spec 03).
                RunResult::SkippedWithError { .. } => outputs.push(Value::Null),
                RunResult::Failed { error } => {
                    outputs.push(Value::Null);
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                    if fail_fast {
                        // Drop the stream: in-flight iterations cancel
                        // at their await points · unspawned never start
                        // (spec 03 · `fail_fast: true` default).
                        break;
                    }
                }
            }
        }
        drop(stream);

        let result = match first_error {
            None => RunResult::Success {
                value: Value::Array(outputs),
                tokens: tokens_sum,
            },
            Some(error) => RunResult::Failed { error },
        };
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
    async fn run_iteration(
        &self,
        task: &RawTask,
        records: &BTreeMap<String, TaskRecord>,
        vars: &BTreeMap<String, Value>,
        item: &Value,
        index: usize,
        permits: Option<&Permits>,
    ) -> RanTask {
        let with_ns = match render_with(task, records, vars, Some(item), Some(index)) {
            Ok(ns) => ns,
            Err(err) => {
                return RanTask {
                    note: format!("for_each[{index}]"),
                    retries: Vec::new(),
                    agent_events: Vec::new(),
                    duration_ms: 0,
                    result: RunResult::Failed {
                        error: runtime_error_record(&err),
                    },
                };
            }
        };
        let scope = Scope {
            records,
            vars,
            with_ns: Some(&with_ns),
            item: Some(item),
            index: Some(index),
            permits,
        };
        let mut ran = self.attempt_loop(task, &scope).await;
        // Stamp the lane: without it a 2-iteration fan-out and a retried
        // single lane produce indistinguishable flat streams (review F3).
        #[allow(clippy::cast_possible_truncation)] // fan-out ≪ u32::MAX
        for stamped in &mut ran.agent_events {
            stamped.iteration = Some(index as u32);
        }
        ran
    }

    /// The attempt loop — `retry:` (transient-only · `on_codes` filter ·
    /// full-jitter backoff via the clock seam) racing the task's ONE
    /// `timeout:` budget (spec 03 · "including any retries and their
    /// backoff sleeps") · then `on_error:` (spec 05).
    async fn attempt_loop(&self, task: &RawTask, scope: &Scope<'_>) -> RanTask {
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
            let attempts = async {
                let mut attempt = 1_u32;
                loop {
                    let dispatched = self.dispatch(&task.action, scope, &agent_buffer).await;
                    note.clone_from(&dispatched.note);
                    attempt_marks.push(agent_buffer.len());
                    match dispatched.result {
                        Ok(ok) => return Ok(ok),
                        Err(error) => {
                            // Retry iff attempts remain AND the policy
                            // admits the error (spec 05) — the config is
                            // present by construction past this gate.
                            let eligible = attempt < max_attempts && retry_eligible(task, &error);
                            let Some(cfg) =
                                task.retry.as_ref().filter(|_| eligible).map(|r| &r.value)
                            else {
                                return Err(error);
                            };
                            let delay = delay_ms(
                                cfg,
                                attempt,
                                rand_unit(self.config.jitter_seed, &jitter_key, attempt),
                            );
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

            match task.timeout.as_ref().map(|t| t.value) {
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
                            Err(TaskErrorRecord {
                                code: TIMEOUT_CODE.to_owned(),
                                message: format!(
                                    "task exceeded its timeout of {} ms",
                                    limit.as_millis()
                                ),
                                transient: false, // never retryable (spec 03)
                            })
                        }
                    }
                }
            }
        };

        let duration_ms = self.since_ms(started);
        if note.is_empty() {
            // Timed out before the first dispatch note landed.
            verb_note_prefix(&task.action).clone_into(&mut note);
        }

        let result = match outcome {
            Ok(DispatchOk { value, tokens }) => RunResult::Success { value, tokens },
            Err(error) => apply_on_error(task, scope, error),
        };
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
        let attempt = std::pin::pin!(self.dispatch(&mini.action, scope, &cleanup_buffer));
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

/// Resolve the `for_each:` collection (the ONLY once-evaluated body
/// expression · spec 03) — an array of items, or the settle verdict
/// for the failure lanes (boxed: the error lane stays pointer-thin).
fn resolve_collection(
    collection: &ForEachValue,
    scope: &Scope<'_>,
) -> Result<Vec<Value>, Box<SettleAs>> {
    let resolved = match collection {
        ForEachValue::List(value) => expr::render_json(value, scope),
        ForEachValue::Expression(text) => expr::render_json(&Value::String(text.clone()), scope),
        // #[non_exhaustive] · a future collection form fails loudly.
        other => Err(RuntimeError::WhenUnsupported {
            expr: format!("for_each form not wired in the runtime yet: {other:?}"),
        }),
    };
    match resolved {
        Ok(Value::Array(items)) => Ok(items),
        // Non-array collection = evaluation error (spec 03 · the
        // NIKA-VAR-006 class).
        Ok(other) => Err(Box::new(SettleAs::FailedBeforeStart {
            stage: "for_each",
            error: TaskErrorRecord {
                code: VAR_TYPE_CODE.to_owned(),
                message: format!(
                    "for_each collection must be an array · got {}",
                    json_kind(&other)
                ),
                transient: false,
            },
        })),
        Err(err) => Err(Box::new(SettleAs::FailedBeforeStart {
            stage: "for_each",
            error: runtime_error_record(&err),
        })),
    }
}

/// The default gate's verdict (spec 03 · `depends_on` IS the
/// success-gate): open iff ALL deps ∈ {success, skipped}.
fn default_gate_open(task: &RawTask, records: &BTreeMap<String, TaskRecord>) -> bool {
    task.depends_on.iter().all(|dep| {
        matches!(
            records.get(&dep.value).map(|r| r.status),
            Some(TaskStatus::Success | TaskStatus::Skipped)
        )
        // Failure · Cancelled · or missing (checker law makes missing
        // unreachable) → unsatisfiable, loudly NOT silently-open.
    })
}

/// The parent's preview record for the cleanup scope (spec 03 · the
/// cleanup sees `tasks.<parent>.status` / `.error`).
fn preview_record(ran: &RanTask) -> TaskRecord {
    let mut record = TaskRecord::unran(match ran.result {
        RunResult::Success { .. } => TaskStatus::Success,
        RunResult::SkippedWithError { .. } => TaskStatus::Skipped,
        RunResult::Failed { .. } => TaskStatus::Failure,
    });
    match &ran.result {
        RunResult::Success { value, .. } => record.output = value.clone(),
        RunResult::SkippedWithError { error } | RunResult::Failed { error } => {
            record.error = Some(error.clone());
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

/// `on_error:` (spec 05) — filter (`on_codes`) → ONE action.
fn apply_on_error(task: &RawTask, scope: &Scope<'_>, error: TaskErrorRecord) -> RunResult {
    let Some(on_error) = task.on_error.as_ref() else {
        return RunResult::Failed { error };
    };
    if !on_error_applies(&on_error.value, &error) {
        // Unlisted code falls through to the default fail (spec 05).
        return RunResult::Failed { error };
    }
    match &on_error.value.action {
        OnErrorAction::Recover(value) => match expr::render_json(&value.value, scope) {
            Ok(recovered) => RunResult::Success {
                value: recovered,
                tokens: None,
            },
            // Recovery itself failed → the task fails as if
            // `on_error:` were absent (spec 05 §recover resolution).
            Err(err) => RunResult::Failed {
                error: runtime_error_record(&err),
            },
        },
        OnErrorAction::Skip => RunResult::SkippedWithError { error },
        OnErrorAction::FailWorkflow => RunResult::Failed { error },
        // #[non_exhaustive] · refuse loudly.
        other => RunResult::Failed {
            error: TaskErrorRecord {
                code: nika_error::codes::NIKA_1703.to_string(),
                message: format!("on_error action not wired in the runtime yet: {other:?}"),
                transient: false,
            },
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
fn on_error_applies(on_error: &OnError, error: &TaskErrorRecord) -> bool {
    on_error.on_codes.is_empty() || on_error.on_codes.iter().any(|c| c.value == error.code)
}

/// Render the task's `with:` map (spec 03 · per-iteration in fan-out ·
/// entries cannot reference each other · spec 04).
fn render_with(
    task: &RawTask,
    records: &BTreeMap<String, TaskRecord>,
    vars: &BTreeMap<String, Value>,
    item: Option<&Value>,
    index: Option<usize>,
) -> Result<BTreeMap<String, Value>, RuntimeError> {
    let scope = Scope {
        records,
        vars,
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

/// A `RuntimeError` as a task error record (the wire code + message).
fn runtime_error_record(err: &RuntimeError) -> TaskErrorRecord {
    TaskErrorRecord {
        code: err.nika_code().to_string(),
        message: err.to_string(),
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

/// JSON value kind word (error messages).
fn json_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
