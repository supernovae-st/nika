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
    let mut document = serde_json::json!({
        "outputs_version": 1,
        "trace": trace,
        "state": projection["state"],
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
        // #1444 · the lineage the JSON already carried, said in prose.
        if let Some(source) = row.integrity_source.as_deref() {
            let _ = write!(preview, " · input from recovered {source}");
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
    let _ = write!(meta, " · {}", shape::fmt_bytes(text.len()));
    if row.recovered {
        let _ = write!(meta, " · recovered");
        if let Some(code) = recovered_from {
            let _ = write!(meta, " from {code}");
        }
    }
    // #1444 · a value that came from a recovered fallback upstream.
    if let Some(source) = row.integrity_source.as_deref() {
        let _ = write!(meta, " · input from recovered {source}");
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
mod tests {
    use super::*;
    use crate::demo;
    use crate::exit;

    fn plain() -> Theme {
        Theme::new(false, false, false)
    }

    /// Stage a real NDJSON trace from the demo storyboard events.
    fn stage(name: &str, events: &[nika_event::Event]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("nika-cli-trace-verb");
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join(name);
        let mut body = String::new();
        for ev in events {
            body.push_str(&serde_json::to_string(ev).expect("event serializes"));
            body.push('\n');
        }
        std::fs::write(&path, body).expect("trace staged");
        path
    }

    /// One row per task: verb (the started note) · duration · tokens ·
    /// preview or the honest dash — plus the totals + peek hint with
    /// the REAL trace path.
    /// `outputs --json` (#1247): one document — the run's state and one
    /// row per task with its id, verb and status (the projection the engine
    /// carried with no verb to print it).
    #[test]
    fn outputs_json_projects_every_task() {
        let path = stage("outputs-json.ndjson", &demo::success());
        let out = outputs_json(&path.to_string_lossy());
        assert_eq!(out.code, exit::OK);
        let doc: serde_json::Value = serde_json::from_str(&out.text).expect("one JSON document");
        assert_eq!(doc["outputs_version"], 1);
        assert_eq!(doc["state"], "succeeded", "{doc}");
        assert_eq!(doc["settlement"]["status"], "succeeded", "{doc}");
        let tasks = doc["tasks"].as_array().expect("the task rows");
        assert!(!tasks.is_empty(), "{doc}");
        for task in tasks {
            assert!(task["id"].is_string(), "{task}");
            assert!(task["status"].is_string(), "{task}");
            assert!(task.get("recovered_from").is_some(), "{task}");
        }
        assert!(
            tasks.iter().any(|t| t["status"] == "ok"),
            "the demo completes tasks: {doc}"
        );
    }

    #[test]
    fn outputs_table_renders_per_task_rows_and_totals() {
        let path = stage("outputs-demo.ndjson", &demo::success());
        let trace = path.to_string_lossy();
        let out = outputs(&trace, plain());
        assert_eq!(out.code, exit::OK);
        let text = &out.text;
        assert!(
            text.contains("task") && text.contains("verb") && text.contains("output"),
            "header row: {text}"
        );
        assert!(
            text.contains("invoke · nika:fetch"),
            "verb column carries the started note: {text}"
        );
        // The demo reports tokens on exactly one completion (710).
        assert!(text.contains("710"), "token cell: {text}");
        // Demo completions carry no ADR-099 output field → honest dash.
        assert!(text.contains('—'), "no output → dash: {text}");
        assert!(
            text.contains(&format!("full value: nika trace peek {trace} <task>")),
            "peek hint carries the real path: {text}"
        );
        assert!(text.contains("5 tasks"), "totals: {text}");
        assert!(text.contains("710 tok"), "token total: {text}");
    }

    /// Output-carrying completions preview their bounded shape + size.
    #[test]
    fn outputs_table_previews_shapes_with_sizes() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let events = vec![
            demo::bare_event(EventKind::TaskStarted, 0)
                .with_field(KeyValue::new("task", Value::String("audit".into())))
                .with_field(KeyValue::new(
                    "note",
                    Value::String("infer · mock/echo".into()),
                )),
            demo::bare_event(EventKind::TaskCompleted, 40)
                .with_field(KeyValue::new("task", Value::String("audit".into())))
                .with_field(KeyValue::new(
                    "output",
                    Value::String(r#"{"total":9,"fixes":["a","b"]}"#.into()),
                ))
                .with_field(KeyValue::new("tokens", Value::Int(90)))
                .with_field(KeyValue::new("duration_ms", Value::Int(38))),
        ];
        let path = stage("outputs-shapes.ndjson", &events);
        let out = outputs(&path.to_string_lossy(), plain());
        assert!(
            out.text.contains("{fixes[2], total} · 29B"),
            "bounded preview + byte size: {}",
            out.text
        );
        assert!(out.text.contains("38ms"), "measured duration: {}", out.text);
        // ASCII parity: the dash cell + no unicode leak.
        let ascii = outputs(&path.to_string_lossy(), Theme::new(false, true, false));
        assert!(!ascii.text.contains('—'), "ascii dash: {}", ascii.text);
    }

    /// An unreadable path is the environment class — actionable message,
    /// exit 3, never a panic.
    #[test]
    fn missing_trace_is_env_class() {
        let out = outputs("/nonexistent/trace.ndjson", plain());
        assert_eq!(out.code, exit::ENV);
        assert!(out.text.contains("cannot read"), "{}", out.text);
    }

    /// The interactive accents bracket the dur column (`[38ms]` ·
    /// nextest school) while the sober register (accents off · every
    /// pipe) keeps the bare right-aligned cell — pinned on the SAME
    /// staged trace.
    #[test]
    fn outputs_table_brackets_durations_under_accents_only() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let events = vec![
            demo::bare_event(EventKind::TaskStarted, 0)
                .with_field(KeyValue::new("task", Value::String("audit".into()))),
            demo::bare_event(EventKind::TaskCompleted, 40)
                .with_field(KeyValue::new("task", Value::String("audit".into())))
                .with_field(KeyValue::new("duration_ms", Value::Int(38))),
        ];
        let path = stage("outputs-accents.ndjson", &events);
        let sober = outputs(&path.to_string_lossy(), plain());
        assert!(
            !sober.text.contains("[38ms]"),
            "sober register: no brackets: {}",
            sober.text
        );
        let mut accented = plain();
        accented.accents = true;
        let rich = outputs(&path.to_string_lossy(), accented);
        assert!(
            rich.text.contains("[38ms]"),
            "accents bracket the dur cell: {}",
            rich.text
        );
    }

    /// A trace with the ADR-099 checkpoint trio for one task.
    fn peek_fixture(name: &str) -> std::path::PathBuf {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let events = vec![
            demo::bare_event(EventKind::TaskStarted, 0)
                .with_field(KeyValue::new("task", Value::String("audit".into())))
                .with_field(KeyValue::new(
                    "note",
                    Value::String("infer · mock/echo".into()),
                )),
            demo::bare_event(EventKind::TaskCompleted, 40)
                .with_field(KeyValue::new("task", Value::String("audit".into())))
                .with_field(KeyValue::new(
                    "output",
                    Value::String(r#"{"fixes":["a"],"total":9}"#.into()),
                ))
                .with_field(KeyValue::new("tokens", Value::Int(90)))
                .with_field(KeyValue::new("duration_ms", Value::Int(38)))
                .with_field(KeyValue::new(
                    "def_hash",
                    Value::String("5b2fa9e9232ed4174f3af03bf835".into()),
                ))
                .with_field(KeyValue::new(
                    "input_hash",
                    Value::String("7f14c732ad33dd042b82325cda86".into()),
                )),
            demo::bare_event(EventKind::TaskSkipped, 50)
                .with_field(KeyValue::new("task", Value::String("deploy".into())))
                .with_field(KeyValue::new(
                    "note",
                    Value::String("when: gate closed".into()),
                )),
        ];
        stage(name, &events)
    }

    /// The pretty peek: identity block (verb · time · tokens · clipped
    /// hashes) then the FULL value pretty-printed.
    #[test]
    fn peek_renders_identity_block_and_pretty_value() {
        let path = peek_fixture("peek-pretty.ndjson");
        let out = peek(&path.to_string_lossy(), "audit", false, plain());
        assert_eq!(out.code, exit::OK);
        let text = &out.text;
        assert!(text.contains("audit · infer · mock/echo"), "title: {text}");
        assert!(text.contains("38ms · 90 tok · 25B"), "meta: {text}");
        assert!(
            text.contains("def_hash 5b2fa9e9232e… · input_hash 7f14c732ad33…"),
            "clipped hashes: {text}"
        );
        assert!(
            text.contains("\"fixes\": [") && text.contains("\"total\": 9"),
            "pretty value: {text}"
        );
        // ASCII parity: the hash clip mark degrades, no unicode leak.
        let ascii = peek(
            &path.to_string_lossy(),
            "audit",
            false,
            Theme::new(false, true, false),
        );
        assert!(
            ascii.text.contains("5b2fa9e9232e.."),
            "ascii clip: {}",
            ascii.text
        );
        assert!(!ascii.text.contains('…'), "no unicode under --ascii");
    }

    /// A failed task's peek performs the autopsy the failure card
    /// promised: the recorded failure + the explain teach line — never
    /// the « older engine's trace? » shrug. `--raw` keeps its jq-pipe
    /// contract and still refuses (a failure has no value).
    #[test]
    fn peek_on_a_failed_task_performs_the_autopsy() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let events = vec![
            demo::bare_event(EventKind::TaskStarted, 0)
                .with_field(KeyValue::new("task", Value::String("greet".into())))
                .with_field(KeyValue::new(
                    "note",
                    Value::String("infer · mistral/mistral-small-latest".into()),
                )),
            demo::bare_event(EventKind::TaskFailed, 12)
                .with_field(KeyValue::new("task", Value::String("greet".into())))
                .with_field(KeyValue::new("duration_ms", Value::Int(9)))
                .with_field(KeyValue::new(
                    "detail",
                    Value::String(
                        "NIKA-INFER-001 · model `mistral/mistral-small-latest` failed to \
                         resolve: no API key for 'mistral'"
                            .into(),
                    ),
                )),
        ];
        let path = stage("peek-autopsy.ndjson", &events);
        let out = peek(&path.to_string_lossy(), "greet", false, plain());
        assert_eq!(out.code, exit::OK);
        assert!(
            out.text
                .contains("greet · infer · mistral/mistral-small-latest"),
            "identity: {}",
            out.text
        );
        assert!(
            out.text.contains("no API key for 'mistral'"),
            "the recorded failure: {}",
            out.text
        );
        assert!(
            out.text.contains("fix: nika explain NIKA-INFER-001"),
            "teach line: {}",
            out.text
        );
        assert!(!out.text.contains("older engine"), "no shrug: {}", out.text);
        let raw = peek(&path.to_string_lossy(), "greet", true, plain());
        assert_eq!(raw.code, exit::ENV, "raw refuses a valueless row");
        assert!(
            raw.text.contains("settled before it produced a value"),
            "raw teach: {}",
            raw.text
        );
    }

    /// The wire-code scanner: finds real codes, never prose.
    #[test]
    fn wire_code_finds_codes_and_ignores_prose() {
        assert_eq!(
            nika_dap::recover::first_wire_code("NIKA-INFER-001 · model x failed"),
            Some("NIKA-INFER-001")
        );
        assert_eq!(
            nika_dap::recover::first_wire_code("cycle found (DAG-003) in wave 2"),
            Some("DAG-003")
        );
        assert_eq!(
            nika_dap::recover::first_wire_code("plain prose failure - nothing coded"),
            None
        );
    }

    /// A guarded skip explains itself — no hypothesis, no blame.
    #[test]
    fn peek_on_a_skipped_task_explains_the_skip() {
        let path = peek_fixture("peek-skip.ndjson");
        let out = peek(&path.to_string_lossy(), "deploy", false, plain());
        assert_eq!(out.code, exit::ENV);
        assert!(
            out.text
                .contains("a guarded skip never runs, so never records"),
            "skip teach: {}",
            out.text
        );
        assert!(
            out.text.contains("outputs recorded for: audit"),
            "still names the rows that have one: {}",
            out.text
        );
    }

    /// `--raw` prints the EXACT recorded JSON text and nothing else —
    /// the jq-pipe contract.
    #[test]
    fn peek_raw_is_the_exact_value_only() {
        let path = peek_fixture("peek-raw.ndjson");
        let out = peek(&path.to_string_lossy(), "audit", true, plain());
        assert_eq!(out.code, exit::OK);
        assert_eq!(out.text, r#"{"fixes":["a"],"total":9}"#);
    }

    /// Stage a workflow file whose bindings draw the mockup DAG:
    /// `read_payload` → `audit` → `outputs.geo_score`.
    fn flow_workflow(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("nika-cli-trace-verb");
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let path = dir.join(name);
        std::fs::write(
            &path,
            "nika: geo-audit\nmodel: mock/echo\ntasks:\n  read_payload:\n    invoke: { tool: \"nika:read\", args: { path: \"x.json\" } }\n  audit:\n    with:\n      payload: ${{ tasks.read_payload.output }}\n    infer: { prompt: \"score ${{ with.payload }}\" }\noutputs:\n  geo_score: ${{ tasks.audit.output }}\n",
        )
        .expect("workflow staged");
        path
    }

    /// A trace with recorded outputs for both tasks (real sizes).
    fn flow_trace(name: &str) -> std::path::PathBuf {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let s = |k: &str, v: &str| KeyValue::new(k, Value::String(v.to_owned()));
        let events = vec![
            demo::bare_event(EventKind::WorkflowStarted, 0).with_field(s("workflow", "geo-audit")),
            demo::bare_event(EventKind::TaskCompleted, 10)
                .with_field(s("task", "read_payload"))
                .with_field(s("output", &format!("\"{}\"", "p".repeat(3598)))),
            demo::bare_event(EventKind::TaskCompleted, 40)
                .with_field(s("task", "audit"))
                .with_field(s("output", r#"{"total":9}"#)),
        ];
        stage(name, &events)
    }

    /// The waterfall: plan-binding edges × trace sizes + the
    /// outputs.<name> terminal edge + the totals line naming the widest.
    #[test]
    fn flow_joins_plan_edges_with_trace_sizes() {
        let wf = flow_workflow("flow.nika.yaml");
        let tr = flow_trace("flow.ndjson");
        let out = flow(&tr.to_string_lossy(), &wf.to_string_lossy(), plain());
        assert_eq!(out.code, exit::OK, "{}", out.text);
        let text = &out.text;
        assert!(
            text.contains("read_payload ─3.6KB→ audit"),
            "sized task edge: {text}"
        );
        assert!(
            text.contains("audit        ─11B→ outputs.geo_score"),
            "terminal outputs edge (aligned): {text}"
        );
        assert!(
            text.contains("2 edges · widest: read_payload→audit"),
            "totals + widest: {text}"
        );
        assert!(
            text.contains("derived from plan bindings × trace sizes"),
            "honesty label: {text}"
        );
        // No mismatch note when the names agree.
        assert!(!text.contains("note:"), "{text}");

        // ASCII parity — rails, arrows and the join glyph all degrade.
        let ascii = flow(
            &tr.to_string_lossy(),
            &wf.to_string_lossy(),
            Theme::new(false, true, false),
        );
        assert!(
            ascii.text.contains("read_payload -3.6KB-> audit"),
            "{}",
            ascii.text
        );
        assert!(
            ascii.text.contains("plan bindings x trace sizes"),
            "{}",
            ascii.text
        );
        for glyph in ['─', '→', '×'] {
            assert!(
                !ascii.text.contains(glyph),
                "unicode {glyph} leaked into --ascii: {}",
                ascii.text
            );
        }
    }

    /// A trace from an older engine (no output fields): the STRUCTURE
    /// still renders (bare arrows · never an invented size), and a
    /// name mismatch between trace and file says so.
    #[test]
    fn flow_degrades_honestly_without_sizes_and_flags_mismatch() {
        use nika_types::resource::{KeyValue, Value};
        let wf = flow_workflow("flow-bare.nika.yaml");
        let events = vec![
            demo::bare_event(nika_event::EventKind::WorkflowStarted, 0)
                .with_field(KeyValue::new("workflow", Value::String("other-run".into()))),
            demo::bare_event(nika_event::EventKind::TaskCompleted, 10)
                .with_field(KeyValue::new("task", Value::String("read_payload".into()))),
        ];
        let tr = stage("flow-bare.ndjson", &events);
        let out = flow(&tr.to_string_lossy(), &wf.to_string_lossy(), plain());
        assert_eq!(out.code, exit::OK);
        assert!(
            out.text.contains("read_payload → audit"),
            "structure without sizes: {}",
            out.text
        );
        assert!(
            out.text
                .contains("note: the trace records workflow `other-run`"),
            "mismatch surfaces: {}",
            out.text
        );
        assert!(
            !out.text.contains("widest"),
            "no sized edge → no widest claim: {}",
            out.text
        );
    }

    /// Errors teach: an unknown task lists what the trace records; a
    /// task without an output names its state + the rows that have one.
    #[test]
    fn peek_errors_are_readable_and_actionable() {
        let path = peek_fixture("peek-errors.ndjson");
        let trace = path.to_string_lossy();
        let unknown = peek(&trace, "ghost", false, plain());
        assert_eq!(unknown.code, exit::ENV);
        assert!(
            unknown.text.contains("unknown task `ghost`")
                && unknown.text.contains("audit · deploy"),
            "{}",
            unknown.text
        );
        let skipped = peek(&trace, "deploy", false, plain());
        assert_eq!(skipped.code, exit::ENV);
        assert!(
            skipped.text.contains("recorded no output (skipped)")
                && skipped.text.contains("outputs recorded for: audit"),
            "{}",
            skipped.text
        );
    }

    /// Wave 3 · persona 10: a fan-out over a runtime collection earns no
    /// resume stamp, so its aggregate value is never checkpointed — but its
    /// item table (index · item · status · code · message) IS on the
    /// terminal frame. `peek` must deliver the table instead of refusing
    /// with « recorded no output (ok) »; `--raw` still has no value to pipe.
    #[test]
    fn peek_delivers_the_item_table_when_the_value_was_not_checkpointed() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let items = r#"[{"index":0,"item":"./items/a.md","status":"ok"},{"index":1,"item":"./items/b.md","status":"ok"},{"index":2,"item":"./items/c.md","status":"recovered","code":"NIKA-BUILTIN-READ-001","message":"file not found: ./items/c.md"}]"#;
        let events = vec![
            demo::bare_event(EventKind::TaskStarted, 0)
                .with_field(KeyValue::new("task", Value::String("read".into())))
                .with_field(KeyValue::new(
                    "note",
                    Value::String("for_each · nika:read".into()),
                )),
            demo::bare_event(EventKind::TaskRecovered, 20)
                .with_field(KeyValue::new("task", Value::String("read".into())))
                .with_field(KeyValue::new(
                    "code",
                    Value::String("NIKA-BUILTIN-READ-001".into()),
                )),
            demo::bare_event(EventKind::TaskCompleted, 40)
                .with_field(KeyValue::new("task", Value::String("read".into())))
                .with_field(KeyValue::new(
                    "note",
                    Value::String("for_each · 2/3 ok · 1 recovered: ./items/c.md".into()),
                ))
                .with_field(KeyValue::new("duration_ms", Value::Int(4)))
                .with_field(KeyValue::new("items", Value::String(items.into()))),
            demo::bare_event(EventKind::WorkflowCompleted, 50),
        ];
        let path = stage("unstamped-fan.ndjson", &events);
        let trace = path.to_string_lossy();

        let out = peek(&trace, "read", false, plain());
        assert_eq!(out.code, exit::OK, "{}", out.text);
        for needle in [
            "items · 3",
            "NIKA-BUILTIN-READ-001",
            "file not found: ./items/c.md",
            "recovered from NIKA-BUILTIN-READ-001",
            "value not checkpointed",
        ] {
            assert!(
                out.text.contains(needle),
                "peek carries `{needle}`:\n{}",
                out.text
            );
        }

        let raw = peek(&trace, "read", true, plain());
        assert_eq!(raw.code, exit::ENV, "no value, no pipe: {}", raw.text);
        assert!(
            raw.text.contains("not checkpointed") && raw.text.contains("no resume stamp"),
            "the refusal says why the value is absent: {}",
            raw.text
        );
    }

    /// B23 / issue 1275: peek + outputs + json never render a recovered
    /// task as a clean success.
    #[test]
    fn recovered_task_is_not_a_clean_success_on_peek_outputs_or_json() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let events = vec![
            demo::bare_event(EventKind::TaskStarted, 0)
                .with_field(KeyValue::new("task", Value::String("each".into())))
                .with_field(KeyValue::new("note", Value::String("exec · do".into()))),
            demo::bare_event(EventKind::TaskRecovered, 20)
                .with_field(KeyValue::new("task", Value::String("each".into())))
                .with_field(KeyValue::new("code", Value::String("NIKA-EXEC-001".into()))),
            demo::bare_event(EventKind::TaskCompleted, 40)
                .with_field(KeyValue::new("task", Value::String("each".into())))
                .with_field(KeyValue::new(
                    "output",
                    Value::String("\"FALLBACK-DATA\"".into()),
                )),
            demo::bare_event(EventKind::WorkflowCompleted, 50),
        ];
        let path = stage("recovered-fan.ndjson", &events);
        let trace = path.to_string_lossy();

        let peek_out = peek(&trace, "each", false, plain());
        assert_eq!(peek_out.code, exit::OK);
        assert!(
            peek_out.text.contains("recovered") && peek_out.text.contains("NIKA-EXEC-001"),
            "peek names recovered_from: {}",
            peek_out.text
        );

        let table = outputs(&trace, plain());
        assert!(
            table.text.contains("recovered"),
            "outputs marks recovered: {}",
            table.text
        );

        let (view, evs) = load_view_and_events(&trace).expect("loads");
        let json = tasks_json(&view, &evs);
        // ADR-128 · `recovered` is a fact on the task row (and a tally on the
        // settlement), never a run STATE: the run succeeded.
        assert_eq!(json["state"], "succeeded");
        assert_eq!(json["tasks"][0]["status"], "recovered");
        assert_eq!(json["tasks"][0]["recovered_from"], "NIKA-EXEC-001");
        assert_eq!(json["tasks"][0]["error_code"], "NIKA-EXEC-001");
    }

    /// #1276 · #1397 · a fan-out's item table reaches every reader: the
    /// autopsy prints one line per item with the recorded code, the machine
    /// projection carries the rows, `show`'s companion tallies them.
    #[test]
    fn a_fan_out_autopsy_prints_the_item_table() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let items = r#"[{"index":0,"item":"alpha","status":"ok"},{"index":1,"item":"beta","status":"failed","code":"NIKA-EXEC-001","message":"for_each item [1] beta: exit 1"},{"index":2,"item":"gamma","status":"never_started"}]"#;
        let events = vec![
            demo::bare_event(EventKind::TaskStarted, 0)
                .with_field(KeyValue::new("task", Value::String("fan".into())))
                .with_field(KeyValue::new("note", Value::String("exec · false".into()))),
            demo::bare_event(EventKind::TaskFailed, 12)
                .with_field(KeyValue::new("task", Value::String("fan".into())))
                .with_field(KeyValue::new("duration_ms", Value::Int(9)))
                .with_field(KeyValue::new(
                    "detail",
                    Value::String("NIKA-EXEC-001 · for_each item [1] beta: exit 1".into()),
                ))
                .with_field(KeyValue::new("items", Value::String(items.into()))),
        ];
        let path = stage("peek-fan-items.ndjson", &events);
        let out = peek(&path.to_string_lossy(), "fan", false, plain());
        assert_eq!(out.code, exit::OK);
        assert!(
            out.text.contains("items · 3"),
            "the table header: {}",
            out.text
        );
        assert!(out.text.contains("alpha"), "{}", out.text);
        assert!(
            out.text.contains("beta") && out.text.contains("NIKA-EXEC-001"),
            "the failed item with its code: {}",
            out.text
        );
        assert!(
            out.text.contains("gamma") && out.text.contains("never started"),
            "the never-started item: {}",
            out.text
        );
        let (view, events) = load_view_and_events(&path.to_string_lossy()).expect("loads");
        let json = tasks_json(&view, &events);
        assert_eq!(json["tasks"][0]["items"][2]["status"], "never_started");
        assert_eq!(json["tasks"][0]["items"][1]["code"], "NIKA-EXEC-001");
        let summary = item_summary_lines(&view, &path.to_string_lossy(), plain());
        assert_eq!(summary.len(), 1, "{summary:?}");
        assert!(
            summary[0].contains("1 ok · 1 failed · 1 never started"),
            "the tally: {}",
            summary[0]
        );
    }

    /// #1444 · a task fed by a recovered fallback says so on `outputs`, on
    /// `peek` and in the JSON · the 3 am reader no longer mistakes it for a
    /// clean success.
    #[test]
    fn a_task_fed_by_a_recovered_fallback_names_its_source_on_every_surface() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let task = |name: &str| KeyValue::new("task", Value::String(name.into()));
        let events = vec![
            demo::bare_event(EventKind::TaskStarted, 0)
                .with_field(task("b"))
                .with_field(KeyValue::new("note", Value::String("exec · false".into()))),
            demo::bare_event(EventKind::TaskRecovered, 1)
                .with_field(task("b"))
                .with_field(KeyValue::new("code", Value::String("NIKA-EXEC-001".into()))),
            demo::bare_event(EventKind::TaskCompleted, 2)
                .with_field(task("b"))
                .with_field(KeyValue::new(
                    "output",
                    Value::String("\"FALLBACK-DATA\"".into()),
                )),
            demo::bare_event(EventKind::TaskStarted, 3)
                .with_field(task("c"))
                .with_field(KeyValue::new(
                    "note",
                    Value::String("invoke · nika:jq".into()),
                )),
            demo::bare_event(EventKind::TaskCompleted, 4)
                .with_field(task("c"))
                .with_field(KeyValue::new(
                    "output",
                    Value::String("\"FALLBACK-DATA\"".into()),
                ))
                .with_field(KeyValue::new(
                    "integrity",
                    Value::String("untrusted".into()),
                ))
                .with_field(KeyValue::new("integrity_source", Value::String("b".into()))),
            demo::bare_event(EventKind::WorkflowCompleted, 5),
        ];
        let path = stage("lineage.ndjson", &events);
        let table = outputs(&path.to_string_lossy(), plain());
        assert_eq!(table.code, exit::OK);
        let c_row = table
            .text
            .lines()
            .find(|l| l.trim_start().starts_with("c "))
            .expect("c's row");
        assert!(
            c_row.contains("input from recovered b"),
            "outputs names the lineage: {c_row}"
        );
        let peeked = peek(&path.to_string_lossy(), "c", false, plain());
        assert!(
            peeked.text.contains("input from recovered b"),
            "peek names the lineage: {}",
            peeked.text
        );
        let (view, events) = load_view_and_events(&path.to_string_lossy()).expect("loads");
        let json = tasks_json(&view, &events);
        assert_eq!(json["tasks"][1]["integrity_source"], "b");
        assert!(
            json["tasks"][0]["integrity_source"].is_null(),
            "b itself has no source"
        );
    }

    /// The OBS-E `warning` a terminal frame carried (a `nika:glob` naming
    /// the directories it left out · V9 wave 3 p10) reaches the machine
    /// projection per task, and a clean task projects none — `trace
    /// outputs --json` must say what `trace show` says.
    #[test]
    fn a_task_warning_is_projected_per_task() {
        use nika_event::EventKind;
        use nika_types::resource::{KeyValue, Value};
        let task = |id: &str| KeyValue::new("task", Value::String(id.into()));
        let said = "nika:glob returns files only · 1 directory also matched `./items/*.md` and was left out: ./items/item-07.md";
        let events = vec![
            demo::bare_event(EventKind::WorkflowStarted, 0),
            demo::bare_event(EventKind::TaskStarted, 1)
                .with_field(task("discover"))
                .with_field(KeyValue::new(
                    "note",
                    Value::String("invoke · nika:glob".into()),
                )),
            demo::bare_event(EventKind::TaskCompleted, 2)
                .with_field(task("discover"))
                .with_field(KeyValue::new("output", Value::String("[]".into())))
                .with_field(KeyValue::new("warning", Value::String(said.into()))),
            demo::bare_event(EventKind::TaskStarted, 3)
                .with_field(task("merge"))
                .with_field(KeyValue::new(
                    "note",
                    Value::String("infer · mock/echo".into()),
                )),
            demo::bare_event(EventKind::TaskCompleted, 4)
                .with_field(task("merge"))
                .with_field(KeyValue::new("output", Value::String("\"ok\"".into()))),
            demo::bare_event(EventKind::WorkflowCompleted, 5),
        ];
        let path = stage("glob-warning.ndjson", &events);
        let (view, events) = load_view_and_events(&path.to_string_lossy()).expect("loads");
        let json = tasks_json(&view, &events);
        assert_eq!(json["tasks"][0]["id"], "discover");
        assert_eq!(json["tasks"][0]["warning"], said);
        assert!(
            json["tasks"][1]["warning"].is_null(),
            "a clean task projects no warning"
        );
    }
}
