// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Trace retention (ADR-100) — bounded by default · never silently ·
//! never a pending gate.
//!
//! The policy (D1 · engine-enforced defaults): per workflow keep the
//! last N finished-run traces (`10`) · a 30-day age cap · a 256MB
//! project size budget. Two ABSOLUTE exemptions no cap overrides: a
//! `paused` trace (an unanswered gate is an obligation, not garbage)
//! and the newest trace of each workflow (the standing resume
//! candidate).
//!
//! Collection is FAIL-OPEN end to end: a broken GC must never block a
//! run. But a SUCCESSFUL collection speaks exactly ONE stderr line —
//! silent deletion is forbidden (the anti-hidden-magic invariant); the
//! CLI renders that line from [`GcReport`]'s counts.
//!
//! Descended from `nika-cli`'s `verbs/trace/retention` (2026-07-09 ·
//! the W0 trace descent); the CLI keeps the line render and the
//! `nika run` hook, and re-exports this seam at the old path.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::store::{self, TraceMeta, TraceState};

/// The `traces.keep` rung, file-side (D-2026-08-11-N5) — discovery
/// from `start`, mapped onto the SAME age-cap semantics the env var
/// carries. Fail-open by note, never by silence: a project file that
/// will not parse is SAID (the note lane `doctor` prints) and the
/// knob stays at its default — a broken GC never blocks a run.
fn project_keep(start: &Path, notes: &mut Vec<String>) -> Option<Duration> {
    match nika_vocab::project::discover(start) {
        Ok(Some((_path, project))) => project.traces.map(|t| t.keep),
        Ok(None) => None,
        Err(e) => {
            notes.push(format!("{e} — the retention knobs ignore the project file"));
            None
        }
    }
}

/// The three knobs (D4) — engine defaults per the ADR-100 contract,
/// overridable via `NIKA_TRACE_KEEP` · `NIKA_TRACE_MAX_AGE_DAYS` ·
/// `NIKA_TRACE_BUDGET_MB` (the same env family as every other engine
/// knob · `nika doctor` reports the active values). Below the env
/// rung sits the project's `nika.yaml` `traces.keep` (D-2026-08-11-N5)
/// — `max_age` is its one knob; each env var wins its own.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RetentionConfig {
    /// Keep the last N finished-run traces per workflow (default 10).
    pub keep_last: usize,
    /// A trace older than this exits even under N (default 30d).
    pub max_age: Duration,
    /// The project-total hard stop (default 256MB · decimal).
    pub budget_bytes: u64,
}

/// Default keep-last-N (D1).
const DEFAULT_KEEP: usize = 10;
/// Default age cap in days (D1).
const DEFAULT_AGE_DAYS: u64 = 30;
/// Default project budget in MB (D1 · decimal — the same unit the
/// CLI's bytes vocabulary renders).
const DEFAULT_BUDGET_MB: u64 = 256;

impl Default for RetentionConfig {
    fn default() -> Self {
        Self::new(
            DEFAULT_KEEP,
            Duration::from_secs(DEFAULT_AGE_DAYS * 86_400),
            DEFAULT_BUDGET_MB * 1_000_000,
        )
    }
}

impl RetentionConfig {
    /// Assemble a config (invariant #19: every `#[non_exhaustive]`
    /// struct constructs through `new`, never a literal).
    #[must_use]
    pub const fn new(keep_last: usize, max_age: Duration, budget_bytes: u64) -> Self {
        Self {
            keep_last,
            max_age,
            budget_bytes,
        }
    }

    /// Resolve the three knobs from their raw env values — PURE, so the
    /// policy is testable without touching the process environment. A
    /// value that will not parse falls back to its default AND yields a
    /// note (`doctor` prints it — a typo'd knob silently doing nothing
    /// would be hidden magic).
    #[must_use]
    pub fn resolve(
        keep: Option<&str>,
        age_days: Option<&str>,
        budget_mb: Option<&str>,
    ) -> (Self, Vec<String>) {
        let mut cfg = Self::default();
        let mut notes = Vec::new();
        let mut read = |name: &str, raw: Option<&str>, apply: &mut dyn FnMut(u64)| {
            let Some(raw) = raw else { return };
            match raw.trim().parse::<u64>() {
                Ok(n) => apply(n),
                Err(_) => notes.push(format!("{name}={raw} is not a number — using the default")),
            }
        };
        read("NIKA_TRACE_KEEP", keep, &mut |n| {
            cfg.keep_last = usize::try_from(n).unwrap_or(usize::MAX);
        });
        read("NIKA_TRACE_MAX_AGE_DAYS", age_days, &mut |n| {
            cfg.max_age = Duration::from_secs(n.saturating_mul(86_400));
        });
        read("NIKA_TRACE_BUDGET_MB", budget_mb, &mut |n| {
            cfg.budget_bytes = n.saturating_mul(1_000_000);
        });
        (cfg, notes)
    }

    /// The one-line active-values summary `nika doctor` reports (D4):
    /// `keep 10 per workflow · max age 30d · budget 256MB`. Units render
    /// exactly as the knobs speak them (whole days · decimal MB), so the
    /// report round-trips what the operator set.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "keep {} per workflow · max age {}d · budget {}MB",
            self.keep_last,
            self.max_age.as_secs() / 86_400,
            self.budget_bytes / 1_000_000,
        )
    }

    /// The knob ladder, assembled (D-2026-08-11-N5): each env var wins
    /// its knob · an UNSET `max_age` falls to the project's `nika.yaml`
    /// `traces.keep` · last resort the built-in defaults. A project
    /// file that will not read or parse yields a NOTE, not a refusal —
    /// the fail-open law outranks it at THIS seam (a broken GC never
    /// blocks a run); the same error closes the budget/registry gates.
    ///
    /// `start` is the discovery root (the CWD in production · a
    /// tempdir under test) — the walk-up law lives in
    /// [`nika_vocab::project::discover`].
    #[must_use]
    pub fn layered_at(
        keep: Option<&str>,
        age_days: Option<&str>,
        budget_mb: Option<&str>,
        start: &Path,
    ) -> (Self, Vec<String>) {
        let (mut cfg, mut notes) = Self::resolve(keep, age_days, budget_mb);
        if age_days.is_none()
            && let Some(keep_for) = project_keep(start, &mut notes)
        {
            cfg.max_age = keep_for;
        }
        (cfg, notes)
    }

    /// Read the knobs from the environment (the `NIKA_TRACE_*` family),
    /// falling back to the project file's `traces.keep` for any knob
    /// the env leaves unset (D-2026-08-11-N5 — the ladder per key:
    /// env var · project file · built-in default).
    ///
    /// Config knobs are presentation/behavior state, not secrets — the
    /// same scoped `std::env` exemption `NIKA_REDUCED_MOTION` and
    /// `env_flag` carry (the workspace ban routes SECRET reads through
    /// the kernel vault seam, which has no business in a GC knob).
    #[allow(clippy::disallowed_methods)]
    #[must_use]
    pub fn from_env() -> (Self, Vec<String>) {
        let get = |name: &str| std::env::var(name).ok();
        let (keep, age, budget) = (
            get("NIKA_TRACE_KEEP"),
            get("NIKA_TRACE_MAX_AGE_DAYS"),
            get("NIKA_TRACE_BUDGET_MB"),
        );
        // The CWD is the discovery root (the git-style walk-up); a
        // current_dir failure degrades to the env leg alone.
        let start = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self::layered_at(keep.as_deref(), age.as_deref(), budget.as_deref(), &start)
    }
}

/// Why one trace is collected — each removal carries exactly ONE reason
/// (the D2 line groups them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Reason {
    /// Beyond the keep-last-N observability window of its workflow.
    Rotated,
    /// Older than the age cap.
    Aged,
    /// Removed oldest-first to fit the project size budget.
    OverBudget,
}

impl Reason {
    /// The word the D2 line groups removals under.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rotated => "rotated",
            Self::Aged => "aged",
            Self::OverBudget => "over-budget",
        }
    }
}

/// The pure policy (D1): which traces to collect, and why.
///
/// Exemptions are ABSOLUTE — a `paused` trace and the newest trace of
/// each workflow are never marked, regardless of any cap. Rotation
/// applies to finished runs only (completed · failed · cancelled); the
/// age cap and the budget bound everything eligible (so a torn trace
/// ages out, while a live run's fresh journal is naturally safe).
#[must_use]
pub fn plan(traces: &[TraceMeta], cfg: &RetentionConfig, now: SystemTime) -> Vec<(usize, Reason)> {
    let newest = newest_per_workflow(traces);
    let exempt = |i: usize| traces[i].state == TraceState::Paused || newest.contains(&i);
    let mut marked: BTreeMap<usize, Reason> = BTreeMap::new();

    // Rotation — keep the last N finished traces per workflow.
    for indices in finished_by_workflow(traces).values() {
        for &i in indices.iter().skip(cfg.keep_last) {
            if !exempt(i) {
                marked.insert(i, Reason::Rotated);
            }
        }
    }

    // Age cap — stale traces exit even under N.
    for (i, trace) in traces.iter().enumerate() {
        if exempt(i) || marked.contains_key(&i) {
            continue;
        }
        let age = now.duration_since(trace.modified).unwrap_or_default();
        if age > cfg.max_age {
            marked.insert(i, Reason::Aged);
        }
    }

    // Budget — remove oldest-first until the project total fits. Exempt
    // traces count toward usage but are never collected (the budget may
    // stay unmet by design — fixture: the newest survives even alone
    // over budget).
    let mut total: u64 = traces
        .iter()
        .enumerate()
        .filter(|(i, _)| !marked.contains_key(i))
        .map(|(_, t)| t.bytes)
        .sum();
    if total > cfg.budget_bytes {
        for i in oldest_first(traces) {
            if total <= cfg.budget_bytes {
                break;
            }
            if exempt(i) || marked.contains_key(&i) {
                continue;
            }
            total = total.saturating_sub(traces[i].bytes);
            marked.insert(i, Reason::OverBudget);
        }
    }

    marked.into_iter().collect()
}

/// The newest trace of each workflow (ANY state) — the standing resume
/// candidate. Tie-break by name so the pick is deterministic. Shared
/// with `trace ls` (the ★ marker paints exactly this set).
#[must_use]
pub fn newest_per_workflow(traces: &[TraceMeta]) -> BTreeSet<usize> {
    let mut newest: BTreeMap<&str, usize> = BTreeMap::new();
    for (i, trace) in traces.iter().enumerate() {
        let slot = newest.entry(trace.workflow.as_str()).or_insert(i);
        let held = &traces[*slot];
        if (trace.modified, &trace.name) > (held.modified, &held.name) {
            *slot = i;
        }
    }
    newest.into_values().collect()
}

/// Finished traces per workflow, newest first (the rotation windows).
fn finished_by_workflow(traces: &[TraceMeta]) -> BTreeMap<&str, Vec<usize>> {
    let mut by_wf: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, trace) in traces.iter().enumerate() {
        if trace.state.is_finished() {
            by_wf.entry(trace.workflow.as_str()).or_default().push(i);
        }
    }
    for indices in by_wf.values_mut() {
        indices.sort_by(|&a, &b| {
            (traces[b].modified, &traces[b].name).cmp(&(traces[a].modified, &traces[a].name))
        });
    }
    by_wf
}

/// Every index, oldest first (the budget's removal order).
fn oldest_first(traces: &[TraceMeta]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..traces.len()).collect();
    order.sort_by(|&a, &b| {
        (traces[a].modified, &traces[a].name).cmp(&(traces[b].modified, &traces[b].name))
    });
    order
}

/// One collection's outcome — the D2 line's inputs (the CLI renders
/// the exactly-one stderr line from these counts; the policy never
/// speaks a display vocabulary itself).
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct GcReport {
    /// Removed beyond the keep-last-N window.
    pub rotated: usize,
    /// Removed past the age cap.
    pub aged: usize,
    /// Removed oldest-first to fit the budget.
    pub over_budget: usize,
    /// Traces left standing after the pass.
    pub kept: usize,
    /// Bytes left standing after the pass.
    pub kept_bytes: u64,
}

impl GcReport {
    /// Assemble a report (invariant #19).
    #[must_use]
    pub const fn new(
        rotated: usize,
        aged: usize,
        over_budget: usize,
        kept: usize,
        kept_bytes: u64,
    ) -> Self {
        Self {
            rotated,
            aged,
            over_budget,
            kept,
            kept_bytes,
        }
    }
}

/// Scan → plan → remove (D1+D2). `None` when nothing was removed (a
/// quiet dir stays quiet); `Some(report)` when the collection removed
/// at least one trace — the caller MUST speak its line.
///
/// Fail-open per file: a removal that errors (a vanished file · a
/// permission wall) leaves that trace counted as kept and continues —
/// never an error, never a veto on the run.
#[must_use = "a successful collection MUST speak its line (D2) — dropping the report is silent deletion"]
pub fn collect(dir: &Path, cfg: &RetentionConfig, now: SystemTime) -> Option<GcReport> {
    let traces = store::scan(dir);
    let marked = plan(&traces, cfg, now);
    let mut removed: BTreeMap<usize, Reason> = BTreeMap::new();
    for (i, reason) in marked {
        if std::fs::remove_file(&traces[i].path).is_ok() {
            removed.insert(i, reason);
        }
    }
    if removed.is_empty() {
        return None;
    }
    let count = |r: Reason| removed.values().filter(|&&v| v == r).count();
    let kept_bytes = traces
        .iter()
        .enumerate()
        .filter(|(i, _)| !removed.contains_key(i))
        .map(|(_, t)| t.bytes)
        .sum();
    Some(GcReport::new(
        count(Reason::Rotated),
        count(Reason::Aged),
        count(Reason::OverBudget),
        traces.len() - removed.len(),
        kept_bytes,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::{ndjson, run_events, stage_trace, temp_store};
    use nika_event::EventKind;
    use std::path::PathBuf;

    /// A synthetic trace fact for the PURE policy tests (no fs).
    fn meta(name: &str, workflow: &str, state: TraceState, bytes: u64, age_s: u64) -> TraceMeta {
        TraceMeta::new(
            PathBuf::from(name),
            name.to_owned(),
            workflow.to_owned(),
            state,
            (state == TraceState::Paused).then(|| "gate".to_owned()),
            bytes,
            now() - Duration::from_secs(age_s),
        )
    }

    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_780_000_000)
    }

    const DAY: u64 = 86_400;

    /// ADR-100 conformance fixture 1 · **paused-survives**: a paused
    /// trace older than EVERY cap survives the plan — an unanswered
    /// gate is an obligation, not garbage.
    #[test]
    fn fixture_paused_survives_every_cap() {
        let cfg = RetentionConfig::new(0, Duration::from_secs(DAY), 1);
        let traces = vec![
            meta(
                "paused.ndjson",
                "gatey",
                TraceState::Paused,
                5_000,
                90 * DAY,
            ),
            // A finished sibling proves collection DOES happen around it.
            meta(
                "done.ndjson",
                "gatey",
                TraceState::Completed,
                5_000,
                80 * DAY,
            ),
            meta(
                "newer.ndjson",
                "gatey",
                TraceState::Completed,
                5_000,
                70 * DAY,
            ),
        ];
        let marked = plan(&traces, &cfg, now());
        assert!(
            marked.iter().all(|&(i, _)| i != 0),
            "the paused trace is never marked: {marked:?}"
        );
        // `newer` (index 2) is the workflow's newest → also exempt; the
        // older finished sibling collects (keep 0 · aged · over budget).
        assert!(marked.iter().any(|&(i, _)| i == 1), "{marked:?}");
        assert!(marked.iter().all(|&(i, _)| i != 2), "{marked:?}");
    }

    /// ADR-100 conformance fixture 2 · **resume-candidate-survives**:
    /// the newest trace of a workflow survives even when it is ALONE
    /// over the whole project budget.
    #[test]
    fn fixture_resume_candidate_survives_alone_over_budget() {
        let cfg = RetentionConfig::new(0, Duration::from_secs(DAY), 100);
        let traces = vec![meta(
            "only.ndjson",
            "veille",
            TraceState::Completed,
            999_999,
            60 * DAY,
        )];
        assert!(
            plan(&traces, &cfg, now()).is_empty(),
            "the standing resume candidate survives every cap"
        );
    }

    /// ADR-100 conformance fixture 3 · **visible-collection**: an
    /// over-N history collects OLDEST-FIRST and the report carries the
    /// counts the D2 line speaks.
    #[test]
    fn fixture_visible_collection_rotates_oldest_first() {
        let dir = temp_store("rotate");
        let body = ndjson(&run_events("veille", Some(EventKind::WorkflowCompleted)));
        for i in 0..12u64 {
            // i=0 is the newest · i=11 the oldest.
            stage_trace(
                &dir,
                &format!("run-{i:02}.ndjson"),
                &body,
                Duration::from_secs(i * 3_600),
            );
        }
        let report = collect(&dir, &RetentionConfig::default(), SystemTime::now())
            .expect("an over-N history collects");
        assert_eq!(
            report,
            GcReport::new(2, 0, 0, 10, 10 * body.len() as u64),
            "two rotated · ten kept · the kept bytes are real"
        );
        // Oldest-first: run-11 + run-10 collected · the other ten stay.
        assert!(!dir.join("run-11.ndjson").exists(), "oldest collected");
        assert!(
            !dir.join("run-10.ndjson").exists(),
            "second-oldest collected"
        );
        for i in 0..10u64 {
            assert!(dir.join(format!("run-{i:02}.ndjson")).exists(), "run-{i}");
        }
        // Idempotent: a second pass finds nothing → silence (None).
        assert!(
            collect(&dir, &RetentionConfig::default(), SystemTime::now()).is_none(),
            "a quiet dir stays quiet"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The age cap fires under N; each removal carries ONE reason.
    #[test]
    fn age_cap_fires_under_n() {
        let cfg = RetentionConfig::default();
        let traces = vec![
            meta("new.ndjson", "w", TraceState::Completed, 100, 3_600),
            meta("stale.ndjson", "w", TraceState::Failed, 100, 45 * DAY),
        ];
        let marked = plan(&traces, &cfg, now());
        assert_eq!(marked, vec![(1, Reason::Aged)], "aged even under keep-10");
    }

    /// The budget removes OLDEST-first across workflows and stops as
    /// soon as the total fits.
    #[test]
    fn budget_removes_oldest_first_until_it_fits() {
        let cfg = RetentionConfig::new(10, Duration::from_secs(365 * DAY), 250);
        let traces = vec![
            meta("a-new.ndjson", "a", TraceState::Completed, 100, DAY),
            meta("b-new.ndjson", "b", TraceState::Completed, 100, 2 * DAY),
            meta("a-old.ndjson", "a", TraceState::Completed, 100, 9 * DAY),
            meta("b-old.ndjson", "b", TraceState::Completed, 100, 8 * DAY),
        ];
        let marked = plan(&traces, &cfg, now());
        // 400 total → remove oldest eligible (a-old · 9d) → 300 → then
        // b-old (8d) → 200 ≤ 250 → stop. The two newest stay.
        assert_eq!(
            marked,
            vec![(2, Reason::OverBudget), (3, Reason::OverBudget)]
        );
    }

    /// Rotation windows are PER WORKFLOW — eleven runs of `a` never
    /// evict `b`'s only history.
    #[test]
    fn rotation_windows_are_per_workflow() {
        let cfg = RetentionConfig {
            keep_last: 2,
            ..RetentionConfig::default()
        };
        let mut traces: Vec<TraceMeta> = (0..4u64)
            .map(|i| {
                meta(
                    &format!("a-{i}.ndjson"),
                    "a",
                    TraceState::Completed,
                    10,
                    i * 3_600,
                )
            })
            .collect();
        traces.push(meta("b-0.ndjson", "b", TraceState::Completed, 10, 9 * DAY));
        let marked = plan(&traces, &cfg, now());
        assert_eq!(
            marked,
            vec![(2, Reason::Rotated), (3, Reason::Rotated)],
            "a keeps its newest 2 · b's only trace is untouchable (newest)"
        );
    }

    /// A running trace (no terminal event) is exempt from rotation but
    /// the age cap still bounds a torn one.
    #[test]
    fn running_traces_never_rotate_but_do_age_out() {
        let cfg = RetentionConfig {
            keep_last: 0,
            ..RetentionConfig::default()
        };
        let traces = vec![
            meta("head.ndjson", "w", TraceState::Completed, 10, 600),
            meta("live.ndjson", "w", TraceState::Running, 10, 1_200),
            meta("torn.ndjson", "w", TraceState::Running, 10, 90 * DAY),
        ];
        let marked = plan(&traces, &cfg, now());
        assert!(
            marked.iter().all(|&(i, _)| i != 1),
            "a fresh in-flight journal is safe: {marked:?}"
        );
        assert!(
            marked.contains(&(2, Reason::Aged)),
            "a torn trace ages out: {marked:?}"
        );
    }

    /// Config resolution: defaults · valid overrides · a typo'd value
    /// falls back LOUDLY (a note doctor prints — never silent).
    #[test]
    fn config_resolves_defaults_overrides_and_notes() {
        let (cfg, notes) = RetentionConfig::resolve(None, None, None);
        assert_eq!(cfg, RetentionConfig::default());
        assert!(notes.is_empty());
        assert_eq!(cfg.keep_last, 10);
        assert_eq!(cfg.max_age, Duration::from_secs(30 * DAY));
        assert_eq!(cfg.budget_bytes, 256_000_000);

        let (cfg, notes) = RetentionConfig::resolve(Some("5"), Some("7"), Some("64"));
        assert_eq!(cfg.keep_last, 5);
        assert_eq!(cfg.max_age, Duration::from_secs(7 * DAY));
        assert_eq!(cfg.budget_bytes, 64_000_000);
        assert!(notes.is_empty());
        // The doctor line (D4) round-trips the knob units exactly.
        assert_eq!(
            cfg.summary(),
            "keep 5 per workflow · max age 7d · budget 64MB"
        );
        assert_eq!(
            RetentionConfig::default().summary(),
            "keep 10 per workflow · max age 30d · budget 256MB"
        );

        let (cfg, notes) = RetentionConfig::resolve(Some("ten"), None, None);
        assert_eq!(cfg, RetentionConfig::default(), "typo → default");
        assert_eq!(
            notes,
            vec!["NIKA_TRACE_KEEP=ten is not a number — using the default"]
        );
    }

    /// The project-file rung (D-2026-08-11-N5): an unset
    /// `NIKA_TRACE_MAX_AGE_DAYS` falls to `nika.yaml` `traces.keep`;
    /// a SET env var wins its knob, file or no file.
    #[test]
    fn the_project_file_fills_only_the_knob_the_env_leaves_unset() {
        let dir = std::env::temp_dir().join(format!("nika-retproj-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("nika.yaml"), "nika: v1\ntraces:\n  keep: 45d\n").expect("seed");

        // Env unset → the file rung governs the age cap.
        let (cfg, notes) = RetentionConfig::layered_at(None, None, None, &dir);
        assert_eq!(cfg.max_age, Duration::from_secs(45 * DAY), "the file fills");
        assert_eq!(cfg.keep_last, 10, "the other knobs stay built-in");
        assert!(notes.is_empty(), "a clean file is silent: {notes:?}");

        // Env set → the env var wins, the file is not even consulted.
        let (cfg, notes) = RetentionConfig::layered_at(None, Some("7"), None, &dir);
        assert_eq!(cfg.max_age, Duration::from_secs(7 * DAY), "env beats file");
        assert!(notes.is_empty(), "{notes:?}");

        // Absent file → the built-in default, zero ceremony.
        let empty = std::env::temp_dir().join(format!("nika-retnone-{}", std::process::id()));
        std::fs::remove_dir_all(&empty).ok();
        std::fs::create_dir_all(&empty).expect("mkdir");
        let (cfg, notes) = RetentionConfig::layered_at(None, None, None, &empty);
        assert_eq!(cfg, RetentionConfig::default());
        assert!(notes.is_empty(), "{notes:?}");

        // A BROKEN file → the default PLUS a loud note (fail-open,
        // never silent — the same error closes the budget gate).
        std::fs::write(dir.join("nika.yaml"), "nika: v1\ntraces:\n  keep: soon\n").expect("bad");
        let (cfg, notes) = RetentionConfig::layered_at(None, None, None, &dir);
        assert_eq!(
            cfg.max_age,
            Duration::from_secs(30 * DAY),
            "fail-open default"
        );
        assert_eq!(notes.len(), 1, "one note, said: {notes:?}");
        assert!(
            notes[0].contains("retention"),
            "the lane is named: {}",
            notes[0]
        );
        assert!(
            notes[0].contains("project.bad-value"),
            "the slug: {}",
            notes[0]
        );

        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_dir_all(&empty).ok();
    }

    /// Fail-open through collect: a garbage file older than every cap is
    /// INVISIBLE to the collector (skip that file + continue) — it stays
    /// on disk and never blocks the pass.
    #[test]
    fn collect_skips_unreadable_files_and_continues() {
        let dir = temp_store("failopen-collect");
        stage_trace(
            &dir,
            "junk.ndjson",
            "{not json\n",
            Duration::from_secs(365 * DAY),
        );
        let body = ndjson(&run_events("w", Some(EventKind::WorkflowCompleted)));
        stage_trace(&dir, "old.ndjson", &body, Duration::from_secs(60 * DAY));
        stage_trace(&dir, "new.ndjson", &body, Duration::from_secs(60));
        let report = collect(&dir, &RetentionConfig::default(), SystemTime::now())
            .expect("the aged readable trace collects");
        assert_eq!(report.aged, 1, "one aged removal: {report:?}");
        assert!(dir.join("junk.ndjson").exists(), "unreadable → untouched");
        assert!(!dir.join("old.ndjson").exists(), "aged → collected");
        assert!(dir.join("new.ndjson").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
