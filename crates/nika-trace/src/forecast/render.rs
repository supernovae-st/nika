// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The forecast section — learned truth rendered honestly: the rung
//! decides the vocabulary (never a percentile under n=5), `≥` composes
//! whenever unpriced spend participates, and every exclusion is named.
//! Plain text like the sibling explain sections (theming is R3);
//! durations speak `fmt_wall_ms`, money speaks `fmt_cost_usd`.

use std::fmt::Write as _;

use crate::display::flow::fmt_wall_ms;
use crate::display::format::fmt_cost_usd;

use super::{ForecastReport, TaskPrior};
use nika_dap::stats::Prior;

/// Low-confidence tag threshold: p90 ≈ max until n > 10 (1/(1−p)).
const LOW_CONFIDENCE_N: usize = 10;

/// The whole FORECAST section (the composer — one honest block).
pub fn forecast_section(s: &mut String, report: &ForecastReport) {
    let _ = writeln!(s);
    if report.runs.total == 0 {
        let _ = writeln!(
            s,
            "FORECAST · no forecast — this workflow has never run here"
        );
        return;
    }
    let _ = writeln!(s, "FORECAST · {}", window_line(report));
    let width = report
        .tasks
        .iter()
        .map(|t| t.id.len())
        .max()
        .unwrap_or(4)
        .max(4);
    for task in &report.tasks {
        let _ = writeln!(s, "{}", task_row(task, width, report.runs.total));
    }
    let _ = writeln!(s, "  {}", "─".repeat(width + 34));
    let _ = writeln!(s, "{}", run_line(report, width));
    let _ = writeln!(s, "  {}", hedge_line());
}

/// The « based on … » header tail: population, window bounds, identity
/// caveats, retention note, confidence tag — information, never an
/// error tone.
fn window_line(report: &ForecastReport) -> String {
    let runs = &report.runs;
    let w = &report.window;
    let mut line = format!(
        "based on last {} run{} (window {} runs / {} days",
        runs.total,
        if runs.total == 1 { "" } else { "s" },
        w.k,
        w.days
    );
    if runs.stale_hash > 0 {
        let _ = write!(line, " · {} predate the last edit", runs.stale_hash);
    }
    if runs.unknown_hash > 0 {
        let _ = write!(line, " · {} unverifiable", runs.unknown_hash);
    }
    // `>=`, never `==`: GC rides `nika run` START, so right after a run
    // the dir holds keep+1 matching traces — the saturated-window case
    // is the COMMON shape, not an exact hit (e2e-proven: 12 runs → 11
    // seen · keep 10 · the annotation must still fire).
    if w.retention_keep > 0 && runs.total >= w.retention_keep {
        let _ = write!(line, " · trace retention keeps last {}", w.retention_keep);
    }
    if w.files_unreadable > 0 {
        let _ = write!(line, " · {} unreadable skipped", w.files_unreadable);
    }
    if w.files_truncated > 0 {
        let _ = write!(
            line,
            " · {} torn tail{} trimmed",
            w.files_truncated,
            if w.files_truncated == 1 { "" } else { "s" }
        );
    }
    if w.nonfinite_dropped > 0 {
        let _ = write!(line, " · {} non-finite dropped", w.nonfinite_dropped);
    }
    line.push(')');
    if report.runs.total < LOW_CONFIDENCE_N {
        let _ = write!(line, " · low confidence (n<{LOW_CONFIDENCE_N})");
    }
    line
}

/// One task line: duration prior · cost prior · the risk notes.
fn task_row(task: &TaskPrior, width: usize, total_runs: usize) -> String {
    // A task that RAN but never completed (failures · torn attempts ·
    // cache-only) has no duration sample — that is a dash, NEVER the
    // « never ran » words (it visibly ran; the failed count sits beside).
    let duration = if matches!(task.duration_ms, Prior::NeverRan) && task.ran_in_runs > 0 {
        "—".to_owned()
    } else {
        prior_cell(&task.duration_ms)
    };
    let mut line = format!(
        "  {:width$}  {:22} {:10}",
        task.id,
        duration,
        cost_cell(task.cost_usd.as_ref(), task.unpriced.is_some()),
    );
    let mut notes: Vec<String> = Vec::new();
    // The when-gate hedge made visible (C6): the task executed in fewer
    // runs than the window holds — say so, in run units.
    if task.ran_in_runs < total_runs {
        notes.push(format!("ran in {}/{} runs", task.ran_in_runs, total_runs));
    }
    if task.failed > 0 {
        notes.push(format!("failed {}/{}", task.failed, task.ran_in_runs));
    }
    if task.passed_on_retry > 0 {
        notes.push(format!(
            "passed on retry {}/{} ⚠",
            task.passed_on_retry, task.ran_in_runs
        ));
    }
    if let Some(slug) = &task.unpriced {
        notes.push(format!("unpriced: {slug}"));
    }
    if task.cache_hits > 0 {
        notes.push(format!(
            "{} cache hit{}",
            task.cache_hits,
            if task.cache_hits == 1 { "" } else { "s" }
        ));
    }
    if task.other_model > 0 {
        notes.push(format!(
            "{} run{} on other models excluded",
            task.other_model,
            if task.other_model == 1 { "" } else { "s" }
        ));
    }
    if !notes.is_empty() {
        let _ = write!(line, " {}", notes.join(" · "));
    }
    line.trim_end().to_owned()
}

/// The whole-run line: elapsed prior over COMPLETED runs · the totals
/// prior · what was excluded, named.
fn run_line(report: &ForecastReport, width: usize) -> String {
    let any_unpriced = report.tasks.iter().any(|t| t.unpriced.is_some());
    let dn = report.run_duration.n();
    // Same dash law as the task rows: runs happened but none completed →
    // no duration sample, never « never ran » beside a real census.
    let duration = if matches!(report.run_duration, Prior::NeverRan) && report.runs.total > 0 {
        "—".to_owned()
    } else {
        prior_cell(&report.run_duration)
    };
    let mut line = format!(
        "  {:width$}  {:22} {:10}",
        "run",
        duration,
        cost_cell(report.run_cost.as_ref(), any_unpriced),
    );
    let mut notes: Vec<String> = Vec::new();
    // Including dn == 0: « duration from 0 completed » names why the
    // cell is a dash — the count only stays silent when EVERY run fed it.
    if dn != report.runs.total {
        notes.push(format!("duration from {dn} completed"));
    }
    if report.runs.failed > 0 {
        notes.push(format!(
            "failed {}/{}",
            report.runs.failed, report.runs.total
        ));
    }
    if report.runs.paused > 0 {
        notes.push(format!(
            "{} paused run{} excluded",
            report.runs.paused,
            if report.runs.paused == 1 { "" } else { "s" }
        ));
    }
    if report.runs.no_terminal > 0 {
        notes.push(format!("{} unfinished excluded", report.runs.no_terminal));
    }
    if !notes.is_empty() {
        let _ = write!(line, " · {}", notes.join(" · "));
    }
    line.trim_end().to_owned()
}

/// The Argo-shaped honesty hedge — printed beside the numbers, always.
const fn hedge_line() -> &'static str {
    "estimates vary with `when` branches, inputs, and provider latency"
}

/// A duration prior cell — the rung decides the vocabulary. The ladder
/// lives in the forensics crate now (`#[non_exhaustive]` — the same
/// tolerate-unknown-kinds law the JSON twin teaches): a NEWER rung this
/// renderer cannot speak renders as its honest sample count.
fn prior_cell(prior: &Prior) -> String {
    match *prior {
        Prior::NeverRan => "never ran".to_owned(),
        Prior::LastRun { value } => format!("last run {}", fmt_wall_ms(to_ms(value))),
        Prior::Range { min, max, .. } => {
            format!("{}–{}", fmt_wall_ms(to_ms(min)), fmt_wall_ms(to_ms(max)))
        }
        Prior::Bands { p50, p90, .. } => {
            format!(
                "~{} (p90 {})",
                fmt_wall_ms(to_ms(p50)),
                fmt_wall_ms(to_ms(p90))
            )
        }
        _ => format!("learned (n={})", prior.n()),
    }
}

/// A cost prior cell — `≥` composes whenever unpriced spend
/// participates; no prior AND no unpriced is an honest dash, never $0.
fn cost_cell(prior: Option<&Prior>, unpriced: bool) -> String {
    let floor = if unpriced { "≥ " } else { "" };
    match prior {
        None => {
            if unpriced {
                "≥ —".to_owned()
            } else {
                "—".to_owned()
            }
        }
        Some(Prior::NeverRan) => "—".to_owned(),
        Some(Prior::LastRun { value }) => format!("{floor}{}", fmt_cost_usd(*value)),
        Some(Prior::Range { min, max, .. }) => {
            format!("{floor}{}–{}", fmt_cost_usd(*min), fmt_cost_usd(*max))
        }
        Some(Prior::Bands { p50, p90, .. }) => {
            format!(
                "{floor}~{} (p90 {})",
                fmt_cost_usd(*p50),
                fmt_cost_usd(*p90)
            )
        }
        // A NEWER rung this renderer cannot speak — honest floor only.
        Some(p) => format!("{floor}learned (n={})", p.n()),
    }
}

/// f64 milliseconds → u64 for the shared formatter. Values are wall
/// times ≪ 2^53, so the round-trip is exact; negatives clamp to zero.
fn to_ms(v: f64) -> u64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ms = v.round().max(0.0) as u64;
    ms
}

#[cfg(test)]
mod tests {
    use super::super::{RunCensus, Window};
    use super::*;

    fn report(total: usize) -> ForecastReport {
        ForecastReport {
            workflow: "wf".into(),
            runs: RunCensus {
                total,
                same_hash: total,
                completed: total,
                ..RunCensus::default()
            },
            window: Window::new(),
            run_duration: Prior::from_finite(
                #[allow(clippy::cast_precision_loss)] // test sizes ≪ 2^52
                &(0..total).map(|i| 1_000.0 + i as f64).collect::<Vec<_>>(),
            ),
            run_cost: None,
            tasks: Vec::new(),
        }
    }

    #[test]
    fn never_run_says_so_and_invents_nothing() {
        let mut s = String::new();
        forecast_section(&mut s, &report(0));
        assert!(s.contains("no forecast — this workflow has never run here"));
        assert!(!s.contains("p90"), "no percentile vocabulary at n=0");
    }

    #[test]
    fn one_run_speaks_last_run_never_percentiles() {
        let mut s = String::new();
        forecast_section(&mut s, &report(1));
        assert!(s.contains("based on last 1 run "));
        assert!(s.contains("last run 1.0s"));
        assert!(!s.contains("p90"));
        assert!(s.contains("low confidence (n<10)"));
    }

    #[test]
    fn three_runs_earn_a_range_five_earn_bands() {
        let mut s = String::new();
        forecast_section(&mut s, &report(3));
        assert!(s.contains('–'), "range dash present");
        assert!(!s.contains("p90"));
        let mut s5 = String::new();
        forecast_section(&mut s5, &report(5));
        assert!(s5.contains("(p90 "));
        assert!(s5.contains("~1.0s"));
    }

    #[test]
    fn floor_composes_and_absence_stays_a_dash() {
        assert_eq!(cost_cell(None, false), "—");
        assert_eq!(cost_cell(None, true), "≥ —");
        let one = Prior::LastRun { value: 0.021 };
        assert_eq!(cost_cell(Some(&one), true), "≥ $0.02");
        assert_eq!(cost_cell(Some(&one), false), "$0.02");
    }

    #[test]
    fn window_caveats_are_information_not_errors() {
        let mut r = report(7);
        r.runs.stale_hash = 2;
        r.runs.unknown_hash = 1;
        r.window.retention_keep = 7;
        r.window.files_truncated = 1;
        let mut s = String::new();
        forecast_section(&mut s, &r);
        assert!(s.contains("2 predate the last edit"));
        assert!(s.contains("1 unverifiable"));
        assert!(s.contains("trace retention keeps last 7"));
        assert!(s.contains("1 torn tail trimmed"));
        assert!(s.contains(hedge_line()));
    }

    #[test]
    fn paused_is_named_paused_never_a_crash_word() {
        let mut r = report(3);
        r.runs.paused = 1;
        let mut s = String::new();
        forecast_section(&mut s, &r);
        assert!(s.contains("1 paused run excluded"));
        assert!(!s.contains("incomplete"));
    }

    /// R-8 (e2e-proven): GC rides run START, so the post-run dir holds
    /// keep+1 — the saturation annotation fires at `>=`, never only `==`.
    #[test]
    fn retention_annotation_fires_above_the_keep_knob_too() {
        let mut r = report(11);
        r.window.retention_keep = 10;
        let mut s = String::new();
        forecast_section(&mut s, &r);
        assert!(
            s.contains("trace retention keeps last 10"),
            "total 11 ≥ keep 10 must name retention:\n{s}"
        );
    }

    /// R-2: a task that ran and only FAILED shows a dash — « never ran »
    /// beside « failed 2/2 » would be a contradiction.
    #[test]
    fn failed_only_task_is_a_dash_never_never_ran() {
        let mut r = report(2);
        r.tasks.push(TaskPrior {
            id: "flaky".into(),
            ran_in_runs: 2,
            failed: 2,
            passed_on_retry: 0,
            cache_hits: 0,
            other_model: 0,
            duration_ms: Prior::NeverRan,
            cost_usd: None,
            unpriced: None,
        });
        let mut s = String::new();
        forecast_section(&mut s, &r);
        assert!(!s.contains("never ran"), "{s}");
        assert!(s.contains("failed 2/2"), "{s}");
    }

    /// C6 made visible: a when-gated task names its own run count.
    #[test]
    fn when_gated_task_names_its_run_share() {
        let mut r = report(3);
        r.tasks.push(TaskPrior {
            id: "gated".into(),
            ran_in_runs: 1,
            failed: 0,
            passed_on_retry: 0,
            cache_hits: 0,
            other_model: 0,
            duration_ms: Prior::LastRun { value: 40.0 },
            cost_usd: None,
            unpriced: None,
        });
        let mut s = String::new();
        forecast_section(&mut s, &r);
        assert!(s.contains("ran in 1/3 runs"), "{s}");
    }

    /// R-2 at run level: a census with zero completions dashes the cell
    /// and NAMES the zero (« duration from 0 completed »).
    #[test]
    fn zero_completed_runs_dash_the_run_cell_and_say_why() {
        let mut r = report(2);
        r.runs.completed = 0;
        r.runs.failed = 2;
        r.run_duration = Prior::NeverRan;
        let mut s = String::new();
        forecast_section(&mut s, &r);
        assert!(!s.contains("never ran"), "{s}");
        assert!(s.contains("duration from 0 completed"), "{s}");
        assert!(s.contains("failed 2/2"), "{s}");
    }
}
