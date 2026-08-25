// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `invoke: workflow:` dispatch arm — the composition lane's
//! runtime half (spec `14-composition.md`), split from `dispatch.rs`
//! at the 1500-LOC file cap (the settle/task split precedent). The
//! router lives in [`crate::Runtime::dispatch`]; the laws judged here
//! are the module doc of [`crate::child`].

use crate::errors::RuntimeError;
use nika_kernel::ai::provider::{ProviderInferDyn, ProviderMeta};
use nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn;
use nika_kernel::http::HttpPostDyn;
use nika_kernel::process::ShellRunDyn;
use nika_kernel::tool_executor::ToolExecuteDyn;
use serde_json::Value;

use crate::Runtime;
use crate::dispatch::{DispatchOk, Dispatched, FailedDispatch};
use crate::expr::{self, Scope};
use crate::record::TaskErrorRecord;

impl<S, T, H, P, D, C> Runtime<S, T, H, P, D, C>
where
    S: ShellRunDyn + Sync,
    T: ToolExecuteDyn,
    H: HttpPostDyn + Send + Sync + 'static,
    P: ProviderInferDyn + ProviderMeta,
    D: ToolDefinitionProviderDyn,
{
    /// Dispatch an `invoke: workflow:` call — the composition lane
    /// (spec `14-composition.md`). The laws judged HERE, before the
    /// injected [`crate::child::ChildRunner`] runs the child:
    ///
    /// - **depth** (`NIKA-SEC-003` · fail-closed) — the runtime backstop
    ///   behind static acyclicity; refused BEFORE any I/O.
    /// - **budget input** (law 6) — the ledger's remaining USD rides the
    ///   call; the runner composes `min(remaining, child declared)`.
    /// - **deadline** (law 6) — the attempt loop already bounds this
    ///   whole dispatch with the task's `timeout:` (a child cannot
    ///   outlive its caller); the deadline also rides the call.
    /// - **authority input** (laws 3/4) — the parent's declared boundary
    ///   rides the call; the runner intersects the child's into it.
    /// - **trace forest** (law 8) + **receipt commit** (law 9) — the
    ///   returned child summary rides the parent's terminal frame, which
    ///   is itself hash-chained: the parent's receipt commits to the
    ///   child's chain head.
    pub(crate) async fn dispatch_workflow_call(
        &self,
        target: &nika_schema::source::Spanned<String>,
        raw_args: Option<&nika_schema::source::Spanned<Value>>,
        scope: &Scope<'_>,
        (deadline, child_budget): (Option<std::time::Duration>, Option<f64>),
        contract: Option<&crate::contract::TaskContract<'_>>,
    ) -> Dispatched {
        let note = format!("invoke · workflow:{}", target.value);
        // Depth gate FIRST — fail-closed, before the runner, before I/O.
        let child_depth = self.run_depth.saturating_add(1);
        if child_depth > crate::child::MAX_RUN_DEPTH {
            return Dispatched::comp_refusal(
                &note,
                "NIKA-SEC-003",
                format!(
                    "nested-run depth {child_depth} exceeds the run-recursion bound \
                     ({max}) — the static call graph is acyclic by check \
                     (NIKA-COMP-003); this backstop refuses what a static checker \
                     cannot draw (spec 14 §errors)",
                    max = crate::child::MAX_RUN_DEPTH
                ),
            );
        }
        let Some(runner) = self.child_runner.as_ref() else {
            return Dispatched::comp_refusal(
                &note,
                "NIKA-COMP-001",
                format!(
                    "no child-workflow surface is composed on this runtime — \
                     `workflow: {}` cannot resolve here (the composer injects the \
                     runner via with_child_runner; spec 14 §the form)",
                    target.value
                ),
            );
        };
        // Render `args:` (they MAY carry `${{ }}` — the target may not).
        let args = match raw_args {
            None => serde_json::Map::new(),
            Some(a) => match expr::render_json(&a.value, scope) {
                Ok(Value::Object(map)) => map,
                Ok(_) => {
                    return Dispatched::comp_refusal(
                        &note,
                        "NIKA-COMP-004",
                        "`invoke.args` must render to an object — the child's typed \
                         `vars:` inputs (spec 14 law 2)"
                            .to_owned(),
                    );
                }
                Err(err) => return Dispatched::template_err(&note, &RuntimeError::from(err)),
            },
        };
        let call = crate::child::ChildCall {
            target: target.value.clone(),
            args: args.into_iter().collect(),
            depth: child_depth,
            remaining_budget_usd: child_budget,
            deadline,
            parent_permits: scope.permits.cloned(),
        };
        match runner.run_child(call).await {
            Ok(out) => Self::settle_child_outcome(&note, &target.value, out, contract),
            Err(refusal) => Dispatched::comp_refusal(&note, &refusal.code, refusal.message),
        }
    }

    /// Fold a finished child run into the dispatch outcome — the value
    /// is the child's typed `outputs:` object (law 2 · the parent's
    /// `returns:` contract fits it downstream in the SAME pipeline as
    /// every verb); the spend debits the parent ledger (laws 5/6); the
    /// summary rides to the terminal frame (laws 8/9).
    fn settle_child_outcome(
        note: &str,
        target: &str,
        out: crate::child::ChildOutcome,
        contract: Option<&crate::contract::TaskContract<'_>>,
    ) -> Dispatched {
        let cost_source = out.cost_usd.is_some().then(|| format!("workflow:{target}"));
        if out.ok {
            let value = Value::Object(out.outputs.into_iter().collect());
            // Law 2, run half — the child's outputs fit the parent's
            // `returns:` (the SAME judgment every verb clears ·
            // NIKA-TYPE-101). The check proved it statically for the
            // DECLARED types; this covers the child's untyped outputs.
            if let Some(c) = contract
                && let Err(err) = c.check_fit(note, &value)
            {
                return Dispatched::template_err(note, &err);
            }
            let mut ok = DispatchOk {
                value,
                tokens: None,
                warning: None,
                child: out.trace.map(Box::new),
                cost_usd: out.cost_usd,
                cost_source,
                cost_unpriced: None,
                // F-P6 · the child's OWN trace attests its steps (spec 14
                // law 9) — the call itself fires no exec/tool bytes.
                commit: None,
            };
            // Belt: a runner that kept no trace still records the
            // outcome class (the forest row is check≡run material).
            if ok.child.is_none() {
                ok.child = Some(Box::new(crate::child::ChildRunSummary::new(
                    target,
                    true,
                    (None, None, None),
                )));
            }
            return Dispatched {
                note: note.to_owned(),
                result: Ok(ok),
            };
        }
        let (code, message) = out.failure.unwrap_or_else(|| {
            (
                "NIKA-COMP-001".to_owned(),
                "child run failed without a failure surface".to_owned(),
            )
        });
        let trace_note = out
            .trace
            .as_ref()
            .map(|t| {
                format!(
                    " · child trace {} · chain head {}",
                    t.trace_id.as_deref().unwrap_or("(none)"),
                    t.chain_head.as_deref().unwrap_or("(none)")
                )
            })
            .unwrap_or_default();
        Dispatched {
            note: note.to_owned(),
            result: Err(FailedDispatch {
                record: TaskErrorRecord {
                    code,
                    message: format!("child workflow `{target}` failed: {message}{trace_note}"),
                    transient: false,
                },
                cost_usd: out.cost_usd,
                cost_source,
                cost_unpriced: None,
                // F-P6 · the child's own trace attests its steps.
                evidence: None,
            }),
        }
    }
}

use std::sync::Arc;

impl<S, T, H, P, D, C> Runtime<S, T, H, P, D, C> {
    /// Inject the child-workflow execution seam (spec 14 · composition).
    /// The runner owns the I/O half of a nested run (resolve · parse ·
    /// check · compose · run); the runtime keeps the laws it can judge
    /// locally (depth gate · budget/deadline inputs · the trace-forest
    /// record). Absent (default), an `invoke: workflow:` task fails
    /// loudly with the composition code — never a silent no-op.
    #[must_use]
    pub fn with_child_runner(mut self, runner: Arc<dyn crate::child::ChildRunner>) -> Self {
        self.child_runner = Some(runner);
        self
    }

    /// Declare THIS run's nesting depth (root = 0). The composer sets
    /// `parent depth + 1` on each child runtime so the fail-closed
    /// [`crate::child::MAX_RUN_DEPTH`] gate (`NIKA-SEC-003`) sees the truth.
    #[must_use]
    pub fn with_run_depth(mut self, depth: u32) -> Self {
        self.run_depth = depth;
        self
    }
}

#[cfg(test)]
mod tests;
