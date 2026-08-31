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
/// The LIVING map — the whole DAG as one wave-column line right under
/// the header: every node wears its state (pending dim · running = its
/// verb's own motion frame, bright · settled Good/Bad · skipped ⊘),
/// `⇉` between waves. Repainted every tick, so the running node's
/// spinner turns INSIDE the map. Interactive surface only (accents ·
/// a plan · more than one task); wide runs drop the ids and keep the
/// chips so the map never wraps.
fn map_line(view: &RunView, theme: Theme, tick: usize) -> Option<String> {
    if !theme.accents || view.external_map || view.rows().len() < 2 {
        return None;
    }
    let plan = view.plan()?;
    let by_id: std::collections::BTreeMap<&str, &TaskRow> =
        view.rows().iter().map(|r| (r.id.as_str(), r)).collect();
    let total: usize = plan.iter().map(Vec::len).sum();
    let with_ids = total <= 8;
    let sep = format!(" {} ", theme.paint(Role::Dim, "⇉"));
    let waves: Vec<String> = plan
        .iter()
        .map(|wave| {
            let nodes: Vec<String> = wave
                .iter()
                .map(|id| map_node(by_id.get(id.as_str()).copied(), id, theme, tick, with_ids))
                .collect();
            nodes.join(if with_ids { " · " } else { "" })
        })
        .collect();
    Some(format!("     {}", waves.join(&sep)))
}

/// One map node: the state-painted glyph (+ id on small runs).
fn map_node(row: Option<&TaskRow>, id: &str, theme: Theme, tick: usize, with_id: bool) -> String {
    let (glyph, role) = match row.map(|r| &r.state) {
        Some(TaskState::Running) => {
            let spin = theme.verb_spin(row.and_then(row_verb), tick);
            return if with_id {
                format!("{spin}{}", theme.paint(Role::Strong, id))
            } else {
                spin.trim_end().to_owned()
            };
        }
        Some(TaskState::Ok) => (
            theme.verb_glyph_bare(row.and_then(row_verb)).to_owned(),
            Role::Good,
        ),
        Some(TaskState::Failed) => (
            theme.verb_glyph_bare(row.and_then(row_verb)).to_owned(),
            Role::Bad,
        ),
        Some(TaskState::Skipped | TaskState::Cancelled) => ("⊘".to_owned(), Role::Dim),
        _ => (
            theme.verb_glyph_bare(row.and_then(row_verb)).to_owned(),
            Role::Dim,
        ),
    };
    let painted = theme.paint(role, &glyph);
    if with_id {
        let id_painted = match row.map(|r| &r.state) {
            Some(TaskState::Ok) => theme.paint(Role::Good, id),
            Some(TaskState::Failed) => theme.paint(Role::Bad, id),
            _ => theme.paint(Role::Dim, id),
        };
        format!("{painted} {id_painted}")
    } else {
        painted
    }
}

fn header_lines(view: &RunView, theme: Theme, tasks: usize) -> Vec<String> {
    let mut lines = Vec::with_capacity(3);
    let count = if tasks > 0 {
        format!(" · {}", crate::vocab::count(tasks, "task"))
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
    lines.extend(header_lines(view, *theme, view.rows().len()));

    // Task rows — stable order, aligned ids, notes dimmed. Time and cost
    // are first-class columns: a settled row carries its REAL wall time
    // (+ per-task spend when the stream reported one), a running row its
    // live elapsed, and `∥` marks wave-siblings that actually overlapped.
    // Never-ran rows speak their REASON (`cache hit (resume)` · `when:
    // false` · `blocked · <task> failed`) through the display-note map.
    if let Some(map) = map_line(view, *theme, tick) {
        lines.push(map);
    }
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

    lines.extend(warning_lines(view, theme));
    lines.extend(caution_lines(view, theme));
    lines.push(meter_line(view, theme));
    // The HUD bar under the meter — interactive surface only (the
    // sober registers keep the meter as their last line, byte-exact).
    if theme.accents {
        let done = view
            .rows()
            .iter()
            .filter(|r| {
                matches!(
                    r.state,
                    TaskState::Ok | TaskState::Failed | TaskState::Skipped | TaskState::Cancelled
                )
            })
            .count();
        let total = view.rows().len();
        if total > 1 {
            lines.push(format!(
                "  {} {}",
                crate::chrome::bar(*theme, done, total, 24),
                theme.paint(Role::Dim, &format!("{done}/{total}")),
            ));
        }
    }

    // Failure card (only on a failed verdict · derives the explain hint) —
    // the SAME card the compact `--quiet` surface renders (shared helper).
    if view.verdict == Some(false) {
        append_failure_card(&mut lines, view, theme);
    } else if view.paused_task.is_some() {
        append_paused_card(&mut lines, view, theme);
    }
    lines
}

/// The form-sanity caution block (user gauntlet 2026-07-31 · the
/// green-run-that-lies class): the `fruit::cautions` reads — an answer
/// that asks for its inputs back · every input recovered · an empty
/// answer — painted Warn, above the meter beside the OBS-E warnings.
/// The surface renders EVERY caution the read derives (never a
/// hard-coded subset): a future lying-green class added to `fruit`
/// reaches all closing frames without touching them.
// `&Theme` to match the frame borrows that thread it here.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn caution_lines(view: &RunView, theme: &Theme) -> Vec<String> {
    crate::fruit::cautions(view, theme.ascii)
        .into_iter()
        .map(|raw| format!("  {}", theme.paint(Role::Warn, &raw)))
        .collect()
}

/// The OBS-E warning block (#410): one `⚠ <task> · <warning>` line per
/// row whose terminal frame carried the non-fatal diagnostic (a thinking
/// model that spent its budget and answered blank). Above the meter so a
/// green verdict never buries it — the exit code stays 0, the console
/// stops being silent about the "" feeding downstream.
// `&Theme` to match the frame borrows that thread it here.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn warning_lines(view: &RunView, theme: &Theme) -> Vec<String> {
    let mark = if theme.ascii { "!" } else { "⚠" };
    view.rows()
        .iter()
        .filter_map(|row| {
            let warning = row.warning.as_deref()?;
            Some(format!(
                "  {} {} · {}",
                theme.paint(Role::Warn, mark),
                row.id,
                theme.paint(Role::Warn, warning),
            ))
        })
        .collect()
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
    // Fallout beside the root cause: ONE failed gate cancelling 22
    // downstream rows used to read `23/23 done · 1 failed` — the wall
    // of `⊘` had no summary voice and the meter read near-clean.
    let blocked = match view.cancelled_count() {
        0 => String::new(),
        n => format!("{n} blocked · "),
    };
    let repaired = match view.recovered_count() {
        0 => String::new(),
        n => format!("{n} recovered · "),
    };
    #[allow(clippy::cast_precision_loss)] // display-only seconds
    let secs = view.elapsed_ms as f64 / 1000.0;
    let meter = format!(
        "── {}/{} done · {failed}{blocked}{repaired}{cost} · elapsed {secs:.1}s ",
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
    header_lines(view, *theme, tasks)
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

/// The streamed close (#321): the OBS-E warnings (#410) + the sanity
/// cautions + the meter + the FRUIT block (A-2 · user gauntlet
/// 2026-07-31) + the failure card. The rows already spoke at their
/// settle — the plain final print never repeats them (a captured log
/// reads the run ONCE, top to bottom).
///
/// `notes` = the caller's composed fruit lines (`wrote output.md
/// (412B)` · `said "…"`) — byte sizes are the CALLER's stat (no I/O in
/// this crate); the rehearsal fact folds here so no closing surface can
/// forget it.
// `&Theme` to match the sink borrow that threads it here.
#[allow(clippy::trivially_copy_pass_by_ref)]
#[must_use]
pub fn stream_summary(view: &RunView, theme: &Theme, notes: &[String]) -> Vec<String> {
    let mut lines = warning_lines(view, theme);
    lines.extend(caution_lines(view, theme));
    lines.push(meter_line(view, theme));
    lines.extend(
        notes
            .iter()
            .map(|n| format!("    {}", theme.paint(Role::Dim, n))),
    );
    if let Some(note) = crate::fruit::rehearsal_note(view) {
        lines.push(format!("    {}", theme.paint(Role::Dim, note)));
    }
    if view.verdict == Some(false) {
        append_failure_card(&mut lines, view, theme);
    } else if view.paused_task.is_some() {
        append_paused_card(&mut lines, view, theme);
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
        // A paused gate reads like a running row frozen at the pause
        // stamp: the time it stood open before the run parked.
        TaskState::Running | TaskState::Retrying | TaskState::Paused => {
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
        // The GATE cancellation (#1198) — the runtime names the producer
        // whose settle closed the edge, and the producer's own row says
        // what it settled AS. Both were already on screen; the note
        // spoke neither, so `gate: an edge did not admit` was the whole
        // of what a reader got: no edge, no upstream, no outcome.
        //
        // The outcome is the load-bearing half. A gate closes when the
        // producer settled OUTSIDE the pass-set the binding admits, and
        // the case that reads as a contradiction is exactly the one that
        // says `ok`: a `tasks.X.error` binding admits {failure, skipped}
        // and a task that SUCCEEDED has no error to read, so the
        // consumer is a dead path. That is the gate working. Naming the
        // outcome is what turns it from a wall into a sentence.
        (TaskState::Cancelled, "gate: an edge did not admit") => {
            match view.blocked_by(&row.id) {
                Some(producer) => {
                    let settled = view.rows().iter().find(|r| r.id == producer).map_or(
                        "never settled",
                        |r| match r.state {
                            TaskState::Ok => "ok",
                            TaskState::Failed => "failed",
                            TaskState::Skipped => "skipped",
                            TaskState::Cancelled => "blocked",
                            _ => "never settled",
                        },
                    );
                    format!("blocked · {producer} settled {settled} · no binding here admits that")
                }
                None => "blocked · an upstream settled outside what this task's bindings admit"
                    .to_owned(),
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
    // A RUNNING row animates in its verb's OWN motion (the website's
    // tile vocabulary: sampling · scanline · roundtrip · orbit), verb
    // from the started note — the same derivation as the chip column.
    if row.state == TaskState::Running && theme.animate && !theme.ascii {
        return theme.verb_spin(row_verb(row), tick);
    }
    theme.glyph(row.state, tick)
}

/// The verb word out of a row's started note (`infer · <model>` …).
fn row_verb(row: &TaskRow) -> Option<&str> {
    row.started_note
        .as_deref()
        .and_then(|n| n.split(" · ").next())
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
    // The verb chip (tokens-SSOT ◇▷◆✦) rides the INTERACTIVE surface
    // only (`accents` = Live TTY): every sober register keeps its exact
    // bytes. Derived from the started note's own vocabulary (`infer ·
    // <model>`); a not-yet-started row wears the dim placeholder so the
    // column never jitters.
    let chip = if theme.accents {
        row.started_note
            .as_deref()
            .and_then(|n| n.split(" \u{b7} ").next())
            .map_or_else(
                || theme.paint(Role::Dim, "\u{b7} "),
                |v| theme.verb_glyph(v),
            )
    } else {
        String::new()
    };
    let mut line = format!(
        "  {} {chip}{:<id_w$}  {}",
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
/// before the terminal frame) renders the header alone. The form-sanity
/// cautions ride even here: `--quiet` promised compactness, never a
/// green verdict that lies (the same stance as "errors always").
#[must_use]
pub fn verdict_frame(view: &RunView, theme: &Theme) -> Vec<String> {
    let mut lines = Vec::with_capacity(4);
    let glyph = match view.verdict {
        Some(true) if crate::fruit::recovered_ok(view) => {
            // Recovered is a success cause (exit 0) but not an unblemished
            // tick — persona 14 grepped the quiet headline and saw ✔.
            theme.paint(Role::Warn, if theme.ascii { "! " } else { "⚠ " })
        }
        Some(true) => theme.glyph(TaskState::Ok, 0),
        Some(false) => theme.glyph(TaskState::Failed, 0),
        None => theme.glyph(TaskState::Pending, 0),
    };
    let cost = spend_meter(view);
    #[allow(clippy::cast_precision_loss)] // display-only seconds
    let secs = view.elapsed_ms as f64 / 1000.0;
    lines.push(format!(
        "  {} {} · {} · {secs:.1}s · {cost}",
        glyph,
        theme.paint(Role::Strong, &view.workflow),
        crate::vocab::count(view.rows().len(), "task"),
    ));
    lines.extend(caution_lines(view, theme));

    // Errors always (spec §3.5) — the same failure card the full frame emits,
    // appended so a quiet run still surfaces WHY it failed + the explain hint.
    if view.verdict == Some(false) {
        append_failure_card(&mut lines, view, theme);
    } else if view.paused_task.is_some() {
        append_paused_card(&mut lines, view, theme);
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

/// The paused card (ADR-099 rider) — the frame's own voice for a run
/// that parked at a human gate: the `◇` amber mark + the awaiting task.
/// A pause carries no wire code and earns no `fix:` hint — the lane
/// beside the frame teaches the exact resume command (epilogue seam).
// `&Theme` — the same one-calling-convention rationale as the failure card.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn append_paused_card(lines: &mut Vec<String>, view: &RunView, theme: &Theme) {
    let Some(task) = &view.paused_task else {
        return;
    };
    lines.push(String::new());
    lines.push(format!(
        "  {}{}",
        theme.glyph(TaskState::Paused, 0),
        theme.paint(
            Role::Strong,
            &format!("paused · awaiting an answer for `{task}`")
        ),
    ));
    lines.push(format!(
        "    {}",
        crate::vocab::hint(
            *theme,
            "answer",
            &format!("nika run --answer {task}=true FILE  (boolean true/false, not yes)")
        )
    ));
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
#[path = "render_tests.rs"]
mod tests;
