// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The static trace readers — `nika trace outputs` (per-task browser)
//! + `nika trace peek` (full fidelity).
//!
//! Pure reads over a recorded NDJSON trace: load (the SAME tolerant
//! recovery `--resume` and `trace show` fold through) → fold into the
//! ONE [`RunView`] truth → render. Three densities, one source: the
//! storyboard shows SHAPE tails, the table shows bounded previews,
//! `peek` shows the whole value + its ADR-099 identity.

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

/// `nika trace peek <trace> <task>` — the full-fidelity read: the
/// task's whole output pretty-printed under a compact identity block
/// (verb · duration · tokens · the ADR-099 hashes). `--raw` prints the
/// EXACT recorded value as one JSON text — pipeable to jq, never
/// coloured, nothing else on stdout.
#[must_use]
pub fn peek(trace: &str, task: &str, raw: bool, theme: Theme) -> VerbOutput {
    let view = match load_view(trace) {
        Ok(view) => view,
        Err(out) => return out,
    };
    let Some(row) = view.rows().iter().find(|r| r.id == task) else {
        return VerbOutput::env(unknown_task_message(&view, trace, task));
    };
    let Some(text) = row.output_json.as_deref() else {
        return VerbOutput::env(no_output_message(&view, row));
    };
    if raw {
        // The exact recorded value — the machine arm of peek.
        return VerbOutput::ok(text.to_owned());
    }
    VerbOutput::ok(render_peek(row, text, theme))
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
    if with_outputs.is_empty() {
        message.push_str(" — no task in this trace carries one (an older engine's trace?)");
    } else {
        let _ = write!(
            message,
            " — outputs recorded for: {}",
            with_outputs.join(" · ")
        );
    }
    message
}

/// The pretty read: identity block (task · verb · time · tokens ·
/// hashes) then the full value, pretty-printed. A value that is not
/// valid JSON (a hand-edited trace) prints verbatim — honesty over
/// polish.
fn render_peek(row: &TaskRow, text: &str, theme: Theme) -> String {
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
            Theme {
                color: false,
                ascii: true,
                animate: false,
            },
        );
        assert!(
            ascii.text.contains("5b2fa9e9232e.."),
            "ascii clip: {}",
            ascii.text
        );
        assert!(!ascii.text.contains('…'), "no unicode under --ascii");
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
}
