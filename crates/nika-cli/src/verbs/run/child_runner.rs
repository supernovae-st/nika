// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The production child-workflow runner (spec `14-composition.md`) — the
//! I/O half of `invoke: workflow:` the runtime's [`ChildRunner`] seam
//! delegates: resolve the target against the PARENT file's directory,
//! parse + `check_composed` the child (the same gates a standalone run
//! clears), compose a child production runtime, run it as a NESTED run
//! with its OWN trace file, and report `{outputs, cost, trace row}`.
//!
//! The composed laws, run-side:
//! - **budget** (law 6) — the child's `--max-cost-usd` IS the parent's
//!   remaining at call time (`min(remaining, declared)`: a workflow
//!   declares no budget in v1, so the min is the remaining).
//! - **deadline** (law 6) — the parent's attempt loop drops this whole
//!   future at the task `timeout:` (`race_budget`) — a child cannot
//!   outlive its caller structurally; no second timer needed here.
//! - **authority** (laws 3/4) — the child's effective boundary is
//!   `child ∩ parent` (the nika-cap meet · conservative): a child NEVER
//!   runs wider than its caller, even if a stale check was skipped.
//! - **trace forest** (law 8) — the child gets its OWN `TraceFileSink`
//!   (own hash chain); the parent frame records `{trace_id, chain_head,
//!   def_hash, outcome}`.
//! - **recursion** — grandchildren compose through a fresh runner rooted
//!   at the child's path; the DEPTH gate lives engine-side
//!   (`MAX_RUN_DEPTH` · `NIKA-SEC-003` · fail-closed before any I/O).

use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use nika_runtime::child::{ChildCall, ChildOutcome, ChildRunRefusal, ChildRunSummary, ChildRunner};
use nika_schema::raw::RawWorkflow;
use nika_schema::types::Permits;
use nika_schema::{FileId, ParseMode};

use nika_dap::source_id::sha256_hex;

use super::compose::{RuntimeCapabilities, fs_boundary_of_permits, net_boundary_of_permits};
use super::sink::{TRACE_DIR, TraceFileSink};
use super::stamp::SystemStamper;

/// The production runner — one per composed runtime, rooted at the file
/// whose tasks it serves.
pub(crate) struct ProdChildRunner {
    /// The CALLING workflow's own path (targets resolve against its dir).
    parent_path: PathBuf,
    /// Whether child runs keep trace files (`--no-trace-file` inherits).
    trace: bool,
}

impl ProdChildRunner {
    pub(crate) fn new(parent_path: impl Into<PathBuf>, trace: bool) -> Self {
        Self {
            parent_path: parent_path.into(),
            trace,
        }
    }

    /// Resolve + load + gate the child — everything before the run.
    /// `Err` = a composition refusal (the check-time codes, run-side —
    /// the skills dual-surface precedent).
    #[allow(clippy::type_complexity)] // one seam · the tuple IS the contract
    fn load_child(
        &self,
        call: &ChildCall,
    ) -> Result<(PathBuf, String, RawWorkflow, nika_schema::CheckReport), ChildRunRefusal> {
        if call.target.starts_with("registry:") {
            return Err(refusal(
                "NIKA-COMP-001",
                format!(
                    "`{}` — registry child execution lands with the registry \
                     lane; run a filesystem child today (spec 14 §the form)",
                    call.target
                ),
            ));
        }
        let path = resolve_against(&self.parent_path, &call.target);
        let source = std::fs::read_to_string(&path).map_err(|e| {
            refusal(
                "NIKA-COMP-001",
                format!("cannot read child `{}`: {e}", path.display()),
            )
        })?;
        let wf = nika_schema::parse(&source, FileId::new(0), ParseMode::Strict).map_err(|e| {
            refusal(
                "NIKA-COMP-001",
                format!("child `{}` does not parse: {e}", path.display()),
            )
        })?;
        // The child clears the SAME gate a standalone run clears — its
        // own composed check (grandchildren judged from ITS root).
        let root = path.to_string_lossy().into_owned();
        let report = nika_schema::check_composed(&wf, &root, &mut |p| {
            std::fs::read_to_string(p).map_err(|e| e.to_string())
        });
        if !report.is_clean() {
            let (code, detail) = first_finding(&report);
            return Err(refusal(
                &code,
                format!("child `{}` fails its own check: {detail}", path.display()),
            ));
        }
        Ok((path, source, wf, report))
    }
}

/// The child's effective capability boundary — `child ∩ parent` (laws
/// 3/4 made structural at run): absent parent wall = the child's own;
/// absent child declaration under a parent wall = the parent's binds.
fn effective_permits(child: Option<&Permits>, parent: Option<&Permits>) -> Option<Permits> {
    match (child, parent) {
        (None, None) => None,
        (Some(c), None) => Some(c.clone()),
        (None, Some(p)) => Some(p.clone()),
        (Some(c), Some(p)) => Some(c.intersect(p)),
    }
}

/// The first blocking finding of a dirty child report — `(code, row)`,
/// the child's OWN voice surfaced by the parent.
fn first_finding(report: &nika_schema::CheckReport) -> (String, String) {
    report.findings.first().map_or_else(
        || ("NIKA-COMP-001".to_owned(), "unnamed finding".to_owned()),
        |f| {
            (
                f.code.clone().unwrap_or_else(|| "NIKA-COMP-001".to_owned()),
                f.message.clone(),
            )
        },
    )
}

fn refusal(code: &str, message: String) -> ChildRunRefusal {
    ChildRunRefusal {
        code: code.to_owned(),
        message,
    }
}

/// Resolve `target` against the DIRECTORY of `parent` (the same law the
/// static lane applies lexically — here on the real filesystem path).
fn resolve_against(parent: &Path, target: &str) -> PathBuf {
    let t = Path::new(target);
    if t.is_absolute() {
        return t.to_path_buf();
    }
    parent.parent().unwrap_or_else(|| Path::new("")).join(t)
}

/// The first failed task's error of a settled child run — the honest
/// failure surface the parent's task error carries.
fn first_failure(outcome: &nika_runtime::RunOutcome) -> (String, String) {
    for (id, rec) in &outcome.records {
        if let Some(err) = &rec.error {
            return (err.code.clone(), format!("task `{id}`: {}", err.message));
        }
    }
    if outcome.budget_exceeded {
        return (
            "NIKA-1704".to_owned(),
            "the inherited cost budget was exceeded (spec 14 law 6 · \
             min(parent remaining, child declared))"
                .to_owned(),
        );
    }
    (
        "NIKA-COMP-001".to_owned(),
        "child run failed without a task error".to_owned(),
    )
}

impl ChildRunner for ProdChildRunner {
    fn run_child<'a>(
        &'a self,
        call: ChildCall,
    ) -> Pin<Box<dyn Future<Output = Result<ChildOutcome, ChildRunRefusal>> + 'a>> {
        Box::pin(async move {
            let (path, source, wf, report) = self.load_child(&call)?;
            // Compose the child runtime — the child's OWN envelope model;
            // capabilities from the INTERSECTED boundary (laws 3/4).
            let child_permits = effective_permits(
                wf.permits.as_ref().map(|s| &s.value),
                call.parent_permits.as_ref(),
            );
            let caps = RuntimeCapabilities {
                fs: fs_boundary_of_permits(child_permits.as_ref()),
                net: net_boundary_of_permits(child_permits.as_ref()),
                exec_tasks: wf
                    .tasks
                    .iter()
                    .any(|t| matches!(t.value.action, nika_schema::raw::RawAction::Exec(_))),
            };
            let model = wf.model.as_ref().map_or("", |m| m.value.as_str());
            let runtime = super::compose::production_runtime(model, caps)
                .map_err(|e| refusal("NIKA-COMP-001", format!("child runtime: {e}")))?
                .with_var_overrides(call.args.clone().into_iter().collect())
                // law 6 — the inherited budget IS the parent's remaining.
                .with_max_cost_usd(call.remaining_budget_usd)
                .with_source_sha256(sha256_hex(source.as_bytes()))
                // depth rides to the child so ITS dispatch gate sees the
                // truth (NIKA-SEC-003 · fail-closed).
                .with_run_depth(call.depth)
                // grandchildren resolve against the CHILD's path.
                .with_child_runner(Arc::new(ProdChildRunner::new(&path, self.trace)));
            let mut stamper = SystemStamper;
            let mut sink = if self.trace {
                TraceFileSink::new(TRACE_DIR)
            } else {
                TraceFileSink::disabled()
            };
            let def_hash = sha256_hex(source.as_bytes());
            let run = runtime.run(&wf, &report, &mut stamper, &mut sink).await;
            let (ok, outputs, cost, failure) = match &run {
                Ok(outcome) => {
                    // A paused child cannot be answered through a call
                    // boundary — surfaced as the prompt contract failure.
                    let ok = outcome.ok && outcome.paused.is_none() && !outcome.budget_exceeded;
                    let failure = (!ok).then(|| first_failure(outcome));
                    (ok, outcome.outputs.clone(), outcome.total_cost_usd, failure)
                }
                Err(err) => (
                    false,
                    BTreeMap::new(),
                    None,
                    Some((err.spec_code(), err.to_string())),
                ),
            };
            let trace = Some(ChildRunSummary::new(
                call.target.clone(),
                ok,
                (
                    sink.path()
                        .and_then(|p| p.file_name())
                        .map(|s| s.to_string_lossy().into_owned()),
                    Some(sink.chain_head().to_owned()),
                    Some(def_hash),
                ),
            ));
            Ok(ChildOutcome {
                ok,
                outputs,
                cost_usd: cost,
                trace,
                failure,
            })
        })
    }
}
