// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Frame rendering (spec §3.3) — a pure function `(RunView, Theme, tick) →
//! lines`. No I/O here: the replay loop owns the terminal, this module owns
//! the truth-to-text mapping. Snapshot tests pin BOTH glyph themes.

use crate::flow::{fmt_wall_ms, lane_marks};
use crate::format::fmt_cost_usd;
use crate::state::{RunView, TaskRow, TaskState};
use crate::theme::{Role, Theme};

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

/// The header block (identity + ceiling + the audit-as-greeting permits
/// line + one blank): shared by the full frame (task count = the rows)
/// and the streamed plain narration (count from the injected plan —
/// rows don't exist yet at the header moment). `tasks = 0` omits the
/// count cell — a stream without a plan must not open on a lie.
// `&Theme` to match the frame borrows that thread it here.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn header_lines(view: &RunView, theme: &Theme, tasks: usize) -> Vec<String> {
    let mut lines = Vec::with_capacity(3);
    let count = if tasks > 0 {
        format!(" · {tasks} tasks")
    } else {
        String::new()
    };
    let ceiling = view
        .ceiling_usd
        .map(|c| format!(" · ceiling ≤ {}", fmt_cost_usd(c)))
        .unwrap_or_default();
    lines.push(format!(
        "  {} nika · {}{count}{ceiling}",
        theme.logo(),
        theme.paint(Role::Strong, &view.workflow),
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
    lines
}

/// The one frame assembler behind both public forms.
// `&Theme` to match the public `frame` borrow that threads it here — the
// same one-calling-convention rationale as `task_line`.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn frame_impl(view: &RunView, theme: &Theme, tick: usize, outputs: bool) -> Vec<String> {
    let mut lines = Vec::with_capacity(view.rows().len() + 6);
    lines.extend(header_lines(view, theme, view.rows().len()));

    // Task rows — stable order, aligned ids, notes dimmed. Time and cost
    // are first-class columns: a settled row carries its REAL wall time
    // (+ per-task spend when the stream reported one), a running row its
    // live elapsed, and `∥` marks wave-siblings that actually overlapped.
    // Never-ran rows speak their REASON (`cache hit (resume)` · `when:
    // false` · `blocked · <task> failed`) through the display-note map.
    let width = view.rows().iter().map(|r| r.id.len()).max().unwrap_or(8);
    let marks = lane_marks(view);
    let times: Vec<Option<String>> = view.rows().iter().map(|r| row_wall(r, view)).collect();
    let notes: Vec<String> = view.rows().iter().map(|r| display_note(r, view)).collect();
    let note_w = notes
        .iter()
        .map(|n| n.chars().count())
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
            crate::shape::output_tail(row.output_json.as_deref(), row.tokens, theme)
        } else {
            None
        };
        lines.push(task_line(
            row,
            (view, &notes[i]),
            theme,
            tick,
            (width, note_w, time_w),
            times[i].as_deref(),
            mark,
            tail.as_deref(),
        ));
    }

    lines.push(meter_line(view, theme));

    // Failure card (only on a failed verdict · derives the explain hint) —
    // the SAME card the compact `--quiet` surface renders (shared helper).
    if view.verdict == Some(false) {
        append_failure_card(&mut lines, view, theme);
    }
    lines
}

/// The footer meter: progress · live cost vs ceiling · wall clock. The
/// spend speaks the ONE cost formatter (format.rs) — the meter and the
/// verdict card can never again disagree on the same run's dollars.
/// The repair count rides beside `done` when non-zero (#319 · the
/// `(N unpriced)` honesty style): a repaired run's final summary line
/// must never read byte-identical to a clean one. Shared by the full
/// frame and the streamed plain close (#321).
// `&Theme` to match the frame borrows that thread it here.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn meter_line(view: &RunView, theme: &Theme) -> String {
    let cost = spend_meter(view);
    // Failed rides beside `done` (which counts EVERY terminal state) so a
    // failing run's meter never reads like a clean one (live 2026-07-10 · #393).
    let failed = match view.failed_count() {
        0 => String::new(),
        n => format!("{n} failed · "),
    };
    let repaired = match view.recovered_count() {
        0 => String::new(),
        n => format!("{n} recovered · "),
    };
    #[allow(clippy::cast_precision_loss)] // display-only seconds
    let secs = view.elapsed_ms as f64 / 1000.0;
    let meter = format!(
        "── {}/{} done · {failed}{repaired}{cost} · elapsed {secs:.1}s ",
        view.done_count(),
        view.rows().len(),
    );
    format!("  {}", theme.paint(Role::Dim, &pad_rule(&meter, 64)))
}

/// The streamed header (#321 · the plain narration lane): the same
/// identity + permits lines the frame opens with, printable the moment
/// `workflow_started` folds. The task count prefers the injected wave
/// plan (the run verb injects it BEFORE driving — no row exists yet at
/// the header moment); a plan-less fold (a bare replay) falls back to
/// the rows seen so far, and a zero count is omitted, never printed.
// `&Theme` to match the sink borrow that threads it here.
#[allow(clippy::trivially_copy_pass_by_ref)]
#[must_use]
pub fn stream_header(view: &RunView, theme: &Theme) -> Vec<String> {
    let tasks = view
        .plan()
        .map(|waves| waves.iter().map(Vec::len).sum())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| view.rows().len());
    header_lines(view, theme, tasks)
}

/// One settled row, streamed at its terminal frame (#321 · the plain
/// narration lane): the same cells the final frame renders (glyph · id ·
/// note · wall · spend · the repair fact · ∥ — all final by settle
/// time) minus the table-wide note alignment (a stream cannot pad
/// against rows that haven't spoken yet). `None` when the id names no
/// folded row (a malformed frame renders nothing, never garbage).
// `&Theme` to match the sink borrow that threads it here.
#[allow(clippy::trivially_copy_pass_by_ref)]
#[must_use]
pub fn stream_settled_line(
    view: &RunView,
    task: &str,
    theme: &Theme,
    outputs: bool,
) -> Option<String> {
    let (i, row) = view.rows().iter().enumerate().find(|(_, r)| r.id == task)?;
    let id_w = view.rows().iter().map(|r| r.id.len()).max().unwrap_or(8);
    let note = display_note(row, view);
    let time = row_wall(row, view);
    let time_w = time.as_ref().map_or(0, |t| t.chars().count());
    let mark = lane_marks(view).get(i).copied().unwrap_or(false);
    let tail = if outputs {
        crate::shape::output_tail(row.output_json.as_deref(), row.tokens, theme)
    } else {
        None
    };
    Some(task_line(
        row,
        (view, &note),
        theme,
        0,
        (id_w, note.chars().count(), time_w),
        time.as_deref(),
        mark,
        tail.as_deref(),
    ))
}

/// The streamed close (#321): the meter + the failure card. The rows
/// already spoke at their settle — the plain final print never repeats
/// them (a captured log reads the run ONCE, top to bottom).
// `&Theme` to match the sink borrow that threads it here.
#[allow(clippy::trivially_copy_pass_by_ref)]
#[must_use]
pub fn stream_summary(view: &RunView, theme: &Theme) -> Vec<String> {
    let mut lines = vec![meter_line(view, theme)];
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

/// The row's DISPLAY note — the skip-reason vocabulary over the raw
/// fold note. Three never-ran classes speak distinctly: a rehydrated
/// row says `cache hit (resume)`, a closed `when:` gate says `when:
/// false`, a dead-path cancellation says `blocked · <task> failed`
/// (naming the failed upstream when the run has exactly ONE failed
/// task — with several, ancestry is ambiguous from the stream alone,
/// so the honest generic `upstream failed` stays). Every other note
/// renders verbatim (the runtime's vocabulary is already teaching).
fn display_note(row: &TaskRow, view: &RunView) -> String {
    if row.cached {
        return "cache hit (resume)".to_owned();
    }
    match (row.state, row.note.as_str()) {
        (TaskState::Skipped, "when: gate closed") => "when: false".to_owned(),
        (TaskState::Cancelled, "upstream failed") => {
            let failed: Vec<&str> = view
                .rows()
                .iter()
                .filter(|r| r.state == TaskState::Failed)
                .map(|r| r.id.as_str())
                .collect();
            match failed.as_slice() {
                [one] => format!("blocked · {one} failed"),
                _ => "blocked · upstream failed".to_owned(),
            }
        }
        _ => row.note.clone(),
    }
}

/// The row's glyph — the ONE render-level override: a cache-hit row is
/// Ok in the fold (the value is real) but reads as the skip family
/// (`↷` · it never ran HERE), so green stays reserved for work done.
// `&Theme` to match the `task_line` borrow that threads it here.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn row_glyph(row: &TaskRow, theme: &Theme, tick: usize) -> String {
    if row.cached {
        return theme.glyph(TaskState::Skipped, tick);
    }
    theme.glyph(row.state, tick)
}

/// Assemble one storyboard row: glyph · id · dimmed note · then the
/// time/cost/tail/lane suffix, column-aligned on RAW (pre-paint) widths
/// so ANSI escapes never skew the layout. Rows with no suffix keep the
/// legacy shape exactly (no trailing padding).
// `&Theme` to match the `frame` borrow that threads it here — the same
// one-calling-convention rationale as `append_failure_card`. The 8th
// parameter is the optional output tail — the same clap-surface idiom
// the run verb carries; the display note rides beside the view it was
// derived from.
#[allow(clippy::trivially_copy_pass_by_ref, clippy::too_many_arguments)]
fn task_line(
    row: &TaskRow,
    (view, display_note): (&RunView, &str),
    theme: &Theme,
    tick: usize,
    (id_w, note_w, time_w): (usize, usize, usize),
    time: Option<&str>,
    mark: bool,
    tail: Option<&str>,
) -> String {
    let mut note = display_note.to_owned();
    if row.state == TaskState::Running && !view.token_samples.is_empty() {
        let spark = theme.sparkline(&view.token_samples);
        if !spark.is_empty() {
            note = format!("{note} {spark}");
        }
    }
    let mut line = format!(
        "  {} {:<id_w$}  {}",
        row_glyph(row, theme, tick),
        row.id,
        theme.paint(Role::Dim, &note),
    );
    let cost = row.cost_usd.map(|c| format!(" · {}", fmt_cost_usd(c)));
    if time.is_none() && cost.is_none() && !mark && tail.is_none() && !row.recovered {
        return line;
    }
    // Column pad computed on the RAW note (paint added escapes, the
    // sparkline may ride a running row — transient shift accepted).
    let pad = note_w.saturating_sub(display_note.chars().count());
    line.push_str(&" ".repeat(pad + 2));
    let slow = theme.accents && is_slow(row, view);
    let cell = duration_cell(theme, time, time_w);
    let cell_role = if slow { Role::Warn } else { Role::Dim };
    line.push_str(&theme.paint(cell_role, cell.trim_end()));
    if slow {
        line.push(' ');
        line.push_str(&theme.paint(Role::Warn, "slow"));
    }
    if let Some(cost) = cost {
        line.push_str(&theme.paint(Role::Dim, &cost));
    }
    // The repair fact (#319 · D-2026-07-08-N4): a row that settled
    // through `on_error.recover` says so — yellow, the retry family's
    // survived-incident colour (sober themes render it plain).
    if row.recovered {
        line.push_str(&theme.paint(Role::Warn, " · recovered"));
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

/// The SLOW threshold (nextest school · design §1.4): a settled task
/// whose wall time exceeds `max(2 × median settled duration, 30s)`
/// self-identifies. `None` until at least TWO tasks settled — a median
/// of one can only compare the task to itself. The 30s floor keeps
/// fast runs (mock demos · sub-second pipelines) accent-free: nothing
/// moves that doesn't inform.
fn slow_threshold_ms(view: &RunView) -> Option<u64> {
    let mut walls: Vec<u64> = view
        .rows()
        .iter()
        .filter(|r| matches!(r.state, TaskState::Ok | TaskState::Failed))
        .filter_map(TaskRow::wall_ms)
        .collect();
    if walls.len() < 2 {
        return None;
    }
    walls.sort_unstable();
    let median = walls[walls.len() / 2];
    Some(median.saturating_mul(2).max(30_000))
}

/// Does this row's REAL wall time cross the run's SLOW threshold?
/// Settled rows only — a running row's elapsed is still moving, its
/// verdict can wait for the terminal frame.
fn is_slow(row: &TaskRow, view: &RunView) -> bool {
    matches!(row.state, TaskState::Ok | TaskState::Failed)
        && slow_threshold_ms(view)
            .zip(row.wall_ms())
            .is_some_and(|(threshold, wall)| wall > threshold)
}

/// The duration cell: bare right-aligned (`  2.7s` · the sober
/// registers) or the nextest bracket form (`[  2.7s]`) under the
/// interactive accents. Width math happens on RAW text (paint comes
/// after) — ANSI never skews the column; a row without a duration
/// stays empty in both forms.
// `&Theme` to match the `task_line` borrow that threads it here.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn duration_cell(theme: &Theme, time: Option<&str>, time_w: usize) -> String {
    match (theme.accents, time) {
        (true, Some(t)) => format!("[{t:>time_w$}]"),
        (true, None) => String::new(),
        (false, t) => format!("{:>time_w$}", t.unwrap_or("")),
    }
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
    let cost = spend_meter(view);
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
/// Presentation-only dedup: a builtin's content opens with its own spec
/// code AND the stamped headline prefixes the same code — the RENDERED
/// line says it once. The error DATA keeps both (the agent-repair oracle
/// reads the typed code from the raw string — the #392 CI lesson).
fn dedup_code_line(detail: &str) -> String {
    let Some(code) = detail.split_whitespace().find(|w| w.starts_with("NIKA-")) else {
        return detail.to_owned();
    };
    let opener = format!("{code} · ");
    match detail.find(&opener) {
        Some(first) => {
            let tail_at = first + opener.len();
            match detail[tail_at..].find(&opener) {
                Some(second) => {
                    let abs = tail_at + second;
                    format!("{}{}", &detail[..abs], &detail[abs + opener.len()..])
                }
                None => detail.to_owned(),
            }
        }
        None => detail.to_owned(),
    }
}

// `&Theme` (not by-value) to match the `frame`/`verdict_frame` borrow that
// threads it here — one calling convention across the render surface.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn append_failure_card(lines: &mut Vec<String>, view: &RunView, theme: &Theme) {
    if let Some(detail) = &view.workflow_detail {
        let detail = &dedup_code_line(detail);
        lines.push(String::new());
        lines.push(format!(
            "  {}{}",
            theme.glyph(TaskState::Failed, 0),
            theme.paint(Role::Strong, detail),
        ));
        if let Some(code) = detail.split_whitespace().find(|w| w.starts_with("NIKA-")) {
            lines.push(format!(
                "    {}",
                crate::vocab::hint(*theme, "fix", &format!("nika explain {code}"))
            ));
        }
    }
    for row in view.rows() {
        if row.state == TaskState::Failed && !row.detail.is_empty() {
            let detail = dedup_code_line(&row.detail);
            lines.push(String::new());
            lines.push(format!(
                "  {}{}",
                theme.glyph(TaskState::Failed, 0),
                theme.paint(Role::Strong, &detail),
            ));
            if let Some(code) = detail.split_whitespace().find(|w| w.starts_with("NIKA-")) {
                lines.push(format!(
                    "    {}",
                    crate::vocab::hint(*theme, "fix", &format!("nika explain {code}"))
                ));
            }
        }
    }
}

/// Extend a meter line with rule dashes to a stable width.
/// The run-level spend string — the ONE composition both the footer
/// meter and the verdict card speak: `X of ≤Y` against a static
/// ceiling when one is known, and a `≥ … (N unpriced)` marker when
/// part of the run carried no meterable price (a partial total must
/// never read as complete).
fn spend_meter(view: &RunView) -> String {
    let base = match view.ceiling_usd {
        Some(c) => format!("{} of ≤{}", fmt_cost_usd(view.cost_usd), fmt_cost_usd(c)),
        None => fmt_cost_usd(view.cost_usd),
    };
    if view.unpriced_calls > 0 {
        format!("≥ {base} ({} unpriced)", view.unpriced_calls)
    } else {
        base
    }
}

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

    const UNICODE: Theme = Theme::new(false, false, false);
    const ASCII: Theme = Theme::new(false, true, false);

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
            "  ✔  summarize     claude-sonnet · 3.1s · $0.011   3.0s · $0.01",
            "  ✔  write_md      2.1 KB written                 290ms",
            "  ↷  notify_slack  when: env.CI != 'true'",
        ];
        assert_eq!(&lines[..8], &expected[..]);
        // The meter line: pinned prefix + rule-padded to a stable width.
        assert!(
            lines[8].starts_with("  ── 5/5 done · $0.01 of ≤$0.04 · elapsed 4.7s "),
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
    fn the_per_row_failure_detail_reaches_the_operator() {
        // The user-sim finding: `row.detail` was documented « feeds the
        // failure card » but stamp_terminal never copied it — the journal
        // carried « NIKA-VAR-001 · supply it with --var … » while the
        // live render showed a mute ✖. The WHY must be IN the frame.
        let lines = frame(&fold(&demo::failure()), &UNICODE, 0);
        assert!(
            lines
                .iter()
                .any(|l| l.contains("provider refused (429) · retried 2×")),
            "the TaskFailed `detail` field must render: {lines:?}"
        );
    }

    #[test]
    fn golden_failure_card_carries_the_explain_hint() {
        let lines = frame(&fold(&demo::failure()), &UNICODE, 0);
        let tail = &lines[lines.len() - 2..];
        assert!(tail[0].contains("NIKA-431"), "headline: {tail:?}");
        assert_eq!(tail[1], "    fix: nika explain NIKA-431");
        // The meter's honesty counter (#393): a failing run's summary line
        // never reads byte-identical to a clean one.
        let meter = lines.iter().find(|l| l.contains("done")).expect("meter");
        assert!(
            meter.contains(" failed · "),
            "meter counts the failure: {meter}"
        );
    }

    /// Presentation dedup (#393 · the #392 CI lesson kept the DATA intact):
    /// a headline carrying the same spec code twice renders it once ·
    /// foreign or single codes stay untouched.
    #[test]
    fn failure_card_dedups_a_double_spec_code() {
        assert_eq!(
            dedup_code_line(
                "NIKA-BUILTIN-WRITE-001 · tool `nika:write` reported an error: NIKA-BUILTIN-WRITE-001 · parent directory missing"
            ),
            "NIKA-BUILTIN-WRITE-001 · tool `nika:write` reported an error: parent directory missing"
        );
        assert_eq!(
            dedup_code_line("NIKA-431 · provider refused (429)"),
            "NIKA-431 · provider refused (429)"
        );
        assert_eq!(dedup_code_line("no code here"), "no code here");
    }

    /// The cascade rows RENDER as blocked `⊘` (the runtime's
    /// upstream-failure cancellation · dim · never red) — and because
    /// the demo failure has exactly ONE failed task, the note NAMES it:
    /// `blocked · summarize failed` (unambiguous from the stream).
    #[test]
    fn golden_failure_frame_renders_cancelled_rows() {
        let lines = frame(&fold(&demo::failure()), &UNICODE, 0);
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("  ⊘  write_md") && l.contains("blocked · summarize failed")),
            "unicode blocked row names the one failed upstream: {lines:?}"
        );
        let ascii = frame(&fold(&demo::failure()), &ASCII, 0);
        assert!(
            ascii
                .iter()
                .any(|l| l.starts_with("  x  write_md") && l.contains("blocked · summarize failed")),
            "ascii blocked row (err X ≠ blocked x): {ascii:?}"
        );
    }

    /// The three never-ran classes render DISTINCTLY (the skip-reason
    /// law): `↷ cache hit (resume)` · `↷ when: false` · `⊘ blocked ·
    /// upstream failed` — the generic blocked form when SEVERAL tasks
    /// failed (ancestry is ambiguous from the stream alone).
    #[test]
    fn skip_reasons_render_their_three_classes() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let task = |n: &str| KeyValue::new("task", Value::String(n.to_owned()));
        let note = |n: &str| KeyValue::new("note", Value::String(n.to_owned()));

        let mut view = RunView::new();
        view.apply(&demo::bare_event(EventKind::TaskCacheHit, 5).with_field(task("read")));
        view.apply(
            &demo::bare_event(EventKind::TaskSkipped, 10)
                .with_field(task("deploy"))
                .with_field(note("when: gate closed")),
        );
        // TWO failures → the blocked row cannot name ONE upstream.
        view.apply(&demo::bare_event(EventKind::TaskFailed, 15).with_field(task("a")));
        view.apply(&demo::bare_event(EventKind::TaskFailed, 16).with_field(task("b")));
        view.apply(
            &demo::bare_event(EventKind::TaskCancelled, 20)
                .with_field(task("notify"))
                .with_field(note("upstream failed")),
        );

        let lines = frame(&view, &UNICODE, 0);
        let find = |id: &str| {
            lines
                .iter()
                .find(|l| l.contains(id))
                .cloned()
                .expect("every staged row renders")
        };
        assert!(
            find("read").starts_with("  ↷ ") && find("read").contains("cache hit (resume)"),
            "cache hit: {}",
            find("read")
        );
        assert!(
            find("deploy").starts_with("  ↷ ") && find("deploy").contains("when: false"),
            "when gate: {}",
            find("deploy")
        );
        assert!(
            find("notify").starts_with("  ⊘ ")
                && find("notify").contains("blocked · upstream failed"),
            "ambiguous blocked stays generic: {}",
            find("notify")
        );

        // ASCII parity for the whole vocabulary: ~> skip · x blocked.
        let ascii = frame(&view, &ASCII, 0);
        assert!(
            ascii.iter().any(|l| l.starts_with("  ~> read")),
            "ascii skip glyph: {ascii:?}"
        );
        assert!(
            ascii.iter().any(|l| l.starts_with("  x  notify")),
            "ascii blocked glyph: {ascii:?}"
        );
        assert!(
            !ascii.iter().any(|l| l.contains('↷') || l.contains('⊘')),
            "no unicode leaks into --ascii: {ascii:?}"
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
        assert!(lines[0].contains("$0.01 of ≤$0.04"), "cost: {}", lines[0]);
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

    /// The interactive accents bracket the duration column (nextest
    /// school · `[  1.2s]`), right-aligned inside the SAME width the
    /// sober form uses — and rows without a duration (the skipped row)
    /// grow no brackets. The sober frame stays byte-identical to the
    /// golden above by construction (accents default OFF).
    #[test]
    fn accents_bracket_the_duration_column_tty_only() {
        let mut accented = UNICODE;
        accented.accents = true;
        let lines = frame(&fold(&demo::success()), &accented, 0);
        assert!(
            lines[3].ends_with("[ 1.2s]"),
            "bracketed right-aligned cell: {}",
            lines[3]
        );
        assert!(
            lines[4].ends_with("[130ms]"),
            "the widest cell sets the width: {}",
            lines[4]
        );
        assert!(
            !lines[7].contains('['),
            "the skipped row grows no brackets: {}",
            lines[7]
        );
        // Cost still rides AFTER the bracketed cell on the row that has one.
        assert!(
            lines[5].contains("[ 3.0s] · $0.01"),
            "cost follows the cell: {}",
            lines[5]
        );
        // The demo runs sub-second — under the 30s SLOW floor, so even
        // the accented frame carries no accidental `slow` marker.
        assert!(
            !lines.iter().any(|l| l.contains("slow")),
            "fast runs stay marker-free: {lines:?}"
        );
    }

    /// The SLOW accent (design §1.4): a settled task past
    /// `max(2 × median, 30s)` renders its duration YELLOW + the `slow`
    /// word — interactive accents only, and the threshold floor keeps
    /// mid-scale tasks quiet.
    #[test]
    fn slow_tasks_self_identify_under_accents() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let task = |n: &str| KeyValue::new("task", Value::String(n.to_owned()));
        let dur = |ms: i64| KeyValue::new("duration_ms", Value::Int(ms));

        let mut view = RunView::new();
        for (name, ms, at) in [
            ("a", 1_000, 1_000u64),
            ("b", 1_200, 2_000),
            ("c", 100_000, 5_000),
        ] {
            view.apply(&demo::bare_event(EventKind::TaskStarted, 0).with_field(task(name)));
            view.apply(
                &demo::bare_event(EventKind::TaskCompleted, at)
                    .with_field(task(name))
                    .with_field(dur(ms)),
            );
        }

        // Sober register: never the marker, whatever the durations.
        let sober = frame(&view, &UNICODE, 0);
        assert!(!sober.iter().any(|l| l.contains("slow")), "{sober:?}");

        // Accents + colour: the 100s task (median 1.2s → floor 30s)
        // carries the yellow cell + the word; its siblings stay dim.
        let mut accented = Theme::new(true, false, false);
        accented.accents = true;
        let lines = frame(&view, &accented, 0);
        let c_row = lines.iter().find(|l| l.contains(" c ")).expect("c row");
        assert!(
            c_row.contains("\x1b[33m") && c_row.contains("slow"),
            "the slow task self-identifies in yellow: {c_row:?}"
        );
        let a_row = lines.iter().find(|l| l.contains(" a ")).expect("a row");
        assert!(
            !a_row.contains("slow") && !a_row.contains("\x1b[33m"),
            "median-scale siblings stay quiet: {a_row:?}"
        );

        // The floor: 25s over a 20s median is NOT slow (2×median = 40s
        // wins the max) — no marker.
        let mut mid = RunView::new();
        for (name, ms, at) in [
            ("x", 10_000, 1_000u64),
            ("y", 20_000, 2_000),
            ("z", 25_000, 3_000),
        ] {
            mid.apply(&demo::bare_event(EventKind::TaskStarted, 0).with_field(task(name)));
            mid.apply(
                &demo::bare_event(EventKind::TaskCompleted, at)
                    .with_field(task(name))
                    .with_field(dur(ms)),
            );
        }
        let quiet = frame(&mid, &accented, 0);
        assert!(
            !quiet.iter().any(|l| l.contains("slow")),
            "2x-median dominates the floor: {quiet:?}"
        );
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

    /// #319 — a repaired success SAYS so: the settled row gains
    /// ` · recovered` and the meter line counts the repairs — while a
    /// clean run's frame stays byte-identical (the golden tests above
    /// pin that: the demo storyboard carries no `task_recovered`).
    #[test]
    fn recovered_rows_and_meter_carry_the_repair_fact() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let task = |n: &str| KeyValue::new("task", Value::String(n.to_owned()));

        let mut view = RunView::new();
        view.apply(
            &demo::bare_event(EventKind::TaskStarted, 0)
                .with_field(task("fragile"))
                .with_field(KeyValue::new(
                    "note",
                    Value::String("invoke · nika:read".to_owned()),
                )),
        );
        view.apply(
            &demo::bare_event(EventKind::TaskRecovered, 1)
                .with_field(task("fragile"))
                .with_field(KeyValue::new(
                    "code",
                    Value::String("NIKA-BUILTIN-READ-001".to_owned()),
                )),
        );
        view.apply(
            &demo::bare_event(EventKind::TaskCompleted, 2)
                .with_field(task("fragile"))
                .with_field(KeyValue::new("duration_ms", Value::Int(1))),
        );
        view.apply(&demo::bare_event(EventKind::WorkflowCompleted, 3));

        let lines = frame(&view, &UNICODE, 0);
        let row = lines.iter().find(|l| l.contains("fragile")).expect("row");
        assert!(
            row.contains("1ms · recovered"),
            "the settled line says recovered: {row}"
        );
        let meter = lines.iter().find(|l| l.contains("done")).expect("meter");
        assert!(
            meter.contains("1/1 done · 1 recovered · "),
            "the summary line counts the repair: {meter}"
        );

        // The SUCCESS path only — no failure card grew out of the repair.
        assert!(
            !lines.iter().any(|l| l.contains("fix:")),
            "a recovered success is a success: {lines:?}"
        );

        // Colour ON: the fact paints yellow (the retry family).
        let coloured = frame(&view, &Theme::new(true, false, false), 0);
        let painted = coloured
            .iter()
            .find(|l| l.contains("fragile"))
            .expect("row");
        assert!(
            painted.contains("\x1b[33m · recovered\x1b[0m"),
            "recovered paints Warn: {painted:?}"
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
