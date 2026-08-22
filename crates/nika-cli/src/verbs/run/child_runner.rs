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

use nika_event::source_id::sha256_hex;
use nika_execution::ExecutionSnapshot;

use nika_dap::journal::TraceFileSink;
use nika_runtime::RunSeams;
use nika_runtime::compose::{RuntimeCapabilities, fs_boundary_of_permits, net_boundary_of_permits};

/// The production runner — one per composed runtime, rooted at the file
/// whose tasks it serves.
pub(crate) struct ProdChildRunner {
    /// The complete immutable world admitted before the root run started.
    /// `None` remains only for the stdin/ARM compatibility lane until W04.B.
    snapshot: Option<ExecutionSnapshot>,
    /// The calling workflow's host path on the compatibility lane.
    parent_path: PathBuf,
    /// The CALLING workflow's logical path inside that world.
    parent_logical: String,
    /// Presentation-only root used to preserve existing diagnostic paths.
    display_root: PathBuf,
    /// Whether child runs keep trace files (`--no-trace-file` inherits).
    trace: bool,
}

impl ProdChildRunner {
    pub(crate) fn new(parent_path: impl Into<PathBuf>, trace: bool) -> Self {
        let parent_path = parent_path.into();
        Self {
            snapshot: None,
            parent_logical: parent_path.to_string_lossy().into_owned(),
            display_root: PathBuf::new(),
            parent_path,
            trace,
        }
    }

    pub(crate) fn admitted(
        snapshot: ExecutionSnapshot,
        parent_logical: impl Into<String>,
        display_root: impl Into<PathBuf>,
        trace: bool,
    ) -> Self {
        Self {
            snapshot: Some(snapshot),
            parent_path: PathBuf::new(),
            parent_logical: parent_logical.into(),
            display_root: display_root.into(),
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
    ) -> Result<
        (
            String,
            PathBuf,
            String,
            RawWorkflow,
            nika_check::CheckReport,
        ),
        ChildRunRefusal,
    > {
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
        let (logical, path, source) = if let Some(snapshot) = &self.snapshot {
            let logical =
                resolve_logical(&self.parent_logical, &call.target).map_err(|detail| {
                    refusal(
                        "NIKA-COMP-001",
                        format!("cannot resolve child `{}`: {detail}", call.target),
                    )
                })?;
            let path = self.display_root.join(&logical);
            let source = snapshot.text(&logical).ok_or_else(|| {
                refusal(
                    "NIKA-COMP-001",
                    format!("captured world has no child `{}`", path.display()),
                )
            })?;
            (logical, path, source.to_owned())
        } else {
            let path = resolve_against(&self.parent_path, &call.target);
            let source = std::fs::read_to_string(&path).map_err(|error| {
                refusal(
                    "NIKA-COMP-001",
                    format!("cannot read child `{}`: {error}", path.display()),
                )
            })?;
            (path.to_string_lossy().into_owned(), path, source)
        };
        let wf = nika_schema::parse(&source, FileId::new(0), ParseMode::Strict).map_err(|e| {
            refusal(
                "NIKA-COMP-001",
                format!("child `{}` does not parse: {e}", path.display()),
            )
        })?;
        // The child clears the SAME gate a standalone run clears — its
        // own composed check (grandchildren judged from ITS root).
        let report = if let Some(snapshot) = &self.snapshot {
            nika_check::check_composed(&wf, &logical, &mut |p| {
                snapshot
                    .text(p)
                    .map(str::to_owned)
                    .ok_or_else(|| format!("captured world has no unit `{p}`"))
            })
        } else {
            nika_check::check_composed(&wf, &logical, &mut |p| {
                std::fs::read_to_string(p).map_err(|error| error.to_string())
            })
        };
        if !report.is_clean() {
            let (code, detail) = first_finding(&report);
            return Err(refusal(
                &code,
                format!("child `{}` fails its own check: {detail}", path.display()),
            ));
        }
        Ok((logical, path, source, wf, report))
    }
}

fn resolve_logical(parent: &str, target: &str) -> Result<String, String> {
    let target = Path::new(target);
    if target.is_absolute() {
        return Err("absolute paths are outside the admitted project".to_owned());
    }
    let mut parts: Vec<String> = Path::new(parent)
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => part.to_str().map(ToOwned::to_owned),
            _ => None,
        })
        .collect();
    for component in target.components() {
        match component {
            std::path::Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or_else(|| "path is not valid UTF-8".to_owned())?;
                parts.push(part.to_owned());
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                parts
                    .pop()
                    .ok_or_else(|| "path escapes the admitted project".to_owned())?;
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err("path escapes the admitted project".to_owned());
            }
        }
    }
    if parts.is_empty() {
        return Err("path resolves to the project root".to_owned());
    }
    Ok(parts.join("/"))
}

pub(super) fn resolve_admitted(parent: &str, target: &str) -> Result<String, String> {
    resolve_logical(parent, target)
}

/// The child's effective capability boundary — `child ∩ parent` (laws
/// 3/4 made structural at run). F-O8 « absent = zero authority »: an
/// absent parent block is the EMPTY boundary (not « no wall »), so the
/// meet caps every child at zero — a parent's grants never descend
/// implicitly, and an absent child declaration under a parent wall keeps
/// the parent's binds.
fn effective_permits(child: Option<&Permits>, parent: Option<&Permits>) -> Permits {
    let zero = Permits::new();
    let parent = parent.unwrap_or(&zero);
    match child {
        None => parent.clone(),
        Some(c) => c.intersect(parent),
    }
}

/// The first blocking finding of a dirty child report — `(code, row)`,
/// the child's OWN voice surfaced by the parent.
fn first_finding(report: &nika_check::CheckReport) -> (String, String) {
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

/// The transitive child-source closure digest per STATIC `workflow:`
/// target of `wf` — the calling tasks' resume-identity input (ADR-099
/// trap 6 across the file boundary · spec 14 law 10 at the `def_hash`
/// tier). Each digest is a Merkle fold over the child's source bytes
/// AND its own children's digests, so a grandchild edit re-keys every
/// caller up the chain. A target that fails to resolve, parse, or
/// bound (cycle · depth · registry) simply gets NO entry — the
/// referencing task is then not resume-eligible: it re-runs live,
/// never wrong-skips (the honest direction, the skills #473 law).
pub(crate) fn admitted_closure_digests(
    wf: &RawWorkflow,
    snapshot: &ExecutionSnapshot,
    parent_logical: &str,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for target in workflow_targets_of(wf) {
        let Ok(resolved) = resolve_logical(parent_logical, &target) else {
            continue;
        };
        let mut stack = Vec::new();
        if let Some(digest) = closure_digest(snapshot, &resolved, &mut stack, 1) {
            out.insert(target, digest);
        }
    }
    out
}

pub(crate) fn closure_digests(wf: &RawWorkflow, file: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for target in workflow_targets_of(wf) {
        let resolved = resolve_against(file, &target);
        let mut stack = Vec::new();
        if let Some(digest) = closure_digest_fs(&resolved, &mut stack, 1) {
            out.insert(target, digest);
        }
    }
    out
}

fn closure_digest_fs(path: &Path, stack: &mut Vec<PathBuf>, depth: u32) -> Option<String> {
    if depth > nika_runtime::child::MAX_RUN_DEPTH {
        return None;
    }
    let identity = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if stack.contains(&identity) {
        return None;
    }
    let source = std::fs::read_to_string(path).ok()?;
    let wf = nika_schema::parse(&source, FileId::new(0), ParseMode::Strict).ok()?;
    stack.push(identity);
    let mut children = BTreeMap::new();
    for target in workflow_targets_of(&wf) {
        if target.starts_with("registry:") {
            stack.pop();
            return None;
        }
        let digest = closure_digest_fs(&resolve_against(path, &target), stack, depth + 1);
        let Some(digest) = digest else {
            stack.pop();
            return None;
        };
        children.insert(target, digest);
    }
    stack.pop();
    let mut fold = String::from("nika-child-closure:v1\u{0}");
    fold.push_str(&sha256_hex(source.as_bytes()));
    for (target, digest) in &children {
        fold.push('\u{0}');
        fold.push_str(target);
        fold.push('\u{0}');
        fold.push_str(digest);
    }
    Some(sha256_hex(fold.as_bytes()))
}

fn resolve_against(parent: &Path, target: &str) -> PathBuf {
    let target = Path::new(target);
    if target.is_absolute() {
        return target.to_path_buf();
    }
    parent
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(target)
}

/// Every static `workflow:` target `wf`'s tasks carry (main verbs +
/// `on_finally` minis) — the resolution roots of [`closure_digests`].
fn workflow_targets_of(wf: &RawWorkflow) -> Vec<String> {
    fn of(action: &nika_schema::raw::RawAction) -> Option<String> {
        match action {
            nika_schema::raw::RawAction::Invoke(a) => match &a.target {
                nika_schema::raw::RawInvokeTarget::Workflow(w) => Some(w.value.clone()),
                nika_schema::raw::RawInvokeTarget::Tool(_) => None,
            },
            _ => None,
        }
    }
    let mut out = Vec::new();
    for task in &wf.tasks {
        out.extend(of(&task.value.action));
    }
    out
}

/// One child's closure digest — sha256 over a version-tagged fold of
/// `sha256(source bytes)` + every `(target, child digest)` pair in
/// `BTreeMap` order (deterministic · framing NUL-separated). The direct
/// source component is the SAME primitive as the trace-forest row's
/// `def_hash` (sha256 of the child's exact bytes). `None` = the
/// closure cannot be drawn honestly (unreadable · unparseable · a
/// registry target · a cycle · past the run depth bound).
fn closure_digest(
    snapshot: &ExecutionSnapshot,
    logical: &str,
    stack: &mut Vec<String>,
    depth: u32,
) -> Option<String> {
    if depth > nika_runtime::child::MAX_RUN_DEPTH {
        return None;
    }
    if stack.iter().any(|identity| identity == logical) {
        return None;
    }
    let source = snapshot.text(logical)?;
    let wf = nika_schema::parse(source, FileId::new(0), ParseMode::Strict).ok()?;
    stack.push(logical.to_owned());
    let mut children = BTreeMap::new();
    for target in workflow_targets_of(&wf) {
        if target.starts_with("registry:") {
            stack.pop();
            return None;
        }
        let Ok(resolved) = resolve_logical(logical, &target) else {
            stack.pop();
            return None;
        };
        let digest = closure_digest(snapshot, &resolved, stack, depth + 1);
        let Some(digest) = digest else {
            stack.pop();
            return None;
        };
        children.insert(target, digest);
    }
    stack.pop();
    let mut fold = String::from("nika-child-closure:v1\u{0}");
    fold.push_str(&sha256_hex(source.as_bytes()));
    for (target, digest) in &children {
        fold.push('\u{0}');
        fold.push_str(target);
        fold.push('\u{0}');
        fold.push_str(digest);
    }
    Some(sha256_hex(fold.as_bytes()))
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
            let (logical, path, source, wf, report) = self.load_child(&call)?;
            // Compose the child runtime — the child's OWN envelope model;
            // capabilities from the INTERSECTED boundary (laws 3/4).
            let child_permits = effective_permits(
                wf.permits.as_ref().map(|s| &s.value),
                call.parent_permits.as_ref(),
            );
            let caps = RuntimeCapabilities {
                fs: fs_boundary_of_permits(Some(&child_permits)),
                net: net_boundary_of_permits(Some(&child_permits)),
                exec_tasks: wf
                    .tasks
                    .iter()
                    .any(|t| matches!(t.value.action, nika_schema::raw::RawAction::Exec(_))),
                // spec 14's intersection: the boundary is DECLARED over the
                // child when either side named one (the parent's binds too).
                permits_declared: wf.permits.is_some() || call.parent_permits.is_some(),
            };
            let model = wf.model.as_ref().map_or("", |m| m.value.as_str());
            // F-P3 · the CHILD's own run: declaration governs its seams
            // (one run = one clock · each file declares for itself).
            let runtime = nika_runtime::compose::production_runtime(
                model,
                caps,
                wf.run.as_ref().map(|s| &s.value),
            )
            .map_err(|e| refusal("NIKA-COMP-001", format!("child runtime: {e}")))?
            .with_var_overrides(call.args.clone().into_iter().collect())
            // law 6 — the inherited budget IS the parent's remaining.
            .with_max_cost_usd(call.remaining_budget_usd)
            .with_source_sha256(sha256_hex(source.as_bytes()))
            // depth rides to the child so ITS dispatch gate sees the
            // truth (NIKA-SEC-003 · fail-closed).
            .with_run_depth(call.depth);
            let runtime = if let Some(snapshot) = &self.snapshot {
                let mut reader = |authored: &str| {
                    let skill = resolve_logical(&logical, authored)?;
                    snapshot
                        .text(&skill)
                        .map(str::to_owned)
                        .ok_or_else(|| format!("captured world has no unit `{skill}`"))
                };
                runtime
                    .with_skills(nika_schema::resolve_skills(&wf, &mut reader).texts)
                    .with_child_runner(Arc::new(ProdChildRunner::admitted(
                        snapshot.clone(),
                        &logical,
                        &self.display_root,
                        self.trace,
                    )))
                    .with_child_closures(admitted_closure_digests(&wf, snapshot, &logical))
            } else {
                runtime
                    .with_child_runner(Arc::new(ProdChildRunner::new(&path, self.trace)))
                    .with_child_closures(closure_digests(&wf, &path))
            };
            let mut sink = if self.trace {
                TraceFileSink::new(nika_dap::store::TRACE_DIR)
            } else {
                TraceFileSink::disabled()
            };
            let def_hash = sha256_hex(source.as_bytes());
            let mut stamper = RunSeams::of(wf.run.as_ref().map(|s| &s.value)).stamper();
            let run = runtime.run(&wf, &report, stamper.as_mut(), &mut sink).await;
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

#[cfg(test)]
mod tests {
    //! F-O8 « absent = zero authority » — the containment meet with an
    //! ABSENT parent block caps at ∅ (EPERM-style: the refusal is proven,
    //! not supposed).
    use super::*;

    #[test]
    fn admitted_runner_keeps_the_captured_child_after_disk_mutation() {
        let dir = tempfile::tempdir().expect("temp project");
        let root = dir.path().join("root.nika.yaml");
        let child = dir.path().join("child.nika.yaml");
        std::fs::write(
            &root,
            "nika: root\nmodel: mock/echo\npermits: {}\ntasks:\n  call:\n    invoke: { workflow: \"./child.nika.yaml\" }\n",
        )
        .expect("root fixture");
        let admitted_child = "nika: child\nmodel: mock/echo\npermits: {}\ntasks:\n  answer:\n    infer: { prompt: \"admitted\" }\noutputs:\n  answer: ${{ tasks.answer.output }}\n";
        std::fs::write(&child, admitted_child).expect("child fixture");
        let project = nika_fs::OwnedDir::open(dir.path()).expect("held project");
        let admitted = nika_execution::ExecutionService::default()
            .admit(&project, Path::new("root.nika.yaml"))
            .expect("admitted world");

        std::fs::write(
            &child,
            "nika: changed\nmodel: mock/echo\npermits: {}\ntasks:\n  answer:\n    infer: { prompt: \"mutated\" }\n",
        )
        .expect("mutated child");
        let runner = ProdChildRunner::admitted(
            admitted.snapshot().clone(),
            "root.nika.yaml",
            dir.path(),
            false,
        );
        let call = ChildCall {
            target: "./child.nika.yaml".to_owned(),
            args: BTreeMap::new(),
            depth: 1,
            remaining_budget_usd: None,
            deadline: None,
            parent_permits: None,
        };

        let (_, _, source, _, _) = runner.load_child(&call).expect("captured child");
        assert_eq!(source, admitted_child);
    }

    #[test]
    fn absent_parent_caps_every_child_at_zero() {
        // (None, None) — the pre-F-O8 « no wall » arm: now the EMPTY
        // boundary, so nothing runs unconfined across a `workflow:` call.
        let eff = effective_permits(None, None);
        assert_eq!(eff, Permits::new(), "absent ∩ absent = ∅");
        assert!(!eff.allows_exec(), "∅ refuses exec");
        assert!(!eff.allows_tool("nika:fetch"), "∅ refuses tools");

        // (Some(child), None) — a child declaring under an absent parent
        // is capped at zero too: the parent's grants never descend
        // implicitly (the meet ∩ stays the law).
        let mut child = Permits::new();
        child.exec = Some(nika_schema::types::ExecPermit::Any);
        let eff = effective_permits(Some(&child), None);
        assert!(
            !eff.allows_exec(),
            "child ∩ ∅ = ∅ — the declared child grant is capped"
        );

        // (None, Some(parent)) — unchanged: the parent's binds hold.
        let mut parent = Permits::new();
        parent.exec = Some(nika_schema::types::ExecPermit::Any);
        let eff = effective_permits(None, Some(&parent));
        assert!(eff.allows_exec(), "the declared parent wall descends");

        // (Some, Some) — unchanged: the meet.
        let eff = effective_permits(Some(&child), Some(&parent));
        assert!(eff.allows_exec(), "both declare exec → the meet keeps it");
    }
}
