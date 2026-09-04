// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika explain --forecast` — learned truth from the local trace
//! history: duration/cost/risk priors BEFORE a run, computed
//! deterministically from `.nika/traces/` (stats, never a model call,
//! never the network — Rule 1). The honesty ladder is a TYPE
//! ([`nika_dap::stats::Prior`]): never-run says so · one run is « last run » ·
//! 2-4 runs earn a range · bands are earned at n ≥ 5. Every skip is
//! counted; a suggestion never outweighs a proof.

pub mod gather;
pub mod render;

use std::collections::BTreeMap;

use crate::display::state::TaskState;
use crate::trace::store::TraceState;

use gather::{Gathered, RunSample, TaskSample};
use nika_dap::stats::Prior;

/// `explain <file>` includes the section unprompted once the window
/// holds this many runs (under it, the recorder glance suffices);
/// `--forecast` always forces.
pub const AUTO_FORECAST_MIN_RUNS: usize = 3;

/// The current file's source identity — computed ONCE by the caller
/// (`load_checked_with_source` + `run/source_id.rs` hashes; the stdin
/// `-` arm works for free).
#[derive(Debug, Clone)]
pub struct WorkflowIdentity {
    /// The workflow name (matches `workflow_started`'s field).
    pub name: String,
    /// sha256 hex over the exact bytes read.
    pub sha256: String,
    /// sha256 hex over the LF normal form (CRLF/BOM-insensitive — an
    /// editor re-encode must not read as an edit).
    pub sha256_lf: String,
}

/// The gather accounting — every bound and every skip, counted.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Window {
    /// The run-count cap (policy · C4).
    pub k: usize,
    /// The age cap in days (policy · C4).
    pub days: u32,
    /// Directory entries examined (≤ `gather::SCAN_CAP`).
    pub files_examined: usize,
    /// GC race mid-scan · I/O refusal · dead first line.
    pub files_unreadable: usize,
    /// Valid prefix kept, torn tail dropped (crash · future kind).
    pub files_truncated: usize,
    /// NaN/inf samples filtered before sorting (C2 accounting).
    pub nonfinite_dropped: usize,
    /// The resolved `NIKA_TRACE_KEEP` knob — when the window saturates
    /// at it, retention is the reason n stays small (the render names
    /// it · information, never an error tone).
    pub retention_keep: usize,
}

impl Window {
    pub(crate) fn new() -> Self {
        Self {
            k: gather::WINDOW_RUNS,
            days: gather::WINDOW_DAYS,
            files_examined: 0,
            files_unreadable: 0,
            files_truncated: 0,
            nonfinite_dropped: 0,
            retention_keep: 0,
        }
    }
}

impl Default for Window {
    fn default() -> Self {
        Self::new()
    }
}

/// How a historical run's recorded source relates to the file being
/// explained — three states, never a silent guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShaMatch {
    /// Either recorded hash equals the current one (raw or LF form).
    Same,
    /// Hashes recorded, none match — the run predates the last edit.
    Stale,
    /// The run recorded no source hash (optional fields) — unverifiable.
    Unknown,
}

/// The run population census — who is in the window, by identity and
/// by terminal state.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct RunCensus {
    /// Matching runs inside the window.
    pub total: usize,
    /// Runs whose recorded source hash matches the current file.
    pub same_hash: usize,
    /// Runs recorded against an older source — counted, caveated.
    pub stale_hash: usize,
    /// Runs with no recorded hash — named « unverifiable », never
    /// silently bucketed.
    pub unknown_hash: usize,
    /// `workflow_completed` runs (the duration sample population).
    pub completed: usize,
    /// `workflow_failed` runs.
    pub failed: usize,
    /// `workflow_cancelled` runs.
    pub cancelled: usize,
    /// Paused on a human gate — an obligation, not a crash; excluded
    /// from bands, named apart.
    pub paused: usize,
    /// No terminal event — in flight or torn.
    pub no_terminal: usize,
}

/// One task's learned priors across the window.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskPrior {
    /// The task id.
    pub id: String,
    /// Runs where the task actually EXECUTED (ok · failed · torn
    /// mid-attempt · cache-satisfied). A `when` skip or an upstream
    /// cancellation is a decision, not a run — the when-gate hedge
    /// (« ran in K/N runs ») depends on this split.
    pub ran_in_runs: usize,
    /// Hard failures (terminal Failed) — C5: never mixed with flaky.
    pub failed: usize,
    /// Retried then reached Ok — FLAKY, never counted failed (C5).
    pub passed_on_retry: usize,
    /// Cache-hit rehydrations — excluded from bands, counted apart.
    pub cache_hits: usize,
    /// Occurrences on a model other than the newest occurrence's —
    /// EXCLUDED from bands (no cross-model extrapolation), counted.
    pub other_model: usize,
    /// Wall-duration prior over completed, non-cached, current-model
    /// occurrences.
    pub duration_ms: Prior,
    /// Realized-spend prior over the same population — `None` when no
    /// occurrence carried a cost (absence is absence, never $0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<Prior>,
    /// The dominant `cost_unpriced` slug, when the window carried one —
    /// forces the `≥` floor rendering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unpriced: Option<String>,
}

/// The forecast — learned truth, labeled as such, sample counts on
/// every number.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ForecastReport {
    /// The workflow name the history was matched on.
    pub workflow: String,
    /// The window population census.
    pub runs: RunCensus,
    /// The gather accounting.
    pub window: Window,
    /// Whole-run wall-duration prior (completed runs only).
    pub run_duration: Prior,
    /// Whole-run realized-spend prior — quantiles of per-run TOTALS
    /// (never a sum of per-task p50s); `None` when no terminal carried
    /// a priced total.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_cost: Option<Prior>,
    /// Per-task priors, first-seen order.
    pub tasks: Vec<TaskPrior>,
}

/// Classify one sample's recorded source against the current identity.
fn sha_match(sample: &RunSample, id: &WorkflowIdentity) -> ShaMatch {
    match (&sample.sha256, &sample.sha256_lf) {
        (None, None) => ShaMatch::Unknown,
        (raw, lf) => {
            let same_raw = raw.as_deref() == Some(id.sha256.as_str());
            let same_lf = lf.as_deref() == Some(id.sha256_lf.as_str());
            if same_raw || same_lf {
                ShaMatch::Same
            } else {
                ShaMatch::Stale
            }
        }
    }
}

/// Push a finite sample; count the non-finite drop (C2 accounting).
fn push_finite(values: &mut Vec<f64>, v: f64, dropped: &mut usize) {
    if v.is_finite() {
        values.push(v);
    } else {
        *dropped += 1;
    }
}

/// Fold gathered samples into the forecast report. PURE — zero I/O,
/// zero clock: the same gather always computes the same report.
#[must_use]
pub fn compute(id: &WorkflowIdentity, gathered: &Gathered) -> ForecastReport {
    let mut window = gathered.window;
    let mut runs = RunCensus {
        total: gathered.samples.len(),
        ..RunCensus::default()
    };
    let mut run_durations = Vec::new();
    let mut run_costs = Vec::new();
    for sample in &gathered.samples {
        match sha_match(sample, id) {
            ShaMatch::Same => runs.same_hash += 1,
            ShaMatch::Stale => runs.stale_hash += 1,
            ShaMatch::Unknown => runs.unknown_hash += 1,
        }
        match sample.state {
            TraceState::Succeeded => {
                runs.completed += 1;
                #[allow(clippy::cast_precision_loss)] // wall ms ≪ 2^52
                run_durations.push(sample.elapsed_ms as f64);
            }
            TraceState::Failed => runs.failed += 1,
            TraceState::Cancelled => runs.cancelled += 1,
            TraceState::Paused => runs.paused += 1,
            // `#[non_exhaustive]` future states fold with the in-flight
            // (the store's own unknown-terminal stance) — never a
            // settle this census would invent a verdict for.
            _ => runs.no_terminal += 1,
        }
        if let Some(total) = sample.total_cost_usd {
            push_finite(&mut run_costs, total, &mut window.nonfinite_dropped);
        }
    }

    let tasks = fold_tasks(&gathered.samples, &mut window.nonfinite_dropped);

    ForecastReport {
        workflow: id.name.clone(),
        runs,
        window,
        run_duration: Prior::from_finite(&run_durations),
        run_cost: (!run_costs.is_empty()).then(|| Prior::from_finite(&run_costs)),
        tasks,
    }
}

/// Per-task accumulation scratch (samples arrive newest-first, so the
/// FIRST occurrence seen fixes the current model — R-modèle).
#[derive(Default)]
struct TaskAcc {
    ran_in_runs: usize,
    failed: usize,
    passed_on_retry: usize,
    cache_hits: usize,
    other_model: usize,
    model_fixed: bool,
    current_model: Option<String>,
    durations: Vec<f64>,
    costs: Vec<f64>,
    unpriced: BTreeMap<String, usize>,
}

/// Group occurrences per task id and earn each prior.
fn fold_tasks(samples: &[RunSample], nonfinite: &mut usize) -> Vec<TaskPrior> {
    let mut order: Vec<String> = Vec::new();
    let mut accs: BTreeMap<String, TaskAcc> = BTreeMap::new();
    for sample in samples {
        for occ in &sample.tasks {
            let acc = accs.entry(occ.id.clone()).or_insert_with(|| {
                order.push(occ.id.clone());
                TaskAcc::default()
            });
            fold_occurrence(acc, occ, nonfinite);
        }
    }
    order
        .into_iter()
        .filter_map(|id| {
            let acc = accs.remove(&id)?;
            Some(TaskPrior {
                id,
                ran_in_runs: acc.ran_in_runs,
                failed: acc.failed,
                passed_on_retry: acc.passed_on_retry,
                cache_hits: acc.cache_hits,
                other_model: acc.other_model,
                duration_ms: Prior::from_finite(&acc.durations),
                cost_usd: (!acc.costs.is_empty()).then(|| Prior::from_finite(&acc.costs)),
                unpriced: dominant(&acc.unpriced),
            })
        })
        .collect()
}

/// Fold one occurrence into its task's scratch — the admission rules
/// (cache exclusion · C5 retry split · R-modèle scoping) live HERE,
/// in one place.
fn fold_occurrence(acc: &mut TaskAcc, occ: &TaskSample, nonfinite: &mut usize) {
    if let Some(slug) = &occ.unpriced {
        *acc.unpriced.entry(slug.clone()).or_insert(0) += 1;
    }
    if occ.cached {
        // Rehydrated: the task was SATISFIED here (counts as ran for the
        // when-gate hedge) — never a duration/cost/failure sample.
        acc.ran_in_runs += 1;
        acc.cache_hits += 1;
        return;
    }
    match occ.state {
        TaskState::Failed => {
            acc.ran_in_runs += 1;
            acc.failed += 1;
            return; // task_failed carries no duration/cost sample
        }
        // Attempted, torn before a terminal — it RAN, no sample. A
        // paused gate (ADR-099) is the same shape: opened, awaiting.
        TaskState::Running | TaskState::Retrying | TaskState::Paused => {
            acc.ran_in_runs += 1;
            return;
        }
        TaskState::Ok => {
            acc.ran_in_runs += 1;
            if occ.retried {
                acc.passed_on_retry += 1; // C5 · flaky, NEVER failed
            }
        }
        // Pending (never started) · Skipped (a `when` decision) ·
        // Cancelled (upstream died): the task did NOT run here — a skip
        // counting as « ran » would kill the when-gate hedge signal.
        TaskState::Pending | TaskState::Skipped | TaskState::Cancelled => return,
    }
    // R-modèle: the newest occurrence (seen first) fixes the model
    // bucket; other models are counted out, never mixed in.
    if !acc.model_fixed {
        acc.model_fixed = true;
        acc.current_model.clone_from(&occ.model);
    }
    if acc.current_model != occ.model {
        acc.other_model += 1;
        return;
    }
    if let Some(d) = occ.duration_ms {
        #[allow(clippy::cast_precision_loss)] // wall ms ≪ 2^52
        acc.durations.push(d as f64);
    }
    if let Some(c) = occ.cost_usd {
        push_finite(&mut acc.costs, c, nonfinite);
    }
}

/// The most frequent slug (ties break on `BTreeMap` order — stable).
fn dominant(counts: &BTreeMap<String, usize>) -> Option<String> {
    counts
        .iter()
        .max_by_key(|(_, n)| **n)
        .map(|(slug, _)| slug.clone())
}

#[cfg(test)]
mod tests {
    use super::gather::tests::{done, ev, run_body, task_done};
    use super::gather::{Gathered, RunSample, TaskSample};
    use super::*;
    use nika_event::EventKind;
    use nika_types::resource::Value;

    fn id() -> WorkflowIdentity {
        WorkflowIdentity {
            name: "wf".into(),
            sha256: "cafe".into(),
            sha256_lf: "cafe-lf".into(),
        }
    }

    fn sample_from(body: &str) -> RunSample {
        let recovered = nika_dap::recover::recover_events(body, "test").expect("parses");
        let (_, state, _) = crate::trace::store::fold_facts(&recovered.events);
        super::gather::fold_sample(&recovered.events, state).expect("sample")
    }

    fn gathered(samples: Vec<RunSample>) -> Gathered {
        Gathered {
            samples,
            window: Window::new(),
        }
    }

    #[test]
    fn census_buckets_same_stale_unknown_and_crlf_same() {
        let same_raw = sample_from(&run_body(
            "wf",
            Some("cafe"),
            1_000,
            &[],
            Some(done(1_010, None)),
        ));
        let stale = sample_from(&run_body(
            "wf",
            Some("beef"),
            2_000,
            &[],
            Some(done(2_010, None)),
        ));
        let unknown = sample_from(&run_body("wf", None, 3_000, &[], Some(done(3_010, None))));
        // CRLF twin: raw hash differs, LF form matches → Same.
        let mut crlf = sample_from(&run_body(
            "wf",
            Some("beef"),
            4_000,
            &[],
            Some(done(4_010, None)),
        ));
        crlf.sha256 = Some("other".into());
        crlf.sha256_lf = Some("cafe-lf".into());
        let report = compute(&id(), &gathered(vec![crlf, unknown, stale, same_raw]));
        assert_eq!(report.runs.total, 4);
        assert_eq!(report.runs.same_hash, 2, "raw-same + lf-same");
        assert_eq!(report.runs.stale_hash, 1);
        assert_eq!(report.runs.unknown_hash, 1);
        assert_eq!(report.runs.completed, 4);
    }

    #[test]
    fn run_duration_uses_completed_only_and_paused_is_named() {
        let completed = sample_from(&run_body("wf", None, 0, &[], Some(done(100, None))));
        let paused_body = run_body(
            "wf",
            None,
            0,
            &[],
            Some(ev(
                EventKind::WorkflowPaused,
                50,
                &[("task", Value::string("gate"))],
            )),
        );
        let paused = sample_from(&paused_body);
        let torn = sample_from(&run_body("wf", None, 0, &[], None));
        let report = compute(&id(), &gathered(vec![completed, paused, torn]));
        assert_eq!(report.runs.paused, 1);
        assert_eq!(report.runs.no_terminal, 1);
        assert_eq!(report.run_duration.n(), 1, "completed only feeds the prior");
        assert_eq!(report.run_duration, Prior::LastRun { value: 100.0 });
    }

    #[test]
    fn fake_zero_is_dead_all_unpriced_window_has_no_run_cost() {
        let a = sample_from(&run_body("wf", None, 0, &[], Some(done(10, None))));
        let b = sample_from(&run_body("wf", None, 20, &[], Some(done(30, None))));
        let report = compute(&id(), &gathered(vec![b, a]));
        assert_eq!(report.run_cost, None, "no priced terminal → no cost prior");
    }

    #[test]
    fn run_cost_is_quantile_of_totals_never_task_sums() {
        // Two runs: totals 110 and 110 — while per-task costs are
        // 100+10 and 60+50. The prior speaks TOTALS.
        let r1 = sample_from(&run_body(
            "wf",
            None,
            0,
            &[
                task_done("a", 5, 10, Some(100.0), None),
                task_done("b", 8, 10, Some(10.0), None),
            ],
            Some(done(10, Some(110.0))),
        ));
        let r2 = sample_from(&run_body(
            "wf",
            None,
            20,
            &[
                task_done("a", 25, 10, Some(60.0), None),
                task_done("b", 28, 10, Some(50.0), None),
            ],
            Some(done(30, Some(110.0))),
        ));
        let report = compute(&id(), &gathered(vec![r2, r1]));
        let Some(Prior::Range { min, max, .. }) = report.run_cost else {
            unreachable!("two priced totals earn a range, got {:?}", report.run_cost)
        };
        assert!((min - 110.0).abs() < 1e-9 && (max - 110.0).abs() < 1e-9);
    }

    #[test]
    fn r_model_scopes_bands_to_the_newest_model() {
        // Newest run on mock/echo · older run on other/model: the older
        // occurrence is counted out, the band n shrinks to 1.
        let newest = sample_from(&run_body(
            "wf",
            None,
            100,
            &[task_done("t", 105, 40, Some(0.02), Some("mock/echo"))],
            Some(done(110, Some(0.02))),
        ));
        let older = sample_from(&run_body(
            "wf",
            None,
            0,
            &[task_done("t", 5, 400, Some(0.2), Some("other/model"))],
            Some(done(10, Some(0.2))),
        ));
        let report = compute(&id(), &gathered(vec![newest, older]));
        let t = &report.tasks[0];
        assert_eq!(t.other_model, 1);
        assert_eq!(t.duration_ms, Prior::LastRun { value: 40.0 });
        assert_eq!(t.ran_in_runs, 2);
    }

    #[test]
    fn retry_split_and_cache_exclusion_hold_at_compute_level() {
        let flaky_run = sample_from(&run_body(
            "wf",
            None,
            0,
            &[
                ev(EventKind::TaskRetrying, 2, &[("task", Value::string("t"))]),
                task_done("t", 5, 30, None, None),
            ],
            Some(done(10, None)),
        ));
        let cached_run = {
            let mut s = sample_from(&run_body("wf", None, 20, &[], Some(done(30, None))));
            s.tasks.push(TaskSample {
                id: "t".into(),
                state: TaskState::Ok,
                cached: true,
                model: None,
                duration_ms: Some(0),
                cost_usd: None,
                retried: false,
                unpriced: None,
            });
            s
        };
        let report = compute(&id(), &gathered(vec![cached_run, flaky_run]));
        let t = &report.tasks[0];
        assert_eq!(t.passed_on_retry, 1, "C5: flaky, not failed");
        assert_eq!(t.failed, 0);
        assert_eq!(t.cache_hits, 1);
        assert_eq!(t.duration_ms.n(), 1, "the cache hit fed no band");
    }

    #[test]
    fn nonfinite_costs_are_dropped_and_counted() {
        let mut s = sample_from(&run_body("wf", None, 0, &[], Some(done(10, None))));
        s.total_cost_usd = Some(f64::NAN);
        let report = compute(&id(), &gathered(vec![s]));
        assert_eq!(report.run_cost, None);
        assert_eq!(report.window.nonfinite_dropped, 1);
    }
}
