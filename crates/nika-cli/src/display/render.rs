// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Frame rendering (spec §3.3) — a pure function `(RunView, Theme, tick) →
//! lines`. No I/O here: the replay loop owns the terminal, this module owns
//! the truth-to-text mapping. Snapshot tests pin BOTH glyph themes.

use crate::display::flow::{fmt_wall_ms, lane_marks};
use crate::display::state::{RunView, TaskRow, TaskState};
use crate::display::theme::{Role, Theme};

/// Widest the note column grows before the time column floats free —
/// keeps a typical frame graceful under 80 columns.
const NOTE_COL_CAP: usize = 40;

/// Render one frame of the run card.
#[must_use]
pub fn frame(view: &RunView, theme: &Theme, tick: usize) -> Vec<String> {
    frame_impl(view, theme, tick, false)
}

/// Render one frame WITH the shape tails — completed rows carry their
/// bounded output summary + tokens (`→ {…} · 312B · 90 tok`). The
/// interactive-TTY surface only: `frame` (no tails) stays the byte-exact
/// register for pipes · CI logs · `--no-outputs`.
#[must_use]
pub fn frame_with_outputs(view: &RunView, theme: &Theme, tick: usize) -> Vec<String> {
    frame_impl(view, theme, tick, true)
}

/// The one frame assembler behind both public forms.
// `&Theme` to match the public `frame` borrow that threads it here — the
// same one-calling-convention rationale as `task_line`.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn frame_impl(view: &RunView, theme: &Theme, tick: usize, outputs: bool) -> Vec<String> {
    let mut lines = Vec::with_capacity(view.rows().len() + 6);

    // Header: identity + the statically-proven ceiling.
    let ceiling = view
        .ceiling_usd
        .map(|c| format!(" · ceiling ≤ ${c:.2}"))
        .unwrap_or_default();
    lines.push(format!(
        "  {} nika · {} · {} tasks{ceiling}",
        theme.logo(),
        theme.paint(Role::Strong, &view.workflow),
        view.rows().len(),
    ));

    // The audit-as-greeting line (the trust moment, every run).
    if let Some(permits) = &view.permits {
        let mark = if theme.ascii { "OK" } else { "✓" };
        lines.push(format!(
            "     permits {} {}",
            theme.paint(Role::Good, mark),
            theme.paint(Role::Dim, permits),
        ));
    }
    lines.push(String::new());

    // Task rows — stable order, aligned ids, notes dimmed. Time and cost
    // are first-class columns: a settled row carries its REAL wall time
    // (+ per-task spend when the stream reported one), a running row its
    // live elapsed, and `∥` marks wave-siblings that actually overlapped.
    let width = view.rows().iter().map(|r| r.id.len()).max().unwrap_or(8);
    let marks = lane_marks(view);
    let times: Vec<Option<String>> = view.rows().iter().map(|r| row_wall(r, view)).collect();
    let note_w = view
        .rows()
        .iter()
        .map(|r| r.note.chars().count())
        .max()
        .unwrap_or(0)
        .min(NOTE_COL_CAP);
    let time_w = times
        .iter()
        .flatten()
        .map(|t| t.chars().count())
        .max()
        .unwrap_or(0);
    for (i, row) in view.rows().iter().enumerate() {
        let mark = marks.get(i).copied().unwrap_or(false);
        let tail = if outputs {
            crate::display::shape::output_tail(row.output_json.as_deref(), row.tokens, theme)
        } else {
            None
        };
        lines.push(task_line(
            row,
            view,
            theme,
            tick,
            (width, note_w, time_w),
            times[i].as_deref(),
            mark,
            tail.as_deref(),
        ));
    }

    // Footer meter: progress · live cost vs ceiling · wall clock.
    let cost = match view.ceiling_usd {
        Some(c) => format!("${:.3} of ≤${c:.2}", view.cost_usd),
        None => format!("${:.3}", view.cost_usd),
    };
    #[allow(clippy::cast_precision_loss)] // display-only seconds
    let secs = view.elapsed_ms as f64 / 1000.0;
    let meter = format!(
        "── {}/{} done · {cost} · elapsed {secs:.1}s ",
        view.done_count(),
        view.rows().len(),
    );
    lines.push(format!(
        "  {}",
        theme.paint(Role::Dim, &pad_rule(&meter, 64))
    ));

    // Failure card (only on a failed verdict · derives the explain hint) —
    // the SAME card the compact `--quiet` surface renders (shared helper).
    if view.verdict == Some(false) {
        append_failure_card(&mut lines, view, theme);
    }
    lines
}

/// The wall-time cell for one row: a settled row's REAL duration (the
/// runtime-measured `duration_ms` · else the stamp span), a running or
/// retrying row's LIVE elapsed against the latest stamp the fold has
/// seen. Rows that never ran show nothing — never an invented number.
fn row_wall(row: &TaskRow, view: &RunView) -> Option<String> {
    match row.state {
        TaskState::Ok | TaskState::Failed => row.wall_ms().map(fmt_wall_ms),
        TaskState::Running | TaskState::Retrying => {
            let start = row.started_ms?;
            let now = view.last_ts_ms()?;
            Some(fmt_wall_ms(u64::try_from(now.saturating_sub(start)).ok()?))
        }
        TaskState::Pending | TaskState::Skipped | TaskState::Cancelled => None,
    }
}

/// Assemble one storyboard row: glyph · id · dimmed note · then the
/// time/cost/tail/lane suffix, column-aligned on RAW (pre-paint) widths
/// so ANSI escapes never skew the layout. Rows with no suffix keep the
/// legacy shape exactly (no trailing padding).
// `&Theme` to match the `frame` borrow that threads it here — the same
// one-calling-convention rationale as `append_failure_card`. The 8th
// parameter is the optional output tail — the same clap-surface idiom
// the run verb carries.
#[allow(clippy::trivially_copy_pass_by_ref, clippy::too_many_arguments)]
fn task_line(
    row: &TaskRow,
    view: &RunView,
    theme: &Theme,
    tick: usize,
    (id_w, note_w, time_w): (usize, usize, usize),
    time: Option<&str>,
    mark: bool,
    tail: Option<&str>,
) -> String {
    let mut note = row.note.clone();
    if row.state == TaskState::Running && !view.token_samples.is_empty() {
        let spark = theme.sparkline(&view.token_samples);
        if !spark.is_empty() {
            note = format!("{note} {spark}");
        }
    }
    let mut line = format!(
        "  {} {:<id_w$}  {}",
        theme.glyph(row.state, tick),
        row.id,
        theme.paint(Role::Dim, &note),
    );
    let cost = row.cost_usd.map(|c| format!(" · ${c:.4}"));
    if time.is_none() && cost.is_none() && !mark && tail.is_none() {
        return line;
    }
    // Column pad computed on the RAW note (paint added escapes, the
    // sparkline may ride a running row — transient shift accepted).
    let pad = note_w.saturating_sub(row.note.chars().count());
    line.push_str(&" ".repeat(pad + 2));
    let cell = format!("{:>time_w$}", time.unwrap_or(""));
    line.push_str(&theme.paint(Role::Dim, cell.trim_end()));
    if let Some(cost) = cost {
        line.push_str(&theme.paint(Role::Dim, &cost));
    }
    if let Some(tail) = tail {
        // Already painted by the shape module — one metadata unit.
        line.push_str("  ");
        line.push_str(tail);
    }
    if mark {
        line.push_str(&theme.paint(Role::Accent, if theme.ascii { " ||" } else { " ∥" }));
    }
    line
}

/// Render the COMPACT final card (spec §3.5 `--quiet` · "final card only ·
/// errors always") — the one-line verdict + cost, plus the failure card when
/// the run failed. NO per-task storyboard. A run with no verdict yet (called
/// before the terminal frame) renders the header alone.
#[must_use]
pub fn verdict_frame(view: &RunView, theme: &Theme) -> Vec<String> {
    let mut lines = Vec::with_capacity(4);
    let glyph = match view.verdict {
        Some(true) => theme.glyph(TaskState::Ok, 0),
        Some(false) => theme.glyph(TaskState::Failed, 0),
        None => theme.glyph(TaskState::Pending, 0),
    };
    let cost = match view.ceiling_usd {
        Some(c) => format!("${:.3} of ≤${c:.2}", view.cost_usd),
        None => format!("${:.3}", view.cost_usd),
    };
    #[allow(clippy::cast_precision_loss)] // display-only seconds
    let secs = view.elapsed_ms as f64 / 1000.0;
    lines.push(format!(
        "  {} {} · {} tasks · {secs:.1}s · {cost}",
        glyph,
        theme.paint(Role::Strong, &view.workflow),
        view.rows().len(),
    ));

    // Errors always (spec §3.5) — the same failure card the full frame emits,
    // appended so a quiet run still surfaces WHY it failed + the explain hint.
    if view.verdict == Some(false) {
        append_failure_card(&mut lines, view, theme);
    }
    lines
}

/// The failure card (workflow-level detail + per-failed-row detail + the
/// `nika explain` hint). Shared by the full [`frame`] and the compact
/// [`verdict_frame`] so the two surfaces can never drift on a failure.
// `&Theme` (not by-value) to match the `frame`/`verdict_frame` borrow that
// threads it here — one calling convention across the render surface.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn append_failure_card(lines: &mut Vec<String>, view: &RunView, theme: &Theme) {
    if let Some(detail) = &view.workflow_detail {
        lines.push(String::new());
        lines.push(format!(
            "  {}{}",
            theme.glyph(TaskState::Failed, 0),
            theme.paint(Role::Strong, detail),
        ));
        if let Some(code) = detail.split_whitespace().find(|w| w.starts_with("NIKA-")) {
            lines.push(theme.paint(Role::Dim, &format!("    fix: nika explain {code}")));
        }
    }
    for row in view.rows() {
        if row.state == TaskState::Failed && !row.detail.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "  {}{}",
                theme.glyph(TaskState::Failed, 0),
                theme.paint(Role::Strong, &row.detail),
            ));
            if let Some(code) = row
                .detail
                .split_whitespace()
                .find(|w| w.starts_with("NIKA-"))
            {
                lines.push(theme.paint(Role::Dim, &format!("    fix: nika explain {code}")));
            }
        }
    }
}

/// Extend a meter line with rule dashes to a stable width.
fn pad_rule(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        return text.to_owned();
    }
    let mut out = String::with_capacity(width * 3);
    out.push_str(text);
    out.extend(std::iter::repeat_n('─', width - len));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;

    fn fold(events: &[nika_event::Event]) -> RunView {
        let mut view = RunView::new();
        for ev in events {
            view.apply(ev);
        }
        view
    }

    const UNICODE: Theme = Theme {
        color: false,
        ascii: false,
        animate: false,
    };
    const ASCII: Theme = Theme {
        color: false,
        ascii: true,
        animate: false,
    };

    /// Golden frame — the unicode theme, colour off (the exact spec story).
    /// Time is a first-class column now: each settled row carries its wall
    /// time right-aligned after the note column, per-task spend follows on
    /// the rows whose completion reported one; the skipped row stays bare.
    #[test]
    fn golden_success_frame_unicode() {
        let lines = frame(&fold(&demo::success()), &UNICODE, 0);
        let expected = [
            "  🦋 nika · veille-news · 5 tasks · ceiling ≤ $0.04",
            "     permits ✓ network:read(hn.algolia.com) · fs:write(./out)",
            "",
            "  ✔  fetch_top     http 200 · 1.2s · 34 KB         1.2s",
            "  ✔  extract_ai    jq · 0.1s · 12 items           130ms",
            "  ✔  summarize     claude-sonnet · 3.1s · $0.011   3.0s · $0.0110",
            "  ✔  write_md      2.1 KB written                 290ms",
            "  ⊘  notify_slack  when: env.CI != 'true'",
        ];
        assert_eq!(&lines[..8], &expected[..]);
        // The meter line: pinned prefix + rule-padded to a stable width.
        assert!(
            lines[8].starts_with("  ── 5/5 done · $0.011 of ≤$0.04 · elapsed 4.7s "),
            "meter: {}",
            lines[8]
        );
        assert_eq!(lines[8].chars().count(), 66, "2-indent + 64-rule");
    }

    /// Golden frame — the ASCII theme is first-class, not best-effort.
    #[test]
    fn golden_success_frame_ascii() {
        let lines = frame(&fold(&demo::success()), &ASCII, 0);
        assert_eq!(
            lines[0],
            "  [nika] nika · veille-news · 5 tasks · ceiling ≤ $0.04"
        );
        assert_eq!(
            lines[3],
            "  ok fetch_top     http 200 · 1.2s · 34 KB         1.2s"
        );
    }

    #[test]
    fn golden_failure_card_carries_the_explain_hint() {
        let lines = frame(&fold(&demo::failure()), &UNICODE, 0);
        let tail = &lines[lines.len() - 2..];
        assert!(tail[0].contains("NIKA-431"), "headline: {tail:?}");
        assert_eq!(tail[1], "    fix: nika explain NIKA-431");
    }

    /// The cascade rows RENDER as §3.1 `◼` (the runtime's
    /// upstream-failure cancellation · dim · never red) — the fold and
    /// the glyph were each pinned alone; this pins the assembled line.
    #[test]
    fn golden_failure_frame_renders_cancelled_rows() {
        let lines = frame(&fold(&demo::failure()), &UNICODE, 0);
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("  ◼  write_md") && l.contains("upstream failed")),
            "unicode cancelled row: {lines:?}"
        );
        let ascii = frame(&fold(&demo::failure()), &ASCII, 0);
        assert!(
            ascii
                .iter()
                .any(|l| l.starts_with("  x  write_md") && l.contains("upstream failed")),
            "ascii cancelled row (err X ≠ cancelled x): {ascii:?}"
        );
    }

    /// A mid-retry run RENDERS the `↻` row (§3.1 — the attempt failed ·
    /// the TASK has not · the row holds until a terminal frame).
    #[test]
    fn golden_retrying_frame_renders_the_yellow_arrow() {
        let lines = frame(&fold(&demo::retrying()), &UNICODE, 0);
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("  ↻  summarize") && l.contains("rate limited")),
            "unicode retrying row: {lines:?}"
        );
        let ascii = frame(&fold(&demo::retrying()), &ASCII, 0);
        assert!(
            ascii
                .iter()
                .any(|l| l.starts_with("  r  summarize") && l.contains("rate limited")),
            "ascii retrying row: {ascii:?}"
        );
        // Still in flight: no terminal frame · no verdict line.
        let view = fold(&demo::retrying());
        assert_eq!(view.verdict, None, "a retrying run has no verdict yet");
    }

    /// `--quiet` compact card: the verdict line + cost, NO per-task rows.
    #[test]
    fn verdict_frame_is_compact_success() {
        let lines = verdict_frame(&fold(&demo::success()), &UNICODE);
        assert_eq!(lines.len(), 1, "success = one verdict line: {lines:?}");
        // Glyph carries its own trailing space (§3.1) + the line's space →
        // two, exactly the task-row convention (`✔  fetch_top`).
        assert!(
            lines[0].starts_with("  ✔  veille-news · 5 tasks · "),
            "verdict line: {}",
            lines[0]
        );
        assert!(lines[0].contains("$0.011 of ≤$0.04"), "cost: {}", lines[0]);
        // NOT the storyboard — no per-task row leaks into the quiet card.
        assert!(
            !lines.iter().any(|l| l.contains("fetch_top")),
            "quiet hides the per-task rows: {lines:?}"
        );
    }

    /// `--quiet` still surfaces errors (spec §3.5 "errors always") — the
    /// failure card + explain hint, the SAME the full frame renders.
    #[test]
    fn verdict_frame_keeps_the_failure_card() {
        let lines = verdict_frame(&fold(&demo::failure()), &UNICODE);
        assert!(
            lines[0].starts_with("  ✖ "),
            "failed verdict glyph: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("NIKA-431")),
            "the failure reason surfaces: {lines:?}"
        );
        assert!(
            lines.iter().any(|l| l == "    fix: nika explain NIKA-431"),
            "explain hint: {lines:?}"
        );
    }

    /// Called before a terminal frame (no verdict): header line only, no card.
    #[test]
    fn verdict_frame_no_verdict_is_header_only() {
        let lines = verdict_frame(&fold(&demo::retrying()), &UNICODE);
        assert_eq!(lines.len(), 1, "no verdict → one line: {lines:?}");
        assert!(lines[0].contains('○'), "pending glyph: {}", lines[0]);
    }

    /// The ASCII theme is first-class for the quiet card too.
    #[test]
    fn verdict_frame_ascii_theme() {
        let lines = verdict_frame(&fold(&demo::success()), &ASCII);
        assert!(lines[0].starts_with("  ok veille-news · "), "{}", lines[0]);
    }

    /// The verdict line carries elapsed time as SECONDS (`ms / 1000`): a
    /// mutated divisor (`* 1000` → `4_700_000s` · `% 1000` → `700s`) renders a
    /// wildly wrong duration, so the exact `4.7s` pins the conversion.
    #[test]
    fn verdict_frame_renders_elapsed_as_seconds() {
        let mut view = fold(&demo::success());
        view.elapsed_ms = 4700;
        let lines = verdict_frame(&view, &ASCII);
        assert!(lines[0].contains("4.7s"), "elapsed → seconds: {}", lines[0]);
    }

    #[test]
    fn frame_is_stable_under_ticks_when_nothing_runs() {
        let view = fold(&demo::success());
        assert_eq!(frame(&view, &UNICODE, 0), frame(&view, &UNICODE, 9));
    }

    /// A view with output-carrying completions: `frame_with_outputs`
    /// appends the bounded shape tail (+ tokens) on those rows while
    /// `frame` stays BYTE-IDENTICAL to today — the piped/CI register
    /// never grows tails.
    #[test]
    fn frame_with_outputs_adds_tails_and_frame_stays_bare() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};

        let mut view = RunView::new();
        let task = |n: &str| KeyValue::new("task", Value::String(n.to_owned()));
        view.apply(&demo::bare_event(EventKind::TaskStarted, 0).with_field(task("audit")));
        view.apply(
            &demo::bare_event(EventKind::TaskCompleted, 100)
                .with_field(task("audit"))
                .with_field(KeyValue::new(
                    "output",
                    Value::String(r#"{"verdict":"P0","fixes":[1,2]}"#.to_owned()),
                ))
                .with_field(KeyValue::new("tokens", Value::Int(90))),
        );
        let with = frame_with_outputs(&view, &UNICODE, 0);
        let audit = with.iter().find(|l| l.contains("audit")).expect("row");
        assert!(
            audit.contains("→ {fixes[2], verdict} · 30B · 90 tok"),
            "tail rides the completed row: {audit}"
        );
        let bare = frame(&view, &UNICODE, 0);
        assert!(
            !bare.iter().any(|l| l.contains('→')),
            "the bare frame never grows tails: {bare:?}"
        );

        // ASCII parity — the arrow degrades, nothing unicode leaks.
        let ascii = frame_with_outputs(&view, &ASCII, 0);
        assert!(
            ascii.iter().any(|l| l.contains("-> {fixes[2], verdict}")),
            "{ascii:?}"
        );

        // The demo storyboard carries no output fields — with-outputs
        // renders byte-identically to the bare frame (no invented data).
        let demo_view = fold(&demo::success());
        assert_eq!(
            frame_with_outputs(&demo_view, &UNICODE, 0),
            frame(&demo_view, &UNICODE, 0),
            "no output fields → no tails, ever"
        );
    }

    /// The running row shows a LIVE elapsed (now − started) and `∥` marks
    /// the wave-siblings that actually overlapped — with `||` parity under
    /// the ASCII theme (never a unicode leak).
    #[test]
    fn lanes_and_live_elapsed_render_in_both_themes() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};

        let mut view = RunView::new();
        let task = |name: &str| KeyValue::new("task", Value::String(name.to_owned()));
        view.apply(&demo::bare_event(EventKind::TaskStarted, 100).with_field(task("a")));
        view.apply(&demo::bare_event(EventKind::TaskStarted, 150).with_field(task("b")));
        // a settles at 1000 with a REAL measured duration (900ms) → its
        // reconstructed interval [100, 1000] overlaps b's [150, now].
        view.apply(
            &demo::bare_event(EventKind::TaskCompleted, 1000)
                .with_field(task("a"))
                .with_field(KeyValue::new("duration_ms", Value::Int(900))),
        );

        let lines = frame(&view, &UNICODE, 0);
        let a = lines.iter().find(|l| l.contains(" a ")).expect("a row");
        assert!(
            a.contains("900ms") && a.ends_with('∥'),
            "settled sibling: duration + lane marker: {a}"
        );
        let b = lines.iter().find(|l| l.contains(" b ")).expect("b row");
        assert!(
            b.contains("850ms") && b.ends_with('∥'),
            "running sibling: LIVE elapsed (1000−150) + marker: {b}"
        );

        let ascii = frame(&view, &ASCII, 0);
        assert!(
            ascii.iter().any(|l| l.ends_with("900ms ||")),
            "ascii parity ∥→||: {ascii:?}"
        );
        assert!(
            !ascii.iter().any(|l| l.contains('∥')),
            "no unicode leaks into --ascii: {ascii:?}"
        );
    }

    /// The sparkline rides the RUNNING row exactly when samples exist —
    /// both injection guards are semantic, not cosmetic.
    #[test]
    fn running_row_carries_sparkline_only_with_samples() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};

        // A running task with NO samples: no spark glyph anywhere.
        let mut without = RunView::new();
        without.apply(
            &demo::bare_event(EventKind::TaskStarted, 10)
                .with_field(KeyValue::new("task", Value::String("summarize".into())))
                .with_field(KeyValue::new("note", Value::String("infer".into()))),
        );
        let lines = frame(&without, &UNICODE, 0);
        assert!(
            !lines.iter().any(|l| l.contains('▇')),
            "no samples → no spark: {lines:?}"
        );

        // A completed task reported tokens: the spark appears on the
        // RUNNING line (single sample 710 → top bar).
        let mut with = RunView::new();
        with.apply(
            &demo::bare_event(EventKind::TaskCompleted, 5)
                .with_field(KeyValue::new("task", Value::String("fetch".into())))
                .with_field(KeyValue::new("tokens", Value::Int(710))),
        );
        with.apply(
            &demo::bare_event(EventKind::TaskStarted, 10)
                .with_field(KeyValue::new("task", Value::String("summarize".into())))
                .with_field(KeyValue::new("note", Value::String("infer".into()))),
        );
        let lines = frame(&with, &UNICODE, 0);
        let running = lines
            .iter()
            .find(|l| l.contains("summarize"))
            .expect("running row renders");
        assert!(
            running.contains('▇'),
            "tokens reported → spark on the running row: {running}"
        );
    }

    /// The failure card targets FAILED rows only — an Ok row that happens
    /// to carry a `detail` field renders no card (the `&&` is semantic).
    #[test]
    fn failure_card_ignores_ok_rows_with_detail() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};

        let mut view = RunView::new();
        view.apply(
            &demo::bare_event(EventKind::TaskCompleted, 5)
                .with_field(KeyValue::new("task", Value::String("ok_task".into())))
                .with_field(KeyValue::new(
                    "detail",
                    Value::String("NIKA-999 retried twice, recovered".into()),
                )),
        );
        view.apply(
            &demo::bare_event(EventKind::TaskFailed, 10)
                .with_field(KeyValue::new("task", Value::String("bad_task".into())))
                .with_field(KeyValue::new(
                    "detail",
                    Value::String("NIKA-440 · boom".into()),
                )),
        );
        view.apply(&demo::bare_event(EventKind::WorkflowFailed, 20));

        let lines = frame(&view, &UNICODE, 0);
        let card_lines: Vec<&String> = lines.iter().filter(|l| l.contains("NIKA-")).collect();
        assert_eq!(
            card_lines.len(),
            2,
            "headline + explain hint for the ONE failed row only: {lines:?}"
        );
        assert!(card_lines[0].contains("NIKA-440"));
    }
}
