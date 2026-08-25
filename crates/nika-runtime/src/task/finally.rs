// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `on_finally` cleanup lane (spec 03 · sequential · best-effort ·
//! errors journaled, never propagated · per-cleanup timeout 30s) —
//! split out of `task.rs` under the ADR-023 1,500-LOC ceiling.

use std::time::Duration;

use nika_kernel::ai::provider::{ProviderInferDyn, ProviderMeta};
use nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn;
use nika_kernel::clock::ClockDyn;
use nika_kernel::http::HttpPostDyn;
use nika_kernel::process::ShellRunDyn;
use nika_kernel::tool_executor::ToolExecuteDyn;
use nika_schema::raw::{RawTask, RawWorkflow};
use nika_schema::types::AfterPredicate;

use crate::Runtime;
use crate::expr::Scope;
use crate::record::{TaskRecord, TaskStatus};

use super::{RanTask, RunResult, eval_gate};

/// Default per-cleanup-task timeout (spec 03 §`on_finally`).
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(30);

/// (`run_finally`) stamps the pipeline's integrity label on the returned
/// record — the overlay is the one records entry the settle spine never
/// sees (F-O1 PR-2 · the cleanup re-gate reads it).
/// The parent's preview record for the cleanup scope (spec 03 · the
/// cleanup sees `tasks.<parent>.status` / `.error`). A PENDING recovery
/// previews as its pre-recovery failure: the attempts DID fail, and the
/// deferred render has produced no value when the cleanup runs (the
/// cleanup is task-scoped · it never awaits the spine). The CALLER
fn preview_record(ran: &RanTask) -> TaskRecord {
    use crate::record::{TerminalCause, failure_cause};
    let attempts = ran.attempts();
    let mut record = match &ran.result {
        RunResult::Success { recovered_from, .. } => {
            let cause = if recovered_from.is_some() {
                TerminalCause::Recovered
            } else {
                TerminalCause::Normal
            };
            let mut rec = TaskRecord::unran(TaskStatus::Success, cause);
            rec.attempts = Some(attempts);
            rec.recovered_from.clone_from(recovered_from);
            rec.error.clone_from(recovered_from);
            rec
        }
        RunResult::SkippedWithError { .. } => {
            TaskRecord::unran(TaskStatus::Skipped, TerminalCause::ErrorSkip)
        }
        RunResult::Failed { error, .. } => {
            let mut rec = TaskRecord::unran(TaskStatus::Failure, failure_cause(error, attempts));
            rec.attempts = Some(attempts);
            rec
        }
        RunResult::PendingRecovery(pending) => {
            let mut rec = TaskRecord::unran(
                TaskStatus::Failure,
                failure_cause(&pending.failed.record, attempts),
            );
            rec.attempts = Some(attempts);
            rec
        }
    };
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

/// The cleanup tasks attached to `producer` by an `unwind` edge, in
/// DECLARATION order (the source order of `tasks:`).
///
/// Membership is read off the task's own `after:` — the same place the
/// checker reads it — so the runtime and the graph can never disagree
/// about what cleanup exists.
fn unwind_tasks_of<'a>(wf: &'a RawWorkflow, producer: &str) -> Vec<&'a RawTask> {
    wf.tasks
        .iter()
        .map(|t| &t.value)
        .filter(|t| {
            t.after.iter().any(|(target, pred)| {
                target.value == producer && matches!(pred.value, AfterPredicate::Unwind)
            })
        })
        .collect()
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
    /// Run the cleanup mini-tasks (spec 03 §`on_finally` · sequential ·
    /// best-effort · a failure/timeout/skip is journaled on the
    /// witness, never propagated · per-cleanup timeout 30s).
    pub(crate) async fn run_finally(
        &self,
        task: &RawTask,
        wf: &RawWorkflow,
        scope: &Scope<'_>,
        ran: &RanTask,
        integrity: &nika_cap::Integrity,
        witness: &crate::witness::PermitWitness,
        run_start: nika_kernel::tool_executor::ToolRunStart,
    ) {
        // The cleanup bodies are TASKS now, joined by an `unwind` edge
        // (spec 03 §unwind). They run in DECLARATION order — the source
        // order of `tasks:` — so the sequence is stable across re-runs.
        let cleanups = unwind_tasks_of(wf, task.id.value.as_str());
        if cleanups.is_empty() {
            return;
        }
        // The cleanup scope sees the PARENT's fresh status/error via a
        // one-record overlay (spec 03 · status/error routing).
        // PERF (documented trade): one records-map clone per
        // task-WITH-cleanup (early-return above keeps the common lane
        // free) · workflow size is certificate-bounded (degree-1) — a
        // copy-on-read overlay Scope would save it at the cost of a
        // two-level resolve on EVERY lookup.
        let mut records = scope.records().clone();
        let mut preview = preview_record(ran);
        // F-O1 · the overlay carries the parent's integrity label: a
        // cleanup argv/arg reading `${{ tasks.<parent>.output }}` re-gates
        // on its taint (PR-2) — the overlay is the ONLY records entry the
        // settle spine has not stamped.
        preview.integrity = integrity.clone();
        records.insert(task.id.value.clone(), preview);
        let cleanup_scope = Scope::workflow_with_value_authorities(
            &records,
            scope.inputs(),
            scope.consts(),
            scope.secrets(), // a cleanup may reference secrets.X too
        )
        // Locals are out of scope after fan-out; `on_finally` exec retains the
        // workflow capability boundary.
        .with_task_context(scope.with_namespace(), None, None, scope.permits());
        for (index, cleanup) in cleanups.iter().enumerate() {
            self.run_one_cleanup(cleanup, &cleanup_scope, witness, index, run_start)
                .await;
        }
    }

    /// One cleanup TASK · its own `when:` + `timeout:` · outcome
    /// journaled, never propagated (best-effort by construction · its
    /// failure never reaches the producer · spec 03 §unwind guarantee
    /// 3: « its errors are logged » — a skip/failure/timeout rides the
    /// witness as a `permit_checked` frame on plane `on_finally`, so a
    /// refused cleanup is distinguishable from a dead trigger).
    async fn run_one_cleanup(
        &self,
        cleanup: &RawTask,
        scope: &Scope<'_>,
        witness: &crate::witness::PermitWitness,
        index: usize,
        run_start: nika_kernel::tool_executor::ToolRunStart,
    ) {
        if let Some(gate) = cleanup.when.as_ref() {
            // Closed gate OR eval error → the cleanup is skipped
            // (a cleanup error never propagates) — and the skip is
            // journaled: without this frame a gate-closed cleanup
            // is pixel-identical to a dead trigger on the trace.
            if !matches!(eval_gate(&gate.value, scope), Ok(true)) {
                witness.record(
                    "on_finally",
                    format!("cleanup #{index}"),
                    "skipped",
                    "when: gate closed or errored — the cleanup did not run \
                     (best-effort lane · spec 03 §unwind)",
                );
                return;
            }
        }
        let limit = cleanup
            .timeout
            .as_ref()
            .map_or(CLEANUP_TIMEOUT, |t| t.value);
        // Cleanup agent decisions are NOT collected (best-effort lane ·
        // outcome dropped by design) — a throwaway buffer satisfies the
        // dispatch seam; collecting it is a trigger-gated ratchet.
        let cleanup_buffer = crate::agent_events::BufferingObserver::new();
        // Mini-tasks carry no `returns:` (closed shape) — no contract.
        // The re-gate oracle is the BARE one (a mini-task has no
        // `with:`/`for_each` — the records + inputs lookups still label
        // a tainted cleanup argv/arg · F-O1 PR-2).
        let value_taint = crate::integrity::ValueTaint::bare();
        // NEP-0007 law 2 (the final review's catch · 2026-07-23): the
        // cleanup lane's decisions are recorded into the PARENT's
        // witness — they settle with it as `permit_checked` frames (the
        // lane-local witness that never drained is retired · the
        // attestation blind spot is closed).
        witness.record(
            "on_finally",
            format!("cleanup #{index}"),
            "attempt",
            "cleanup mini-task starts (spec 03 · best-effort lane)",
        );
        let attempt = std::pin::pin!(self.dispatch(
            &cleanup.action,
            scope,
            &value_taint,
            &cleanup_buffer,
            crate::dispatch::DispatchCtx {
                deadline: Some(limit),
                // best-effort lane: no ledger here — a finally child
                // inherits no cost bound (the lane has no budget
                // admission by design); the select timer below still
                // bounds it in TIME.
                child_budget: None,
                // a finally mini-task carries no inert: door (NEP-0006)
                // — a code-bearing cleanup fetch refuses like any other.
                inert: None,
                witness,
                // a cleanup mini-task never carries a gate answer (B5).
                gate_answer: None,
                run_start,
            },
            None,
        ));
        let timer = std::pin::pin!(self.clock.sleep(limit));
        match futures_util::future::select(attempt, timer).await {
            futures_util::future::Either::Left((dispatched, _)) => {
                if let Err(failed) = dispatched.result {
                    Self::journal_cleanup_failure(witness, index, &failed.record);
                }
            }
            futures_util::future::Either::Right(((), _)) => {
                Self::journal_cleanup_timeout(witness, index, limit);
            }
        }
    }

    /// The outcome never PROPAGATES (best-effort lane) but it is
    /// JOURNALED (spec 03 §unwind guarantee 3 · « its errors are
    /// logged »): a failure rides the parent's witness as one more
    /// `permit_checked` frame on plane `on_finally`. A cleanup refused
    /// at the permit/sandbox boundary (NIKA-SEC-004) lands here with
    /// its code — no longer pixel-identical to a dead trigger. A clean
    /// finish stays silent: the cleanup's own effects are its
    /// observability (e.g. `nika:emit` · spec 03).
    fn journal_cleanup_failure(
        witness: &crate::witness::PermitWitness,
        index: usize,
        record: &nika_dataflow::TaskErrorRecord,
    ) {
        witness.record(
            "on_finally",
            format!("cleanup #{index}"),
            "failure",
            format!(
                "cleanup failed — the error does not propagate but is journaled \
                 (spec 03 §unwind guarantee 3) · {}: {}",
                record.code, record.message
            ),
        );
    }

    /// The cleanup's own timer won: abandoned, never propagated — and
    /// the abandon is journaled, so a timed-out cleanup is
    /// distinguishable from a dead trigger (spec 03 §unwind).
    fn journal_cleanup_timeout(
        witness: &crate::witness::PermitWitness,
        index: usize,
        limit: std::time::Duration,
    ) {
        witness.record(
            "on_finally",
            format!("cleanup #{index}"),
            "timeout",
            format!(
                "cleanup exceeded its {}s budget — abandoned, not propagated \
                 (spec 03 §unwind)",
                limit.as_secs()
            ),
        );
    }
}
