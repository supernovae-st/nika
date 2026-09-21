// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The static trace readers — `nika trace outputs` (per-task browser) ·
//! `nika trace peek` (full fidelity) · `nika trace flow` (the data
//! waterfall: which output fed which task, with real sizes).
//!
//! Pure reads over a recorded NDJSON trace: load (the SAME tolerant
//! recovery `--resume` and `trace show` fold through) → fold into the
//! ONE [`RunView`] truth → render. Three densities, one source: the
//! storyboard shows SHAPE tails, the table shows bounded previews,
//! `peek` shows the whole value + its ADR-099 identity. `flow` joins
//! the plan (the checked definition's bindings) with the trace (the
//! recorded sizes) — a fold over two existing truths, zero new
//! analysis.

pub mod action;
pub mod manage;
pub mod session;
pub(crate) use nika_cli_host::retention;
#[cfg(test)]
mod retention_tests;
pub(crate) mod store;

pub use action::{TraceAction, TraceArgs};

use std::fmt::Write as _;

use crate::display::flow::fmt_wall_ms;
use crate::display::shape;
use crate::display::theme::{Role, Theme};
use crate::{RunView, TaskRow, TaskState};

pub(crate) use nika_dap::flow::{FlowEdge, flow_edges};

use super::VerbOutput;

/// Widest the output-preview column grows (display cells) — the table
/// has more room than a storyboard row, less than a pager.
const PREVIEW_CELLS: usize = 36;

/// `nika trace outputs <trace>` — one row per task: verb · duration ·
/// tokens · bounded output preview, then the totals line + the peek
/// hint. The browsing density between the storyboard tail and `peek`.
#[must_use]
pub fn outputs(trace: &str, theme: Theme) -> VerbOutput {
    let view = match load_view(trace) {
        Ok(view) => view,
        Err(out) => return out,
    };
    VerbOutput::ok(render_outputs(&view, trace, theme))
}

/// `nika trace outputs --json <trace>` — the per-task machine projection
/// (#1247 · #1275): one document, `trace` and `tasks` (each with its id,
/// verb, status, cause, error code and the original error a recovered
/// task was repaired from). The projection `tasks_json` carries.
#[must_use]
pub fn outputs_json(trace: &str) -> VerbOutput {
    let (view, events) = match load_view_and_events(trace) {
        Ok(pair) => pair,
        Err(out) => return out,
    };
    let projection = tasks_json(&view, &events);
    // Writer liveness is evidence, never a replacement for a settlement.
    let liveness = projection
        .get("settlement")
        .is_none()
        .then(|| nika_dap::liveness::probe(std::path::Path::new(trace)).as_str());
    let mut document = serde_json::json!({
        "outputs_version": 1,
        "trace": trace,
        "state": projection["state"],
        "liveness": liveness,
        "tasks": projection["tasks"],
    });
    // ADR-128 · the settlement rides whole when the journal reached one.
    if let Some(settlement) = projection.get("settlement") {
        document["settlement"] = settlement.clone();
    }
    VerbOutput::ok(serde_json::to_string_pretty(&document).unwrap_or_default())
}

/// Load + tolerantly parse + fold one trace file (the shared entry of
/// every static trace reader).
pub(crate) fn load_view(trace: &str) -> Result<RunView, VerbOutput> {
    // The file half (read + tolerant recover) lives in the forensics
    // crate (nika_dap::recover — the 15k descent); the fold into the
    // ONE RunView truth stays display-side.
    let events = nika_dap::recover::load_events(trace).map_err(VerbOutput::env)?;
    let mut view = RunView::new();
    for event in &events {
        view.apply(event);
    }
    Ok(view)
}

/// The em-dash cell for "no data" — `-` under `--ascii`.
fn dash(theme: Theme) -> &'static str {
    if theme.ascii { "-" } else { "—" }
}

/// One task's preview cell: the bounded shape + its byte size, or the
/// no-data dash (a skip · a failure · an older engine's trace).
fn preview_cell(row: &TaskRow, theme: Theme) -> String {
    match row.output_json.as_deref() {
        Some(text) => match shape::summarize(text, PREVIEW_CELLS) {
            Some(s) => format!("{s} · {}", shape::fmt_bytes(text.len())),
            None => dash(theme).to_owned(),
        },
        None => dash(theme).to_owned(),
    }
}

/// Render the per-task table + totals + the peek hint.
fn render_outputs(view: &RunView, trace: &str, theme: Theme) -> String {
    let rows = view.rows();
    let cells: Vec<[String; 4]> = rows
        .iter()
        .map(|r| {
            [
                r.id.clone(),
                r.started_note
                    .clone()
                    .unwrap_or_else(|| dash(theme).to_owned()),
                r.wall_ms().map(fmt_wall_ms).unwrap_or_default(),
                r.tokens
                    .map_or_else(|| dash(theme).to_owned(), |t| t.to_string()),
            ]
        })
        .collect();
    let header = ["task", "verb", "dur", "tok"];
    let width = |i: usize| {
        cells
            .iter()
            .map(|c| c[i].chars().count())
            .chain(std::iter::once(header[i].len()))
            .max()
            .unwrap_or(0)
    };
    let (w0, w1, w2, w3) = (width(0), width(1), width(2), width(3));

    // The dur column speaks the nextest bracket form (`[  2.7s]`) under
    // the interactive accents (TTY) — sober registers keep the bare
    // right-aligned cell. Empty cells and the header pad to the same
    // width in both forms so the tok column never drifts.
    let dur_cell = |d: &str, bare: bool| -> String {
        if !theme.accents {
            format!("{d:>w2$}")
        } else if bare {
            format!(" {d:>w2$} ")
        } else {
            format!("[{d:>w2$}]")
        }
    };

    let mut out = String::new();
    let head = format!(
        "  {:<w0$}  {:<w1$}  {}  {:>w3$}  output",
        header[0],
        header[1],
        dur_cell(header[2], true),
        header[3],
    );
    let _ = writeln!(out, "{}", theme.paint(Role::Dim, &head));
    for (row, c) in rows.iter().zip(&cells) {
        let mut preview = if row.recovered {
            format!("{} · recovered", preview_cell(row, theme))
        } else {
            preview_cell(row, theme)
        };
        // F-O1 · the born origin of an untrusted value, said in prose.
        if let Some(source) = row.integrity_source.as_deref() {
            let _ = write!(preview, " · untrusted input from {source}");
        }
        let _ = writeln!(
            out,
            "  {:<w0$}  {}  {}  {:>w3$}  {}",
            c[0],
            theme.paint(Role::Dim, &format!("{:<w1$}", c[1])),
            dur_cell(&c[2], c[2].is_empty()),
            c[3],
            preview,
        );
    }
    let _ = writeln!(out, "{}", totals_line(view, trace, theme));
    out
}

/// The closing line: `N tasks · <wall> · <tok> tok · full value: …` —
/// the peek hint carries the REAL trace path (copy-paste ready, the
/// task id is the one placeholder).
fn totals_line(view: &RunView, trace: &str, theme: Theme) -> String {
    let mut line = format!(
        "  {} · {}",
        crate::text::count(view.rows().len(), "task"),
        fmt_wall_ms(view.elapsed_ms)
    );
    let tokens: u64 = view.rows().iter().filter_map(|r| r.tokens).sum();
    if tokens > 0 {
        let _ = write!(line, " · {tokens} tok");
    }
    // Recorded spend — `≥` + the unpriced count when part of the run
    // carried no meterable price (never a silent partial-as-total).
    if view.unpriced_calls > 0 {
        let _ = write!(
            line,
            " · ≥ {} ({} unpriced)",
            crate::display::format::fmt_cost_usd(view.cost_usd),
            view.unpriced_calls
        );
    } else if view.cost_usd > 0.0 {
        let _ = write!(
            line,
            " · {}",
            crate::display::format::fmt_cost_usd(view.cost_usd)
        );
    }
    // The trace path is CLICKABLE on link-capable terminals (OSC-8).
    let _ = write!(
        line,
        " · full value: nika trace peek {} <task>",
        crate::linked_path(theme, trace)
    );
    theme.paint(Role::Dim, &line)
}

/// `nika trace peek <trace> <task>` — the full-fidelity read: the
/// task's whole output pretty-printed under a compact identity block
/// (verb · duration · tokens · the ADR-099 hashes). `--raw` prints the
/// EXACT recorded value as one JSON text — pipeable to jq, never
/// coloured, nothing else on stdout.
#[must_use]
pub fn peek(trace: &str, task: &str, raw: bool, theme: Theme) -> VerbOutput {
    let (view, events) = match load_view_and_events(trace) {
        Ok(pair) => pair,
        Err(out) => return out,
    };
    let Some(row) = view.rows().iter().find(|r| r.id == task) else {
        return VerbOutput::env(unknown_task_message(&view, trace, task));
    };
    let Some(text) = row.output_json.as_deref() else {
        // A failed task records no output — its autopsy IS the recorded
        // failure. The failure card promised « autopsy: nika trace peek » ;
        // peek delivers it instead of shrugging. (`--raw` keeps its
        // jq-pipe contract — a failure has no value to pipe.)
        if !raw && row.state == TaskState::Failed && !row.detail.is_empty() {
            let mut out = render_failure_peek(row, theme);
            out.push_str(&item_table(row, theme));
            return VerbOutput::ok(out);
        }
        // A fan-out whose aggregate value was never checkpointed still
        // recorded its item table on the terminal frame (#1276 · #1397):
        // the per-item codes and messages ARE what an on-call reader came
        // for, and this refusal was hiding them (wave 3 · persona 10 · « no
        // error code anywhere » — it was on the frame, behind « recorded
        // no output »). `--raw` keeps its jq contract: no value, no pipe.
        if !raw && row.items_json.is_some() {
            let mut out =
                render_unrecorded_peek(row, recovered_from(&events, task).as_deref(), theme);
            out.push_str(&item_table(row, theme));
            return VerbOutput::ok(out);
        }
        return VerbOutput::env(no_output_message(&view, row));
    };
    if raw {
        // The exact recorded value — the machine arm of peek.
        return VerbOutput::ok(text.to_owned());
    }
    let mut out = render_peek(row, text, recovered_from(&events, task).as_deref(), theme);
    out.push_str(&item_table(row, theme));
    VerbOutput::ok(out)
}

/// Load + fold, keeping the events so `recovered_from` is readable.
fn load_view_and_events(trace: &str) -> Result<(RunView, Vec<nika_event::Event>), VerbOutput> {
    let events = nika_dap::recover::load_events(trace).map_err(VerbOutput::env)?;
    let mut view = RunView::new();
    for event in &events {
        view.apply(event);
    }
    Ok((view, events))
}

/// Original error code a recovered task was repaired FROM (`task_recovered.code`).
fn recovered_from(events: &[nika_event::Event], task: &str) -> Option<String> {
    events.iter().find_map(|event| {
        if event.kind != nika_event::EventKind::TaskRecovered {
            return None;
        }
        if crate::display::state::str_field(event, "task") != Some(task) {
            return None;
        }
        crate::display::state::str_field(event, "code").map(str::to_owned)
    })
}

/// Machine projection of every task (B23 / issue 1275 · the `--json` leg).
#[must_use]
pub fn tasks_json(view: &RunView, events: &[nika_event::Event]) -> serde_json::Value {
    let tasks: Vec<serde_json::Value> = view
        .rows()
        .iter()
        .map(|row| {
            let recovered = recovered_from(events, &row.id);
            let status = if row.recovered {
                "recovered"
            } else {
                match row.state {
                    TaskState::Ok => "ok",
                    TaskState::Failed => "failed",
                    TaskState::Skipped => "skipped",
                    TaskState::Cancelled => "cancelled",
                    TaskState::Paused => "paused",
                    TaskState::Retrying => "retrying",
                    TaskState::Running => "running",
                    TaskState::Pending => "pending",
                }
            };
            let items = row
                .items_json
                .as_deref()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
            serde_json::json!({
                "id": row.id,
                "verb": row.started_note,
                "status": status,
                "error_code": recovered,
                "recovered_from": recovered,
                "integrity_source": row.integrity_source,
                "warning": row.warning,
                "items": items,
            })
        })
        .collect();
    // ADR-128 · the run's state is the terminal frame's settlement, read
    // by the ONE reader (`recovered` is a tally on it, never a state); a
    // journal with no terminal frame is still running — or torn, which
    // the store's liveness law tells apart (#1442).
    let settlement = nika_event::settlement::RunSettlement::from_events(events);
    let run_state = settlement.as_ref().map_or("running", |s| s.state.as_str());
    let mut doc = serde_json::json!({
        "state": run_state,
        "tasks": tasks,
    });
    if let Some(value) = settlement
        .as_ref()
        .and_then(|s| serde_json::to_value(s).ok())
    {
        doc["settlement"] = value;
    }
    doc
}

/// One decoded item row of a fan-out's `items` table.
struct ItemRow {
    index: u64,
    item: String,
    status: String,
    code: Option<String>,
    message: Option<String>,
}

fn item_rows(row: &TaskRow) -> Vec<ItemRow> {
    let Some(text) = row.items_json.as_deref() else {
        return Vec::new();
    };
    let Ok(serde_json::Value::Array(rows)) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    rows.iter()
        .map(|r| ItemRow {
            index: r["index"].as_u64().unwrap_or_default(),
            item: r["item"].as_str().unwrap_or("?").to_owned(),
            status: r["status"].as_str().unwrap_or("?").to_owned(),
            code: r["code"].as_str().map(str::to_owned),
            message: r["message"].as_str().map(str::to_owned),
        })
        .collect()
}

/// The human word for an item's status (`never_started` reads as prose).
fn item_status_word(status: &str) -> &str {
    match status {
        "never_started" => "never started",
        other => other,
    }
}

/// The fan-out's item table (#1276 · #1397): one line per item in input
/// order · index · item · status · the recorded code and message when the
/// item failed or recovered. Empty for a row that carries no table.
fn item_table(row: &TaskRow, theme: Theme) -> String {
    let rows = item_rows(row);
    if rows.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  {}",
        theme.paint(Role::Strong, &format!("items · {}", rows.len()))
    );
    let width = rows
        .iter()
        .map(|r| r.item.chars().count())
        .max()
        .unwrap_or(1)
        .min(40);
    for r in &rows {
        let role = match r.status.as_str() {
            "ok" => Role::Good,
            "failed" => Role::Bad,
            _ => Role::Dim,
        };
        let item: String = r.item.chars().take(40).collect();
        let mut line = format!(
            "    {:>3}  {item:<width$}  {}",
            r.index,
            theme.paint(role, item_status_word(&r.status))
        );
        if let Some(code) = &r.code {
            let _ = write!(line, "  {code}");
            if let Some(message) = &r.message {
                let _ = write!(line, " · {message}");
            }
        }
        let _ = writeln!(out, "{line}");
    }
    out
}

/// The `trace show` companion (#1397): one summary line per fan-out row
/// that carries an item table · the tally by status and the peek that
/// prints the whole table.
#[must_use]
pub fn item_summary_lines(view: &RunView, trace: &str, theme: Theme) -> Vec<String> {
    view.rows()
        .iter()
        .filter_map(|row| {
            let rows = item_rows(row);
            if rows.is_empty() {
                return None;
            }
            let tally = |status: &str| rows.iter().filter(|r| r.status == status).count();
            let mut parts = vec![format!("{} ok", tally("ok"))];
            for status in ["recovered", "failed", "never_started"] {
                let n = tally(status);
                if n > 0 {
                    parts.push(format!("{n} {}", item_status_word(status)));
                }
            }
            Some(format!(
                "  {} {}",
                theme.paint(Role::Strong, &row.id),
                theme.paint(
                    Role::Dim,
                    &format!(
                        "items · {} · {} · nika trace peek {} {}",
                        rows.len(),
                        parts.join(" · "),
                        crate::linked_path(theme, trace),
                        row.id
                    )
                )
            ))
        })
        .collect()
}

/// The readable unknown-task refusal: name what the trace DOES record.
fn unknown_task_message(view: &RunView, trace: &str, task: &str) -> String {
    let known: Vec<&str> = view.rows().iter().map(|r| r.id.as_str()).collect();
    if known.is_empty() {
        return format!("unknown task `{task}` — {trace} records no tasks");
    }
    format!(
        "unknown task `{task}` — this trace records: {}",
        known.join(" · ")
    )
}

/// The readable no-output refusal: say WHY this row has no value and
/// name the rows that do.
fn no_output_message(view: &RunView, row: &TaskRow) -> String {
    let with_outputs: Vec<&str> = view
        .rows()
        .iter()
        .filter(|r| r.output_json.is_some())
        .map(|r| r.id.as_str())
        .collect();
    let state = format!("{:?}", row.state).to_lowercase();
    let mut message = format!("task `{}` recorded no output ({state})", row.id);
    // Each state explains itself — the « older engine? » hypothesis is
    // reserved for the one case that actually suggests it: a task that
    // SUCCEEDED in a trace where nothing carries an output field.
    match row.state {
        TaskState::Skipped => message.push_str(" — a guarded skip never runs, so never records"),
        TaskState::Cancelled => message.push_str(" — the path died upstream before it ran"),
        TaskState::Failed => message.push_str(" — the run settled before it produced a value"),
        // ADR-099 · only a task that earned a resume stamp checkpoints its
        // value; one that did not (inputs not replayable from the file)
        // succeeded without the journal ever carrying the value. A row
        // that recorded its item table is a NEW engine's row, whatever the
        // rest of the trace carries.
        TaskState::Ok if row.items_json.is_some() || !with_outputs.is_empty() => message.push_str(
            " — the value was not checkpointed: this task earned no resume stamp (its inputs are not replayable from the file), so the journal never carried it",
        ),
        _ if with_outputs.is_empty() => {
            message.push_str(" — no task in this trace carries one (an older engine's trace?)");
        }
        _ => {}
    }
    if !with_outputs.is_empty() {
        let _ = write!(
            message,
            " — outputs recorded for: {}",
            with_outputs.join(" · ")
        );
    }
    message
}

/// The autopsy: a failed task's peek renders the RECORDED failure —
/// same identity block as a value peek, then the detail the settle
/// event carried, then the teach line when the detail names a code.
fn render_failure_peek(row: &TaskRow, theme: Theme) -> String {
    let mut out = String::new();
    let title = match row.started_note.as_deref() {
        Some(note) => format!("{} · {note}", row.id),
        None => row.id.clone(),
    };
    let _ = writeln!(out, "  {}", theme.paint(Role::Strong, &title));
    let mut meta = row
        .wall_ms()
        .map_or_else(|| dash(theme).to_owned(), fmt_wall_ms);
    if let Some(tok) = row.tokens {
        let _ = write!(meta, " · {tok} tok");
    }
    let _ = write!(meta, " · failed");
    let _ = writeln!(out, "  {}", theme.paint(Role::Dim, &meta));
    let _ = writeln!(out);
    let _ = writeln!(out, "  {}", theme.paint(Role::Bad, &row.detail));
    if let Some(code) = nika_dap::recover::first_wire_code(&row.detail) {
        let _ = writeln!(
            out,
            "  {}",
            theme.paint(Role::Dim, &format!("fix: nika explain {code}"))
        );
    }
    out
}

/// The peek of a succeeded task whose value was never checkpointed but
/// whose item table was recorded: the same identity block as a value
/// peek, then the one honest line about the absent value — the table
/// follows from the caller.
fn render_unrecorded_peek(row: &TaskRow, recovered_from: Option<&str>, theme: Theme) -> String {
    let mut out = String::new();
    let title = match row.started_note.as_deref() {
        Some(note) => format!("{} · {note}", row.id),
        None => row.id.clone(),
    };
    let _ = writeln!(out, "  {}", theme.paint(Role::Strong, &title));
    let mut meta = row
        .wall_ms()
        .map_or_else(|| dash(theme).to_owned(), fmt_wall_ms);
    if let Some(tok) = row.tokens {
        let _ = write!(meta, " · {tok} tok");
    }
    if row.recovered {
        let _ = write!(meta, " · recovered");
        if let Some(code) = recovered_from {
            let _ = write!(meta, " from {code}");
        }
    }
    let _ = writeln!(out, "  {}", theme.paint(Role::Dim, &meta));
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  {}",
        theme.paint(
            Role::Dim,
            "value not checkpointed — this task earned no resume stamp (its inputs are not replayable from the file); the item table below is the recorded truth"
        )
    );
    out
}

/// The pretty read: identity block (task · verb · time · tokens ·
/// hashes) then the full value, pretty-printed. A value that is not
/// valid JSON (a hand-edited trace) prints verbatim — honesty over
/// polish.
fn render_peek(row: &TaskRow, text: &str, recovered_from: Option<&str>, theme: Theme) -> String {
    let mut out = String::new();
    let title = match row.started_note.as_deref() {
        Some(note) => format!("{} · {note}", row.id),
        None => row.id.clone(),
    };
    let _ = writeln!(out, "  {}", theme.paint(Role::Strong, &title));
    let mut meta = row
        .wall_ms()
        .map_or_else(|| dash(theme).to_owned(), fmt_wall_ms);
    if let Some(tok) = row.tokens {
        let _ = write!(meta, " · {tok} tok");
    }
    // The transport's account: a call the backoff re-sent says how many
    // tries, how long it waited and on what (the sealed frame's fields).
    if let Some(attempts) = row.attempts.filter(|a| *a > 1) {
        let _ = write!(meta, " · {attempts} attempts");
        match (row.waited_ms, row.retried_on.as_deref()) {
            (Some(waited), Some(status)) => {
                let _ = write!(meta, " (waited {} on {status})", fmt_wall_ms(waited));
            }
            (Some(waited), None) => {
                let _ = write!(meta, " (waited {})", fmt_wall_ms(waited));
            }
            (None, Some(status)) => {
                let _ = write!(meta, " (on {status})");
            }
            (None, None) => {}
        }
    }
    let _ = write!(meta, " · {}", shape::fmt_bytes(text.len()));
    if row.recovered {
        let _ = write!(meta, " · recovered");
        if let Some(code) = recovered_from {
            let _ = write!(meta, " from {code}");
        }
    }
    // F-O1 · the born origin of an untrusted value.
    if let Some(source) = row.integrity_source.as_deref() {
        let _ = write!(meta, " · untrusted input from {source}");
    }
    let _ = writeln!(out, "  {}", theme.paint(Role::Dim, &meta));
    if let (Some(def), Some(input)) = (row.def_hash.as_deref(), row.input_hash.as_deref()) {
        let line = format!(
            "def_hash {} · input_hash {}",
            clip_hash(def, theme),
            clip_hash(input, theme)
        );
        let _ = writeln!(out, "  {}", theme.paint(Role::Dim, &line));
    }
    let _ = writeln!(out);
    let pretty = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| text.to_owned());
    for line in pretty.lines() {
        let _ = writeln!(out, "  {line}");
    }
    out
}

/// A hash for eyeballing: the leading 12 hex chars + a theme-true mark
/// (comparison across runs · the full hex lives in the trace itself).
fn clip_hash(hash: &str, theme: Theme) -> String {
    if hash.chars().count() <= 12 {
        return hash.to_owned();
    }
    let head: String = hash.chars().take(12).collect();
    format!("{head}{}", if theme.ascii { ".." } else { "…" })
}

/// `nika trace flow <trace> <workflow>` — the data waterfall: edges
/// from the checked definition's bindings (`after:` + `${{ tasks.X
/// }}` references · the SAME over-collecting scan `--resume --from`
/// walks) × output sizes from the trace, plus the `outputs.<name>`
/// terminal edges. The time-waterfall shows WHEN; this shows WHY.
/// The edge COMPUTE lives in the forensics crate (`nika_dap::flow` —
/// the 15k descent); this verb keeps the view fold + the render.
#[must_use]
pub fn flow(trace: &str, workflow: &str, theme: Theme) -> VerbOutput {
    let view = match load_view(trace) {
        Ok(view) => view,
        Err(out) => return out,
    };
    let (wf, _report) = match super::load_checked(workflow) {
        Ok(pair) => pair,
        Err(out) => return out,
    };
    let mut out = String::new();
    // Honesty header when the two inputs disagree on the workflow name.
    let declared = wf.workflow.as_ref().map(|w| w.value.as_str());
    if let Some(name) = declared
        && !view.workflow.is_empty()
        && view.workflow != name
    {
        let note = format!(
            "note: the trace records workflow `{}` · {workflow} declares `{name}`",
            view.workflow
        );
        let _ = writeln!(out, "  {}", theme.paint(Role::Warn, &note));
    }
    let mut size_of = |task: &str| -> Option<usize> {
        view.rows()
            .iter()
            .find(|r| r.id == task)
            .and_then(output_size)
    };
    out.push_str(&render_flow(&flow_edges(&wf, &mut size_of), theme));
    VerbOutput::ok(out)
}

/// The recorded byte size of one task row's output (the display-side
/// answer the edge compute asks through the injected lookup).
fn output_size(row: &TaskRow) -> Option<usize> {
    row.output_json.as_deref().map(str::len)
}

/// Render the waterfall: one `from ─size→ to` line per edge (a source
/// the trace never sized keeps the bare arrow — never an invented
/// number), then the totals line naming the widest edge.
fn render_flow(edges: &[FlowEdge], theme: Theme) -> String {
    let mut out = String::new();
    if edges.is_empty() {
        let _ = writeln!(
            out,
            "  {}",
            theme.paint(
                Role::Dim,
                "no data edges — no task references another task's output"
            )
        );
        return out;
    }
    let from_w = edges
        .iter()
        .map(|e| e.from.chars().count())
        .max()
        .unwrap_or(0);
    for edge in edges {
        let arrow = crate::display::vocab::arrow(theme.ascii);
        let rail = match edge.bytes {
            Some(n) => {
                let size = shape::fmt_bytes(n);
                let dash = if theme.ascii { "-" } else { "─" };
                format!("{dash}{size}{arrow}")
            }
            None => arrow.to_owned(),
        };
        let _ = writeln!(
            out,
            "  {:<from_w$} {} {}",
            edge.from,
            theme.paint(Role::Dim, &rail),
            edge.to
        );
    }
    let arrow = crate::display::vocab::arrow(theme.ascii);
    let join = if theme.ascii { "x" } else { "×" };
    let mut totals = format!("  {}", crate::text::count(edges.len(), "edge"));
    if let Some(widest) = edges
        .iter()
        .filter(|e| e.bytes.is_some())
        .max_by_key(|e| e.bytes)
    {
        let _ = write!(totals, " · widest: {}{arrow}{}", widest.from, widest.to);
    }
    let _ = write!(totals, " · derived from plan bindings {join} trace sizes");
    let _ = writeln!(out, "{}", theme.paint(Role::Dim, &totals));
    out
}

#[cfg(test)]
mod outputs_liveness_tests;

#[cfg(test)]
mod tests;
