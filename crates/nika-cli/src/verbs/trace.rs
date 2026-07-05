// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The static trace readers — `nika trace outputs` (per-task browser).
//!
//! Pure reads over a recorded NDJSON trace: load (the SAME tolerant
//! recovery `--resume` and `trace show` fold through) → fold into the
//! ONE [`RunView`] truth → render. Three densities, one source: the
//! storyboard shows SHAPE tails, this table shows bounded previews,
//! `trace peek` shows full fidelity.

use std::fmt::Write as _;

use crate::display::flow::fmt_wall_ms;
use crate::display::shape;
use crate::display::theme::{Role, Theme};
use crate::{RunView, TaskRow};

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

/// Load + tolerantly parse + fold one trace file (the shared entry of
/// every static trace reader).
pub(crate) fn load_view(trace: &str) -> Result<RunView, VerbOutput> {
    let raw = std::fs::read_to_string(trace)
        .map_err(|e| VerbOutput::env(format!("cannot read {trace}: {e}")))?;
    let recovered = super::run::recover_events(&raw, trace).map_err(VerbOutput::env)?;
    let mut view = RunView::new();
    for event in &recovered.events {
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

    let mut out = String::new();
    let head = format!(
        "  {:<w0$}  {:<w1$}  {:>w2$}  {:>w3$}  output",
        header[0], header[1], header[2], header[3],
    );
    let _ = writeln!(out, "{}", theme.paint(Role::Dim, &head));
    for (row, c) in rows.iter().zip(&cells) {
        let _ = writeln!(
            out,
            "  {:<w0$}  {}  {:>w2$}  {:>w3$}  {}",
            c[0],
            theme.paint(Role::Dim, &format!("{:<w1$}", c[1])),
            c[2],
            c[3],
            preview_cell(row, theme),
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
        "  {} task(s) · {}",
        view.rows().len(),
        fmt_wall_ms(view.elapsed_ms)
    );
    let tokens: u64 = view.rows().iter().filter_map(|r| r.tokens).sum();
    if tokens > 0 {
        let _ = write!(line, " · {tokens} tok");
    }
    let _ = write!(line, " · full value: nika trace peek {trace} <task>");
    theme.paint(Role::Dim, &line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;
    use crate::verbs::exit;

    fn plain() -> Theme {
        Theme {
            color: false,
            ascii: false,
            animate: false,
        }
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
        assert!(text.contains("5 task(s)"), "totals: {text}");
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
        let ascii = outputs(
            &path.to_string_lossy(),
            Theme {
                color: false,
                ascii: true,
                animate: false,
            },
        );
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
}
