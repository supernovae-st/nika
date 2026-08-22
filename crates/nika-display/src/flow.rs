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

use crate::format::fmt_cost_usd;
use crate::state::{RunView, TaskRow, TaskState};
use crate::theme::{Role, Theme};

/// Bar-region width of the waterfall (cells) — with the id + duration
/// columns a typical frame stays graceful under 80 columns.
const BAR_WIDTH: usize = 34;

/// One task's reconstructed wall interval on the run's timeline (unix ms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
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
#[must_use]
pub fn interval_of(row: &TaskRow, now_ms: Option<i64>) -> Option<Interval> {
    match row.state {
        TaskState::Ok | TaskState::Failed => {
            let end = row.ended_ms?;
            let start = row
                .wall_ms()
                .and_then(|d| end.checked_sub(i64::try_from(d).ok()?))
                .or(row.started_ms)?;
            Some(Interval { start, end })
        }
        // A paused gate spans its start to the pause stamp — the open
        // interval the waterfall can honestly draw.
        TaskState::Running | TaskState::Retrying | TaskState::Paused => {
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
#[must_use]
pub fn lane_marks(view: &RunView) -> Vec<bool> {
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

/// The verb chip in front of a gantt lane id (accents only — sober
/// gantts keep their historical bytes): the timeline speaks the same
/// 4-verb vocabulary as the storyboard rows above it.
fn lane_chip(row: &TaskRow, theme: Theme) -> String {
    if !theme.accents {
        return String::new();
    }
    row.started_note
        .as_deref()
        .and_then(|n| n.split(" · ").next())
        .map_or_else(|| "  ".to_owned(), |v| theme.verb_glyph(v))
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

    // The heat scale anchors on the run's own long pole (max wall time).
    let wall_of = |iv: &Interval| u64::try_from(iv.end.saturating_sub(iv.start)).unwrap_or(0);
    let wall_max = ran.iter().map(|(_, iv)| wall_of(iv)).max().unwrap_or(1);

    let mut lines = Vec::with_capacity(ran.len() + 1);
    for ((row, iv), dur) in ran.iter().zip(&durs) {
        let off = cell(iv.start).min(BAR_WIDTH - 1);
        let len = cell(iv.end).saturating_sub(off).clamp(1, BAR_WIDTH - off);
        let role = match row.state {
            TaskState::Failed => Role::Bad,
            TaskState::Running | TaskState::Retrying => Role::Accent,
            _ => Role::Good,
        };
        // Duration heat rides SUCCESS bars only (design §1.5): the
        // verdict hues (failed red · running accent) always win — heat
        // is data, never a verdict. Off (flat Good) unless COLORTERM
        // proved truecolor (`theme.heat`).
        let bar_raw = bar.to_string().repeat(len);
        let painted = if theme.heat && role == Role::Good {
            theme.heat_step(heat_bucket(wall_of(iv), wall_max), &bar_raw)
        } else {
            theme.paint(role, &bar_raw)
        };
        let mut line = format!(
            "  {}{:<id_w$}  {edge_l}{}{painted}{edge_r}",
            lane_chip(row, *theme),
            row.id,
            " ".repeat(off),
        );
        line.push_str(&" ".repeat(BAR_WIDTH - off - len));
        line.push_str("  ");
        line.push_str(&theme.paint(Role::Dim, &format!("{dur:>time_w$}")));
        if let Some(cost) = row.cost_usd {
            line.push_str(&theme.paint(Role::Dim, &format!(" · {}", fmt_cost_usd(cost))));
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

/// Quantize one duration onto the 5-step heat ramp: 0 (the fastest
/// band) … 4 (the run's long pole). The run's own max anchors the
/// scale — heat compares tasks WITHIN a run, never across runs.
#[must_use]
pub fn heat_bucket(wall_ms: u64, max_ms: u64) -> usize {
    usize::try_from(wall_ms.saturating_mul(4) / max_ms.max(1))
        .unwrap_or(4)
        .min(4)
}

/// Widest the verdict card grows (inner cells) — 2-indent + corners keeps
/// the line ≤ 64, comfortably shareable.
const CARD_INNER_CAP: usize = 58;

/// The wave sizes the shape glyph speaks: the injected plan when the run
/// provided one (the scheduler's truth), else reconstructed by chaining
/// overlapping intervals in start order (a replayed trace's best honest
/// read — only tasks that RAN count).
fn wave_sizes(view: &RunView) -> Vec<usize> {
    if let Some(plan) = view.plan() {
        return plan.iter().map(Vec::len).collect();
    }
    let now = view.last_ts_ms();
    let mut ivs: Vec<Interval> = view
        .rows()
        .iter()
        .filter_map(|r| interval_of(r, now))
        .collect();
    ivs.sort_by_key(|iv| (iv.start, iv.end));
    let mut sizes = Vec::new();
    let mut group_end: Option<i64> = None;
    for iv in ivs {
        match (group_end, sizes.last_mut()) {
            (Some(end), Some(last)) if iv.start <= end => {
                *last += 1;
                group_end = Some(end.max(iv.end));
            }
            _ => {
                sizes.push(1);
                group_end = Some(iv.end);
            }
        }
    }
    sizes
}

/// The mini DAG-shape glyph (design §2d): wave sizes as diamond runs
/// joined by flow arrows — `◆◆◆ ⇉ ◆` (`###` ` => ` `#` in ASCII).
/// Unique per workflow shape, instantly recognizable. Wide waves cap at
/// five diamonds (`◆◆◆◆◆+`), long chains at six waves (`… `-tailed).
#[must_use]
pub fn dag_shape(view: &RunView, theme: &Theme) -> String {
    let (diamond, arrow, plus, tail) = if theme.ascii {
        ("#", " => ", "+", "...")
    } else {
        ("◆", " ⇉ ", "+", "…")
    };
    let sizes = wave_sizes(view);
    let mut runs: Vec<String> = sizes
        .iter()
        .take(6)
        .map(|&n| {
            let mut run = diamond.repeat(n.min(5));
            if n > 5 {
                run.push_str(plus);
            }
            run
        })
        .collect();
    if sizes.len() > 6 {
        runs.push(tail.to_owned());
    }
    runs.join(arrow)
}

/// The shareable verdict card (design §2d) — the run's signature frame:
/// the DAG-shape glyph, the totals (tasks · waves · retries · wall ·
/// spend · models), the caller's note rows (the FRUIT block — `wrote
/// output.md (412B)` · `said "…"` · `outputs → …` — sizes are the
/// caller's stat, no I/O here), then the rehearsal fact + the
/// form-sanity cautions (derived HERE from the view, so a shared
/// screenshot of the card alone can never hide a lying green — A-2 ·
/// user gauntlet 2026-07-31). Renders nothing before a verdict (a card
/// mid-run would lie).
#[must_use]
pub fn verdict_card(view: &RunView, theme: &Theme, notes: &[String]) -> Vec<String> {
    let Some(ok) = view.verdict else {
        return Vec::new();
    };
    let (tl, tr, bl, br, h, v) = if theme.ascii {
        ('+', '+', '+', '+', '-', '|')
    } else {
        ('╭', '╮', '╰', '╯', '─', '│')
    };
    let mark_raw = match (ok, crate::fruit::recovered_ok(view), theme.ascii) {
        (true, true, false) => "⚠",
        (true, true, true) => "!",
        (true, false, false) => "✓",
        (true, false, true) => "OK",
        (false, _, false) => "✖",
        (false, _, true) => "X",
    };
    let title_raw = format!("{h} nika {mark_raw} {} ", view.workflow);

    let waves = wave_sizes(view).len();
    let retries_cell = crate::vocab::count(view.retries as usize, "retry");
    // The repair count (#319 · D-2026-07-08-N4) rides beside retries —
    // only when non-zero (the verdict-count discipline: `0 retries` is
    // a stable cell, a repair is an EVENT worth a cell only when real).
    let recovered_cell = match view.recovered_count() {
        0 => None,
        n => Some(format!("{n} recovered")),
    };
    let mut head = format!(
        "{}    {} · {} · {retries_cell}",
        dag_shape(view, theme),
        crate::vocab::count(view.rows().len(), "task"),
        crate::vocab::count(waves, "wave"),
    );
    if let Some(cell) = &recovered_cell {
        use std::fmt::Write as _;
        let _ = write!(head, " · {cell}");
    }
    let mut rows = vec![head, totals_row(view)];
    rows.extend(notes.iter().cloned());
    if let Some(note) = crate::fruit::rehearsal_note(view) {
        rows.push(note.to_owned());
    }
    // The caution rows close the card — Warn-painted below, AFTER the
    // width math (the same post-fit paint law the count cells follow).
    let caution_start = rows.len();
    rows.extend(crate::fruit::cautions(view, theme.ascii));

    let inner = rows
        .iter()
        .map(|r| r.chars().count() + 4)
        .chain(std::iter::once(title_raw.chars().count() + 1))
        .max()
        .unwrap_or(0)
        .min(CARD_INNER_CAP);
    let ellipsis = if theme.ascii { "~" } else { "…" };
    let fill: String =
        std::iter::repeat_n(h, inner.saturating_sub(title_raw.chars().count())).collect();
    let mark_role = if ok { Role::Good } else { Role::Bad };
    let title = format!(
        "{h} nika {} {} ",
        theme.paint(mark_role, mark_raw),
        theme.paint(Role::Strong, &view.workflow),
    );
    let mut lines = vec![format!("  {tl}{title}{fill}{tr}")];
    for (i, row) in rows.iter().enumerate() {
        let fitted = fit_cells(row, inner.saturating_sub(4), ellipsis);
        let pad = inner
            .saturating_sub(4)
            .saturating_sub(fitted.chars().count());
        // Verdict-count discipline (cargo school · design §1.7): only a
        // NON-ZERO bad count earns colour — `0 retries` stays plain, a
        // real retry count paints yellow. The paint lands on the FITTED
        // text AFTER the width math (escapes add zero visible cells; a
        // truncated row simply keeps its plain form).
        let mut shown = if view.retries > 0 {
            fitted.replace(&retries_cell, &theme.paint(Role::Warn, &retries_cell))
        } else {
            fitted
        };
        // The repair count wears the same survived-incident yellow.
        if let Some(cell) = &recovered_cell {
            shown = shown.replace(cell.as_str(), &theme.paint(Role::Warn, cell));
        }
        // A caution row wears Warn WHOLE — the truth line is the point.
        if i >= caution_start {
            shown = theme.paint(Role::Warn, &shown);
        }
        lines.push(format!("  {v}  {shown}{}  {v}", " ".repeat(pad)));
    }
    let bottom: String = std::iter::repeat_n(h, inner).collect();
    lines.push(format!("  {bl}{bottom}{br}"));
    lines
}

/// The card's totals row: wall time · total tokens (when any task
/// reported usage — tokens are real TODAY, dollars stay honest-zero
/// until the engine prices them) · spend · the models the stream named
/// (the fold kept them off the `infer · <model>` / `agent · <model>`
/// notes — rendered only when the stream actually said them).
fn totals_row(view: &RunView) -> String {
    use std::fmt::Write as _;
    let mut row = fmt_wall_ms(view.elapsed_ms);
    let tokens: u64 = view.token_samples.iter().sum();
    if tokens > 0 {
        let _ = write!(row, " · {tokens} tok");
    }
    // Partial totals never read as complete: `≥` + the unpriced count
    // when some calls carried no meterable price (local · mock ·
    // uncataloged · provider silent) — never silently sum nulls as zero.
    if view.unpriced_calls > 0 {
        let _ = write!(
            row,
            " · ≥ {} ({} unpriced)",
            fmt_cost_usd(view.cost_usd),
            view.unpriced_calls
        );
    } else {
        let _ = write!(row, " · {}", fmt_cost_usd(view.cost_usd));
    }
    let mut models: Vec<&str> = Vec::new();
    for r in view.rows() {
        if let Some(m) = r.model.as_deref()
            && !models.contains(&m)
        {
            models.push(m);
        }
    }
    for m in models.iter().take(2) {
        row.push_str(" · ");
        row.push_str(m);
    }
    row
}

/// Truncate to `width` display cells with a theme-true mark — the card
/// border never breaks on an overlong outputs note.
fn fit_cells(s: &str, width: usize, ellipsis: &str) -> String {
    if s.chars().count() <= width {
        return s.to_owned();
    }
    let keep = width.saturating_sub(ellipsis.chars().count());
    let mut out: String = s.chars().take(keep).collect();
    out.push_str(ellipsis);
    out
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

    const PLAIN: Theme = Theme::new(false, false, false);
    const ASCII: Theme = Theme::new(false, true, false);

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
            lines[0].contains("work") && lines[0].ends_with("· $0.002"),
            "failed row bars + keeps its spend: {lines:?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("late")),
            "cancelled never bars: {lines:?}"
        );
    }

    /// The card speaks the plan's shape when one was injected: wave sizes
    /// become diamond runs joined by flow arrows — the signature glyph.
    #[test]
    fn dag_shape_reads_the_plan_first() {
        let mut view = RunView::new();
        for e in demo::success() {
            view.apply(&e);
        }
        // Reconstructed (no plan): the demo ran strictly sequentially.
        assert_eq!(dag_shape(&view, &PLAIN), "◆ ⇉ ◆ ⇉ ◆ ⇉ ◆");
        // The scheduler's truth: 3 parallel sources then the join.
        view.set_plan(vec![
            vec!["a".into(), "b".into(), "c".into()],
            vec!["join".into()],
        ]);
        assert_eq!(dag_shape(&view, &PLAIN), "◆◆◆ ⇉ ◆");
        assert_eq!(dag_shape(&view, &ASCII), "### => #");
    }

    /// Wide waves cap at five diamonds (+), long chains at six waves (…).
    #[test]
    fn dag_shape_caps_width_and_length() {
        let mut view = RunView::new();
        view.apply(&ev(EventKind::WorkflowCompleted, 0, &[]));
        view.set_plan(vec![vec!["t".into(); 8], vec!["x".into()]]);
        assert_eq!(dag_shape(&view, &PLAIN), "◆◆◆◆◆+ ⇉ ◆");
        view.set_plan(vec![vec!["t".into()]; 8]);
        assert_eq!(dag_shape(&view, &PLAIN), "◆ ⇉ ◆ ⇉ ◆ ⇉ ◆ ⇉ ◆ ⇉ ◆ ⇉ …");
    }

    /// Golden verdict card — the shareable frame: shape glyph · totals ·
    /// the models the stream named · the caller's outputs note. Borders
    /// stay intact (padded to one inner width) in both themes.
    #[test]
    fn verdict_card_is_the_shareable_frame() {
        let mut view = RunView::new();
        for e in demo::success() {
            view.apply(&e);
        }
        view.set_plan(vec![
            vec!["fetch_top".into(), "extract_ai".into(), "summarize".into()],
            vec!["write_md".into()],
            vec!["notify_slack".into()],
        ]);
        let lines = verdict_card(&view, &PLAIN, &["outputs → review (object)".to_owned()]);
        assert_eq!(lines.len(), 5, "top + 3 rows + bottom: {lines:?}");
        assert!(
            lines[0].starts_with("  ╭─ nika ✓ veille-news ─"),
            "{lines:?}"
        );
        assert!(lines[0].ends_with('╮'), "{lines:?}");
        assert!(
            lines[1].contains("◆◆◆ ⇉ ◆ ⇉ ◆") && lines[1].contains("5 tasks · 3 waves · 0 retries"),
            "{lines:?}"
        );
        assert!(
            lines[2].contains("4.7s · 710 tok · $0.01 · claude-sonnet"),
            "totals (wall · tokens · spend) + the model the stream named: {lines:?}"
        );
        assert!(lines[3].contains("outputs → review (object)"), "{lines:?}");
        assert!(
            lines[4].starts_with("  ╰─") && lines[4].ends_with('╯'),
            "{lines:?}"
        );
        // Every border row closes at the SAME column (the box holds).
        let widths: Vec<usize> = lines.iter().map(|l| l.chars().count()).collect();
        assert!(
            widths.iter().all(|w| *w == widths[0]),
            "aligned card: {widths:?} {lines:?}"
        );

        // ASCII parity: corners + mark + shape glyph, zero unicode leaks.
        let ascii = verdict_card(&view, &ASCII, &[]);
        assert!(
            ascii[0].starts_with("  +- nika OK veille-news -"),
            "{ascii:?}"
        );
        assert!(ascii[1].contains("### => # => #"), "{ascii:?}");
        for glyph in ['╭', '╮', '╰', '╯', '─', '│', '◆', '⇉', '✓'] {
            assert!(
                !ascii.iter().any(|l| l.contains(glyph)),
                "unicode {glyph} leaked into --ascii: {ascii:?}"
            );
        }
    }

    /// Persona 14 · gauntlet g2: the shareable card titled `nika ✓`
    /// on a recovered run. Exit 0 stays; the title mark does not.
    #[test]
    fn verdict_card_recovered_is_not_a_green_tick() {
        let mut view = RunView::new();
        for e in demo::recovered() {
            view.apply(&e);
        }
        let uni = verdict_card(&view, &PLAIN, &[]);
        assert!(
            uni[0].contains("nika ⚠ recovered"),
            "recovered title is not a green tick: {uni:?}"
        );
        assert!(
            !uni[0].contains('✓'),
            "recovered title keeps no tick: {uni:?}"
        );
        let ascii = verdict_card(&view, &ASCII, &[]);
        assert!(
            ascii[0].contains("nika ! recovered"),
            "ascii twin: {ascii:?}"
        );
        assert!(
            !ascii[0].contains("OK"),
            "ascii recovered is not OK: {ascii:?}"
        );
    }

    /// Verdict-count discipline (cargo school): `0 retries` renders
    /// PLAIN even with colour on — only a real retry count paints
    /// yellow, and the paint never disturbs the box alignment (escapes
    /// land after the width math).
    #[test]
    fn verdict_card_colours_only_non_zero_retries() {
        let coloured = Theme::new(true, false, false);

        let mut clean = RunView::new();
        for e in demo::success() {
            clean.apply(&e);
        }
        let lines = verdict_card(&clean, &coloured, &[]);
        let totals = lines.iter().find(|l| l.contains("retries")).expect("row");
        assert!(
            totals.contains("0 retries") && !totals.contains("\x1b[33m"),
            "a zero count stays plain: {totals:?}"
        );

        let mut retried = RunView::new();
        for e in demo::retrying() {
            retried.apply(&e);
        }
        retried.apply(&ev(EventKind::WorkflowCompleted, 9_000, &[]));
        let lines = verdict_card(&retried, &coloured, &[]);
        let totals = lines.iter().find(|l| l.contains("retry")).expect("row");
        assert!(
            totals.contains("\x1b[33m1 retry\x1b[0m"),
            "a real retry count paints yellow, singular agreed: {totals:?}"
        );
        // The box still closes at one visible column: strip the escapes
        // and every border row measures the same width.
        let bare: Vec<usize> = lines
            .iter()
            .map(|l| {
                l.replace("\x1b[33m", "")
                    .replace("\x1b[32m", "")
                    .replace("\x1b[1m", "")
                    .replace("\x1b[0m", "")
                    .chars()
                    .count()
            })
            .collect();
        assert!(
            bare.iter().all(|w| *w == bare[0]),
            "escape-stripped alignment holds: {bare:?}"
        );
    }

    /// #319 — the verdict card carries the repair count beside retries
    /// when a task settled through `on_error.recover` (and NEVER grows
    /// the cell on a clean run — the demo card above pins `0 retries`
    /// with no `recovered` in sight).
    #[test]
    fn verdict_card_counts_recovered_beside_retries() {
        let mut view = RunView::new();
        view.apply(&ev(EventKind::TaskStarted, 0, &[("task", s("fragile"))]));
        view.apply(&ev(
            EventKind::TaskRecovered,
            5,
            &[("task", s("fragile")), ("code", s("NIKA-BUILTIN-READ-001"))],
        ));
        view.apply(&ev(
            EventKind::TaskCompleted,
            10,
            &[("task", s("fragile")), ("duration_ms", Value::Int(1))],
        ));
        view.apply(&ev(EventKind::WorkflowCompleted, 20, &[]));

        let lines = verdict_card(&view, &PLAIN, &[]);
        let head = lines.iter().find(|l| l.contains("retries")).expect("row");
        assert!(
            head.contains("0 retries · 1 recovered"),
            "the repair count rides beside retries: {head:?}"
        );

        // Colour on: the repair cell wears the survived-incident yellow
        // and the box alignment survives the escapes.
        let coloured = verdict_card(&view, &Theme::new(true, false, false), &[]);
        let head = coloured
            .iter()
            .find(|l| l.contains("recovered"))
            .expect("row");
        assert!(
            head.contains("\x1b[33m1 recovered\x1b[0m"),
            "the repair count paints yellow: {head:?}"
        );
        let bare: Vec<usize> = coloured
            .iter()
            .map(|l| {
                l.replace("\x1b[33m", "")
                    .replace("\x1b[32m", "")
                    .replace("\x1b[1m", "")
                    .replace("\x1b[0m", "")
                    .chars()
                    .count()
            })
            .collect();
        assert!(
            bare.iter().all(|w| *w == bare[0]),
            "escape-stripped alignment holds: {bare:?}"
        );

        // A clean run never grows the cell.
        let mut clean = RunView::new();
        for e in demo::success() {
            clean.apply(&e);
        }
        assert!(
            !verdict_card(&clean, &PLAIN, &[])
                .iter()
                .any(|l| l.contains("recovered")),
            "no repair → no cell"
        );
    }

    /// A failed run cards the ✖ mark; a run with no verdict cards nothing
    /// (mid-run frames must never carry a verdict card).
    #[test]
    fn verdict_card_marks_failure_and_stays_silent_mid_run() {
        let mut view = RunView::new();
        for e in demo::failure() {
            view.apply(&e);
        }
        let lines = verdict_card(&view, &PLAIN, &[]);
        assert!(lines[0].contains("✖ veille-news"), "{lines:?}");

        let mut mid = RunView::new();
        for e in demo::retrying() {
            mid.apply(&e);
        }
        assert!(mid.verdict.is_none());
        assert!(verdict_card(&mid, &PLAIN, &[]).is_empty());
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

    /// The heat quantizer: 5 bands anchored on the run's long pole —
    /// zero-length lands in band 0, the max in band 4, and the scale is
    /// linear in between (the exact boundaries pin the `*4/max` math
    /// against off-by-one mutations).
    #[test]
    fn heat_bucket_quantizes_five_bands() {
        assert_eq!(heat_bucket(0, 1_000), 0);
        assert_eq!(heat_bucket(249, 1_000), 0);
        assert_eq!(heat_bucket(250, 1_000), 1);
        assert_eq!(heat_bucket(500, 1_000), 2);
        assert_eq!(heat_bucket(750, 1_000), 3);
        assert_eq!(heat_bucket(1_000, 1_000), 4);
        // Degenerate scales stay in range (a zero-max run · overshoot).
        assert_eq!(heat_bucket(5, 0), 4, "max clamps · never a panic");
        assert_eq!(heat_bucket(2_000, 1_000), 4, "overshoot clamps to 4");
    }

    /// Duration heat rides SUCCESS bars only, truecolor only: with
    /// `theme.heat` the ok bars carry `38;2;r;g;b` SGRs (the long pole
    /// in the ramp's deepest step · the failed bar KEEPS its red) — and
    /// without it the same view renders zero truecolor bytes (the
    /// 256-colour fallback is flat, never approximated).
    #[test]
    fn waterfall_heat_is_truecolor_gated_and_success_only() {
        let mut view = RunView::new();
        view.apply(&ev(EventKind::TaskStarted, 0, &[("task", s("fast"))]));
        view.apply(&ev(EventKind::TaskCompleted, 250, &[("task", s("fast"))]));
        view.apply(&ev(EventKind::TaskStarted, 250, &[("task", s("long"))]));
        view.apply(&ev(EventKind::TaskCompleted, 1_250, &[("task", s("long"))]));
        view.apply(&ev(EventKind::TaskStarted, 1_250, &[("task", s("bad"))]));
        view.apply(&ev(EventKind::TaskFailed, 1_500, &[("task", s("bad"))]));

        let mut heat = Theme::new(true, false, false);
        heat.heat = true;
        let lines = waterfall(&view, &heat);
        let ramp_top = crate::theme::HEAT_RAMP[4];
        let deepest = format!("\x1b[38;2;{};{};{}m", ramp_top.0, ramp_top.1, ramp_top.2);
        assert!(
            lines[1].contains(&deepest),
            "the long pole wears the deepest step: {lines:?}"
        );
        assert!(
            lines[0].contains("\x1b[38;2;"),
            "the fast bar wears a paler step: {lines:?}"
        );
        assert!(
            lines[2].contains("\x1b[31m") && !lines[2].contains("38;2;"),
            "the failed bar stays RED — verdict beats heat: {lines:?}"
        );

        // COLORTERM absent → theme.heat off → flat bars, zero truecolor.
        let flat = waterfall(&view, &Theme::new(true, false, false));
        assert!(
            !flat.iter().any(|l| l.contains("38;2;")),
            "no COLORTERM proof → no truecolor: {flat:?}"
        );
    }
}
