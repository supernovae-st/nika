// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Execution-flow reads over the folded [`RunView`] — pure functions of
//! the event stream (the fold law), zero new instrumentation.
//!
//! The stream stamps task frames at SETTLE time while the runtime measures
//! the REAL wall duration per task (`duration_ms`), so each task's
//! interval is reconstructed as `[end − duration, end]`: real widths,
//! honest overlap. Everything here (lane markers · durations) derives
//! from that one reconstruction.

use crate::display::state::{RunView, TaskRow, TaskState};
use crate::display::theme::{Role, Theme};

/// Bar-region width of the waterfall (cells) — with the id + duration
/// columns a typical frame stays graceful under 80 columns.
const BAR_WIDTH: usize = 34;

/// One task's reconstructed wall interval on the run's timeline (unix ms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Interval {
    /// Reconstructed start (`end − duration` · else the started stamp).
    pub start: i64,
    /// The terminal stamp (or "now" for a still-running task).
    pub end: i64,
}

impl Interval {
    /// Inclusive overlap — zero-width intervals (sub-millisecond tasks)
    /// at the same stamp still count as concurrent.
    fn overlaps(self, other: Self) -> bool {
        self.start <= other.end && other.start <= self.end
    }
}

/// Reconstruct one row's interval. A terminal row anchors on its end
/// stamp minus the measured duration; a RUNNING row spans its start
/// stamp to `now` (the latest stamp the fold has seen). Rows that never
/// ran (pending · skipped · cancelled-before-start) have no interval.
pub(crate) fn interval_of(row: &TaskRow, now_ms: Option<i64>) -> Option<Interval> {
    match row.state {
        TaskState::Ok | TaskState::Failed => {
            let end = row.ended_ms?;
            let start = row
                .wall_ms()
                .and_then(|d| end.checked_sub(i64::try_from(d).ok()?))
                .or(row.started_ms)?;
            Some(Interval { start, end })
        }
        TaskState::Running | TaskState::Retrying => {
            let start = row.started_ms?;
            let end = now_ms.unwrap_or(start).max(start);
            Some(Interval { start, end })
        }
        TaskState::Pending | TaskState::Skipped | TaskState::Cancelled => None,
    }
}

/// The `∥` lane markers: for each row, did it ACTUALLY run concurrently
/// with a sibling? With an injected wave plan, siblings = same-wave tasks
/// (the scheduler's truth); without one (a replayed trace), any
/// overlapping task counts. Marks derive from reconstructed intervals —
/// a wave whose members happened to run sequentially earns no marker.
pub(crate) fn lane_marks(view: &RunView) -> Vec<bool> {
    let now = view.last_ts_ms();
    let rows = view.rows();
    let intervals: Vec<Option<Interval>> = rows.iter().map(|r| interval_of(r, now)).collect();
    let wave_of = |id: &str| -> Option<usize> {
        view.plan()?
            .iter()
            .position(|wave| wave.iter().any(|t| t == id))
    };
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            let Some(a) = intervals[i] else { return false };
            let my_wave = wave_of(&row.id);
            rows.iter().enumerate().any(|(j, other)| {
                if i == j {
                    return false;
                }
                let Some(b) = intervals[j] else { return false };
                // Plan present on BOTH sides → same-wave siblings only.
                match (my_wave, wave_of(&other.id)) {
                    (Some(w1), Some(w2)) if w1 != w2 => false,
                    _ => a.overlaps(b),
                }
            })
        })
        .collect()
}

/// The post-run waterfall (design §2c): one wall-time-scaled bar per task
/// that RAN, offsets showing the REAL overlap — a pure fold of the trace
/// (the same interval reconstruction as the lane markers · zero new
/// instrumentation). A time axis closes the chart. Fewer than two ran
/// tasks render nothing (a solo bar is noise, same law as the anatomy's
/// analysis footer).
#[must_use]
pub fn waterfall(view: &RunView, theme: &Theme) -> Vec<String> {
    let now = view.last_ts_ms();
    let ran: Vec<(&TaskRow, Interval)> = view
        .rows()
        .iter()
        .filter_map(|r| interval_of(r, now).map(|iv| (r, iv)))
        .collect();
    if ran.len() < 2 {
        return Vec::new();
    }
    let Some(t0) = ran.iter().map(|(_, iv)| iv.start).min() else {
        return Vec::new();
    };
    let Some(t1) = ran.iter().map(|(_, iv)| iv.end).max() else {
        return Vec::new();
    };
    let span = t1.saturating_sub(t0).max(1);
    let (edge_l, edge_r, bar, dot) = if theme.ascii {
        ('[', ']', '#', '.')
    } else {
        ('▕', '▏', '█', '·')
    };
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    // display geometry — ±1 cell is invisible, the tests pin the rounding
    let cell = |ts: i64| -> usize {
        ((ts.saturating_sub(t0)) as f64 / span as f64 * BAR_WIDTH as f64).round() as usize
    };

    let id_w = ran
        .iter()
        .map(|(r, _)| r.id.chars().count())
        .max()
        .unwrap_or(0);
    let durs: Vec<String> = ran
        .iter()
        .map(|(_, iv)| fmt_wall_ms(u64::try_from(iv.end.saturating_sub(iv.start)).unwrap_or(0)))
        .collect();
    let time_w = durs.iter().map(|d| d.chars().count()).max().unwrap_or(0);

    let mut lines = Vec::with_capacity(ran.len() + 1);
    for ((row, iv), dur) in ran.iter().zip(&durs) {
        let off = cell(iv.start).min(BAR_WIDTH - 1);
        let len = cell(iv.end).saturating_sub(off).clamp(1, BAR_WIDTH - off);
        let role = match row.state {
            TaskState::Failed => Role::Bad,
            TaskState::Running | TaskState::Retrying => Role::Accent,
            _ => Role::Good,
        };
        let mut line = format!(
            "  {:<id_w$}  {edge_l}{}{}{edge_r}",
            row.id,
            " ".repeat(off),
            theme.paint(role, &bar.to_string().repeat(len)),
        );
        line.push_str(&" ".repeat(BAR_WIDTH - off - len));
        line.push_str("  ");
        line.push_str(&theme.paint(Role::Dim, &format!("{dur:>time_w$}")));
        if let Some(cost) = row.cost_usd {
            line.push_str(&theme.paint(Role::Dim, &format!(" · ${cost:.4}")));
        }
        lines.push(line);
    }
    // The axis: `0s` under the id column, dots to the bar region's close,
    // the run's total span at the end.
    let total = fmt_wall_ms(u64::try_from(span).unwrap_or(0));
    let dots = (id_w + BAR_WIDTH).saturating_sub(total.chars().count());
    lines.push(theme.paint(
        Role::Dim,
        &format!("  0s {} {total}", dot.to_string().repeat(dots)),
    ));
    lines
}

/// A wall duration for humans: `12ms` · `3.2s` · `2m04s`. Sub-second
/// stays in milliseconds (the mock-run scale), minutes keep the seconds.
#[must_use]
pub fn fmt_wall_ms(ms: u64) -> String {
    if ms < 1_000 {
        return format!("{ms}ms");
    }
    if ms < 60_000 {
        #[allow(clippy::cast_precision_loss)] // display-only seconds
        return format!("{:.1}s", ms as f64 / 1000.0);
    }
    format!("{}m{:02}s", ms / 60_000, (ms % 60_000) / 1000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;
    use nika_event::EventKind;
    use nika_types::resource::{KeyValue, Value};

    fn ev(kind: EventKind, ms: u64, fields: &[(&str, Value)]) -> nika_event::Event {
        let mut e = demo::bare_event(kind, ms);
        for (k, v) in fields {
            e = e.with_field(KeyValue::new(*k, v.clone()));
        }
        e
    }

    fn s(v: &str) -> Value {
        Value::String(v.to_owned())
    }

    /// Two settle-stamped siblings with REAL durations reconstruct to
    /// overlapping intervals (∥ both) while a downstream task that ran
    /// after them stays unmarked — the ∥ law on the settle-time stream.
    #[test]
    fn lane_marks_flag_reconstructed_overlap_only() {
        let mut view = RunView::new();
        // Wave 1: a + b dispatched concurrently · both settle at ~1000ms
        // with real durations 900/850 → intervals [100,1000] · [150,1000].
        for (task, dur, end) in [("a", 900i64, 1000u64), ("b", 850, 1000)] {
            view.apply(&ev(EventKind::TaskStarted, end, &[("task", s(task))]));
            view.apply(&ev(
                EventKind::TaskCompleted,
                end,
                &[("task", s(task)), ("duration_ms", Value::Int(dur))],
            ));
        }
        // Wave 2: c ran strictly after · [1010, 1500].
        view.apply(&ev(EventKind::TaskStarted, 1500, &[("task", s("c"))]));
        view.apply(&ev(
            EventKind::TaskCompleted,
            1500,
            &[("task", s("c")), ("duration_ms", Value::Int(490))],
        ));
        assert_eq!(lane_marks(&view), vec![true, true, false]);
    }

    /// The wave plan SCOPES the marker: tasks in different waves never
    /// mark each other even when their reconstructed intervals touch
    /// (the zero-width mock-run case: everything stamps the same ms).
    #[test]
    fn plan_scopes_marks_to_wave_siblings() {
        let mut view = RunView::new();
        for task in ["a", "b", "c"] {
            view.apply(&ev(EventKind::TaskStarted, 100, &[("task", s(task))]));
            view.apply(&ev(
                EventKind::TaskCompleted,
                100,
                &[("task", s(task)), ("duration_ms", Value::Int(0))],
            ));
        }
        // Without a plan every zero-width interval touches every other.
        assert_eq!(lane_marks(&view), vec![true, true, true]);
        // The plan says c runs alone in wave 2 → its touch is scheduling
        // adjacency, not concurrency.
        view.set_plan(vec![
            vec!["a".to_owned(), "b".to_owned()],
            vec!["c".to_owned()],
        ]);
        assert_eq!(lane_marks(&view), vec![true, true, false]);
    }

    /// The demo storyboard is strictly sequential — no row earns a mark
    /// (golden frames stay marker-free by construction).
    #[test]
    fn demo_storyboard_has_no_concurrency() {
        let mut view = RunView::new();
        for e in demo::success() {
            view.apply(&e);
        }
        assert!(lane_marks(&view).iter().all(|m| !m));
    }

    /// Rows that never ran reconstruct no interval — and a RUNNING row
    /// spans to "now" so it can overlap an already-settled sibling.
    #[test]
    fn interval_reconstruction_covers_the_states() {
        let mut view = RunView::new();
        view.apply(&ev(EventKind::TaskScheduled, 0, &[("task", s("p"))]));
        view.apply(&ev(EventKind::TaskSkipped, 5, &[("task", s("skip"))]));
        view.apply(&ev(EventKind::TaskStarted, 10, &[("task", s("live"))]));
        view.apply(&ev(EventKind::TaskStarted, 400, &[("task", s("done"))]));
        view.apply(&ev(
            EventKind::TaskCompleted,
            400,
            &[("task", s("done")), ("duration_ms", Value::Int(300))],
        ));
        let rows = view.rows();
        let now = view.last_ts_ms();
        assert!(interval_of(&rows[0], now).is_none(), "pending: no interval");
        assert!(interval_of(&rows[1], now).is_none(), "skipped: no interval");
        // live: [10, now=400] · done: [100, 400] → they overlap.
        assert_eq!(
            interval_of(&rows[2], now),
            Some(Interval {
                start: 10,
                end: 400
            })
        );
        assert_eq!(
            interval_of(&rows[3], now),
            Some(Interval {
                start: 100,
                end: 400
            })
        );
        let marks = lane_marks(&view);
        assert!(marks[2] && marks[3], "running ∥ settled sibling: {marks:?}");
    }

    const PLAIN: Theme = Theme {
        color: false,
        ascii: false,
        animate: false,
    };
    const ASCII: Theme = Theme {
        color: false,
        ascii: true,
        animate: false,
    };

    /// Golden waterfall — bars scale to wall time, offsets carry the real
    /// sequencing, the axis closes the chart (design §2c geometry pinned:
    /// a mutated scale factor draws a wrong-but-plausible chart).
    #[test]
    fn waterfall_scales_bars_and_offsets() {
        let mut view = RunView::new();
        // a ran [0, 1000] (stamp span) · b ran [1000, 1500] after it.
        view.apply(&ev(EventKind::TaskStarted, 0, &[("task", s("a"))]));
        view.apply(&ev(EventKind::TaskCompleted, 1000, &[("task", s("a"))]));
        view.apply(&ev(EventKind::TaskStarted, 1000, &[("task", s("b"))]));
        view.apply(&ev(EventKind::TaskCompleted, 1500, &[("task", s("b"))]));

        let lines = waterfall(&view, &PLAIN);
        assert_eq!(lines.len(), 3, "two bars + the axis: {lines:?}");
        // span 1500 over 34 cells: a = cells 0..23 · b = cells 23..34.
        assert_eq!(
            lines[0],
            format!("  a  ▕{}▏{}   1.0s", "█".repeat(23), " ".repeat(11)),
        );
        assert_eq!(
            lines[1],
            format!("  b  ▕{}{}▏  500ms", " ".repeat(23), "█".repeat(11)),
        );
        assert_eq!(lines[2], format!("  0s {} 1.5s", "·".repeat(31)));
    }

    /// ASCII parity for every waterfall glyph (▕█▏ → [#] · axis dots → .)
    /// and the solo-run silence (a single bar is noise, not insight).
    #[test]
    fn waterfall_ascii_parity_and_solo_silence() {
        let mut view = RunView::new();
        view.apply(&ev(EventKind::TaskStarted, 0, &[("task", s("only"))]));
        view.apply(&ev(EventKind::TaskCompleted, 100, &[("task", s("only"))]));
        assert!(
            waterfall(&view, &PLAIN).is_empty(),
            "one ran task → no waterfall"
        );

        view.apply(&ev(EventKind::TaskStarted, 100, &[("task", s("next"))]));
        view.apply(&ev(EventKind::TaskCompleted, 400, &[("task", s("next"))]));
        let lines = waterfall(&view, &ASCII);
        assert_eq!(lines.len(), 3);
        assert!(
            lines[0].contains('[') && lines[0].contains('#'),
            "{lines:?}"
        );
        assert!(lines[2].starts_with("  0s ."), "{lines:?}");
        for line in &lines {
            for glyph in ['▕', '▏', '█', '·'] {
                assert!(
                    !line.contains(glyph),
                    "unicode {glyph} leaked into --ascii: {line}"
                );
            }
        }
    }

    /// Skipped/cancelled rows never bar (they did not run) and a failed
    /// row keeps its per-task spend on the chart.
    #[test]
    fn waterfall_charts_only_ran_rows() {
        let mut view = RunView::new();
        view.apply(&ev(EventKind::TaskStarted, 0, &[("task", s("work"))]));
        view.apply(&ev(
            EventKind::TaskFailed,
            800,
            &[("task", s("work")), ("cost_usd", Value::Float(0.002))],
        ));
        view.apply(&ev(EventKind::TaskStarted, 800, &[("task", s("more"))]));
        view.apply(&ev(EventKind::TaskCompleted, 900, &[("task", s("more"))]));
        view.apply(&ev(EventKind::TaskCancelled, 900, &[("task", s("late"))]));
        let lines = waterfall(&view, &PLAIN);
        assert_eq!(lines.len(), 3, "two ran bars + axis (no cancelled bar)");
        assert!(
            lines[0].contains("work") && lines[0].ends_with("· $0.0020"),
            "failed row bars + keeps its spend: {lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("late")),
            "cancelled never bars: {lines:?}"
        );
    }

    #[test]
    fn wall_format_scales() {
        assert_eq!(fmt_wall_ms(0), "0ms");
        assert_eq!(fmt_wall_ms(12), "12ms");
        assert_eq!(fmt_wall_ms(999), "999ms");
        assert_eq!(fmt_wall_ms(1_000), "1.0s");
        assert_eq!(fmt_wall_ms(3_200), "3.2s");
        assert_eq!(fmt_wall_ms(59_949), "59.9s");
        assert_eq!(fmt_wall_ms(124_000), "2m04s");
    }
}
