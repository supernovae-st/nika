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

use super::math::Prior;
use super::{ForecastReport, TaskPrior};

/// Low-confidence tag threshold: p90 ≈ max until n > 10 (1/(1−p)).
const LOW_CONFIDENCE_N: usize = 10;

/// The whole FORECAST section (the composer — one honest block).
pub(crate) fn forecast_section(s: &mut String, report: &ForecastReport) {
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
        let _ = writeln!(s, "{}", task_row(task, width));
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
    if w.retention_keep > 0 && runs.total == w.retention_keep {
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
fn task_row(task: &TaskPrior, width: usize) -> String {
    let mut line = format!(
        "  {:width$}  {:22} {:10}",
        task.id,
        prior_cell(&task.duration_ms),
        cost_cell(task.cost_usd.as_ref(), task.unpriced.is_some()),
    );
    let mut notes: Vec<String> = Vec::new();
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
    let mut line = format!(
        "  {:width$}  {:22} {:10}",
        "run",
        prior_cell(&report.run_duration),
        cost_cell(report.run_cost.as_ref(), any_unpriced),
    );
    let mut notes: Vec<String> = Vec::new();
    let dn = report.run_duration.n();
    if dn > 0 && dn != report.runs.total {
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

/// A duration prior cell — the rung decides the vocabulary (EXHAUSTIVE
/// match: this is OUR enum, four arms, never a `_`).
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
}
